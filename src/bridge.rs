use crate::{config::Config, error::BridgeError};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio::time::timeout;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Which browser-side action the bridge should execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    SendMessage,
    NewChat,
}

impl Method {
    fn as_str(&self) -> &'static str {
        match self {
            Method::SendMessage => "send_message",
            Method::NewChat => "new_chat",
        }
    }
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type")]
enum ServerMessage {
    #[serde(rename = "request")]
    Request {
        id: String,
        method: &'static str,
        params: RequestParams,
    },
    #[serde(rename = "pong")]
    Pong,
}

#[derive(Serialize, Debug, Clone)]
struct RequestParams {
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_chat: Option<bool>,
    timeout: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
enum ClientMessage {
    #[serde(rename = "register")]
    Register { client: String },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "partial")]
    Partial { id: String, text: String },
    #[serde(rename = "response")]
    Response {
        id: String,
        #[serde(default)]
        result: Option<ResponseResult>,
        #[serde(default)]
        error: Option<ErrorPayload>,
    },
}

#[derive(Deserialize, Debug, Clone)]
pub struct ResponseResult {
    pub text: String,
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct ErrorPayload {
    message: String,
}

struct Client {
    id: Uuid,
    sender: mpsc::UnboundedSender<String>,
}

struct Inner {
    host: String,
    port: u16,
    client: Mutex<Option<Client>>,
    pending: Mutex<HashMap<String, oneshot::Sender<Result<ResponseResult, BridgeError>>>>,
    partials: Mutex<HashMap<String, broadcast::Sender<String>>>,
    max_retries: u32,
    retry_delay_ms: u64,
    has_active_chat: AtomicBool,
}

/// Handle returned by `request_streaming` — provides partial updates and the final result.
pub struct StreamHandle {
    pub partials: broadcast::Receiver<String>,
    pub result: oneshot::Receiver<Result<ResponseResult, BridgeError>>,
}

/// Bridge between the MCP server and the browser extension.
#[derive(Clone)]
pub struct Bridge {
    inner: Arc<Inner>,
}

impl Bridge {
    pub fn new(config: Config) -> Self {
        Self {
            inner: Arc::new(Inner {
                host: config.ws_host,
                port: config.ws_port,
                client: Mutex::new(None),
                pending: Mutex::new(HashMap::new()),
                partials: Mutex::new(HashMap::new()),
                max_retries: config.max_retries,
                retry_delay_ms: config.retry_delay_ms,
                has_active_chat: AtomicBool::new(false),
            }),
        }
    }

    /// Returns `true` if the browser extension is currently connected.
    pub async fn is_connected(&self) -> bool {
        self.inner.client.lock().await.is_some()
    }

    /// Returns `true` if there is an active ChatGPT conversation (at least one message sent).
    pub fn has_active_chat(&self) -> bool {
        self.inner.has_active_chat.load(Ordering::Relaxed)
    }

