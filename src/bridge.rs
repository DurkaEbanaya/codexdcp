use crate::cdp::{CdpClient, ChromeProcess, ChromeConfig, ensure_chrome};
use crate::error::BridgeError;
use crate::js;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{broadcast, oneshot, Mutex, Notify};
use tokio::time::sleep;
use tracing::{info, warn};

pub struct StreamHandle {
    pub partials: broadcast::Receiver<String>,
    pub result: oneshot::Receiver<Result<String, BridgeError>>,
}

struct Inner {
    cdp: Mutex<Option<CdpClient>>,
    _chrome: Mutex<Option<ChromeProcess>>,
    selectors: String,
    initialized: AtomicBool,
    temp_chat_on: AtomicBool,
    max_retries: u32,
    retry_delay_ms: u64,
    ready: Notify,
    ready_flag: AtomicBool,
}

#[derive(Clone)]
pub struct Bridge {
    inner: Arc<Inner>,
}

impl Bridge {
    pub fn new(selectors: String, max_retries: u32, retry_delay_ms: u64) -> Self {
        Self {
            inner: Arc::new(Inner {
                cdp: Mutex::new(None),
                _chrome: Mutex::new(None),
                selectors,
                initialized: AtomicBool::new(false),
                temp_chat_on: AtomicBool::new(false),
                max_retries,
                retry_delay_ms,
                ready: Notify::new(),
                ready_flag: AtomicBool::new(false),
            }),
        }
    }

    pub async fn start(&self, chrome_config: &ChromeConfig) -> anyhow::Result<()> {
        let chrome_proc = ensure_chrome(chrome_config).await?;

        if let Some(proc) = &chrome_proc {
            info!("Chrome launched on port {}", proc.port);
        }

        let client = CdpClient::connect(chrome_config.cdp_port).await?;

        {
            let mut cdp = self.inner.cdp.lock().await;
            *cdp = Some(client);
        }
        {
            let mut chrome = self.inner._chrome.lock().await;
            *chrome = chrome_proc;
        }

        self.ensure_chatgpt_ready().await?;

        self.inner.ready_flag.store(true, Ordering::Relaxed);
        self.inner.ready.notify_waiters();
        info!("bridge ready — ChatGPT tab initialized");
        Ok(())
    }

    /// Wait for the bridge to become ready (up to 60 seconds).
    /// Returns immediately if already ready.
    pub async fn wait_ready(&self) -> Result<(), BridgeError> {
        if self.inner.ready_flag.load(Ordering::Relaxed) {
            return Ok(());
        }
        let timeout = Duration::from_secs(60);
        match tokio::time::timeout(timeout, self.inner.ready.notified()).await {
            Ok(()) => {
                info!("bridge ready signal received");
                Ok(())
            }
            Err(_) => Err(BridgeError::NotConnected),
        }
    }

    async fn cdp(&self) -> Result<CdpClient, BridgeError> {
        let guard = self.inner.cdp.lock().await;
        guard
            .clone()
            .ok_or(BridgeError::NotConnected)
    }

    pub async fn is_connected(&self) -> bool {
        let guard = self.inner.cdp.lock().await;
        guard.as_ref().is_some_and(|c| c.is_connected())
    }

    async fn ensure_chatgpt_ready(&self) -> Result<(), BridgeError> {
        let client = self.cdp().await?;

        let url_result = client.evaluate("window.location.href").await;
        let current_url = url_result
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();

        if !current_url.contains("chatgpt.com") && !current_url.contains("chat.openai.com") {
            info!("navigating to ChatGPT (current: {})", current_url);
            client.navigate("https://chatgpt.com/?temporary-chat=true").await?;
            self.inner.initialized.store(false, Ordering::Relaxed);
        }

        self.ensure_initialized().await
    }

