use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum BridgeError {
    #[error("no browser extension connected to the bridge")]
    NotConnected,
    #[error("request timed out after {0}s")]
    Timeout(u64),
    #[error("browser extension reported an error: {0}")]
    ExtensionError(String),
    #[error("failed to send request to the extension: {0}")]
    SendError(String),
    #[error("invalid response from the extension: {0}")]
    InvalidResponse(String),
    #[error("CDP protocol error: {0}")]
    CdpError(String),
    #[error("Chrome process error: {0}")]
    ChromeError(String),
    #[error("JavaScript evaluation error: {0}")]
    JsError(String),
    #[error("selectors file error: {0}")]
    SelectorsError(String),
}