    /// Send a request with retry logic and wait for the response.
    pub async fn request(
        &self,
        method: Method,
        prompt: String,
        new_chat: bool,
        timeout_secs: u64,
        model: Option<String>,
        format: Option<String>,
    ) -> Result<String, BridgeError> {
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
                .request_once(method, prompt.clone(), new_chat, timeout_secs, model.clone(), format.clone())
                .await
            {
                Ok(result) => {
                    if method == Method::SendMessage {
                        self.inner.has_active_chat.store(true, Ordering::Relaxed);
                    } else if method == Method::NewChat {
                        self.inner.has_active_chat.store(false, Ordering::Relaxed);
                    }
                    return Ok(result);
                }
                Err(e) if is_transient(&e) && attempt < max_retries => {
                    warn!("attempt {} failed (transient): {}", attempt + 1, e);
                    last_err = e;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err)
    }

    /// Send a streaming request — returns a handle with partial updates and the final result.
    pub async fn request_streaming(
        &self,
        method: Method,
        prompt: String,
        new_chat: bool,
        timeout_secs: u64,
        model: Option<String>,
        format: Option<String>,
    ) -> Result<StreamHandle, BridgeError> {
        self.request_streaming_once(method, prompt, new_chat, timeout_secs, model, format)
            .await
    }

    /// Single-attempt request (no retry).
    async fn request_once(
        &self,
        method: Method,
        prompt: String,
        new_chat: bool,
        timeout_secs: u64,
        model: Option<String>,
        format: Option<String>,
    ) -> Result<String, BridgeError> {
        let (id, json) = self.build_request(method, &prompt, new_chat, timeout_secs, &model, &format)?;

        let (response_tx, response_rx) = oneshot::channel();
        self.inner.pending.lock().await.insert(id.clone(), response_tx);

        self.send_to_client(&id, json).await?;

        let total_timeout = Duration::from_secs(timeout_secs.saturating_add(10));
        let outcome = match timeout(total_timeout, response_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(BridgeError::NotConnected),
            Err(_) => Err(BridgeError::Timeout(timeout_secs)),
        };

        self.inner.pending.lock().await.remove(&id);
        outcome.map(|r| r.text)
    }

    /// Single-attempt streaming request.
    async fn request_streaming_once(
        &self,
        method: Method,
        prompt: String,
        new_chat: bool,
        timeout_secs: u64,
        model: Option<String>,
        format: Option<String>,
    ) -> Result<StreamHandle, BridgeError> {
        let (id, json) = self.build_request(method, &prompt, new_chat, timeout_secs, &model, &format)?;

        let (response_tx, response_rx) = oneshot::channel();
        let (partial_tx, partial_rx) = broadcast::channel::<String>(32);

        self.inner.pending.lock().await.insert(id.clone(), response_tx);
        self.inner.partials.lock().await.insert(id.clone(), partial_tx);

        self.send_to_client(&id, json).await?;

        if method == Method::SendMessage {
            self.inner.has_active_chat.store(true, Ordering::Relaxed);
        }

        Ok(StreamHandle {
            partials: partial_rx,
            result: response_rx,
        })
    }

    fn build_request(
        &self,
        method: Method,
        prompt: &str,
        new_chat: bool,
        timeout_secs: u64,
        model: &Option<String>,
        format: &Option<String>,
    ) -> Result<(String, String), BridgeError> {
        let id = Uuid::new_v4().to_string();
        let params = RequestParams {
            prompt: prompt.to_string(),
            new_chat: (method == Method::SendMessage).then_some(new_chat),
            timeout: timeout_secs,
            model: model.clone(),
            format: format.clone(),
        };
        let message = ServerMessage::Request {
            id: id.clone(),
            method: method.as_str(),
            params,
        };
        let json = serde_json::to_string(&message)
            .map_err(|e| BridgeError::SendError(e.to_string()))?;
        Ok((id, json))
    }

    async fn send_to_client(&self, id: &str, json: String) -> Result<(), BridgeError> {
        let client_snapshot = {
            let client_guard = self.inner.client.lock().await;
            client_guard.as_ref().map(|c| (c.id, c.sender.clone()))
        };

        match client_snapshot {
            Some((client_id, sender)) => {
                if sender.send(json).is_err() {
                    let mut guard = self.inner.client.lock().await;
                    if guard.as_ref().map(|c| c.id) == Some(client_id) {
                        guard.take();
                    }
                    self.inner.pending.lock().await.remove(id);
                    self.inner.partials.lock().await.remove(id);
                    return Err(BridgeError::NotConnected);
                }
                Ok(())
            }
            None => {
                self.inner.pending.lock().await.remove(id);
                self.inner.partials.lock().await.remove(id);
                Err(BridgeError::NotConnected)
            }
        }
    }

    /// Start the WebSocket server. Runs until the process is shut down.
    pub async fn start(&self) -> anyhow::Result<()> {
        let addr = self.websocket_addr();
        let listener = TcpListener::bind(&addr).await?;
        info!("WebSocket bridge listening on ws://{}", addr);

        loop {
            let (stream, peer) = listener.accept().await?;
            let this = self.clone();
            tokio::spawn(async move {
                if let Err(e) = this.handle_connection(stream, peer).await {
                    warn!("connection from {} closed: {}", peer, e);
                }
            });
        }
    }

    fn websocket_addr(&self) -> String {
        format!("{}:{}", self.inner.host, self.inner.port)
    }

    async fn handle_connection(
        &self,
        stream: tokio::net::TcpStream,
        peer: SocketAddr,
    ) -> anyhow::Result<()> {
        let ws = accept_async(stream).await?;
        let (mut write, mut read) = ws.split();
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
        let connection_id = Uuid::new_v4();

        {
            let mut client_guard = self.inner.client.lock().await;
            *client_guard = Some(Client {
                id: connection_id,
                sender: out_tx.clone(),
            });
            info!(
                "browser extension connected from {} (id={})",
                peer, connection_id
            );
        }

        let result = loop {
            tokio::select! {
                outgoing = out_rx.recv() => {
                    match outgoing {
                        Some(msg) => {
                            if let Err(e) = write.send(Message::Text(msg.into())).await {
                                break Err(e.into());
                            }
                        }
                        None => break Ok(()),
                    }
                }
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match serde_json::from_str::<ClientMessage>(&text) {
                                Ok(ClientMessage::Ping) => {
                                    let pong = serde_json::to_string(&ServerMessage::Pong)
                                        .unwrap_or_default();
                                    let _ = out_tx.send(pong);
                                }
                                Ok(client_msg) => self.handle_client_message(client_msg, peer).await,
                                Err(e) => warn!(
                                    "invalid JSON from {}: {} (payload: {})",
                                    peer, e, text
                                ),
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => break Ok(()),
                        Some(Ok(_)) => continue,
                        Some(Err(e)) => break Err(e.into()),
                    }
                }
            }
        };

        {
            let mut client_guard = self.inner.client.lock().await;
            if client_guard.as_ref().is_some_and(|client| client.id == connection_id) {
                *client_guard = None;
                self.inner.has_active_chat.store(false, Ordering::Relaxed);
                info!("browser extension disconnected (id={})", connection_id);
            }
        }
        result
    }

