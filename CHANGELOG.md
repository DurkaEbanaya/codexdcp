# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2025-06-20

### Added
- Hidden tab auto-creation: if no ChatGPT tab is open, the extension creates a background tab (`active: false`) so the user is not distracted
- Temporary chat toggle: `chatgpt_temp_chat_on` / `chatgpt_temp_chat_off` MCP tools
- `SetTempChat` bridge method with `temp_chat_enabled` parameter
- `pageSetTempChat` injected function: searches by selector, text content, and opens menus/dropdowns to find the temp chat toggle
- New selectors in `selectors.json`: `tempChatButton`, `tempChatText`
- IPC shared bridge: multiple OpenCode sessions share one WebSocket connection via Unix socket (`/tmp/codexdcp.sock`)
- Master/secondary mode: first process = master (WebSocket + IPC server), subsequent processes = secondary (IPC client)
- Slash commands: `/временный-чат-вкл`, `/временный-чат-выкл`

### Changed
- `findChatGPTTab` renamed to `findOrCreateTab` — creates hidden tab if none exists
- Version bumped to 0.3.0 in Cargo.toml and manifest.json

## [0.2.0] - 2025-06-20

### Added
- HTTP provider: OpenAI-compatible API (`/v1/chat/completions`, `/v1/models`, `/health`) with SSE streaming
- Model selection: pass `model` parameter to select ChatGPT model via DOM dropdown
- Sticky-chat mode (`--sticky-chat`): all messages go to one conversation automatically
- Markdown preservation: extension converts ChatGPT DOM HTML to markdown (code blocks, headers, links, tables)
- Streaming: partial responses via `Partial` protocol message and `request_streaming` in bridge
- Retries with exponential backoff for transient errors (`--max-retries`, `--retry-delay-ms`)
- Configurable DOM selectors via `browser_extension/selectors.json`
- Custom system prompt via `--system-prompt` CLI flag or `CHATGPT_BRIDGE_SYSTEM_PROMPT` env var
- Graceful shutdown via Ctrl+C / SIGTERM
- `conversation_prompt` for HTTP provider (interleaves system/user/assistant messages)
- `format` parameter on MCP tools (`"markdown"` or `"text"`)
- `has_active_chat` tracking in bridge and `chatgpt_status`
- GitHub Actions CI workflow (test + clippy)
- GitHub Actions release workflow (cross-platform binary builds)
- LICENSE, CONTRIBUTING.md, CHANGELOG.md

### Changed
- Renamed project from `chatgpt-mcp-bridge` to `codexdcp` (CodexDCP — Codex Developer Chaos Platform)
- `ChatGptServer::new` now takes `default_timeout` and `system_prompt` from config
- Bridge `request` method signature includes `model` and `format` parameters
- Extension `background.js` completely rewritten: loads selectors from JSON, split send+poll, htmlToMarkdown converter, model dropdown selection
- `manifest.json` version bumped to 0.2.0, added `web_accessible_resources` for `selectors.json`
- README completely rewritten with full setup guide and API examples

### Fixed
- `default_timeout` from config is now actually used (was hardcoded to 120)
- Removed dead `ask_prompt()` function

## [0.1.0] - 2025-06-20

### Added
- Initial release: MCP server with `chatgpt_coder`, `chatgpt_ask`, `chatgpt_new_chat`, `chatgpt_status` tools
- WebSocket bridge on `ws://127.0.0.1:8765`
- Chrome extension (Manifest V3) with `background.js` service worker and `content.js`
- 3-step fallback: send+wait → retry read → reload page + read
- `chrome.scripting.executeScript` with `world: 'MAIN'` for reliable DOM access
- `chrome.alarms` for MV3-compliant reconnection
- Configurable via CLI flags and environment variables
