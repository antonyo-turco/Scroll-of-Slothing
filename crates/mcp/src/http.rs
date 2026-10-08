//! MCP over streamable HTTP on loopback, sharing the caller's [`Service`].
//!
//! Used by the desktop app when the `mcp_http_enabled` setting is on, and by
//! `sleuth-mcp --http <port>`. Binds `127.0.0.1` only; rmcp validates the Host
//! header (localhost / 127.0.0.1 / ::1) to block DNS-rebinding, and every
//! request must carry `Authorization: Bearer <token>` when a token is set.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Request, Response, StatusCode, body::Incoming};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use sleuth_service::Service;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::SleuthServer;

/// Default loopback port of the in-app server.
pub const DEFAULT_PORT: u16 = 8765;

/// A running HTTP server; dropping it does not stop it, call [`HttpHandle::stop`].
pub struct HttpHandle {
    pub addr: SocketAddr,
    cancel: CancellationToken,
}

impl HttpHandle {
    /// The MCP endpoint URL clients connect to.
    pub fn url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

/// A random 128-bit hex token for [`serve`].
pub fn new_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn authorized(req: &Request<Incoming>, token: Option<&str>) -> bool {
    let Some(token) = token else { return true };
    req.headers()
        .get(hyper::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| t == token)
}

/// Starts serving on `127.0.0.1:port` (0 = any free port). With `token`,
/// requests without the matching bearer token get 401. Must be called inside
/// a tokio runtime.
pub async fn serve(svc: Service, port: u16, token: Option<String>) -> std::io::Result<HttpHandle> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    let addr = listener.local_addr()?;
    let cancel = CancellationToken::new();
    let mut config = StreamableHttpServerConfig::default();
    config.cancellation_token = cancel.child_token();
    let mcp = StreamableHttpService::new(
        move || Ok(SleuthServer::new(svc.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    let stop = cancel.clone();
    tokio::spawn(async move {
        loop {
            let (stream, _) = tokio::select! {
                _ = stop.cancelled() => break,
                r = listener.accept() => match r {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("mcp http accept failed: {e}");
                        continue;
                    }
                },
            };
            let mcp = mcp.clone();
            let token = token.clone();
            let svc = hyper::service::service_fn(move |req: Request<Incoming>| {
                let mcp = mcp.clone();
                let ok = authorized(&req, token.as_deref());
                async move {
                    if !ok {
                        let body = Full::new(Bytes::from_static(b"missing or wrong bearer token"))
                            .map_err(|never: Infallible| match never {})
                            .boxed();
                        let mut res = Response::new(body);
                        *res.status_mut() = StatusCode::UNAUTHORIZED;
                        return Ok::<_, Infallible>(res);
                    }
                    Ok(mcp.handle(req).await)
                }
            });
            let conn_stop = stop.clone();
            tokio::spawn(async move {
                let builder = Builder::new(TokioExecutor::new());
                let conn = builder.serve_connection_with_upgrades(TokioIo::new(stream), svc);
                tokio::select! {
                    r = conn => if let Err(e) = r { tracing::debug!("mcp http connection: {e}") },
                    _ = conn_stop.cancelled() => {}
                }
            });
        }
        tracing::info!("mcp http server on {addr} stopped");
    });
    tracing::info!("mcp http server listening on {addr}");
    Ok(HttpHandle { addr, cancel })
}