    async fn ensure_initialized(&self) -> Result<(), BridgeError> {
        if self.inner.initialized.load(Ordering::Relaxed) {
            let client = self.cdp().await?;
            if client.is_connected() {
                let ready = client.evaluate(js::call_is_ready()).await.ok();
                if ready.and_then(|v| v.as_bool()).unwrap_or(false) {
                    return Ok(());
                }
            }
        }

        let client = self.cdp().await?;

        for attempt in 0..15 {
            let title_result = client.evaluate("document.title").await;
            let title = title_result
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();

            if !title.contains("Один момент") && !title.is_empty() {
                break;
            }
            info!("waiting for ChatGPT to load (attempt {})...", attempt + 1);
            sleep(Duration::from_secs(2)).await;
        }

        let script = js::init_script(&self.inner.selectors);
        client.evaluate(&script).await?;

        let ready = client.evaluate(js::call_is_ready()).await?;
        if ready.as_bool() == Some(true) {
            self.inner.initialized.store(true, Ordering::Relaxed);
            self.inner.temp_chat_on.store(false, Ordering::Relaxed);
            info!("JS functions injected successfully");
            Ok(())
        } else {
            Err(BridgeError::JsError("failed to inject JS functions".to_string()))
        }
    }

    async fn ensure_temp_chat_on(&self) -> Result<(), BridgeError> {
        if self.inner.temp_chat_on.load(Ordering::Relaxed) {
            return Ok(());
        }
        let client = self.cdp().await?;

        // Check if temp chat is already active via the label element
        let is_temp = client
            .evaluate(&js::call_is_temp_chat())
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !is_temp {
            info!("navigating to temporary chat URL");
            client.navigate("https://chatgpt.com/?temporary-chat=true").await?;
            self.inner.initialized.store(false, Ordering::Relaxed);
            self.ensure_initialized().await?;
        }

        self.inner.temp_chat_on.store(true, Ordering::Relaxed);
        info!("temporary chat enabled");
        Ok(())
    }

