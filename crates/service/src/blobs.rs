//! Blob access for the UI protocol and exports.

use crate::{Result, Service};

impl Service {
    /// Bytes and MIME type of a stored blob.
    pub async fn blob_bytes(&self, hash: &str) -> Result<(Vec<u8>, String)> {
        let hash = hash.to_string();
        self.blocking(move |s| {
            let meta = s.blob_meta(&hash)?;
            Ok((s.read_blob(&hash)?, meta.mime))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use crate::temp_service;

    #[tokio::test]
    async fn blob_bytes_roundtrip_and_not_found() {
        let (_d, svc) = temp_service();
        let meta = svc.store().put_blob(b"png-bytes", "image/png", Some((2, 2))).unwrap();
        let (bytes, mime) = svc.blob_bytes(&meta.hash).await.unwrap();
        assert_eq!((bytes.as_slice(), mime.as_str()), (&b"png-bytes"[..], "image/png"));
        let err = svc.blob_bytes("ffff").await.unwrap_err();
        assert_eq!(err.code(), crate::ErrorCode::NotFound);
    }
}
