use clap::Parser;

#[derive(Parser, Clone, Debug)]
#[command(name = "codexdcp", about = "CodexDCP — Codex Developer Chaos Platform: MCP bridge that turns browser ChatGPT into a Codex-like agent")]
pub struct Config {
    /// Host for the WebSocket bridge server.
    #[arg(long, env = "CHATGPT_BRIDGE_HOST", default_value = "127.0.0.1")]
    pub ws_host: String,

    /// Port for the WebSocket bridge server.
    #[arg(long, env = "CHATGPT_BRIDGE_PORT", default_value_t = 8765)]
    pub ws_port: u16,

    /// Host for the OpenAI-compatible HTTP provider server.
    #[arg(long, env = "CHATGPT_BRIDGE_HTTP_HOST", default_value = "127.0.0.1")]
    pub http_host: String,

    /// Port for the HTTP provider server (0 = disabled).
    #[arg(long, env = "CHATGPT_BRIDGE_HTTP_PORT", default_value_t = 0)]
    pub http_port: u16,

    /// Default timeout for ChatGPT responses in seconds.
    #[arg(long, env = "CHATGPT_BRIDGE_TIMEOUT", default_value_t = 120)]
    pub default_timeout: u64,

    /// Log level (e.g. error, warn, info, debug, trace).
    #[arg(long, env = "RUST_LOG", default_value = "info")]
    pub log_level: String,

    /// Custom system prompt for coding tasks (overrides built-in default).
    #[arg(long, env = "CHATGPT_BRIDGE_SYSTEM_PROMPT")]
    pub system_prompt: Option<String>,

    /// Maximum retry attempts for transient errors.
    #[arg(long, env = "CHATGPT_BRIDGE_MAX_RETRIES", default_value_t = 2)]
    pub max_retries: u32,

    /// Initial retry delay in milliseconds (doubles on each retry).
    #[arg(long, env = "CHATGPT_BRIDGE_RETRY_DELAY_MS", default_value_t = 2000)]
    pub retry_delay_ms: u64,

    /// Sticky chat mode: all messages go to one ChatGPT conversation.
    /// First message starts a new chat; subsequent messages continue it.
    /// Use chatgpt_new_chat to reset and start a fresh conversation.
    #[arg(long, env = "CHATGPT_BRIDGE_STICKY_CHAT", default_value_t = false)]
    pub sticky_chat: bool,
}

impl Config {
    pub fn websocket_addr(&self) -> String {
        format!("{}:{}", self.ws_host, self.ws_port)
    }

    pub fn http_addr(&self) -> String {
        format!("{}:{}", self.http_host, self.http_port)
    }
}
