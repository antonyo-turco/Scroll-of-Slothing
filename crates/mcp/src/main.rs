//! `sloth-mcp` — MCP server over stdio.
//!
//! Register with Claude Code: `claude mcp add sloth -- <path>\sloth-mcp.exe [--data-dir <dir>]`
//! `--http <port>` serves streamable HTTP on 127.0.0.1 instead of stdio.
//! Logs go to stderr (stdout carries the protocol).

use anyhow::Context;
use rmcp::ServiceExt;
use sloth_mcp::SlothServer;
use sloth_service::Service;

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
    let mut http: Option<u16> = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--data-dir" => data_dir = args.next().map(std::path::PathBuf::from),
            "--http" => {
                http = Some(
                    args.next()
                        .and_then(|p| p.parse().ok())
                        .context("--http needs a port number")?,
                )
            }
            "-h" | "--help" => {
                eprintln!("usage: sloth-mcp [--data-dir <dir>] [--http <port>]");
                return Ok(());
            }
            other => anyhow::bail!("unknown argument {other:?}"),
        }
    }
    let dir = data_dir.unwrap_or_else(sloth_service::default_data_dir);
    let svc = Service::open(&dir).with_context(|| format!("opening data dir {}", dir.display()))?;
    if let Some(port) = http {
        let token = sloth_mcp::http::new_token();
        let h = sloth_mcp::http::serve(svc, port, Some(token.clone())).await?;
        eprintln!("sloth-mcp serving {} (Authorization: Bearer {token})", h.url());
        tokio::signal::ctrl_c().await?;
        h.stop();
        return Ok(());
    }
    tracing::info!(dir = %dir.display(), "sloth-mcp serving on stdio");

    let server = SlothServer::new(svc).serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
