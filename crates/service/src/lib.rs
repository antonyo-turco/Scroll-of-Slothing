//! The application service layer. **Every** user-reachable operation is a method
//! on [`Service`]; the Tauri commands, MCP tools and CLI are thin wrappers.
//! CPU / IO work runs on the blocking pool so async callers never stall.

mod aoe;
mod blobs;
mod cards;
mod clipboard;
mod combat;
mod cutouts;
mod error;
mod events;
mod imgcache;
mod jobs;
mod legacy;
mod library;
mod maps;
mod music;
mod object_sheet;
mod organize;
mod secrets;
mod settings;
mod stt;
mod token_sheet;
mod tokens;
mod ui;
mod workspaces;

use std::path::Path;
use std::sync::Arc;

use sleuth_store::Store;
use tokio::sync::broadcast;

pub use aoe::{AoeInfo, AoePlaced, AoePreset, AoeSheet, AoeSheetItem, AoeSheetLayout};
pub use cards::{CardOptions, CardSummary};
pub use combat::{BestiaryEntry, BestiaryStatus, CombatCatalog, CombatImport, RowPatch};
pub use cutouts::CutoutDraft;
pub use error::{ErrorCode, ServiceError};
pub use events::Event;
pub use jobs::{JobCtx, JobId, JobState, JobStatus};
pub use legacy::{LegacyFailure, LegacyImportOptions, LegacyImportReport};
pub use library::{ASSET_KINDS, BulkAction, ExportWhat, ImportFailure, ImportFile, ImportReport};
pub use maps::{FilterCatalog, FilterPreset, MapDoc, MapLayout, SizingMode, WORK_MAX};
pub use music::{
    ChannelPatch as MusicChannelPatch, DuckMode, EngineEvent as MusicEngineEvent, MixImportResult,
    MixerState, MusicImportReport, PlaylistInput, SoundbitePatch, SoundboardInput, SoundscapeInput,
    TagSuggestion as MusicTagSuggestion, TrackInput, TrackMeta, media_mime,
};
pub use object_sheet::{ObjectItem, ObjectItemPatch, ObjectSheet};
pub use settings::SettingsUpdate;
pub use sleuth_store::{
    Asset, AssetImages, AssetPage, AssetPatch, AssetQuery, AssetSort, Campaign, RenameOutcome,
    Session, TagCount,
};
pub use sleuth_store::{Deck, DeckSummary};
pub use sleuth_store::{
    MusicChannel, MusicPlaylist, MusicSoundbite, MusicSoundboard, MusicSoundboardButton,
    MusicSoundscape, MusicTrack, MusicTrackQuery,
};
pub use sleuth_stt::cloud::{CloudModelSpec, KeyInfo as CloudKeyInfo};
pub use stt::{
    CloudKeyStatus, EngineEstimate, SttModel, TranscribeEstimate, TranscribeOptions,
    TranscribeResult,
};
pub use token_sheet::{PlacedItem, SheetItem, SheetItemPatch, SheetLayout, TokenSheet};
pub use tokens::{Border, SourceImage, TokenDraft};
pub use ui::editor_route;

pub type Result<T> = std::result::Result<T, ServiceError>;

/// Default data directory: `$SLEUTH_DATA_DIR`, else `%APPDATA%\sleuth`
/// (`$XDG_DATA_HOME/sleuth` or `~/.local/share/sleuth` elsewhere).
pub fn default_data_dir() -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Some(d) = std::env::var_os("SLEUTH_DATA_DIR") {
        return PathBuf::from(d);
    }
    if let Some(d) = std::env::var_os("APPDATA") {
        return PathBuf::from(d).join("sleuth");
    }
    if let Some(d) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(d).join("sleuth");
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(".local").join("share").join("sleuth")
}

#[derive(Clone)]
pub struct Service {
    inner: Arc<Inner>,
}

struct Inner {
    store: Arc<Store>,
    events: broadcast::Sender<Event>,
    jobs: jobs::Jobs,
    images: imgcache::ImageCache,
    maps: maps::MapCache,
    music: music::MusicRuntime,
    stt: stt::SttRuntime,
    secrets: secrets::Secrets,
    ui_attached: std::sync::atomic::AtomicBool,
}

impl Service {
    /// Opens the data directory (creating and migrating it if needed).
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(data_dir, secrets::Secrets::system())
    }

    fn open_with(data_dir: impl AsRef<Path>, secrets: secrets::Secrets) -> Result<Self> {
        let store = Arc::new(Store::open(data_dir)?);
        let (events, _) = broadcast::channel(256);
        Ok(Service {
            inner: Arc::new(Inner {
                store,
                events,
                jobs: jobs::Jobs::default(),
                images: imgcache::ImageCache::default(),
                maps: maps::MapCache::default(),
                music: music::MusicRuntime::default(),
                stt: stt::SttRuntime::default(),
                secrets,
                ui_attached: std::sync::atomic::AtomicBool::new(false),
            }),
        })
    }

    /// Subscribes to service events (job progress, store changes).
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    pub fn data_dir(&self) -> &Path {
        self.inner.store.root()
    }

    pub(crate) fn store(&self) -> Arc<Store> {
        self.inner.store.clone()
    }

    pub(crate) fn emit(&self, event: Event) {
        // No subscribers is fine (e.g. CLI); ignore the error.
        let _ = self.inner.events.send(event);
    }

    /// Polls the change log and broadcasts [`Event::StoreChanged`] for every new
    /// row: writes from this process *and* from other processes (e.g. the
    /// standalone MCP server) sharing the data dir. Must run inside a tokio runtime.
    pub fn spawn_change_watcher(&self, every: std::time::Duration) -> tokio::task::JoinHandle<()> {
        let svc = self.clone();
        // Read the baseline before returning, so a change written right after
        // this call is reported instead of being taken as already seen.
        let mut last = self.store().latest_change_seq().unwrap_or(0);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(every);
            loop {
                tick.tick().await;
                let after = last;
                let Ok(changes) = svc.blocking(move |s| Ok(s.changes_since(after)?)).await else {
                    continue;
                };
                for c in changes {
                    last = c.seq;
                    svc.emit(Event::StoreChanged {
                        entity: c.entity,
                        entity_id: c.entity_id,
                        op: c.op,
                    });
                }
            }
        })
    }

    /// Runs `f` with the store on the blocking thread pool.
    pub(crate) async fn blocking<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Store) -> Result<T> + Send + 'static,
    {
        let store = self.store();
        tokio::task::spawn_blocking(move || f(&store))
            .await
            .map_err(|e| ServiceError::Internal(format!("worker panicked: {e}")))?
    }
}

#[cfg(test)]
pub(crate) fn temp_service() -> (tempfile::TempDir, Service) {
    let dir = tempfile::tempdir().unwrap();
    // Never touch the real credential store from tests.
    let svc = Service::open_with(dir.path(), secrets::Secrets::memory()).unwrap();
    (dir, svc)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn watcher_reports_changes_from_another_process() {
        let (dir, svc) = temp_service();
        let mut rx = svc.subscribe();
        let handle = svc.spawn_change_watcher(Duration::from_millis(10));

        // A second, independent store on the same data dir (like sleuth-mcp).
        let other = Store::open(dir.path()).unwrap();
        other.record_change("asset", "42", "update").unwrap();

        let ev = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(Event::StoreChanged { entity, entity_id, .. }) = rx.recv().await {
                    return (entity, entity_id);
                }
            }
        })
        .await
        .expect("change event within 2s");
        assert_eq!(ev, ("asset".to_string(), "42".to_string()));
        handle.abort();
    }
}
