use crate::error::BridgeError;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{oneshot, Mutex};
use tokio::time::{sleep, timeout, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn};

const CDP_HOST: &str = "127.0.0.1";
const DEFAULT_CDP_PORT: u16 = 9222;

#[derive(Clone, Debug)]
pub struct ChromeConfig {
    pub chrome_path: Option<String>,
    pub chrome_profile: PathBuf,
    pub headless: bool,
    pub cdp_port: u16,
    pub visible: bool,
}

impl Default for ChromeConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        Self {
            chrome_path: None,
            chrome_profile: PathBuf::from(home).join(".codexdcp/chrome-profile"),
            headless: true,
            cdp_port: DEFAULT_CDP_PORT,
            visible: false,
        }
    }
}

pub struct ChromeProcess {
    child: tokio::process::Child,
    pub port: u16,
}

impl ChromeProcess {
    pub async fn launch(config: &ChromeConfig) -> Result<Self, BridgeError> {
        let chrome_path = find_chrome(config.chrome_path.as_deref())?;
        let port = config.cdp_port;

        if !config.chrome_profile.exists() {
            std::fs::create_dir_all(&config.chrome_profile)
                .map_err(|e| BridgeError::ChromeError(format!("failed to create profile dir: {}", e)))?;
        }

        let mut args = vec![
            format!("--remote-debugging-port={}", port),
            format!("--user-data-dir={}", config.chrome_profile.display()),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
            "--disable-extensions".to_string(),
            "--mute-audio".to_string(),
            "--disable-popup-blocking".to_string(),
            "--window-size=1280,720".to_string(),
        ];

        if config.headless && !config.visible {
            args.push("--headless=new".to_string());
        }

        info!("launching Chrome: {} {}", chrome_path, args.join(" "));

        let child = tokio::process::Command::new(&chrome_path)
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| BridgeError::ChromeError(format!("failed to launch Chrome: {}", e)))?;

        let mut me = Self { child, port };

        // Wait for Chrome's debug port to be ready
        let ready = timeout(Duration::from_secs(15), async {
            loop {
                if cdp_http_get(CDP_HOST, port, "/json/version").await.is_ok() {
                    return;
                }
                sleep(Duration::from_millis(300)).await;
            }
        })
        .await;

        if ready.is_err() {
            me.kill().await;
            return Err(BridgeError::ChromeError(
                "Chrome debug port did not become ready within 15s".to_string(),
            ));
        }

        info!("Chrome is ready on port {}", port);
        Ok(me)
    }

    pub async fn kill(&mut self) {
        let _ = self.child.kill().await;
    }
}

impl Drop for ChromeProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

async fn cdp_http_get(host: &str, port: u16, path: &str) -> Result<serde_json::Value, BridgeError> {
    cdp_http_request(host, port, "GET", path).await
}

async fn cdp_http_put(host: &str, port: u16, path: &str) -> Result<serde_json::Value, BridgeError> {
    cdp_http_request(host, port, "PUT", path).await
}

