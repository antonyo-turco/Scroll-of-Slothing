//! `sleuth://` URI scheme (on Windows: `http://sleuth.localhost/...`).
//!
//! - `sleuth://localhost/blob/<hash>` — a stored blob (original or derivative).
//! - `sleuth://localhost/media/<hash>` — an uploaded audio/video blob, streamed
//!   with HTTP Range support so `<audio>` can seek without loading the file.
//!
//! Requests are answered from the blocking pool so the UI thread never waits.

use std::io::{Read, Seek, SeekFrom};

use sleuth_service::Service;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

/// Largest slice served for one open-ended range request (`bytes=N-`).
const MEDIA_CHUNK: u64 = 4 * 1024 * 1024;

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    req: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let svc = ctx.app_handle().state::<Service>().inner().clone();
    let path = req.uri().path().trim_start_matches('/').to_string();
    let range = req.headers().get(header::RANGE).and_then(|v| v.to_str().ok()).map(str::to_string);
    tauri::async_runtime::spawn(async move {
        responder.respond(serve(&svc, &path, range.as_deref()).await);
    });
}

async fn serve(svc: &Service, path: &str, range: Option<&str>) -> Response<Vec<u8>> {
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
        (Some("media"), Some(hash)) => match svc.blob_file(hash).await {
            Ok((file, mime)) => {
                let range = range.map(str::to_string);
                tauri::async_runtime::spawn_blocking(move || {
                    serve_media(&file, &mime, range.as_deref())
                })
                .await
                .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR))
            }
            Err(e) => {
                tracing::debug!(%e, hash, "media not served");
                status(StatusCode::NOT_FOUND)
            }
        },
        _ => status(StatusCode::NOT_FOUND),
    }
}

/// Parses a single `bytes=start-end` / `bytes=start-` / `bytes=-suffix` range.
fn parse_range(spec: &str, len: u64) -> Option<(u64, u64)> {
    let spec = spec.trim().strip_prefix("bytes=")?;
    let (a, b) = spec.split(',').next()?.split_once('-')?;
    let (a, b) = (a.trim(), b.trim());
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        (len.saturating_sub(n), len.checked_sub(1)?)
    } else {
        let start: u64 = a.parse().ok()?;
        let end = if b.is_empty() {
            (start + MEDIA_CHUNK - 1).min(len.checked_sub(1)?)
        } else {
            b.parse::<u64>().ok()?.min(len - 1)
        };
        (start, end)
    };
    (start <= end && end < len).then_some((start, end))
}

fn serve_media(file: &std::path::Path, mime: &str, range: Option<&str>) -> Response<Vec<u8>> {
    let read = || -> std::io::Result<Response<Vec<u8>>> {
        let mut f = std::fs::File::open(file)?;
        let len = f.metadata()?.len();
        let base = || {
            Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        };
        let Some(spec) = range else {
            let mut body = Vec::with_capacity(len as usize);
            f.read_to_end(&mut body)?;
            return Ok(base().status(StatusCode::OK).body(body).expect("valid response"));
        };
        let Some((start, end)) = parse_range(spec, len) else {
            return Ok(base()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{len}"))
                .body(Vec::new())
                .expect("valid response"));
        };
        f.seek(SeekFrom::Start(start))?;
        let mut body = vec![0; (end - start + 1) as usize];
        f.read_exact(&mut body)?;
        Ok(base()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
            .body(body)
            .expect("valid response"))
    };
    read().unwrap_or_else(|e| {
        tracing::debug!(%e, "media read failed");
        status(StatusCode::NOT_FOUND)
    })
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    Response::builder().status(code).body(Vec::new()).expect("static response")
}

#[cfg(test)]
mod tests {
    use super::parse_range;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=900-", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=0-5000", 1000), Some((0, 999)));
        assert_eq!(parse_range("bytes=1000-", 1000), None);
        assert_eq!(parse_range("items=0-1", 1000), None);
    }
}
