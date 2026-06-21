# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] - 2026-06-21

### Added
- Hybrid mode: CodexDCP is now a full agent platform, not just a ChatGPT bridge
- Filesystem MCP tools: `read_file`, `write_file`, `edit_file`, `tree`
- Search MCP tool: `search_files` (uses ripgrep with grep fallback)
- Bash MCP tool: `bash` with safe allowlist (blocks rm -rf, git push, curl, sudo)
- Git MCP tools: `git_status`, `git_diff`, `show_changes`
- Skill discovery MCP tools: `load_skill`, `list_skills` (workspace + user dirs)
- Handoff MCP tools: `read_handoff`, `handoff_to_agent` (.ai-bridge/ plan system)
- Context MCP tools: `codex_context` (AGENTS.md chain + git state), `export_pro_context`
- Tiered tool surface: `--tool-mode minimal|standard|full` controls available tools
- Workspace safety: path containment check, blocked globs (.git, .env, *.pem, *.key)
- `--root` flag: workspace root directory (default: current directory)
- `--bash-mode` flag: `safe|off|full` shell execution policy
- `--write-mode` flag: `workspace|off` write access control
- `--http-only` flag: run HTTP provider without MCP stdio server
- Temp chat via URL parameter (`?temporary-chat=true`) — ChatGPT removed the UI toggle
- `pageIsTempChat()` JS function: checks `data-testid="temporary-chat-label"`
- New dependencies: `walkdir`, `globset`

### Changed
- MCP tool count: 2 → 16
- `src/mcp_server.rs`: full rewrite with all workspace tools
- `src/main.rs`: wires workspace, bash mode, write mode, tool mode into server
- `src/bridge.rs`: `ensure_temp_chat_on()` navigates to `?temporary-chat=true` instead of clicking toggle
- `src/js.rs`: `pageSetTempChat` simplified to URL-based, added `pageIsTempChat`
- `src/config.rs`: new fields `root`, `tool_mode`, `bash_mode`, `write_mode`

### Inspired by
- [rebel0789/codexpro](https://github.com/rebel0789/codexpro) — ported filesystem/git/bash/handoff/skill concepts into Rust

## [0.4.0] - 2025-06-20

### Added
- CDP architecture: Rust controls headless Chrome directly via Chrome DevTools Protocol
- Headless Chrome launched as child process with `--headless=new`
- Cookie reuse: copy Brave/Chrome profile cookies to `~/.codexdcp/chrome-profile/`
- Anti-detection: `--disable-blink-features=AutomationControlled` bypasses Cloudflare
- Temporary chat is always enabled automatically before every prompt
- New CLI flags: `--chrome-path`, `--chrome-profile`, `--headless`, `--cdp-port`, `--visible`, `--selectors-path`
- Page load polling: waits for ChatGPT SPA to finish loading (Cloudflare "Один момент…" check)
- Singleton lock cleanup before Chrome launch

### Removed
- Browser extension entirely — no more `browser_extension/` directory
- WebSocket bridge server — replaced by CDP WebSocket client
- IPC master/secondary mode — no longer needed (single process controls Chrome)
- Sticky-chat mode (`--sticky-chat` flag) — temporary chat replaces it
- `chatgpt_new_chat` MCP tool — not needed with temp chat
- `chatgpt_temp_chat_on` / `chatgpt_temp_chat_off` MCP tools — temp chat is always on
- `chatgpt_status` MCP tool — not needed
- `new_chat` parameter from `chatgpt_coder` and `chatgpt_ask` tools
- `Method` enum from bridge — simplified to direct `request()` call
- `has_active_chat` tracking — irrelevant in temp chat mode
- `--ws-host` and `--ws-port` CLI flags
- Error messages mentioning "browser extension" — updated to "Chrome/CDP"

### Changed
- `src/cdp.rs` — new file: Chrome process launcher, CDP client (WebSocket + `Runtime.evaluate`)
- `src/js.rs` — new file: JS function strings with embedded `DEFAULT_SELECTORS`
- `src/bridge.rs` — completely rewritten: CDP instead of WebSocket server
- `src/mcp_server.rs` — simplified: no sticky-chat, no new_chat, temp chat always on
- `src/config.rs` — removed ws-host/ws-port/sticky-chat, added chrome-*/cdp-*/headless/visible
- `src/main.rs` — updated startup: launches Chrome via bridge instead of WebSocket server
- README.md — completely rewritten for CDP architecture
- AGENTS.md — rewritten for CDP architecture

## [0.3.0] - 2025-06-20

### Added
- Hidden tab auto-creation: if no ChatGPT tab is open, the extension creates a background tab
- Temporary chat toggle: `chatgpt_temp_chat_on` / `chatgpt_temp_chat_off` MCP tools
- IPC shared bridge: multiple OpenCode sessions share one WebSocket connection via Unix socket
- Master/secondary mode: first process = master, subsequent = secondary (IPC client)

## [0.2.0] - 2025-06-20

### Added
- HTTP provider: OpenAI-compatible API with SSE streaming
- Model selection via DOM dropdown
- Sticky-chat mode (`--sticky-chat`)
- Markdown preservation: extension converts ChatGPT DOM HTML to markdown
- Streaming: partial responses via `Partial` protocol message
- Retries with exponential backoff
- Configurable DOM selectors via `browser_extension/selectors.json`
- Custom system prompt via CLI flag
- Graceful shutdown via Ctrl+C
- GitHub Actions CI and release workflows

### Changed
- Renamed project from `chatgpt-mcp-bridge` to `codexdcp`

## [0.1.0] - 2025-06-20

### Added
- Initial release: MCP server with `chatgpt_coder`, `chatgpt_ask`, `chatgpt_new_chat`, `chatgpt_status` tools
- WebSocket bridge on `ws://127.0.0.1:8765`
- Chrome extension (Manifest V3) with service worker and content script
- 3-step fallback: send+wait → retry read → reload page + read
