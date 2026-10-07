//! Small LRU of decoded images keyed by blob hash, so interactive editors
//! (dragging, zooming) never re-decode the source on every frame.

use std::sync::{Arc, Mutex};

use image::RgbaImage;
use sleuth_core::imaging;
use sleuth_store::Store;

use crate::{Result, ServiceError};

const CAPACITY: usize = 6;

#[derive(Default)]
pub(crate) struct ImageCache {
    entries: Mutex<Vec<(String, Arc<RgbaImage>)>>,
}

impl ImageCache {
    /// Returns the decoded RGBA image for a stored blob, decoding it on a miss.
    pub(crate) fn get(&self, store: &Store, hash: &str) -> Result<Arc<RgbaImage>> {
        {
            let mut e = self.entries.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(i) = e.iter().position(|(h, _)| h == hash) {
                let hit = e.remove(i);
                let img = hit.1.clone();
                e.push(hit);
                return Ok(img);
            }
        }
        // Decode outside the lock.
        let bytes = store.read_blob(hash)?;
        let img = Arc::new(
            imaging::decode(&bytes)
                .map_err(|e| ServiceError::InvalidInput(e.to_string()))?
                .to_rgba8(),
        );
        let mut e = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        e.retain(|(h, _)| h != hash);
        e.push((hash.to_string(), img.clone()));
        if e.len() > CAPACITY {
            e.remove(0);
        }
        Ok(img)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_and_evicts_least_recently_used() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let cache = ImageCache::default();
        let mut hashes = vec![];
        for i in 0..(CAPACITY as u8 + 1) {
            let png =
                imaging::encode_png(&RgbaImage::from_pixel(2, 2, image::Rgba([i, 0, 0, 255])))
                    .unwrap();
            hashes.push(store.put_blob(&png, "image/png", Some((2, 2))).unwrap().hash);
        }
        let a = cache.get(&store, &hashes[0]).unwrap();
        let again = cache.get(&store, &hashes[0]).unwrap();
        assert!(Arc::ptr_eq(&a, &again), "second get is a cache hit");
        for h in &hashes[1..] {
            cache.get(&store, h).unwrap();
        }
        let after = cache.get(&store, &hashes[0]).unwrap();
        assert!(!Arc::ptr_eq(&a, &after), "oldest entry was evicted");
        assert!(cache.get(&store, "deadbeef").is_err());
    }
}
