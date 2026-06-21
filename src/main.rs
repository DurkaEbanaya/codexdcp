use anyhow::Result;
use codexdcp::{
    bridge::Bridge,
    config::Config,
    http_server::{self, AppState},
    mcp_server::ChatGptServer,
    workspace::Workspace,
};
use clap::Parser;
use rmcp::{ServiceExt, transport::stdio};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::parse();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(&config.log_level))
        .with_writer(std::io::stderr)
        .init();

    let selectors = config.selectors();
    let bridge = Bridge::new(selectors, config.max_retries, config.retry_delay_ms);

    // Launch Chrome and connect via CDP
    let chrome_config = config.chrome_config();
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
        bridge,
        config.default_timeout,
        config.system_prompt,
        workspace,
        bash_mode,
        writes_enabled,
        tool_mode,
    );

    if config.http_only {
        info!("HTTP-only mode; skipping MCP stdio server");
        tokio::signal::ctrl_c().await?;
        info!("received Ctrl+C, shutting down");
        return Ok(());
    }

    let service = server.serve(stdio()).await?;

    info!("CodexDCP started; waiting for OpenCode on stdio");

    // Graceful shutdown: wait for MCP service or Ctrl+C
    tokio::select! {
        result = service.waiting() => {
            result?;
        }
        _ = tokio::signal::ctrl_c() => {
            info!("received Ctrl+C, shutting down");
        }
    }

    info!("shutdown complete");
    Ok(())
}
