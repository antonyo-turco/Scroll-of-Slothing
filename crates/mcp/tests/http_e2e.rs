//! The loopback HTTP transport: bearer-token gate and a real `initialize`
//! round trip over raw HTTP/1.1 (no client library needed).

use sleuth_mcp::http::serve;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn post(port: u16, token: Option<&str>, body: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    let req = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    // SSE responses stay open; read what arrives within a short window.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), s.read_to_end(&mut out)).await;
    String::from_utf8_lossy(&out).into_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn token_gate_and_initialize() {
    let dir = tempfile::tempdir().unwrap();
    let svc = sleuth_service::Service::open(dir.path()).unwrap();
    let h = serve(svc, 0, Some("s3cret".into())).await.unwrap();
    let port = h.addr.port();
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#;

    let denied = post(port, None, init).await;
    assert!(denied.starts_with("HTTP/1.1 401"), "{denied}");
    let wrong = post(port, Some("nope"), init).await;
    assert!(wrong.starts_with("HTTP/1.1 401"), "{wrong}");

    let ok = post(port, Some("s3cret"), init).await;
    assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
    assert!(ok.contains("serverInfo"), "{ok}");
    h.stop();
}