    pub async fn request(
        &self,
        prompt: String,
        timeout_secs: u64,
        model: Option<String>,
        format: Option<String>,
    ) -> Result<String, BridgeError> {
        self.wait_ready().await?;

        let max_retries = self.inner.max_retries;
        let retry_delay_ms = self.inner.retry_delay_ms;

        let mut last_err = BridgeError::NotConnected;
        for attempt in 0..=max_retries {
            if attempt > 0 {
                let multiplier = 2u64.saturating_pow(attempt - 1);
                let delay = Duration::from_millis(retry_delay_ms.saturating_mul(multiplier));
                warn!("retry attempt {} after {}ms", attempt, delay.as_millis());
                tokio::time::sleep(delay).await;
            }

            match self
                .request_once(prompt.clone(), timeout_secs, model.clone(), format.clone())
                .await
            {
                Ok(result) => return Ok(result),
                Err(e) if is_transient(&e) && attempt < max_retries => {
                    warn!("attempt {} failed (transient): {}", attempt + 1, e);
                    last_err = e;
                    let _ = self.ensure_chatgpt_ready().await;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err)
    }

    async fn request_once(
        &self,
        prompt: String,
        timeout_secs: u64,
        model: Option<String>,
        format: Option<String>,
    ) -> Result<String, BridgeError> {
        self.ensure_chatgpt_ready().await?;
        self.ensure_temp_chat_on().await?;
        let client = self.cdp().await?;
        let fmt = format.as_deref().unwrap_or("markdown");

        if let Some(ref model_name) = model {
            info!("selecting model: {}", model_name);
            let _ = client.evaluate(js::call_click_model_button()).await;
            sleep(Duration::from_millis(500)).await;
            let _ = client.evaluate(&js::call_select_model(model_name)).await;
            sleep(Duration::from_millis(500)).await;
        }

        let send_result = client.evaluate(&js::call_send_prompt(&prompt)).await?;
        if let Some(err) = send_result.get("error") {
            return Err(BridgeError::ExtensionError(
                err["message"].as_str().unwrap_or("failed to send prompt").to_string(),
            ));
        }

        let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs.min(90));
        let mut last_text = String::new();
        let mut stable_count = 0u32;

        while std::time::Instant::now() < deadline {
            sleep(Duration::from_millis(1000)).await;

            let result = match client.evaluate(&js::call_read_and_check(fmt)).await {
                Ok(v) => v,
                Err(_) => continue,
            };

            let text = result.get("text").and_then(|t| t.as_str()).unwrap_or("");
            let is_generating = result.get("isGenerating").and_then(|g| g.as_bool()).unwrap_or(false);

            if !text.is_empty() {
                if text != last_text {
                    last_text = text.to_string();
                    stable_count = 0;
                } else if !is_generating {
                    stable_count += 1;
                    if stable_count >= 2 {
                        info!("response stable: {} chars", last_text.len());
                        return Ok(last_text);
                    }
                }
            }
        }

        if !last_text.is_empty() {
            info!("timeout, returning partial: {} chars", last_text.len());
            return Ok(last_text);
        }

        Err(BridgeError::Timeout(timeout_secs))
    }

    pub async fn request_streaming(
        &self,
        prompt: String,
        timeout_secs: u64,
        model: Option<String>,
        format: Option<String>,
    ) -> Result<StreamHandle, BridgeError> {
        self.wait_ready().await?;
        self.ensure_chatgpt_ready().await?;
        self.ensure_temp_chat_on().await?;
        let client = self.cdp().await?;
        let fmt = format.as_deref().unwrap_or("markdown").to_string();

        if let Some(ref model_name) = model {
            let _ = client.evaluate(js::call_click_model_button()).await;
            sleep(Duration::from_millis(500)).await;
            let _ = client.evaluate(&js::call_select_model(model_name)).await;
            sleep(Duration::from_millis(500)).await;
        }

        let send_result = client.evaluate(&js::call_send_prompt(&prompt)).await?;
        if let Some(err) = send_result.get("error") {
            return Err(BridgeError::ExtensionError(
                err["message"].as_str().unwrap_or("failed to send prompt").to_string(),
            ));
        }

        let (partial_tx, partial_rx) = broadcast::channel::<String>(32);
        let (result_tx, result_rx) = oneshot::channel();

        let client_clone = client;
        let fmt_clone = fmt;
        tokio::spawn(async move {
            let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs.min(120));
            let mut last_text = String::new();
            let mut stable_count = 0u32;

            while std::time::Instant::now() < deadline {
                sleep(Duration::from_millis(1000)).await;

                let result = match client_clone.evaluate(&js::call_read_and_check(&fmt_clone)).await {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let text = result.get("text").and_then(|t| t.as_str()).unwrap_or("");
                let is_generating = result.get("isGenerating").and_then(|g| g.as_bool()).unwrap_or(false);

                if !text.is_empty() {
                    if text != last_text {
                        let _ = partial_tx.send(text.to_string());
                        last_text = text.to_string();
                        stable_count = 0;
                    } else if !is_generating {
                        stable_count += 1;
                        if stable_count >= 2 {
                            let _ = result_tx.send(Ok(last_text));
                            return;
                        }
                    }
                }
            }

            if !last_text.is_empty() {
                let _ = result_tx.send(Ok(last_text));
            } else {
                let _ = result_tx.send(Err(BridgeError::Timeout(timeout_secs)));
            }
        });

        Ok(StreamHandle {
            partials: partial_rx,
            result: result_rx,
        })
    }
}

fn is_transient(err: &BridgeError) -> bool {
    match err {
        BridgeError::Timeout(_) => true,
        BridgeError::ExtensionError(msg) => {
            msg.contains("not detected")
                || msg.contains("No assistant message")
                || msg.contains("No response received")
                || msg.contains("not found")
        }
        BridgeError::NotConnected => true,
        BridgeError::CdpError(_) => true,
        _ => false,
    }
}
