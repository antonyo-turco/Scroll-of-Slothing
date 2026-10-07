//! `sleuth://` URI scheme (on Windows: `http://sleuth.localhost/...`).
//!
//! - `sleuth://localhost/blob/<hash>` — a stored blob (original or derivative).
//!
//! Requests are answered from the blocking pool so the UI thread never waits.

use sleuth_service::Service;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    req: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let svc = ctx.app_handle().state::<Service>().inner().clone();
    let path = req.uri().path().trim_start_matches('/').to_string();
    tauri::async_runtime::spawn(async move {
        responder.respond(serve(&svc, &path).await);
    });
}

async fn serve(svc: &Service, path: &str) -> Response<Vec<u8>> {
    let mut parts = path.splitn(2, '/');
    match (parts.next(), parts.next()) {
        (Some("blob"), Some(hash)) => match svc.blob_bytes(hash).await {
            Ok((bytes, mime)) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                // Content-addressed: the bytes for a hash never change.
                .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(bytes)
                .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)),
            Err(e) => {
                tracing::debug!(%e, hash, "blob not served");
                status(StatusCode::NOT_FOUND)
            }
        },
        _ => status(StatusCode::NOT_FOUND),
    }
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    Response::builder().status(code).body(Vec::new()).expect("static response")
}
