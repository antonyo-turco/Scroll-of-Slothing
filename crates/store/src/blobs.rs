//! Content-addressed blob store: `blobs/ab/cd/<blake3>.<ext>` + a `blobs` row.

use std::io::Write;
use std::path::PathBuf;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{Result, Store, StoreError, now};

/// A blake3 hex digest identifying a blob.
pub type BlobRef = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobMeta {
    pub hash: BlobRef,
    pub mime: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bytes: u64,
}

fn ext_for(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "application/pdf" => "pdf",
        _ => "bin",
    }
}

impl Store {
    fn blob_path(&self, hash: &str, mime: &str) -> PathBuf {
        self.root()
            .join("blobs")
            .join(&hash[0..2])
            .join(&hash[2..4])
            .join(format!("{hash}.{}", ext_for(mime)))
    }

    /// Stores `data` (deduplicated by content) and returns its metadata.
    /// The file is written atomically (temp file + rename) before the row is inserted.
    pub fn put_blob(&self, data: &[u8], mime: &str, dims: Option<(u32, u32)>) -> Result<BlobMeta> {
        let hash = blake3::hash(data).to_hex().to_string();
        let path = self.blob_path(&hash, mime);
        if !path.exists() {
            let dir = path.parent().expect("blob path has a parent");
            std::fs::create_dir_all(dir)?;
            // Unique temp name: concurrent writers of the same content must not collide.
            let tmp = dir.join(format!(".{hash}.{}.tmp", uuid::Uuid::new_v4().simple()));
            {
                let mut f = std::fs::File::create(&tmp)?;
                f.write_all(data)?;
                f.sync_all()?;
            }
            if let Err(e) = std::fs::rename(&tmp, &path) {
                let _ = std::fs::remove_file(&tmp);
                // Another writer stored identical bytes first: that's success.
                if !path.exists() {
                    return Err(e.into());
                }
            }
        }
        let meta = BlobMeta {
            hash: hash.clone(),
            mime: mime.to_string(),
            width: dims.map(|d| d.0),
            height: dims.map(|d| d.1),
            bytes: data.len() as u64,
        };
        self.conn().execute(
            "INSERT OR IGNORE INTO blobs (hash, mime, width, height, bytes, created)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![meta.hash, meta.mime, meta.width, meta.height, meta.bytes as i64, now()],
        )?;
        Ok(meta)
    }

    pub fn blob_meta(&self, hash: &str) -> Result<BlobMeta> {
        self.conn()
            .query_row(
                "SELECT hash, mime, width, height, bytes FROM blobs WHERE hash = ?1",
                [hash],
                |r| {
                    Ok(BlobMeta {
                        hash: r.get(0)?,
                        mime: r.get(1)?,
                        width: r.get(2)?,
                        height: r.get(3)?,
                        bytes: r.get::<_, i64>(4)? as u64,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("blob {hash}")))
    }

    /// Filesystem path of a stored blob (for streaming / the `sleuth://` protocol).
    pub fn blob_file(&self, hash: &str) -> Result<PathBuf> {
        if hash.len() < 4 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(StoreError::Invalid(format!("bad blob hash {hash:?}")));
        }
        let meta = self.blob_meta(hash)?;
        Ok(self.blob_path(hash, &meta.mime))
    }

    pub fn read_blob(&self, hash: &str) -> Result<Vec<u8>> {
        Ok(std::fs::read(self.blob_file(hash)?)?)
    }
}

#[cfg(test)]
mod tests {
    use crate::temp_store;

    #[test]
    fn put_is_content_addressed_and_deduplicated() {
        let (_d, s) = temp_store();
        let a = s.put_blob(b"hello", "image/png", Some((1, 2))).unwrap();
        let b = s.put_blob(b"hello", "image/png", Some((1, 2))).unwrap();
        assert_eq!(a.hash, b.hash);
        assert_eq!(a.hash, blake3::hash(b"hello").to_hex().to_string());
        let n: i64 = s.conn().query_row("SELECT count(*) FROM blobs", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        assert_eq!(s.read_blob(&a.hash).unwrap(), b"hello");
        let meta = s.blob_meta(&a.hash).unwrap();
        assert_eq!((meta.width, meta.height, meta.bytes), (Some(1), Some(2), 5));
    }

    #[test]
    fn concurrent_puts_of_identical_content_succeed() {
        let (_d, s) = temp_store();
        let data = vec![7u8; 1 << 20];
        let hashes: Vec<String> = std::thread::scope(|sc| {
            let hs: Vec<_> =
                (0..8).map(|_| sc.spawn(|| s.put_blob(&data, "image/png", None).unwrap().hash)).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert!(hashes.windows(2).all(|w| w[0] == w[1]));
        assert_eq!(s.read_blob(&hashes[0]).unwrap(), data);
    }

    #[test]
    fn blob_file_layout_and_validation() {
        let (d, s) = temp_store();
        let m = s.put_blob(b"%PDF-1.7", "application/pdf", None).unwrap();
        let p = s.blob_file(&m.hash).unwrap();
        assert!(p.starts_with(d.path().join("blobs").join(&m.hash[0..2]).join(&m.hash[2..4])));
        assert_eq!(p.extension().unwrap(), "pdf");
        assert!(s.blob_file("../../etc").is_err());
        assert!(s.blob_file("abcdef").is_err(), "unknown hash is NotFound");
    }
}
