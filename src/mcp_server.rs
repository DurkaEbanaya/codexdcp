use crate::bridge::Bridge;
use crate::bash_ops::{self, BashMode};
use crate::config::ToolMode;
use crate::fs_ops;
use crate::git_ops;
use crate::handoff;
use crate::prompt::{self, CoderInput};
use crate::skill;
use crate::workspace::Workspace;
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use std::sync::Arc;

#[derive(Debug, Clone, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct EmptyRequest {}

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

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadFileRequest {
    #[schemars(description = "File path relative to workspace root")]
    pub path: String,
    #[schemars(description = "Line number to start reading from (1-indexed, default 1)")]
    #[serde(default)]
    pub offset: Option<usize>,
    #[schemars(description = "Maximum number of lines to read (default 2000)")]
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct WriteFileRequest {
    #[schemars(description = "File path relative to workspace root")]
    pub path: String,
    #[schemars(description = "Full file content to write")]
    pub content: String,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct EditFileRequest {
    #[schemars(description = "File path relative to workspace root")]
    pub path: String,
    #[schemars(description = "Exact string to find in the file")]
    pub old_string: String,
    #[schemars(description = "String to replace it with")]
    pub new_string: String,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchRequest {
    #[schemars(description = "Regex pattern to search for")]
    pub pattern: String,
    #[schemars(description = "Directory to search in (default: workspace root)")]
    #[serde(default)]
    pub path: Option<String>,
    #[schemars(description = "File glob to include (e.g. \"*.rs\", \"*.ts\")")]
    #[serde(default)]
    pub include: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct TreeRequest {
    #[schemars(description = "Directory path (default: workspace root)")]
    #[serde(default)]
    pub path: Option<String>,
    #[schemars(description = "Maximum depth to traverse (default 10)")]
    #[serde(default)]
    pub max_depth: Option<usize>,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct BashRequest {
    #[schemars(description = "Shell command to execute")]
    pub command: String,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct GitDiffRequest {
    #[schemars(description = "Show staged changes (true) or unstaged (false, default)")]
    #[serde(default)]
    pub staged: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct SkillRequest {
    #[schemars(description = "Skill name to load")]
    pub name: String,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct HandoffRequest {
    #[schemars(description = "Plan content to write to .ai-bridge/current-plan.md")]
    pub plan: String,
    #[schemars(description = "Target agent: codex, opencode, pi, custom (default: codex)")]
    #[serde(default)]
    pub agent: Option<String>,
    #[schemars(description = "Model to use for the agent")]
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Clone)]
pub struct ChatGptServer {
    bridge: Bridge,
    default_timeout: u64,
    system_prompt: Option<String>,
    workspace: Arc<Workspace>,
    bash_mode: BashMode,
    writes_enabled: bool,
    tool_mode: ToolMode,
}

impl ChatGptServer {
    pub fn new(
        bridge: Bridge,
        default_timeout: u64,
        system_prompt: Option<String>,
        workspace: Workspace,
        bash_mode: BashMode,
        writes_enabled: bool,
        tool_mode: ToolMode,
    ) -> Self {
        Self {
            bridge,
            default_timeout,
            system_prompt,
            workspace: Arc::new(workspace),
            bash_mode,
            writes_enabled,
            tool_mode,
        }
    }

    fn check_fs(&self) -> Result<(), String> {
        if !self.tool_mode.has_fs_tools() {
            return Err("filesystem tools not available in current tool mode".to_string());
        }
        Ok(())
    }

    fn check_writes(&self) -> Result<(), String> {
        self.check_fs()?;
        if !self.writes_enabled {
            return Err("writes are disabled (--write-mode off)".to_string());
        }
        Ok(())
    }

    fn check_git(&self) -> Result<(), String> {
        if !self.tool_mode.has_git_tools() {
            return Err("git tools not available in current tool mode".to_string());
        }
        Ok(())
    }

    fn check_search(&self) -> Result<(), String> {
        if !self.tool_mode.has_search_tools() {
            return Err("search tools not available in current tool mode".to_string());
        }
        Ok(())
    }

    fn check_handoff(&self) -> Result<(), String> {
        if !self.tool_mode.has_handoff_tools() {
            return Err("handoff tools not available in current tool mode".to_string());
        }
        Ok(())
    }

    fn check_skill(&self) -> Result<(), String> {
        if !self.tool_mode.has_skill_tools() {
            return Err("skill tools not available in current tool mode".to_string());
        }
        Ok(())
    }

    fn check_context(&self) -> Result<(), String> {
        if !self.tool_mode.has_context_tools() {
            return Err("context tools not available in current tool mode".to_string());
        }
        Ok(())
    }
}

#[tool_router(server_handler)]
impl ChatGptServer {
    // === ChatGPT bridge tools (always available) ===

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

    // === Filesystem tools ===

    #[tool(description = "Read a text file from the workspace with line numbers. \
                          Use offset/limit for large files. Paths are relative to workspace root.")]
    async fn read_file(
        &self,
        Parameters(req): Parameters<ReadFileRequest>,
    ) -> Result<String, String> {
        self.check_fs()?;
        fs_ops::read_file(&self.workspace, &req.path, req.offset, req.limit)
    }

    #[tool(description = "Create or overwrite a file in the workspace. \
                          Returns a summary of changes. Write mode must be enabled.")]
    async fn write_file(
        &self,
        Parameters(req): Parameters<WriteFileRequest>,
    ) -> Result<String, String> {
        self.check_writes()?;
        fs_ops::write_file(&self.workspace, &req.path, &req.content)
    }

    #[tool(description = "Edit a file by replacing an exact string match. \
                          The old_string must be unique in the file. Returns a summary.")]
    async fn edit_file(
        &self,
        Parameters(req): Parameters<EditFileRequest>,
    ) -> Result<String, String> {
        self.check_writes()?;
        fs_ops::edit_file(&self.workspace, &req.path, &req.old_string, &req.new_string)
    }

    #[tool(description = "Search for a regex pattern across files in the workspace. \
                          Uses ripgrep if available, falls back to grep. \
                          Optionally filter by path and file glob pattern.")]
    async fn search_files(
        &self,
        Parameters(req): Parameters<SearchRequest>,
    ) -> Result<String, String> {
        self.check_search()?;
        fs_ops::search_files(
            &self.workspace,
            &req.pattern,
            req.path.as_deref(),
            req.include.as_deref(),
        )
    }

    #[tool(description = "Show a directory tree of the workspace or a subdirectory. \
                          Excludes .git, node_modules, target, etc.")]
    async fn tree(
        &self,
        Parameters(req): Parameters<TreeRequest>,
    ) -> Result<String, String> {
        self.check_fs()?;
        fs_ops::tree(&self.workspace, req.path.as_deref(), req.max_depth)
    }

    // === Bash tool ===

    #[tool(description = "Execute a shell command in the workspace root. \
                          In safe mode (default), only allowlisted commands run (cargo, npm, python, git inspect, etc.). \
                          Destructive commands (rm -rf, git push, curl, sudo) are blocked. \
                          Use --bash-mode full to allow any command.")]
    async fn bash(
        &self,
        Parameters(req): Parameters<BashRequest>,
    ) -> Result<String, String> {
        self.check_fs()?;
        bash_ops::run_bash(&self.workspace, &req.command, &self.bash_mode)
    }

    // === Git tools ===

    #[tool(description = "Show git status (short format) for the workspace.")]
    async fn git_status(
        &self,
        Parameters(_): Parameters<EmptyRequest>,
    ) -> Result<String, String> {
        self.check_git()?;
        git_ops::git_status(&self.workspace)
    }

    #[tool(description = "Show git diff (--stat) for staged or unstaged changes.")]
    async fn git_diff(
        &self,
        Parameters(req): Parameters<GitDiffRequest>,
    ) -> Result<String, String> {
        self.check_git()?;
        git_ops::git_diff(&self.workspace, req.staged.unwrap_or(false))
    }

    #[tool(description = "Show a summary of all changes: staged, unstaged, and untracked files \
                          with diff stats.")]
    async fn show_changes(
        &self,
        Parameters(_): Parameters<EmptyRequest>,
    ) -> Result<String, String> {
        self.check_git()?;
        git_ops::show_changes(&self.workspace)
    }

    // === Skill tools ===

    #[tool(description = "Load a SKILL.md file by name from workspace or user skills directory. \
                          Returns the full skill content.")]
    async fn load_skill(
        &self,
        Parameters(req): Parameters<SkillRequest>,
    ) -> Result<String, String> {
        self.check_skill()?;
        skill::load_skill(&self.workspace, &req.name)
    }

    #[tool(description = "List all discovered skills in the workspace and user config. \
                          Returns names, descriptions, and source (workspace/user).")]
    async fn list_skills(
        &self,
        Parameters(_): Parameters<EmptyRequest>,
    ) -> Result<String, String> {
        self.check_skill()?;
        let skills = skill::list_skills(&self.workspace);
        if skills.is_empty() {
            Ok("No skills found. Place SKILL.md files in .opencode/skills/ or skills/.".to_string())
        } else {
            let lines: Vec<String> = skills.iter()
                .map(|s| format!("- {} [{}]: {}", s.name, s.source, s.description))
                .collect();
            Ok(lines.join("\n"))
        }
    }

    // === Handoff tools ===

    #[tool(description = "Read the .ai-bridge/ handoff directory contents: \
                          current-plan, agent-status, implementation-diff, decisions, open-questions, execution-log.")]
    async fn read_handoff(
        &self,
        Parameters(_): Parameters<EmptyRequest>,
    ) -> Result<String, String> {
        self.check_handoff()?;
        handoff::read_handoff(&self.workspace)
    }

    #[tool(description = "Write a plan to .ai-bridge/current-plan.md for a local agent to execute. \
                          Creates agent-status.md and execution-log.jsonl. \
                          Agent: codex (default), opencode, pi, custom.")]
    async fn handoff_to_agent(
        &self,
        Parameters(req): Parameters<HandoffRequest>,
    ) -> Result<String, String> {
        self.check_handoff()?;
        if !self.writes_enabled {
            return Err("writes are disabled (--write-mode off)".to_string());
        }
        handoff::handoff_to_agent(
            &self.workspace,
            &req.plan,
            req.agent.as_deref(),
            req.model.as_deref(),
        )
    }

    // === Context tools (full mode only) ===

    #[tool(description = "Get workspace context: AGENTS.md instruction chain + git status. \
                          Useful for providing context to ChatGPT or other agents.")]
    async fn codex_context(
        &self,
        Parameters(_): Parameters<EmptyRequest>,
    ) -> Result<String, String> {
        self.check_context()?;
        Ok(self.workspace.codex_context())
    }

    #[tool(description = "Export a markdown context bundle to .ai-bridge/pro-context.md: \
                          file tree, git status, diff, AGENTS.md. For models without tool-calling.")]
    async fn export_pro_context(
        &self,
        Parameters(_): Parameters<EmptyRequest>,
    ) -> Result<String, String> {
        self.check_context()?;
        if !self.writes_enabled {
            return Err("writes are disabled (--write-mode off)".to_string());
        }
        handoff::export_pro_context(&self.workspace)
    }
}