async fn cdp_http_request(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
) -> Result<serde_json::Value, BridgeError> {
    let mut stream = TcpStream::connect(format!("{}:{}", host, port))
        .await
        .map_err(|e| BridgeError::CdpError(format!("failed to connect to Chrome debug port: {}", e)))?;

    let request = format!(
        "{} {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        method, path, host, port
    );
    stream.write_all(request.as_bytes()).await.map_err(|e| {
        BridgeError::CdpError(format!("failed to send HTTP request: {}", e))
    })?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.map_err(|e| {
        BridgeError::CdpError(format!("failed to read HTTP response: {}", e))
    })?;

    let response = String::from_utf8_lossy(&buf);
    let body_start = response.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    let body = &response[body_start..];

    serde_json::from_str(body)
        .map_err(|e| BridgeError::CdpError(format!("failed to parse CDP JSON: {}", e)))
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
struct TabInfo {
    #[serde(default)]
    _id: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    webSocketDebuggerUrl: String,
    #[serde(rename = "type", default)]
    tab_type: String,
}

pub struct CdpClient {
    inner: Arc<CdpInner>,
}

impl Clone for CdpClient {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value, String>>>>>;

struct CdpInner {
    ws_tx: Mutex<tokio::sync::mpsc::UnboundedSender<String>>,
    pending: PendingMap,
    next_id: AtomicU64,
    connected: Arc<std::sync::atomic::AtomicBool>,
}

impl CdpClient {
    pub async fn connect(port: u16) -> Result<Self, BridgeError> {
        let tabs: Vec<TabInfo> = serde_json::from_value(
            cdp_http_get(CDP_HOST, port, "/json/list").await?,
        )
        .map_err(|e| BridgeError::CdpError(format!("failed to parse tab list: {}", e)))?;

        let chatgpt_tab = tabs.iter().find(|t| {
            t.tab_type == "page" && (t.url.contains("chatgpt.com") || t.url.contains("chat.openai.com"))
        });

        let tab_ws_url = if let Some(tab) = chatgpt_tab {
            info!("found existing ChatGPT tab: {}", tab.url);
            tab.webSocketDebuggerUrl.clone()
        } else {
            info!("no ChatGPT tab found, creating one");
            let new_tab: TabInfo = serde_json::from_value(
                cdp_http_put(CDP_HOST, port, "/json/new?https://chatgpt.com/").await?,
            )
            .map_err(|e| BridgeError::CdpError(format!("failed to parse new tab response: {}", e)))?;
            new_tab.webSocketDebuggerUrl
        };

        if tab_ws_url.is_empty() {
            return Err(BridgeError::CdpError("tab has no WebSocket URL".to_string()));
        }

        info!("connecting to CDP WebSocket: {}", tab_ws_url);
        let (ws, _) = connect_async(&tab_ws_url)
            .await
            .map_err(|e| BridgeError::CdpError(format!("failed to connect to CDP WebSocket: {}", e)))?;

        let (mut write, mut read) = ws.split();
        let (ws_tx, mut ws_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        let pending: Mutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value, String>>>> =
            Mutex::new(HashMap::new());
        let pending_arc = Arc::new(pending);
        let connected = std::sync::atomic::AtomicBool::new(true);
        let connected_arc = Arc::new(connected);

        // Write task
        let _write_task = tokio::spawn(async move {
            while let Some(msg) = ws_rx.recv().await {
                if write.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
        });

        // Read task
        let pending_clone = pending_arc.clone();
        let connected_clone = connected_arc.clone();
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text)
                            && let Some(id) = val.get("id").and_then(|i| i.as_u64())
                        {
                            let result = if let Some(err) = val.get("error") {
                                Err(err.to_string())
                            } else {
                                Ok(val.get("result").cloned().unwrap_or(serde_json::Value::Null))
                            };
                            if let Some(tx) = pending_clone.lock().await.remove(&id) {
                                let _ = tx.send(result);
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        connected_clone.store(false, Ordering::Relaxed);
                        warn!("CDP WebSocket closed");
                        break;
                    }
                    Ok(_) => continue,
                    Err(e) => {
                        connected_clone.store(false, Ordering::Relaxed);
                        warn!("CDP WebSocket error: {}", e);
                        break;
                    }
                }
            }
            connected_clone.store(false, Ordering::Relaxed);
            // Wake up any pending requests
            pending_clone.lock().await.clear();
        });

        let inner = Arc::new(CdpInner {
            ws_tx: Mutex::new(ws_tx),
            pending: pending_arc,
            next_id: AtomicU64::new(1),
            connected: connected_arc,
        });

        let client = Self { inner };

        // Enable Runtime and Page
        client.send_command("Runtime.enable", serde_json::json!({})).await?;
        client.send_command("Page.enable", serde_json::json!({})).await?;

        Ok(client)
    }

    pub fn is_connected(&self) -> bool {
        self.inner.connected.load(Ordering::Relaxed)
    }

    pub async fn send_command(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, BridgeError> {
        self.send_command_with_timeout(method, params, Duration::from_secs(30)).await
    }

    async fn send_command_with_timeout(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout_dur: Duration,
    ) -> Result<serde_json::Value, BridgeError> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let msg = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        });
        let json = serde_json::to_string(&msg)
            .map_err(|e| BridgeError::CdpError(e.to_string()))?;

        let (tx, rx) = oneshot::channel();
        self.inner.pending.lock().await.insert(id, tx);

        let send_result = {
            let ws_tx = self.inner.ws_tx.lock().await;
            ws_tx.send(json)
        };

        if send_result.is_err() {
            self.inner.pending.lock().await.remove(&id);
            return Err(BridgeError::NotConnected);
        }

        let result = timeout(timeout_dur, rx).await;
        self.inner.pending.lock().await.remove(&id);

        match result {
            Ok(Ok(Ok(val))) => Ok(val),
            Ok(Ok(Err(e))) => Err(BridgeError::CdpError(e)),
            Ok(Err(_)) => Err(BridgeError::NotConnected),
            Err(_) => Err(BridgeError::Timeout(30)),
        }
    }

    pub async fn navigate(&self, url: &str) -> Result<(), BridgeError> {
        self.send_command("Page.navigate", serde_json::json!({ "url": url }))
            .await?;
        // Wait for page to settle
        sleep(Duration::from_secs(3)).await;
        Ok(())
    }

    pub async fn evaluate(&self, expression: &str) -> Result<serde_json::Value, BridgeError> {
        self.evaluate_with_timeout(expression, Duration::from_secs(120))
            .await
    }

    pub async fn evaluate_with_timeout(
        &self,
        expression: &str,
        timeout_dur: Duration,
    ) -> Result<serde_json::Value, BridgeError> {
        let result = self
            .send_command_with_timeout(
                "Runtime.evaluate",
                serde_json::json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": true,
                }),
                timeout_dur,
            )
            .await;

        match result {
            Ok(val) => {
                if let Some(exception) = val.get("exceptionDetails") {
                    let desc = exception
                        .get("exception")
                        .and_then(|e| e.get("description"))
                        .and_then(|d| d.as_str())
                        .unwrap_or("unknown JS exception");
                    return Err(BridgeError::JsError(desc.to_string()));
                }
                let value = val
                    .get("result")
                    .and_then(|r| r.get("value"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Ok(value)
            }
            Err(e) => Err(e),
        }
    }
}

