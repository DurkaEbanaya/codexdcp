# Project: CodexDCP — Codex Developer Chaos Platform

MCP connector that turns browser ChatGPT into a Codex-like agent for OpenCode. Rust MCP server over stdio + Chrome DevTools Protocol (CDP) to drive headless Chrome that interacts with the ChatGPT web UI. All requests use temporary chat (no history saved). Includes OpenAI-compatible HTTP provider.

Tech stack: Rust (2024 edition, tokio, rmcp, axum, tokio-tungstenite), CDP, SSE streaming.

## Workspace Overview

* `src/` — Rust MCP server, CDP bridge, HTTP provider, JS injection strings.

## Where To Look

* `src/mcp_server.rs` — MCP tool definitions (`chatgpt_coder`, `chatgpt_ask`).
* `src/cdp.rs` — Chrome process launcher, CDP client (WebSocket + `Runtime.evaluate`), HTTP endpoint for `/json/list`.
* `src/bridge.rs` — CDP bridge: temp chat auto-enable, retries, streaming, page load polling.
* `src/js.rs` — JS function strings injected via `Runtime.evaluate` (`pageSendPrompt`, `pageReadAndCheck`, `pageSetTempChat`, model selection, `htmlToMarkdown`). `DEFAULT_SELECTORS` constant.
* `src/http_server.rs` — OpenAI-compatible HTTP provider (`/v1/chat/completions`, `/v1/models`, `/health`).
* `src/prompt.rs` — prompt builder for coding tasks + conversation prompt for HTTP provider.
* `src/config.rs` — CLI flags and env vars (chrome-path, chrome-profile, headless, cdp-port, http-port, system-prompt, etc).

## Architectural Invariants

* The MCP server writes logs only to **stderr**; stdout is reserved for MCP JSON-RPC.
* Chrome is launched as a **child process** with `--headless=new` and `--disable-blink-features=AutomationControlled` to bypass Cloudflare.
* Temporary chat is **always enabled** before sending any prompt — `ensure_temp_chat_on()` in bridge.rs.
* No browser extension — Rust controls Chrome directly via CDP WebSocket.
* DOM selectors live in `DEFAULT_SELECTORS` constant in `src/js.rs`, injectable from external file via `--selectors-path`.
* Chrome profile at `~/.codexdcp/chrome-profile/` stores cookies for persistent login.

## Key Subsystems

### MCP server

* What it is: `rmcp`-based stdio server exposing ChatGPT as tools.
* Where it lives: `src/mcp_server.rs`, `src/main.rs`.
* When changing it: keep tool return types as `String` or `Result<String, String>` so `rmcp` can build `CallToolResult`. Recompile and check with `cargo build`. Only two tools: `chatgpt_coder` and `chatgpt_ask`.

### CDP bridge

* What it is: launches headless Chrome, connects via CDP, injects JS, sends prompts, polls for responses.
* Where it lives: `src/bridge.rs`, `src/cdp.rs`.
* When changing it: do not block the async loop; use `tokio::select!`. `BridgeError` must remain cloneable. Always call `ensure_temp_chat_on()` before sending prompts.

### HTTP provider

* What it is: OpenAI-compatible API server (axum) on configurable port.
* Where it lives: `src/http_server.rs`.
* When changing it: supports both streaming (SSE) and non-streaming modes. Uses `request_streaming` from bridge for SSE.

## Development Practices

* Build: `cargo build --release`
* Run: `cargo run -- --help`
* Check: `cargo check`
* Lint: `cargo clippy --tests -- -D warnings`
* Verify MCP: register the binary in OpenCode and use `chatgpt_ask` with a simple question.
* Debug with visible Chrome: `cargo run -- --visible --log-level debug`

## Where To Find Details

* `README.md` — full setup, usage, and troubleshooting guide.
