use clap::Parser;
use std::path::PathBuf;
use tracing::warn;

#[derive(Parser, Clone, Debug)]
#[command(name = "codexdcp", about = "CodexDCP — Codex Developer Chaos Platform: MCP bridge that turns browser ChatGPT into a Codex-like agent")]
pub struct Config {
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

    /// Path to Chrome/Brave/Chromium binary (auto-detected if not specified).
    #[arg(long, env = "CODEXDCP_CHROME_PATH")]
    pub chrome_path: Option<String>,

    /// Path to Chrome user-data directory for persistent login session.
    #[arg(long, env = "CODEXDCP_CHROME_PROFILE")]
    pub chrome_profile: Option<PathBuf>,

    /// Run Chrome in headless mode (default true).
    #[arg(long, env = "CODEXDCP_HEADLESS", default_value_t = true)]
    pub headless: bool,

    /// Chrome DevTools Protocol debugging port.
    #[arg(long, env = "CODEXDCP_CDP_PORT", default_value_t = 9222)]
    pub cdp_port: u16,

    /// Launch Chrome with a visible window (use for first-time ChatGPT login).
    #[arg(long)]
    pub visible: bool,

    /// Path to custom selectors.json file.
    #[arg(long)]
    pub selectors_path: Option<PathBuf>,

    /// Run HTTP provider only (skip MCP stdio server).
    #[arg(long)]
    pub http_only: bool,

    /// Workspace root directory for filesystem/git/bash tools (default: current directory).
    #[arg(long, env = "CODEXDCP_ROOT")]
    pub root: Option<PathBuf>,

    /// Tool surface mode: minimal (core tools only), standard (default), full (all tools).
    #[arg(long, env = "CODEXDCP_TOOL_MODE", default_value = "standard")]
    pub tool_mode: String,

    /// Bash execution mode: safe (allowlisted), off (disabled), full (any command).
    #[arg(long, env = "CODEXDCP_BASH_MODE", default_value = "safe")]
    pub bash_mode: String,

    /// Write mode: workspace (allow writes in workspace), off (read-only).
    #[arg(long, env = "CODEXDCP_WRITE_MODE", default_value = "workspace")]
    pub write_mode: String,
}

impl Config {
    pub fn http_addr(&self) -> String {
        format!("{}:{}", self.http_host, self.http_port)
    }

    pub fn chrome_config(&self) -> crate::cdp::ChromeConfig {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        crate::cdp::ChromeConfig {
            chrome_path: self.chrome_path.clone(),
            chrome_profile: self.chrome_profile.clone().unwrap_or_else(|| {
                PathBuf::from(home).join(".codexdcp/chrome-profile")
            }),
            headless: self.headless,
            cdp_port: self.cdp_port,
            visible: self.visible,
        }
    }

    pub fn selectors(&self) -> String {
        if let Some(ref path) = self.selectors_path {
            if let Ok(content) = std::fs::read_to_string(path) {
                return content;
            }
            warn!("selectors file not found at {}, using defaults", path.display());
        }
        crate::js::DEFAULT_SELECTORS.to_string()
    }

    pub fn workspace_root(&self) -> std::path::PathBuf {
        self.root.clone().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        })
    }

    pub fn bash_mode(&self) -> crate::bash_ops::BashMode {
        crate::bash_ops::BashMode::parse(&self.bash_mode)
            .unwrap_or(crate::bash_ops::BashMode::Safe)
    }

    pub fn writes_enabled(&self) -> bool {
        self.write_mode != "off"
    }

    pub fn tool_mode(&self) -> ToolMode {
        ToolMode::parse(&self.tool_mode).unwrap_or(ToolMode::Standard)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolMode {
    Minimal,
    Standard,
    Full,
}

impl ToolMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "minimal" => Some(Self::Minimal),
            "standard" => Some(Self::Standard),
            "full" => Some(Self::Full),
            _ => None,
        }
    }

    pub fn has_fs_tools(&self) -> bool {
        matches!(self, Self::Minimal | Self::Standard | Self::Full)
    }

    pub fn has_git_tools(&self) -> bool {
        matches!(self, Self::Standard | Self::Full)
    }

    pub fn has_search_tools(&self) -> bool {
        matches!(self, Self::Standard | Self::Full)
    }

    pub fn has_handoff_tools(&self) -> bool {
        matches!(self, Self::Standard | Self::Full)
    }

    pub fn has_skill_tools(&self) -> bool {
        matches!(self, Self::Standard | Self::Full)
    }

    pub fn has_context_tools(&self) -> bool {
        matches!(self, Self::Full)
    }
}
