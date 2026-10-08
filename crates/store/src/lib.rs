//! Persistence: one SQLite database (WAL, foreign keys on) plus a
//! content-addressed blob directory. See `docs/data-model.md`.
//!
//! `Store` is synchronous and cheap to share (`Arc<Store>`); the service layer
//! calls it from `spawn_blocking`.

mod aoe;
mod assets;
mod blobs;
mod campaigns;
mod changes;
mod decks;
mod migrate;
mod music;
mod settings;
mod tags;
mod workspaces;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

pub use assets::{
    Asset, AssetImages, AssetPage, AssetPatch, AssetQuery, AssetSort, LinkTarget, NewAsset,
};
pub use blobs::{BlobMeta, BlobRef};
pub use campaigns::{Campaign, Session};
pub use changes::Change;
pub use decks::{Deck, DeckSummary};
pub use music::{
    Channel as MusicChannel, Playlist as MusicPlaylist, PlaylistItem as MusicPlaylistItem,
    Soundbite as MusicSoundbite, Soundboard as MusicSoundboard,
    SoundboardButton as MusicSoundboardButton, Soundscape as MusicSoundscape,
    SoundscapeChannel as MusicSoundscapeChannel, SoundscapeSoundbite as MusicSoundscapeSoundbite,
    Track as MusicTrack, TrackQuery as MusicTrackQuery, TrackRow as MusicTrackRow,
};
pub use tags::{RenameOutcome, TagCount};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub struct Store {
    root: PathBuf,
    conn: Mutex<Connection>,
}

impl Store {
    /// Opens (creating if needed) the store rooted at `data_dir`, applying pending migrations.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self> {
        let root = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(root.join("blobs"))?;
        let mut conn = Connection::open(root.join("sleuth.db"))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate::run(&mut conn)?;
        Ok(Store { root, conn: Mutex::new(conn) })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Exclusive access to the connection. Keep critical sections short.
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        // A poisoned lock only means another thread panicked mid-call; the
        // connection itself is still usable (SQLite rolled back).
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Current schema version (`PRAGMA user_version`).
    pub fn schema_version(&self) -> Result<i64> {
        Ok(self.conn().query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }
}

/// ISO-8601 UTC timestamp, second precision.
pub(crate) fn now() -> String {
    time::OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .ok()
        .and_then(|t| t.format(&time::format_description::well_known::Rfc3339).ok())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn temp_store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_layout_and_migrates() {
        let (dir, store) = temp_store();
        assert!(dir.path().join("sleuth.db").exists());
        assert!(dir.path().join("blobs").is_dir());
        assert_eq!(store.schema_version().unwrap(), migrate::LATEST);
        let fk: i64 = store.conn().query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert_eq!(fk, 1);
    }

    #[test]
    fn reopen_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        drop(Store::open(dir.path()).unwrap());
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.schema_version().unwrap(), migrate::LATEST);
    }
}
