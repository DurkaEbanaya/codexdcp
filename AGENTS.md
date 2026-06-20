# Project: CodexDCP — Codex Developer Chaos Platform

MCP connector that turns browser ChatGPT into a Codex-like agent for OpenCode. Rust MCP server over stdio + WebSocket bridge to a Chrome extension that drives the ChatGPT web UI. Includes OpenAI-compatible HTTP provider for use as a model backend.

Tech stack: Rust (2024 edition, tokio, rmcp, axum), Chrome extension (Manifest V3), WebSocket, SSE streaming.

## Workspace Overview

* `src/` — Rust MCP server, WebSocket bridge, HTTP provider.
* `browser_extension/` — Chrome/Edge extension (`background.js` + `content.js` + `selectors.json`).

## Where To Look

* `src/mcp_server.rs` — MCP tool definitions (`chatgpt_coder`, `chatgpt_ask`, `chatgpt_new_chat`, `chatgpt_status`).
* `src/bridge.rs` — WebSocket server, request/response routing, retries, streaming, sticky-chat state.
* `src/http_server.rs` — OpenAI-compatible HTTP provider (`/v1/chat/completions`, `/v1/models`, `/health`).
* `src/prompt.rs` — prompt builder for coding tasks + conversation prompt for HTTP provider.
* `src/config.rs` — CLI flags and env vars (ws-port, http-port, sticky-chat, system-prompt, max-retries, etc).
* `browser_extension/background.js` — service worker, executeScript, model selection, markdown conversion, 3-step fallback.
* `browser_extension/selectors.json` — configurable DOM selectors (no code changes needed when ChatGPT updates UI).
* `opencode.example.json` — snippet for registering the server in OpenCode.

## Architectural Invariants

* The MCP server writes logs only to **stderr**; stdout is reserved for MCP JSON-RPC.
* One WebSocket client at a time: a new extension connection replaces the previous one.
* The browser extension is the only source of truth for the ChatGPT DOM; the server must not assume a specific ChatGPT layout beyond what the extension handles.
* DOM selectors live in `selectors.json`, not in Rust code.

## Key Subsystems

### MCP server

* What it is: `rmcp`-based stdio server exposing ChatGPT as tools.
* Where it lives: `src/mcp_server.rs`, `src/main.rs`.
* When changing it: keep tool return types as `String` or `Result<String, String>` so `rmcp` can build `CallToolResult`. Recompile and check with `cargo build`.

### WebSocket bridge

* What it is: localhost WebSocket server waiting for the browser extension.
* Where it lives: `src/bridge.rs`.
* When changing it: do not block the async loop; use `tokio::select!`. Maintain `BridgeError` as a cloneable error type because it crosses oneshot boundaries.

### HTTP provider

* What it is: OpenAI-compatible API server (axum) on configurable port.
* Where it lives: `src/http_server.rs`.
* When changing it: supports both streaming (SSE) and non-streaming modes. Uses `request_streaming` from bridge for SSE.

### Browser extension

* What it is: drives `chatgpt.com` via DOM and talks to the Rust server.
* Where it lives: `browser_extension/`.
* When changing it: update DOM selectors in `selectors.json` first; only modify `background.js` if new logic is needed.

## Development Practices

* Build: `cargo build --release`
* Run: `cargo run -- --help`
* Check: `cargo check`
* Lint: `cargo clippy --tests -- -D warnings`
* Install extension: load `browser_extension/` as unpacked in Chrome/Edge and open `https://chatgpt.com`.
* Verify MCP: register the binary in OpenCode using `opencode.example.json` and run `chatgpt_status`.

## Where To Find Details

* `README.md` — full setup, usage, and troubleshooting guide.
* `opencode.example.json` — OpenCode MCP server configuration example.