    async fn handle_client_message(&self, msg: ClientMessage, peer: SocketAddr) {
        match msg {
            ClientMessage::Register { client } => {
                info!("browser extension registered as '{}' from {}", client, peer);
            }
            ClientMessage::Pong => {
                debug!("pong from browser extension");
            }
            ClientMessage::Partial { id, text } => {
                if let Some(tx) = self.inner.partials.lock().await.get(&id) {
                    let _ = tx.send(text);
                }
            }
            ClientMessage::Response { id, result, error } => {
                self.inner.partials.lock().await.remove(&id);
                if let Some(tx) = self.inner.pending.lock().await.remove(&id) {
                    let payload = match (result, error) {
                        (Some(r), _) => Ok(r),
                        (None, Some(e)) => Err(BridgeError::ExtensionError(e.message)),
                        (None, None) => Err(BridgeError::InvalidResponse(
                            "response contained neither result nor error".to_string(),
                        )),
                    };
                    let _ = tx.send(payload);
                }
            }
            ClientMessage::Ping => {
                // Ping is handled inline in the connection loop to keep the WebSocket alive.
            }
        }
    }
}

fn is_transient(err: &BridgeError) -> bool {
    match err {
        BridgeError::Timeout(_) => true,
        BridgeError::ExtensionError(msg) => {
            msg.contains("not detected")
                || msg.contains("No assistant message")
                || msg.contains("No response received")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::{Bridge, Method};
    use crate::config::Config;
    use futures::{SinkExt, StreamExt};
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;
    use tracing::warn;

    fn test_config() -> Config {
        Config {
            ws_host: "127.0.0.1".to_string(),
            ws_port: 0,
            http_host: "127.0.0.1".to_string(),
            http_port: 0,
            default_timeout: 120,
            log_level: "warn".to_string(),
            system_prompt: None,
            max_retries: 0,
            retry_delay_ms: 100,
            sticky_chat: false,
        }
    }

    impl Bridge {
        /// Start the bridge on a random free port for tests.
        pub async fn start_test(&self) -> anyhow::Result<SocketAddr> {
            let addr = self.websocket_addr();
            let listener = TcpListener::bind(&addr).await?;
            let local_addr = listener.local_addr()?;
            let this = self.clone();
            tokio::spawn(async move {
                loop {
                    let (stream, peer) = match listener.accept().await {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("test listener accept error: {}", e);
                            break;
                        }
                    };
                    let this = this.clone();
                    tokio::spawn(async move {
                        if let Err(e) = this.handle_connection(stream, peer).await {
                            warn!("test connection from {} closed: {}", peer, e);
                        }
                    });
                }
            });
            Ok(local_addr)
        }
    }

    #[tokio::test]
    async fn request_roundtrip() {
        let bridge = Bridge::new(test_config());
        let addr = bridge.start_test().await.unwrap();
        let url = format!("ws://{}", addr);

        let (mut ws, _) = connect_async(&url).await.unwrap();
        ws.send(Message::Text(r#"{"type":"register","client":"test"}"#.into()))
            .await
            .unwrap();

        let bridge2 = bridge.clone();
        let server = tokio::spawn(async move {
            bridge2
                .request(Method::SendMessage, "hello".into(), true, 5, None, None)
                .await
        });

        let msg = ws.next().await.unwrap().unwrap();
        let text = msg.to_text().unwrap();
        let req: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(req["type"], "request");
        assert_eq!(req["method"], "send_message");
        let id = req["id"].as_str().unwrap();

        let response = serde_json::json!({
            "type": "response",
            "id": id,
            "result": { "text": "world" }
        });
        ws.send(Message::Text(response.to_string().into()))
            .await
            .unwrap();

        let result = server.await.unwrap().unwrap();
        assert_eq!(result, "world");
    }

    #[tokio::test]
    async fn streaming_roundtrip() {
        let bridge = Bridge::new(test_config());
        let addr = bridge.start_test().await.unwrap();
        let url = format!("ws://{}", addr);

        let (mut ws, _) = connect_async(&url).await.unwrap();
        ws.send(Message::Text(r#"{"type":"register","client":"test"}"#.into()))
            .await
            .unwrap();

        let bridge2 = bridge.clone();
        let server = tokio::spawn(async move {
            bridge2
                .request_streaming(
                    Method::SendMessage,
                    "hello".into(),
                    true,
                    5,
                    None,
                    None,
                )
                .await
        });

        let msg = ws.next().await.unwrap().unwrap();
        let text = msg.to_text().unwrap();
        let req: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(req["type"], "request");
        let id = req["id"].as_str().unwrap();

        // Send partial updates
        let partial1 = serde_json::json!({ "type": "partial", "id": id, "text": "hello" });
        ws.send(Message::Text(partial1.to_string().into()))
            .await
            .unwrap();

        let partial2 = serde_json::json!({ "type": "partial", "id": id, "text": "hello world" });
        ws.send(Message::Text(partial2.to_string().into()))
            .await
            .unwrap();

        // Send final response
        let response = serde_json::json!({
            "type": "response",
            "id": id,
            "result": { "text": "hello world!" }
        });
        ws.send(Message::Text(response.to_string().into()))
            .await
            .unwrap();

        let mut handle = server.await.unwrap().unwrap();

        // Collect partials
        let mut partials = Vec::new();
        while let Ok(p) = handle.partials.recv().await {
            partials.push(p);
        }

        // Wait for final result
        let result = handle.result.await.unwrap().unwrap();
        assert_eq!(result.text, "hello world!");
        assert!(!partials.is_empty());
    }
}
