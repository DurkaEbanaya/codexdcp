use anyhow::Result;
use codexdcp::{
    bridge::Bridge,
    cdp::{cleanup_stale_processes, unblock_sigterm},
    config::Config,
    http_server::{self, AppState},
    mcp_server::ChatGptServer,
    workspace::Workspace,
};
use clap::Parser;
use rmcp::{ServiceExt, transport::stdio};
use tokio::signal::unix::{signal, SignalKind};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::parse();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(&config.log_level))
        .with_writer(std::io::stderr)
        .init();

    // Unblock SIGTERM — OpenCode spawns MCP servers with SIGTERM blocked in
    // the signal mask (inherited via fork/exec). Without this, tokio's signal
    // handler registers but SIGTERM is never delivered, so the process can't
    // be killed with SIGTERM and becomes a zombie.
    unblock_sigterm();

    // Kill stale codexdcp instances and orphaned Chrome from previous runs.
    // OpenCode respawns MCP servers without killing old ones, causing process leaks.
    let chrome_config = config.chrome_config();
    cleanup_stale_processes(&chrome_config).await;

    let selectors = config.selectors();
    let bridge = Bridge::new(selectors, config.max_retries, config.retry_delay_ms);

    // Launch Chrome and connect via CDP
    let bridge_runner = bridge.clone();
    tokio::spawn(async move {
        if let Err(e) = bridge_runner.start(&chrome_config).await {
            warn!("bridge failed to start: {}", e);
        }
    });

    // Start HTTP provider if port > 0
    if config.http_port > 0 {
        let state = AppState {
            bridge: bridge.clone(),
            default_timeout: config.default_timeout,
        };
        let http_addr = config.http_addr();
        tokio::spawn(async move {
            if let Err(e) = http_server::start_http_server(state, &http_addr).await {
                warn!("HTTP provider stopped: {}", e);
            }
        });
    }

    let workspace = Workspace::new(config.workspace_root());
    let bash_mode = config.bash_mode();
    let writes_enabled = config.writes_enabled();
    let tool_mode = config.tool_mode();

    info!(
        "workspace: {} | bash: {:?} | writes: {} | tool-mode: {:?}",
        workspace.root().display(),
        bash_mode,
        writes_enabled,
        tool_mode,
    );

    let server = ChatGptServer::new(
        bridge.clone(),
        config.default_timeout,
        config.system_prompt,
        workspace,
        bash_mode,
        writes_enabled,
        tool_mode,
    );

    if config.http_only {
        info!("HTTP-only mode; skipping MCP stdio server");
        let mut sigterm = signal(SignalKind::terminate())?;
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("received Ctrl+C, shutting down");
            }
            _ = sigterm.recv() => {
                info!("received SIGTERM, shutting down");
            }
        }
        bridge.shutdown().await;
        info!("shutdown complete");
        std::process::exit(0);
    }

    let service = server.serve(stdio()).await?;

    info!("CodexDCP started; waiting for OpenCode on stdio");

    // Graceful shutdown: wait for MCP service, Ctrl+C, or SIGTERM
    let mut sigterm = signal(SignalKind::terminate())?;
    tokio::select! {
        result = service.waiting() => {
            result?;
        }
        _ = tokio::signal::ctrl_c() => {
            info!("received Ctrl+C, shutting down");
        }
        _ = sigterm.recv() => {
            info!("received SIGTERM, shutting down");
        }
    }

    // Kill Chrome before exiting to prevent orphaned processes.
    // Use std::process::exit to guarantee exit — tokio runtime cleanup
    // can hang if there are pending tasks (e.g. spawned bridge tasks).
    bridge.shutdown().await;
    info!("shutdown complete");
    std::process::exit(0);
}
