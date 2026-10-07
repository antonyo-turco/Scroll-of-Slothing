//! `sleuth-mcp` — MCP server over stdio.
//!
//! Register with Claude Code: `claude mcp add sleuth -- <path>\sleuth-mcp.exe [--data-dir <dir>]`
//! Logs go to stderr (stdout carries the protocol).

use anyhow::Context;
use rmcp::ServiceExt;
use sleuth_mcp::SleuthServer;
use sleuth_service::Service;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let mut args = std::env::args().skip(1);
    let mut data_dir = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--data-dir" => data_dir = args.next().map(std::path::PathBuf::from),
            "-h" | "--help" => {
                eprintln!("usage: sleuth-mcp [--data-dir <dir>]");
                return Ok(());
            }
            other => anyhow::bail!("unknown argument {other:?}"),
        }
    }
    let dir = data_dir.unwrap_or_else(sleuth_service::default_data_dir);
    let svc = Service::open(&dir).with_context(|| format!("opening data dir {}", dir.display()))?;
    tracing::info!(dir = %dir.display(), "sleuth-mcp serving on stdio");

    let server = SleuthServer::new(svc).serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
