use crate::bridge::{Bridge, Method};
use crate::prompt::{self, CoderInput};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const fn default_true() -> bool {
    true
}

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
    #[schemars(description = "Start a new ChatGPT chat before sending the task. In sticky-chat mode this is managed automatically.")]
    #[serde(default = "default_true")]
    pub new_chat: bool,
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
    #[schemars(description = "Start a new chat before sending the prompt. In sticky-chat mode this is managed automatically.")]
    #[serde(default)]
    pub new_chat: bool,
    #[schemars(description = "Optional ChatGPT model name (e.g. \"GPT-4o\", \"o1\", \"o1-mini\")")]
    #[serde(default)]
    pub model: Option<String>,
    #[schemars(description = "Response format: \"markdown\" (default) or \"text\"")]
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct NewChatRequest {}

#[derive(Clone)]
pub struct ChatGptServer {
    bridge: Bridge,
    default_timeout: u64,
    system_prompt: Option<String>,
    sticky_chat: bool,
    sticky_needs_new: Arc<AtomicBool>,
}

impl ChatGptServer {
    pub fn new(
        bridge: Bridge,
        default_timeout: u64,
        system_prompt: Option<String>,
        sticky_chat: bool,
    ) -> Self {
        Self {
            bridge,
            default_timeout,
            system_prompt,
            sticky_chat,
            sticky_needs_new: Arc::new(AtomicBool::new(true)),
        }
    }

    fn effective_new_chat(&self, requested: bool) -> bool {
        if self.sticky_chat {
            self.sticky_needs_new.swap(false, Ordering::Relaxed)
        } else {
            requested
        }
    }
}

#[tool_router(server_handler)]
impl ChatGptServer {
    #[tool(description = "Delegate a coding task to browser ChatGPT (Codex-style). \
                         Use when the user says: навайбкодь, напиши код, закодь, code, implement, refactor.")]
    async fn chatgpt_coder(
        &self,
        Parameters(req): Parameters<CoderRequest>,
    ) -> Result<String, String> {
        let new_chat = self.effective_new_chat(req.new_chat);
        let prompt = prompt::coder_prompt(CoderInput {
            task: &req.task,
            context: req.context.as_deref(),
            language: req.language.as_deref(),
            system_prompt: self.system_prompt.as_deref(),
        });
        self.bridge
            .request(
                Method::SendMessage,
                prompt,
                new_chat,
                self.default_timeout,
                req.model,
                req.format,
            )
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Ask a general question to browser ChatGPT. \
                         Use when the user says: спроси, ask, дальше, продолжить, explain, что такое.")]
    async fn chatgpt_ask(
        &self,
        Parameters(req): Parameters<AskRequest>,
    ) -> Result<String, String> {
        let new_chat = self.effective_new_chat(req.new_chat);
        self.bridge
            .request(
                Method::SendMessage,
                req.prompt,
                new_chat,
                self.default_timeout,
                req.model,
                req.format,
            )
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Start a new ChatGPT chat in the browser. \
                         Use when the user says: новый чат, new chat, сбрось, начни заново.")]
    async fn chatgpt_new_chat(
        &self,
        Parameters(_): Parameters<NewChatRequest>,
    ) -> Result<String, String> {
        self.sticky_needs_new.store(true, Ordering::Relaxed);
        self.bridge
            .request(
                Method::NewChat,
                String::new(),
                true,
                30,
                None,
                None,
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

    #[tool(description = "Check whether the ChatGPT browser extension is connected and report conversation status.")]
    async fn chatgpt_status(&self) -> String {
        let connected = self.bridge.is_connected().await;
        let has_chat = self.bridge.has_active_chat();
        let sticky = if self.sticky_chat {
            let needs_new = self.sticky_needs_new.load(Ordering::Relaxed);
            if needs_new {
                " [sticky: waiting for new chat]"
            } else {
                " [sticky: active conversation]"
            }
        } else {
            ""
        };
        match (connected, has_chat) {
            (true, true) => format!(
                "ChatGPT browser extension is connected. Active conversation in progress.{}",
                sticky
            ),
            (true, false) => format!(
                "ChatGPT browser extension is connected. No active conversation.{}",
                sticky
            ),
            (false, _) => "ChatGPT browser extension is NOT connected.".to_string(),
        }
    }
}
