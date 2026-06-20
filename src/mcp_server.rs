use crate::bridge::Bridge;
use crate::prompt::{self, CoderInput};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct CoderRequest {
    #[schemars(description = "Coding task to delegate to ChatGPT. Trigger keywords: навайбкодь, напиши код, code")]
    pub task: String,
    #[schemars(description = "Optional context such as code snippets, error messages, or constraints")]
    #[serde(default)]
    pub context: Option<String>,
    #[schemars(description = "Optional programming language")]
    #[serde(default)]
    pub language: Option<String>,
    #[schemars(description = "Optional ChatGPT model name (e.g. \"GPT-4o\", \"o1\", \"o1-mini\")")]
    #[serde(default)]
    pub model: Option<String>,
    #[schemars(description = "Response format: \"markdown\" (default) or \"text\"")]
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct AskRequest {
    #[schemars(description = "Prompt to send to ChatGPT. Trigger keywords: спроси, ask, дальше, продолжить")]
    pub prompt: String,
    #[schemars(description = "Optional ChatGPT model name (e.g. \"GPT-4o\", \"o1\", \"o1-mini\")")]
    #[serde(default)]
    pub model: Option<String>,
    #[schemars(description = "Response format: \"markdown\" (default) or \"text\"")]
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Clone)]
pub struct ChatGptServer {
    bridge: Bridge,
    default_timeout: u64,
    system_prompt: Option<String>,
}

impl ChatGptServer {
    pub fn new(
        bridge: Bridge,
        default_timeout: u64,
        system_prompt: Option<String>,
    ) -> Self {
        Self {
            bridge,
            default_timeout,
            system_prompt,
        }
    }
}

#[tool_router(server_handler)]
impl ChatGptServer {
    #[tool(description = "Delegate a coding task to ChatGPT via headless Chrome (Codex-style). \
                         Always uses temporary chat (no history saved). \
                         Use when the user says: навайбкодь, напиши код, закодь, code, implement, refactor.")]
    async fn chatgpt_coder(
        &self,
        Parameters(req): Parameters<CoderRequest>,
    ) -> Result<String, String> {
        let prompt = prompt::coder_prompt(CoderInput {
            task: &req.task,
            context: req.context.as_deref(),
            language: req.language.as_deref(),
            system_prompt: self.system_prompt.as_deref(),
        });
        self.bridge
            .request(
                prompt,
                self.default_timeout,
                req.model,
                req.format,
            )
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Ask a general question to ChatGPT via headless Chrome. \
                         Always uses temporary chat (no history saved). \
                         Use when the user says: спроси, ask, дальше, продолжить, explain, что такое.")]
    async fn chatgpt_ask(
        &self,
        Parameters(req): Parameters<AskRequest>,
    ) -> Result<String, String> {
        self.bridge
            .request(
                req.prompt,
                self.default_timeout,
                req.model,
                req.format,
            )
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Enable temporary chat in ChatGPT (responses won't be saved to history). \
                         Use when the user says: включить временный чат, temp chat on, temporary chat on.")]
    async fn chatgpt_temp_chat_on(&self) -> Result<String, String> {
        self.bridge
            .request_set_temp_chat(true)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Disable temporary chat in ChatGPT (responses will be saved to history normally). \
                         Use when the user says: выключить временный чат, temp chat off, temporary chat off.")]
    async fn chatgpt_temp_chat_off(&self) -> Result<String, String> {
        self.bridge
            .request_set_temp_chat(false)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Check whether Chrome is running and ChatGPT is connected.")]
    async fn chatgpt_status(&self) -> String {
        let connected = self.bridge.is_connected().await;
        match connected {
            true => "ChatGPT is connected (headless Chrome, temporary chat mode).".to_string(),
            false => "ChatGPT is NOT connected (Chrome not running).".to_string(),
        }
    }
}