async fn is_chrome_running(port: u16) -> bool {
    TcpStream::connect(format!("{}:{}", CDP_HOST, port))
        .await
        .is_ok()
}

pub async fn ensure_chrome(config: &ChromeConfig) -> Result<Option<ChromeProcess>, BridgeError> {
    if is_chrome_running(config.cdp_port).await {
        info!("Chrome already running on port {}, connecting", config.cdp_port);
        return Ok(None);
    }
    let proc = ChromeProcess::launch(config).await?;
    Ok(Some(proc))
}

fn find_chrome(override_path: Option<&str>) -> Result<String, BridgeError> {
    if let Some(path) = override_path {
        if Path::new(path).exists() {
            return Ok(path.to_string());
        }
        return Err(BridgeError::ChromeError(format!(
            "Chrome binary not found at: {}",
            path
        )));
    }

    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &[
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ]
    } else if cfg!(target_os = "linux") {
        &[
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/usr/bin/brave-browser",
            "/usr/bin/microsoft-edge",
        ]
    } else {
        &[]
    };

    for path in candidates {
        if Path::new(path).exists() {
            return Ok(path.to_string());
        }
    }

    Err(BridgeError::ChromeError(
        "Chrome/Chromium/Brave not found. Use --chrome-path to specify the binary.".to_string(),
    ))
}
