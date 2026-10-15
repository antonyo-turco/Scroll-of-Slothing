//! In-app MCP server over HTTP (loopback only), started/stopped live from the
//! `mcp_http_enabled` setting. It shares the app's `Service`, so an LLM sees
//! and changes exactly what the UI shows.

use serde::Serialize;
use sleuth_mcp::http::{DEFAULT_PORT, HttpHandle, new_token, serve};
use sleuth_service::Service;
use tauri::State;
use tokio::sync::Mutex;

#[derive(Default)]
pub struct McpHttp(Mutex<Option<(HttpHandle, String)>>);

/// Workspace key of the bearer token (stable across restarts so clients
/// only need configuring once).
const TOKEN_KEY: &str = "mcp.http_token";

async fn token(svc: &Service) -> String {
    if let Ok(Some(serde_json::Value::String(t))) = svc.workspace_get(TOKEN_KEY.into()).await
        && !t.is_empty()
    {
        return t;
    }
    let t = new_token();
    let _ = svc.workspace_set(TOKEN_KEY.into(), serde_json::Value::String(t.clone())).await;
    t
}

/// Starts or stops the server to match the current setting.
pub async fn sync(svc: &Service, state: &McpHttp) {
    let enabled = svc.settings_get().await.map(|s| s.mcp_http_enabled).unwrap_or(false);
    let mut cur = state.0.lock().await;
    match (enabled, cur.is_some()) {
        (true, false) => {
            let token = token(svc).await;
            match serve(svc.clone(), DEFAULT_PORT, Some(token.clone())).await {
                Ok(h) => {
                    tracing::info!(url = %h.url(), "in-app MCP server started");
                    *cur = Some((h, token));
                }
                Err(e) => {
                    tracing::warn!("cannot start in-app MCP server on port {DEFAULT_PORT}: {e}")
                }
            }
        }
        (false, true) => {
            if let Some((h, _)) = cur.take() {
                h.stop();
                tracing::info!("in-app MCP server stopped");
            }
        }
        _ => {}
    }
}

#[derive(Serialize)]
pub struct McpHttpStatus {
    url: String,
    token: String,
}

/// Endpoint URL and bearer token while the server runs, else `None`.
#[tauri::command]
pub async fn mcp_http_status(state: State<'_, McpHttp>) -> Result<Option<McpHttpStatus>, String> {
    Ok(state.0.lock().await.as_ref().map(|(h, t)| McpHttpStatus { url: h.url(), token: t.clone() }))
}
