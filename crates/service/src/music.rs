//! Music service. Spec: docs/features/music/*.md, contract: app/src/lib/music.ts.
//!
//! State model
//! - The live mix (channels, soundbites) is in SQLite; the rest of the live
//!   state (loaded soundscape, duck mode, master volume, playlist position,
//!   playing soundboard buttons) is the `music.live` workspace. Transient
//!   actions (one-shots, previews…) requested by a process without an audio
//!   engine (the standalone MCP server) queue in `music.requests`.
//! - The app attaches the webview engine (`music_attach_engine`). `sync`
//!   diffs the desired state against what the engine is playing and emits
//!   `Event::MusicEngine` commands. It runs after every local change and when
//!   another process changed the music state (the UI calls `music_sync` on
//!   `store_changed: music_live`).
//! - Soundbite timers and the SFX queue (with ducking) are tokio tasks that
//!   only run in the process with the engine.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sleuth_store::{
    MusicChannel, MusicPlaylist, MusicSoundbite, MusicSoundboard, MusicSoundboardButton,
    MusicSoundscape, MusicSoundscapeChannel, MusicSoundscapeSoundbite, MusicTrack, MusicTrackQuery,
    MusicTrackRow, Store,
};
use tokio::sync::{mpsc, oneshot};

use crate::{Event, Result, Service, ServiceError};

const LIVE_KEY: &str = "music.live";
const REQ_KEY: &str = "music.requests";
const PLAYLIST_CHANNEL: &str = "playlist";
const SFX_QUEUE_MAX: usize = 12;
const RAMP_MS: u32 = 150;

pub type Track = MusicTrack;
pub type TrackQuery = MusicTrackQuery;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DuckMode {
    #[default]
    Off,
    Soft,
    Strong,
}

impl DuckMode {
    pub fn factor(self) -> f64 {
        match self {
            DuckMode::Off => 1.0,
            DuckMode::Soft => 0.5,
            DuckMode::Strong => 0.25,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistPlayback {
    pub playlist_id: String,
    pub name: String,
    pub index: usize,
    pub track_ids: Vec<String>,
    pub title: String,
    pub paused: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct LiveMeta {
    soundscape_id: Option<String>,
    duck: DuckMode,
    master_volume: i64,
    soundbites_running: bool,
    playlist: Option<PlaylistPlayback>,
    toggles: Vec<String>,
    restarts: Vec<String>,
}

impl Default for LiveMeta {
    fn default() -> Self {
        Self {
            soundscape_id: None,
            duck: DuckMode::Off,
            master_volume: 100,
            soundbites_running: false,
            playlist: None,
            toggles: vec![],
            restarts: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Request {
    Oneshot { track_id: String, volume: i64 },
    Sfx { track_id: String, volume: i64 },
    Preview { track_id: String, seconds: f64 },
    StopPreview,
    ToggleVideo,
    Restart { button_id: String },
    StopAll,
}

/// Track create/update payload (`music.ts` `TrackInput`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackInput {
    pub source: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub file_path: Option<PathBuf>,
    pub title: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub loop_by_default: Option<bool>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackMeta {
    pub title: Option<String>,
    pub duration: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChannelPatch {
    pub volume: Option<i64>,
    pub is_looping: Option<bool>,
    pub is_playing: Option<bool>,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SoundbitePatch {
    pub volume: Option<i64>,
    pub min_interval: Option<f64>,
    pub max_interval: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundscapeInput {
    pub name: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub track_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundboardInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub notes: Option<String>,
    pub buttons: Vec<MusicSoundboardButton>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MixerState {
    pub channels: Vec<MusicChannel>,
    pub soundbites: Vec<MusicSoundbite>,
    pub soundbites_running: bool,
    pub soundscape_id: Option<String>,
    pub soundscape_name: Option<String>,
    pub duck: DuckMode,
    pub ducked: bool,
    pub master_volume: i64,
    pub playlist: Option<PlaylistPlayback>,
    pub toggles_playing: Vec<String>,
    pub previewing: bool,
    pub sfx_queue: usize,
    pub engine_attached: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EngineEvent {
    Ready,
    ChannelEnded { channel_id: String },
    ChannelError { channel_id: String, code: i64 },
    OneshotEnded { osid: String },
    OneshotDuration { osid: String, seconds: f64 },
    TrackDuration { channel_id: String, seconds: f64 },
    PreviewEnded,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MixImportResult {
    pub soundscape_id: String,
    pub name: String,
    pub tracks_created: usize,
    pub tracks_reused: usize,
    pub state: MixerState,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MusicImportReport {
    pub tracks: usize,
    pub soundscapes: usize,
    pub playlists: usize,
    pub soundboards: usize,
    pub skipped: Vec<String>,
}

/// What the engine is currently doing with one channel.
#[derive(Debug, Clone, PartialEq)]
struct EngineCh {
    track_id: String,
    playing: bool,
    volume: i64,
    looping: bool,
}

struct Desired {
    id: String,
    track: Track,
    volume: i64,
    looping: bool,
    playing: bool,
}

#[derive(Default)]
struct Rt {
    engine: HashMap<String, EngineCh>,
    /// The SFX worker is playing a soundbite.
    sfx_ducked: bool,
    /// Soundboard one-shots still playing (they duck too, but never queue).
    board_oneshots: HashSet<String>,
    previewing: bool,
    sfx_tx: Option<mpsc::Sender<(u64, String, i64)>>,
    soundbite_key: String,
    soundbite_tasks: Vec<tokio::task::JoinHandle<()>>,
    waiters: HashMap<String, oneshot::Sender<()>>,
}

/// Process-local music runtime (engine view, timers, SFX queue).
#[derive(Default)]
pub(crate) struct MusicRuntime {
    attached: AtomicBool,
    sfx_len: AtomicUsize,
    generation: AtomicU64,
    os_seq: AtomicU64,
    rt: Mutex<Rt>,
}

impl Rt {
    /// The music is lowered while a soundbite or a soundboard one-shot plays.
    fn ducked(&self) -> bool {
        self.sfx_ducked || !self.board_oneshots.is_empty()
    }
}

fn internal(e: impl std::fmt::Display) -> ServiceError {
    ServiceError::Internal(e.to_string())
}

fn invalid(msg: impl Into<String>) -> ServiceError {
    ServiceError::InvalidInput(msg.into())
}

fn load_meta(s: &Store) -> Result<LiveMeta> {
    Ok(s.workspace_get::<LiveMeta>(LIVE_KEY)?.unwrap_or_default())
}

fn save_meta(s: &Store, m: &LiveMeta) -> Result<()> {
    s.workspace_set(LIVE_KEY, m)?;
    s.record_change("music_live", "live", "update")?;
    Ok(())
}

fn src_of(t: &Track) -> (&'static str, String) {
    if t.source == "youtube" {
        ("youtube", t.url.clone().unwrap_or_default())
    } else {
        ("file", t.blob_hash.clone().unwrap_or_default())
    }
}

fn uniform(lo: f64, hi: f64) -> f64 {
    lo + (hi - lo) * fastrand::f64()
}

/// MIME type for an audio/video upload, by extension.
pub fn media_mime(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/aac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "weba" => "audio/webm",
        "webm" => "video/webm",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        _ => "application/octet-stream",
    }
}

fn ext_for_mime(mime: &str) -> &'static str {
    match mime {
        "audio/mpeg" => "mp3",
        "audio/aac" => "m4a",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/wav" => "wav",
        "audio/flac" => "flac",
        "audio/webm" => "weba",
        "video/webm" => "webm",
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/x-matroska" => "mkv",
        _ => "bin",
    }
}

fn store_media(s: &Store, path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(s.put_blob(&bytes, media_mime(&ext), None)?.hash)
}

fn clamp_vol(v: i64) -> i64 {
    v.clamp(0, 100)
}

fn check_role(r: &str) -> Result<String> {
    match r {
        "music" | "ambient" | "sfx" => Ok(r.to_string()),
        _ => Err(invalid(format!("unknown role {r:?}"))),
    }
}

fn interval(min: f64, max: f64) -> (f64, f64) {
    let (a, b) = (min.clamp(1.0, 3600.0), max.clamp(1.0, 3600.0));
    if a > b { (b, a) } else { (a, b) }
}

impl Service {
    fn music(&self) -> &MusicRuntime {
        &self.inner.music
    }

    fn rt(&self) -> std::sync::MutexGuard<'_, Rt> {
        self.music().rt.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn engine_cmd(&self, command: Value) {
        self.emit(Event::MusicEngine { command });
    }

    /// Called by the app (the process hosting the audio engine) at startup:
    /// clears stale live state (Python left `is_playing` rows after a crash).
    pub async fn music_attach_engine(&self) -> Result<()> {
        self.music().attached.store(true, Ordering::SeqCst);
        self.blocking(|s| {
            s.music_clear_live()?;
            let meta = LiveMeta { ..Default::default() };
            save_meta(s, &meta)?;
            s.workspace_set(REQ_KEY, &Vec::<Request>::new())?;
            Ok(())
        })
        .await
    }

    // ── library ─────────────────────────────────────────────────────────

    pub async fn music_tracks_list(&self, query: TrackQuery) -> Result<Vec<Track>> {
        self.blocking(move |s| Ok(s.music_tracks_list(&query)?)).await
    }

    pub async fn music_track_get(&self, id: String) -> Result<Track> {
        self.blocking(move |s| Ok(s.music_track_get(&id)?)).await
    }

    fn track_row(s: &Store, input: &TrackInput, existing: Option<&Track>) -> Result<MusicTrackRow> {
        if input.title.trim().is_empty() {
            return Err(invalid("title is required"));
        }
        let (url, blob) = match input.source.as_str() {
            "youtube" => {
                let u = input.url.clone().unwrap_or_default();
                if u.trim().is_empty() {
                    return Err(invalid("enter a YouTube URL"));
                }
                (Some(u.trim().to_string()), None)
            }
            "upload" => match (&input.file_path, existing.and_then(|e| e.blob_hash.clone())) {
                (Some(p), _) => (None, Some(store_media(s, p)?)),
                (None, Some(h)) => (None, Some(h)),
                (None, None) => return Err(invalid("choose a file")),
            },
            other => return Err(invalid(format!("unknown source {other:?}"))),
        };
        Ok(MusicTrackRow {
            source: input.source.clone(),
            url,
            blob_hash: blob,
            title: input.title.trim().to_string(),
            notes: input.notes.clone().unwrap_or_default(),
            loop_by_default: input.loop_by_default.unwrap_or(true),
            duration: input.duration.or(existing.and_then(|e| e.duration)),
            // campaigns/sessions are no longer edited; keep any hidden links
            campaign_ids: existing.map(|e| e.campaign_ids.clone()).unwrap_or_default(),
            session_ids: existing.map(|e| e.session_ids.clone()).unwrap_or_default(),
            tags: input.tags.clone(),
            legacy_id: None,
        })
    }

    pub async fn music_track_create(&self, input: TrackInput) -> Result<Track> {
        self.blocking(move |s| {
            let row = Self::track_row(s, &input, None)?;
            Ok(s.music_track_save(None, &row)?)
        })
        .await
    }

    pub async fn music_track_update(&self, id: String, input: TrackInput) -> Result<Track> {
        self.blocking(move |s| {
            let existing = s.music_track_get(&id)?;
            let row = Self::track_row(s, &input, Some(&existing))?;
            Ok(s.music_track_save(Some(&id), &row)?)
        })
        .await
    }

    /// Marks a track as a favourite (or not). Returns the updated track.
    pub async fn music_track_set_favorite(&self, id: String, favorite: bool) -> Result<MusicTrack> {
        self.blocking(move |s| Ok(s.music_track_set_favorite(&id, favorite)?)).await
    }

    pub async fn music_track_delete(&self, id: String) -> Result<()> {
        self.blocking(move |s| Ok(s.music_track_delete(&id)?)).await?;
        self.music_sync().await
    }

    /// Title from YouTube oEmbed and duration scraped from the watch page
    /// (`"lengthSeconds":"N"`), best effort with a 6 s timeout (system curl).
    pub async fn music_fetch_youtube_meta(&self, url: String) -> Result<TrackMeta> {
        tokio::task::spawn_blocking(move || {
            let get = |u: &str| -> Option<String> {
                let out = std::process::Command::new("curl")
                    .args(["-s", "-L", "-m", "6", "-A", "Mozilla/5.0", u])
                    .output()
                    .ok()?;
                out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
            };
            let enc: String = url
                .bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                        (b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect();
            let title = get(&format!("https://www.youtube.com/oembed?url={enc}&format=json"))
                .and_then(|j| serde_json::from_str::<Value>(&j).ok())
                .and_then(|v| v["title"].as_str().map(String::from));
            let duration = get(&url).and_then(|page| {
                let i = page.find("\"lengthSeconds\":\"")? + 17;
                page[i..].split('"').next()?.parse::<f64>().ok()
            });
            TrackMeta { title, duration }
        })
        .await
        .map_err(internal)
    }

    /// Title from the file name; the duration is reported by the engine when played.
    pub async fn music_probe_file(&self, path: PathBuf) -> Result<TrackMeta> {
        Ok(TrackMeta {
            title: path.file_stem().map(|s| s.to_string_lossy().into_owned()),
            duration: None,
        })
    }

    // ── live mix ────────────────────────────────────────────────────────

    pub async fn music_state(&self) -> Result<MixerState> {
        let svc = self.clone();
        self.blocking(move |s| svc.state_of(s)).await
    }

    fn state_of(&self, s: &Store) -> Result<MixerState> {
        let meta = load_meta(s)?;
        let soundscape_name = match &meta.soundscape_id {
            Some(id) => s.music_soundscape_get(id).ok().map(|x| x.name),
            None => None,
        };
        let rt = self.rt();
        Ok(MixerState {
            channels: s.music_channels()?,
            soundbites: s.music_soundbites()?,
            soundbites_running: meta.soundbites_running,
            soundscape_id: meta.soundscape_id.clone(),
            soundscape_name,
            duck: meta.duck,
            ducked: rt.ducked(),
            master_volume: meta.master_volume,
            playlist: meta.playlist.clone(),
            toggles_playing: meta.toggles.clone(),
            previewing: rt.previewing,
            sfx_queue: self.music().sfx_len.load(Ordering::SeqCst),
            engine_attached: self.music().attached.load(Ordering::SeqCst),
        })
    }

    /// Runs `f` on the store, then syncs the engine and returns the state.
    async fn live<F>(&self, f: F) -> Result<MixerState>
    where
        F: FnOnce(&Store) -> Result<()> + Send + 'static,
    {
        let svc = self.clone();
        self.blocking(move |s| {
            f(s)?;
            svc.sync_inner(s)?;
            svc.state_of(s)
        })
        .await
    }

    pub async fn music_channel_add(
        &self,
        track_id: String,
        role: Option<String>,
        volume: Option<i64>,
        looping: Option<bool>,
        play: bool,
    ) -> Result<MixerState> {
        self.live(move |s| {
            let t = s.music_track_get(&track_id)?;
            let role = check_role(role.as_deref().unwrap_or("music"))?;
            // the sfx role forces looping off
            let looping = role != "sfx" && looping.unwrap_or(t.loop_by_default);
            s.music_channel_insert(&t.id, clamp_vol(volume.unwrap_or(80)), looping, play, &role)?;
            Ok(())
        })
        .await
    }

    pub async fn music_channel_update(
        &self,
        id: String,
        patch: ChannelPatch,
    ) -> Result<MixerState> {
        self.live(move |s| {
            let mut c = s
                .music_channels()?
                .into_iter()
                .find(|c| c.id == id)
                .ok_or_else(|| ServiceError::NotFound(format!("channel {id}")))?;
            if let Some(v) = patch.volume {
                c.volume = clamp_vol(v);
            }
            if let Some(l) = patch.is_looping {
                c.is_looping = l;
            }
            if let Some(p) = patch.is_playing {
                c.is_playing = p;
            }
            if let Some(r) = patch.role {
                c.role = check_role(&r)?;
            }
            Ok(s.music_channel_update(&c)?)
        })
        .await
    }

    pub async fn music_channel_remove(&self, id: String) -> Result<MixerState> {
        self.live(move |s| Ok(s.music_channel_delete(&id)?)).await
    }

    /// ▶ All / ⏸ All. Pausing also stops the soundbite timers.
    pub async fn music_set_all_playing(&self, playing: bool) -> Result<MixerState> {
        self.live(move |s| {
            s.music_channels_set_playing(playing)?;
            let mut m = load_meta(s)?;
            m.soundbites_running = playing && !s.music_soundbites()?.is_empty();
            save_meta(s, &m)
        })
        .await
    }

    /// Global stop: stops everything, clears the live mix and the loaded soundscape.
    pub async fn music_stop_all(&self) -> Result<MixerState> {
        let svc = self.clone();
        self.blocking(move |s| {
            s.music_clear_live()?;
            let m = load_meta(s)?;
            save_meta(
                s,
                &LiveMeta { duck: m.duck, master_volume: m.master_volume, ..Default::default() },
            )?;
            if svc.music().attached.load(Ordering::SeqCst) {
                svc.engine_stop_all();
            } else {
                push_request(s, Request::StopAll)?;
            }
            svc.sync_inner(s)?;
            svc.state_of(s)
        })
        .await
    }

    fn engine_stop_all(&self) {
        self.music().generation.fetch_add(1, Ordering::SeqCst);
        self.music().sfx_len.store(0, Ordering::SeqCst);
        let mut rt = self.rt();
        rt.engine.clear();
        rt.sfx_ducked = false;
        rt.board_oneshots.clear();
        rt.previewing = false;
        for t in rt.soundbite_tasks.drain(..) {
            t.abort();
        }
        rt.soundbite_key.clear();
        for (_, w) in rt.waiters.drain() {
            let _ = w.send(());
        }
        drop(rt);
        self.engine_cmd(json!({ "action": "stop_all" }));
    }

    pub async fn music_soundbite_add(
        &self,
        track_id: String,
        min_interval: f64,
        max_interval: f64,
        volume: i64,
    ) -> Result<MixerState> {
        self.live(move |s| {
            s.music_track_get(&track_id)?;
            let (lo, hi) = interval(min_interval, max_interval);
            s.music_soundbite_insert(&track_id, clamp_vol(volume), lo, hi)?;
            let mut m = load_meta(s)?;
            m.soundbites_running = true;
            save_meta(s, &m)
        })
        .await
    }

    pub async fn music_soundbite_update(
        &self,
        id: String,
        patch: SoundbitePatch,
    ) -> Result<MixerState> {
        self.live(move |s| {
            let mut b = s
                .music_soundbites()?
                .into_iter()
                .find(|b| b.id == id)
                .ok_or_else(|| ServiceError::NotFound(format!("soundbite {id}")))?;
            if let Some(v) = patch.volume {
                b.volume = clamp_vol(v);
            }
            let (lo, hi) = interval(
                patch.min_interval.unwrap_or(b.min_interval),
                patch.max_interval.unwrap_or(b.max_interval),
            );
            b.min_interval = lo;
            b.max_interval = hi;
            Ok(s.music_soundbite_update(&b)?)
        })
        .await
    }

    pub async fn music_soundbite_remove(&self, id: String) -> Result<MixerState> {
        self.live(move |s| Ok(s.music_soundbite_delete(&id)?)).await
    }

    /// Plays a soundbite now through the SFX queue (ducks the music).
    pub async fn music_soundbite_play(&self, id: String) -> Result<MixerState> {
        let svc = self.clone();
        self.blocking(move |s| {
            let b = s
                .music_soundbites()?
                .into_iter()
                .find(|b| b.id == id)
                .ok_or_else(|| ServiceError::NotFound(format!("soundbite {id}")))?;
            svc.request(s, Request::Sfx { track_id: b.track_id, volume: b.volume })?;
            svc.state_of(s)
        })
        .await
    }

    pub async fn music_set_duck(&self, mode: DuckMode) -> Result<MixerState> {
        self.live(move |s| {
            let mut m = load_meta(s)?;
            m.duck = mode;
            save_meta(s, &m)
        })
        .await
    }

    pub async fn music_set_master(&self, volume: i64) -> Result<MixerState> {
        self.live(move |s| {
            let mut m = load_meta(s)?;
            m.master_volume = clamp_vol(volume);
            save_meta(s, &m)
        })
        .await
    }

    pub async fn music_preview(&self, track_id: String, seconds: Option<f64>) -> Result<()> {
        let svc = self.clone();
        self.blocking(move |s| {
            svc.request(
                s,
                Request::Preview { track_id, seconds: seconds.unwrap_or(10.0).clamp(1.0, 600.0) },
            )
        })
        .await
    }

    pub async fn music_stop_preview(&self) -> Result<()> {
        let svc = self.clone();
        self.blocking(move |s| svc.request(s, Request::StopPreview)).await
    }

    pub async fn music_toggle_video(&self) -> Result<()> {
        let svc = self.clone();
        self.blocking(move |s| svc.request(s, Request::ToggleVideo)).await
    }

    /// Executes a transient action now (engine attached) or queues it for the app.
    fn request(&self, s: &Store, r: Request) -> Result<()> {
        if self.music().attached.load(Ordering::SeqCst) {
            self.execute(s, r)
        } else {
            push_request(s, r)
        }
    }

    fn execute(&self, s: &Store, r: Request) -> Result<()> {
        match r {
            Request::Oneshot { track_id, volume } => {
                // Soundboard one-shot: ducks the music like a soundbite, but
                // plays at once (no queue), so presses can overlap.
                let t = s.music_track_get(&track_id)?;
                let (kind, src) = src_of(&t);
                let osid = format!("os{}", self.music().os_seq.fetch_add(1, Ordering::SeqCst) + 1);
                let was_ducked = {
                    let mut rt = self.rt();
                    let was = rt.ducked();
                    rt.board_oneshots.insert(osid.clone());
                    was
                };
                if !was_ducked {
                    self.sync_inner(s)?;
                }
                self.engine_cmd(json!({ "action": "play_oneshot", "osid": osid, "kind": kind, "src": src, "volume": clamp_vol(volume) }));
                // Fallback if the engine never reports the end (as for soundbites).
                if let Ok(h) = tokio::runtime::Handle::try_current() {
                    let svc = self.clone();
                    let wait = t.duration.map_or(30.0, |d| d + 1.0);
                    h.spawn(async move {
                        tokio::time::sleep(Duration::from_secs_f64(wait)).await;
                        if svc.rt().board_oneshots.remove(&osid) {
                            let store = svc.store();
                            let _ = svc.sync_inner(&store);
                            let _ = store.record_change("music_live", "live", "update");
                        }
                    });
                }
            }
            Request::Sfx { track_id, volume } => self.sfx_enqueue(track_id, volume),
            Request::Preview { track_id, seconds } => {
                let t = s.music_track_get(&track_id)?;
                let (kind, src) = src_of(&t);
                self.rt().previewing = true;
                self.engine_cmd(json!({ "action": "play_preview", "kind": kind, "src": src, "volume": 80, "seconds": seconds }));
            }
            Request::StopPreview => {
                self.rt().previewing = false;
                self.engine_cmd(json!({ "action": "stop_preview" }));
            }
            Request::ToggleVideo => self.engine_cmd(json!({ "action": "toggle_video" })),
            Request::Restart { button_id } => {
                // stop now; sync replays it from the start
                self.rt().engine.remove(&button_id);
                self.engine_cmd(json!({ "action": "stop_channel", "channel_id": button_id }));
            }
            Request::StopAll => self.engine_stop_all(),
        }
        Ok(())
    }

    // ── SFX queue + ducking ────────────────────────────────────────────

    fn sfx_enqueue(&self, track_id: String, volume: i64) {
        let gen_now = self.music().generation.load(Ordering::SeqCst);
        let tx = {
            let mut rt = self.rt();
            if rt.sfx_tx.is_none() {
                let (tx, rx) = mpsc::channel(SFX_QUEUE_MAX);
                rt.sfx_tx = Some(tx);
                let svc = self.clone();
                if let Ok(h) = tokio::runtime::Handle::try_current() {
                    h.spawn(async move { svc.sfx_worker(rx).await });
                }
            }
            rt.sfx_tx.clone()
        };
        if let Some(tx) = tx
            && tx.try_send((gen_now, track_id, volume)).is_ok()
        {
            self.music().sfx_len.fetch_add(1, Ordering::SeqCst);
        }
        // extra requests beyond 12 are dropped (Python)
    }

    async fn sfx_worker(self, mut rx: mpsc::Receiver<(u64, String, i64)>) {
        while let Some((gen_at, track_id, volume)) = rx.recv().await {
            if gen_at != self.music().generation.load(Ordering::SeqCst) {
                self.sfx_done();
                continue;
            }
            let store = self.store();
            let Ok(track) = store.music_track_get(&track_id) else {
                self.sfx_done();
                continue;
            };
            let was_ducked = {
                let mut rt = self.rt();
                let was = rt.ducked();
                rt.sfx_ducked = true;
                was
            };
            if !was_ducked {
                let _ = self.sync_inner(&store);
            }
            let osid = format!("os{}", self.music().os_seq.fetch_add(1, Ordering::SeqCst) + 1);
            let (tx, rx_end) = oneshot::channel();
            self.rt().waiters.insert(osid.clone(), tx);
            let (kind, src) = src_of(&track);
            self.engine_cmd(json!({ "action": "play_oneshot", "osid": osid, "kind": kind, "src": src, "volume": clamp_vol(volume) }));
            let wait = track.duration.map_or(30.0, |d| d + 1.0);
            let _ = tokio::time::timeout(Duration::from_secs_f64(wait), rx_end).await;
            self.rt().waiters.remove(&osid);
            if self.sfx_done() == 0 {
                self.rt().sfx_ducked = false;
                let _ = self.sync_inner(&store);
            }
            let _ = store.record_change("music_live", "live", "update");
        }
    }

    /// One SFX finished or was dropped; returns how many are left. Saturates:
    /// a global stop resets the count while a clip is still in flight.
    fn sfx_done(&self) -> usize {
        let prev = self
            .music()
            .sfx_len
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| Some(v.saturating_sub(1)))
            .unwrap_or(0);
        prev.saturating_sub(1)
    }

    // ── engine sync ────────────────────────────────────────────────────

    /// Applies the persisted music state (and queued requests) to the engine.
    /// A no-op in processes without an engine.
    pub async fn music_sync(&self) -> Result<()> {
        let svc = self.clone();
        self.blocking(move |s| svc.sync_inner(s)).await
    }

    fn sync_inner(&self, s: &Store) -> Result<()> {
        if !self.music().attached.load(Ordering::SeqCst) {
            return Ok(());
        }
        let reqs: Vec<Request> = s.workspace_get(REQ_KEY)?.unwrap_or_default();
        if !reqs.is_empty() {
            s.workspace_set(REQ_KEY, &Vec::<Request>::new())?;
            for r in reqs {
                if let Err(e) = self.execute(s, r) {
                    tracing::warn!(%e, "music request failed");
                }
            }
        }
        let meta = load_meta(s)?;
        let channels = s.music_channels()?;
        let ducked = self.rt().ducked();
        let duck = if ducked { meta.duck.factor() } else { 1.0 };
        let master = meta.master_volume as f64 / 100.0;
        let eff = |v: i64, f: f64| ((v as f64) * master * f).round() as i64;
        let mut desired: Vec<Desired> = Vec::new();
        for c in &channels {
            desired.push(Desired {
                id: c.id.clone(),
                track: s.music_track_get(&c.track_id)?,
                volume: eff(c.volume, duck),
                looping: c.is_looping,
                playing: c.is_playing,
            });
        }
        if let Some(p) = &meta.playlist
            && let Some(tid) = p.track_ids.get(p.index)
            && let Ok(t) = s.music_track_get(tid)
        {
            desired.push(Desired {
                id: PLAYLIST_CHANNEL.into(),
                track: t,
                volume: eff(80, 1.0),
                looping: false,
                playing: !p.paused,
            });
        }
        for bid in meta.toggles.iter().chain(&meta.restarts) {
            if let Ok(b) = s.music_button_get(bid)
                && let Some(tid) = &b.track_id
                && let Ok(t) = s.music_track_get(tid)
            {
                desired.push(Desired {
                    id: b.id.clone(),
                    track: t,
                    volume: eff(b.volume, 1.0),
                    looping: b.loop_by_default,
                    playing: true,
                });
            }
        }
        let mut cmds = Vec::new();
        {
            let mut rt = self.rt();
            let gone: Vec<String> = rt
                .engine
                .keys()
                .filter(|k| !desired.iter().any(|d| &d.id == *k))
                .cloned()
                .collect();
            for id in gone {
                rt.engine.remove(&id);
                cmds.push(json!({ "action": "stop_channel", "channel_id": id }));
            }
            for d in &desired {
                let (kind, src) = src_of(&d.track);
                let play = json!({ "action": "play_track", "channel_id": d.id, "kind": kind, "src": src, "volume": d.volume, "loop": d.looping });
                match rt.engine.get(&d.id).cloned() {
                    None => {
                        if d.playing {
                            cmds.push(play);
                            rt.engine.insert(
                                d.id.clone(),
                                EngineCh {
                                    track_id: d.track.id.clone(),
                                    playing: true,
                                    volume: d.volume,
                                    looping: d.looping,
                                },
                            );
                        }
                    }
                    Some(e) if e.track_id != d.track.id => {
                        if d.playing {
                            cmds.push(play);
                            rt.engine.insert(
                                d.id.clone(),
                                EngineCh {
                                    track_id: d.track.id.clone(),
                                    playing: true,
                                    volume: d.volume,
                                    looping: d.looping,
                                },
                            );
                        } else {
                            rt.engine.remove(&d.id);
                            cmds.push(json!({ "action": "stop_channel", "channel_id": d.id }));
                        }
                    }
                    Some(e) => {
                        let mut n = e.clone();
                        if e.playing != d.playing {
                            let a = if d.playing { "resume_channel" } else { "pause_channel" };
                            cmds.push(json!({ "action": a, "channel_id": d.id }));
                            n.playing = d.playing;
                        }
                        if e.volume != d.volume {
                            cmds.push(json!({ "action": "set_volume", "channel_id": d.id, "volume": d.volume, "ramp_ms": RAMP_MS }));
                            n.volume = d.volume;
                        }
                        if e.looping != d.looping {
                            cmds.push(json!({ "action": "set_loop", "channel_id": d.id, "loop": d.looping }));
                            n.looping = d.looping;
                        }
                        rt.engine.insert(d.id.clone(), n);
                    }
                }
            }
        }
        for c in cmds {
            self.engine_cmd(c);
        }
        self.sync_soundbites(s, &meta)?;
        Ok(())
    }

    fn sync_soundbites(&self, s: &Store, meta: &LiveMeta) -> Result<()> {
        let bites = if meta.soundbites_running { s.music_soundbites()? } else { vec![] };
        let key = serde_json::to_string(&bites).map_err(internal)?;
        let mut rt = self.rt();
        if rt.soundbite_key == key {
            return Ok(());
        }
        for t in rt.soundbite_tasks.drain(..) {
            t.abort();
        }
        rt.soundbite_key = key;
        let Ok(h) = tokio::runtime::Handle::try_current() else { return Ok(()) };
        for b in bites {
            let svc = self.clone();
            rt.soundbite_tasks.push(h.spawn(async move {
                loop {
                    let (lo, hi) = interval(b.min_interval, b.max_interval);
                    tokio::time::sleep(Duration::from_secs_f64(uniform(lo, hi))).await;
                    svc.sfx_enqueue(b.track_id.clone(), b.volume);
                }
            }));
        }
        Ok(())
    }

    /// Events reported by the webview engine.
    pub async fn music_engine_event(&self, event: EngineEvent) -> Result<()> {
        let svc = self.clone();
        self.blocking(move |s| {
            match event {
                EngineEvent::Ready => {
                    let mut rt = svc.rt();
                    rt.engine.clear();
                    rt.soundbite_key.clear();
                    rt.board_oneshots.clear(); // a reloaded engine plays nothing
                }
                EngineEvent::ChannelEnded { channel_id }
                | EngineEvent::ChannelError { channel_id, .. } => {
                    svc.rt().engine.remove(&channel_id);
                    let mut m = load_meta(s)?;
                    if channel_id == PLAYLIST_CHANNEL {
                        if let Some(p) = m.playlist.as_mut() {
                            if p.index + 1 < p.track_ids.len() {
                                p.index += 1;
                                p.title = s.music_track_title(&p.track_ids[p.index]);
                            } else {
                                m.playlist = None;
                            }
                        }
                        save_meta(s, &m)?;
                    } else if m.toggles.contains(&channel_id) || m.restarts.contains(&channel_id) {
                        m.toggles.retain(|x| *x != channel_id);
                        m.restarts.retain(|x| *x != channel_id);
                        save_meta(s, &m)?;
                    } else if let Some(mut c) =
                        s.music_channels()?.into_iter().find(|c| c.id == channel_id)
                    {
                        c.is_playing = false;
                        s.music_channel_update(&c)?;
                    }
                }
                EngineEvent::OneshotEnded { osid } => {
                    if let Some(w) = svc.rt().waiters.remove(&osid) {
                        let _ = w.send(());
                    }
                    // the sync below restores the volume once nothing ducks
                    if svc.rt().board_oneshots.remove(&osid) {
                        s.record_change("music_live", "live", "update")?;
                    }
                }
                EngineEvent::OneshotDuration { .. } => {}
                EngineEvent::TrackDuration { channel_id, seconds } => {
                    let tid = svc.rt().engine.get(&channel_id).map(|e| e.track_id.clone());
                    if let Some(tid) = tid
                        && seconds.is_finite()
                        && seconds > 0.0
                    {
                        s.music_track_set_duration(&tid, seconds)?;
                    }
                }
                EngineEvent::PreviewEnded => {
                    svc.rt().previewing = false;
                    s.record_change("music_live", "live", "update")?;
                }
            }
            svc.sync_inner(s)
        })
        .await
    }

    // ── soundscapes ─────────────────────────────────────────────────────

    pub async fn music_soundscapes_list(&self) -> Result<Vec<MusicSoundscape>> {
        self.blocking(move |s| Ok(s.music_soundscapes(None)?)).await
    }

    fn live_content(
        s: &Store,
    ) -> Result<(Vec<MusicSoundscapeChannel>, Vec<MusicSoundscapeSoundbite>)> {
        let ch = s
            .music_channels()?
            .into_iter()
            .map(|c| MusicSoundscapeChannel {
                track_id: c.track_id,
                title: c.title,
                volume: c.volume,
                is_looping: c.is_looping,
                role: c.role,
            })
            .collect();
        let sb = s
            .music_soundbites()?
            .into_iter()
            .map(|b| MusicSoundscapeSoundbite {
                track_id: b.track_id,
                title: b.title,
                volume: b.volume,
                min_interval: b.min_interval,
                max_interval: b.max_interval,
            })
            .collect();
        Ok((ch, sb))
    }

    /// Saves the live mix as a new soundscape (it becomes the loaded one).
    pub async fn music_soundscape_save(&self, input: SoundscapeInput) -> Result<MusicSoundscape> {
        if input.name.trim().is_empty() {
            return Err(invalid("name is required"));
        }
        self.blocking(move |s| {
            let (ch, sb) = Self::live_content(s)?;
            let id = s.music_soundscape_write(
                None,
                input.name.trim(),
                None,
                input.notes.as_deref().unwrap_or(""),
                &input.tags,
                &ch,
                &sb,
            )?;
            let mut m = load_meta(s)?;
            m.soundscape_id = Some(id.clone());
            save_meta(s, &m)?;
            Ok(s.music_soundscape_get(&id)?)
        })
        .await
    }

    /// Overwrites a soundscape's mix with the live mix.
    pub async fn music_soundscape_update(&self, id: String) -> Result<MusicSoundscape> {
        self.blocking(move |s| {
            let cur = s.music_soundscape_get(&id)?;
            let (ch, sb) = Self::live_content(s)?;
            s.music_soundscape_write(
                Some(&id),
                &cur.name,
                cur.campaign_id.as_deref(),
                &cur.notes,
                &cur.tags,
                &ch,
                &sb,
            )?;
            Ok(s.music_soundscape_get(&id)?)
        })
        .await
    }

    /// Stop all, replace the live mix with the soundscape, play it and start
    /// its soundbites.
    pub async fn music_soundscape_load(&self, id: String) -> Result<MixerState> {
        self.load_soundscape(id, true).await
    }

    async fn load_soundscape(&self, id: String, play: bool) -> Result<MixerState> {
        let svc = self.clone();
        self.blocking(move |s| {
            let sc = s.music_soundscape_get(&id)?;
            if svc.music().attached.load(Ordering::SeqCst) {
                svc.engine_stop_all();
            } else {
                push_request(s, Request::StopAll)?;
            }
            s.music_soundscape_to_live(&id, play)?;
            let m = load_meta(s)?;
            save_meta(
                s,
                &LiveMeta {
                    soundscape_id: Some(id),
                    soundbites_running: play && !sc.soundbites.is_empty(),
                    duck: m.duck,
                    master_volume: m.master_volume,
                    ..Default::default()
                },
            )?;
            svc.sync_inner(s)?;
            svc.state_of(s)
        })
        .await
    }

    /// Pins a soundscape to the top of the list (or unpins it).
    pub async fn music_soundscape_set_pinned(
        &self,
        id: String,
        pinned: bool,
    ) -> Result<MusicSoundscape> {
        self.blocking(move |s| Ok(s.music_soundscape_set_pinned(&id, pinned)?)).await
    }

    pub async fn music_soundscape_delete(&self, id: String) -> Result<()> {
        self.blocking(move |s| {
            s.music_soundscape_delete(&id)?;
            let mut m = load_meta(s)?;
            if m.soundscape_id.as_deref() == Some(&id) {
                m.soundscape_id = None;
                save_meta(s, &m)?;
            }
            Ok(())
        })
        .await
    }

    // ── playlists ───────────────────────────────────────────────────────

    pub async fn music_playlists_list(&self) -> Result<Vec<MusicPlaylist>> {
        self.blocking(move |s| Ok(s.music_playlists(None)?)).await
    }

    pub async fn music_playlist_save(&self, input: PlaylistInput) -> Result<MusicPlaylist> {
        if input.name.trim().is_empty() {
            return Err(invalid("name is required"));
        }
        self.blocking(move |s| {
            for t in &input.track_ids {
                s.music_track_get(t)?;
            }
            let id = input.id.as_deref().filter(|i| !i.is_empty());
            // campaigns are no longer edited; a replaced playlist keeps its hidden link
            let campaign =
                id.and_then(|i| s.music_playlist_get(i).ok()).and_then(|p| p.campaign_id);
            Ok(s.music_playlist_save(id, input.name.trim(), campaign.as_deref(), &input.track_ids)?)
        })
        .await
    }

    pub async fn music_playlist_delete(&self, id: String) -> Result<()> {
        self.blocking(move |s| {
            s.music_playlist_delete(&id)?;
            let mut m = load_meta(s)?;
            if m.playlist.as_ref().is_some_and(|p| p.playlist_id == id) {
                m.playlist = None;
                save_meta(s, &m)?;
            }
            Ok(())
        })
        .await?;
        self.music_sync().await
    }

    /// Plays a playlist on the fixed `playlist` channel (volume 80, no loop),
    /// auto-advancing when a track ends.
    pub async fn music_playlist_play(
        &self,
        id: String,
        index: Option<usize>,
    ) -> Result<MixerState> {
        let svc = self.clone();
        self.live(move |s| {
            let p = s.music_playlist_get(&id)?;
            if p.items.is_empty() {
                return Err(invalid("the playlist is empty"));
            }
            let index = index.unwrap_or(0).min(p.items.len() - 1);
            let mut m = load_meta(s)?;
            m.playlist = Some(PlaylistPlayback {
                playlist_id: p.id,
                name: p.name,
                index,
                track_ids: p.items.iter().map(|i| i.track_id.clone()).collect(),
                title: p.items[index].title.clone(),
                paused: false,
            });
            svc.rt().engine.remove(PLAYLIST_CHANNEL);
            save_meta(s, &m)
        })
        .await
    }

    pub async fn music_playlist_control(&self, action: String) -> Result<MixerState> {
        let svc = self.clone();
        self.live(move |s| {
            let mut m = load_meta(s)?;
            let Some(p) = m.playlist.as_mut() else { return Ok(()) };
            match action.as_str() {
                "prev" | "next" => {
                    let i = if action == "next" {
                        (p.index + 1).min(p.track_ids.len() - 1)
                    } else {
                        p.index.saturating_sub(1)
                    };
                    p.index = i;
                    p.paused = false;
                    p.title = s.music_track_title(&p.track_ids[i]);
                    svc.rt().engine.remove(PLAYLIST_CHANNEL);
                    svc.engine_cmd(
                        json!({ "action": "stop_channel", "channel_id": PLAYLIST_CHANNEL }),
                    );
                }
                "pause" => p.paused = true,
                "resume" => p.paused = false,
                "stop" => m.playlist = None,
                other => return Err(invalid(format!("unknown playlist action {other:?}"))),
            }
            save_meta(s, &m)
        })
        .await
    }

    // ── soundboards ─────────────────────────────────────────────────────

    pub async fn music_soundboards_list(&self) -> Result<Vec<MusicSoundboard>> {
        self.blocking(move |s| Ok(s.music_soundboards(None)?)).await
    }

    pub async fn music_soundboard_save(&self, input: SoundboardInput) -> Result<MusicSoundboard> {
        if input.name.trim().is_empty() {
            return Err(invalid("name is required"));
        }
        if input.buttons.iter().any(|b| b.track_id.as_deref().is_none_or(str::is_empty)) {
            return Err(invalid("every button needs a track"));
        }
        if let Some(b) = input
            .buttons
            .iter()
            .find(|b| !matches!(b.play_mode.as_str(), "oneshot" | "restart" | "toggle"))
        {
            return Err(invalid(format!("unknown play mode {:?}", b.play_mode)));
        }
        self.blocking(move |s| {
            let id = input.id.as_deref().filter(|i| !i.is_empty());
            // campaigns are no longer edited; a replaced board keeps its hidden link
            let campaign = match id {
                Some(i) => s
                    .music_soundboards(None)?
                    .into_iter()
                    .find(|b| b.id == i)
                    .and_then(|b| b.campaign_id),
                None => None,
            };
            Ok(s.music_soundboard_save(
                id,
                input.name.trim(),
                campaign.as_deref(),
                input.notes.as_deref().unwrap_or(""),
                &input.buttons,
            )?)
        })
        .await
    }

    pub async fn music_soundboard_delete(&self, id: String) -> Result<()> {
        self.blocking(move |s| Ok(s.music_soundboard_delete(&id)?)).await
    }

    /// oneshot: fire-and-forget clip (ducks the music, no queue); restart: (re)start
    /// the button's channel; toggle: start or stop it.
    pub async fn music_soundboard_press(&self, button_id: String) -> Result<MixerState> {
        let svc = self.clone();
        self.blocking(move |s| {
            let b = s.music_button_get(&button_id)?;
            let tid = b.track_id.clone().ok_or_else(|| invalid("the button has no track"))?;
            let mut m = load_meta(s)?;
            match b.play_mode.as_str() {
                "oneshot" => {
                    svc.request(s, Request::Oneshot { track_id: tid, volume: b.volume })?
                }
                "restart" => {
                    m.toggles.retain(|x| *x != b.id);
                    if !m.restarts.contains(&b.id) {
                        m.restarts.push(b.id.clone());
                    }
                    save_meta(s, &m)?;
                    svc.request(s, Request::Restart { button_id: b.id.clone() })?;
                }
                _ => {
                    if m.toggles.contains(&b.id) {
                        m.toggles.retain(|x| *x != b.id);
                    } else {
                        m.restarts.retain(|x| *x != b.id);
                        m.toggles.push(b.id.clone());
                    }
                    save_meta(s, &m)?;
                }
            }
            svc.sync_inner(s)?;
            svc.state_of(s)
        })
        .await
    }

    // ── .dmix files ────────────────────────────────────────────────────

    /// Writes the live mix (or a saved soundscape) as a portable `.dmix`.
    pub async fn music_mix_export(
        &self,
        path: PathBuf,
        soundscape_id: Option<String>,
    ) -> Result<PathBuf> {
        self.blocking(move |s| {
            let meta = load_meta(s)?;
            let (name, ch, sb) = match &soundscape_id {
                Some(id) => {
                    let sc = s.music_soundscape_get(id)?;
                    (sc.name, sc.channels, sc.soundbites)
                }
                None => {
                    let (ch, sb) = Self::live_content(s)?;
                    let name = meta
                        .soundscape_id
                        .as_ref()
                        .and_then(|i| s.music_soundscape_get(i).ok().map(|x| x.name))
                        .or_else(|| path.file_stem().map(|n| n.to_string_lossy().into_owned()))
                        .unwrap_or_else(|| "Mix".into());
                    (name, ch, sb)
                }
            };
            if ch.is_empty() && sb.is_empty() {
                return Err(invalid("the mix is empty"));
            }
            let track_json = |tid: &str| -> Result<Value> {
                let t = s.music_track_get(tid)?;
                let mut v = json!({
                    "source": t.source, "url": t.url, "title": t.title, "notes": t.notes,
                    "loop_by_default": t.loop_by_default, "duration": t.duration,
                });
                if let Some(h) = &t.blob_hash {
                    let mime = s.blob_meta(h)?.mime;
                    v["mime"] = json!(mime);
                    v["file_name"] = json!(format!("{}.{}", t.title, ext_for_mime(&mime)));
                    v["data_b64"] = json!(base64::engine::general_purpose::STANDARD.encode(s.read_blob(h)?));
                }
                Ok(v)
            };
            let channels = ch
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    Ok(json!({ "track": track_json(&c.track_id)?, "volume": c.volume, "is_looping": c.is_looping, "role": c.role, "position": i }))
                })
                .collect::<Result<Vec<_>>>()?;
            let soundbites = sb
                .iter()
                .map(|b| {
                    Ok(json!({ "track": track_json(&b.track_id)?, "volume": b.volume, "min_interval": b.min_interval, "max_interval": b.max_interval }))
                })
                .collect::<Result<Vec<_>>>()?;
            let doc = json!({
                "format": "sleuth-dmix",
                "version": 1,
                "name": name,
                "master_volume": meta.master_volume,
                "channels": channels,
                "soundbites": soundbites,
                "ducking": { "mode": meta.duck, "factor": meta.duck.factor() },
            });
            std::fs::write(&path, serde_json::to_vec_pretty(&doc).map_err(internal)?)?;
            Ok(path)
        })
        .await
    }

    /// Imports a `.dmix`: reuses tracks (same YouTube URL / identical upload)
    /// or creates them, saves (or overwrites) a soundscape named after the
    /// file, loads it into the mixer and starts it when `play`.
    pub async fn music_mix_import(&self, path: PathBuf, play: bool) -> Result<MixImportResult> {
        let (sc_id, name, created, reused) = self
            .blocking(move |s| {
                let raw = std::fs::read(&path)?;
                let doc: Value = serde_json::from_slice(&raw)
                    .map_err(|e| invalid(format!("not a .dmix file: {e}")))?;
                if doc["format"] != "sleuth-dmix" {
                    return Err(invalid("not a .dmix file"));
                }
                if doc["version"].as_i64().unwrap_or(0) > 1 {
                    return Err(invalid("this .dmix was made by a newer version of the app"));
                }
                let name = path
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .or_else(|| doc["name"].as_str().map(String::from))
                    .unwrap_or_else(|| "Mix".into());
                let (mut created, mut reused) = (0, 0);
                let mut track = |v: &Value| -> Result<String> {
                    let source = v["source"].as_str().unwrap_or("youtube").to_string();
                    let (url, blob) = if source == "youtube" {
                        (v["url"].as_str().map(String::from), None)
                    } else {
                        let data = base64::engine::general_purpose::STANDARD
                            .decode(v["data_b64"].as_str().unwrap_or(""))
                            .map_err(|e| invalid(format!("bad embedded audio: {e}")))?;
                        let mime = v["mime"].as_str().unwrap_or("application/octet-stream");
                        (None, Some(s.put_blob(&data, mime, None)?.hash))
                    };
                    if let Some(id) = s.music_track_find(url.as_deref(), blob.as_deref())? {
                        reused += 1;
                        return Ok(id);
                    }
                    created += 1;
                    let row = MusicTrackRow {
                        source,
                        url,
                        blob_hash: blob,
                        title: v["title"].as_str().unwrap_or("Track").to_string(),
                        notes: v["notes"].as_str().unwrap_or("").to_string(),
                        loop_by_default: v["loop_by_default"].as_bool().unwrap_or(true),
                        duration: v["duration"].as_f64(),
                        ..Default::default()
                    };
                    Ok(s.music_track_save(None, &row)?.id)
                };
                let mut ch = Vec::new();
                for c in doc["channels"].as_array().cloned().unwrap_or_default() {
                    let tid = track(&c["track"])?;
                    ch.push(MusicSoundscapeChannel {
                        title: String::new(),
                        track_id: tid,
                        volume: clamp_vol(c["volume"].as_i64().unwrap_or(80)),
                        is_looping: c["is_looping"].as_bool().unwrap_or(true),
                        role: check_role(c["role"].as_str().unwrap_or("ambient"))
                            .unwrap_or_else(|_| "ambient".into()),
                    });
                }
                let mut sb = Vec::new();
                for b in doc["soundbites"].as_array().cloned().unwrap_or_default() {
                    let tid = track(&b["track"])?;
                    let (lo, hi) = interval(
                        b["min_interval"].as_f64().unwrap_or(30.0),
                        b["max_interval"].as_f64().unwrap_or(90.0),
                    );
                    sb.push(MusicSoundscapeSoundbite {
                        title: String::new(),
                        track_id: tid,
                        volume: clamp_vol(b["volume"].as_i64().unwrap_or(80)),
                        min_interval: lo,
                        max_interval: hi,
                    });
                }
                let existing =
                    s.music_soundscapes(None)?.into_iter().find(|x| x.name == name).map(|x| x.id);
                let id =
                    s.music_soundscape_write(existing.as_deref(), &name, None, "", &[], &ch, &sb)?;
                let mut m = load_meta(s)?;
                if let Some(v) = doc["master_volume"].as_i64() {
                    m.master_volume = clamp_vol(v);
                }
                if let Ok(d) = serde_json::from_value::<DuckMode>(doc["ducking"]["mode"].clone()) {
                    m.duck = d;
                }
                save_meta(s, &m)?;
                Ok((id, name, created, reused))
            })
            .await?;
        let state = self.load_soundscape(sc_id.clone(), play).await?;
        Ok(MixImportResult {
            soundscape_id: sc_id,
            name,
            tracks_created: created,
            tracks_reused: reused,
            state,
        })
    }

    // ── legacy ──────────────────────────────────────────────────────────

    /// Imports `<data dir>/legacy/music.json` (written by the legacy importer).
    /// Idempotent: tracks are matched by their legacy id.
    pub async fn music_import_legacy(&self) -> Result<MusicImportReport> {
        self.blocking(|s| {
            let path = s.root().join("legacy").join("music.json");
            if !path.exists() {
                return Err(invalid("no legacy music data — run the legacy import first"));
            }
            let dump: Value = serde_json::from_slice(&std::fs::read(&path)?).map_err(internal)?;
            let rows = |t: &str| dump[t].as_array().cloned().unwrap_or_default();
            let str_of = |v: &Value, k: &str| v[k].as_str().unwrap_or("").to_string();
            let mut rep = MusicImportReport::default();
            let known_campaigns: Vec<String> =
                s.list_campaigns()?.into_iter().map(|c| c.id).collect();
            let known_sessions: Vec<String> =
                s.list_sessions(None)?.into_iter().map(|x| x.id).collect();
            let tag_names: HashMap<String, String> =
                rows("music_tags").iter().map(|t| (str_of(t, "id"), str_of(t, "name"))).collect();
            let mut map: HashMap<String, String> = HashMap::new();
            for t in rows("music_tracks") {
                let lid = str_of(&t, "id");
                if let Some(id) = s.music_track_by_legacy(&lid)? {
                    map.insert(lid, id);
                    continue;
                }
                let url = str_of(&t, "url");
                let (source, url, blob) = if let Some(p) = url.strip_prefix("local:") {
                    match std::fs::read(p) {
                        Ok(bytes) => {
                            let ext = Path::new(p)
                                .extension()
                                .map(|e| e.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            ("upload", None, Some(s.put_blob(&bytes, media_mime(&ext), None)?.hash))
                        }
                        Err(_) => {
                            rep.skipped.push(format!("{} (missing file {p})", str_of(&t, "title")));
                            continue;
                        }
                    }
                } else {
                    ("youtube", Some(url), None)
                };
                let links = |table: &str, col: &str| -> Vec<String> {
                    rows(table)
                        .iter()
                        .filter(|r| str_of(r, "track_id") == lid)
                        .map(|r| str_of(r, col))
                        .collect()
                };
                let mut campaigns = links("music_track_campaigns", "campaign_id");
                let primary = str_of(&t, "campaign_id");
                if !primary.is_empty() && !campaigns.contains(&primary) {
                    campaigns.push(primary);
                }
                campaigns.retain(|c| known_campaigns.contains(c));
                let mut sessions = links("music_track_sessions", "session_id");
                let sid = str_of(&t, "session_id");
                if !sid.is_empty() && !sessions.contains(&sid) {
                    sessions.push(sid);
                }
                sessions.retain(|x| known_sessions.contains(x));
                let tags = links("music_track_tags", "tag_id")
                    .iter()
                    .filter_map(|i| tag_names.get(i).cloned())
                    .collect();
                let row = MusicTrackRow {
                    source: source.into(),
                    url,
                    blob_hash: blob,
                    title: str_of(&t, "title"),
                    notes: str_of(&t, "notes"),
                    loop_by_default: t["loop_by_default"].as_i64().unwrap_or(1) != 0,
                    duration: t["duration"].as_f64(),
                    campaign_ids: campaigns,
                    session_ids: sessions,
                    tags,
                    legacy_id: Some(lid.clone()),
                };
                map.insert(lid, s.music_track_save(None, &row)?.id);
                rep.tracks += 1;
            }
            let camp =
                |v: &Value| Some(str_of(v, "campaign_id")).filter(|c| known_campaigns.contains(c));
            let existing_sc: Vec<String> =
                s.music_soundscapes(None)?.into_iter().map(|x| x.name).collect();
            for sc in rows("music_soundscapes") {
                let sid = str_of(&sc, "id");
                if existing_sc.contains(&str_of(&sc, "name")) {
                    continue;
                }
                let ch: Vec<_> = rows("music_soundscape_channels")
                    .iter()
                    .filter(|c| str_of(c, "soundscape_id") == sid)
                    .filter_map(|c| {
                        Some(MusicSoundscapeChannel {
                            track_id: map.get(&str_of(c, "track_id"))?.clone(),
                            title: String::new(),
                            volume: c["volume"].as_i64().unwrap_or(80),
                            is_looping: c["is_looping"].as_i64().unwrap_or(1) != 0,
                            role: str_of(c, "role"),
                        })
                    })
                    .collect();
                let sb: Vec<_> = rows("music_soundscape_soundbites")
                    .iter()
                    .filter(|c| str_of(c, "soundscape_id") == sid)
                    .filter_map(|c| {
                        Some(MusicSoundscapeSoundbite {
                            track_id: map.get(&str_of(c, "track_id"))?.clone(),
                            title: String::new(),
                            volume: c["volume"].as_i64().unwrap_or(80),
                            min_interval: c["min_interval"].as_f64().unwrap_or(30.0),
                            max_interval: c["max_interval"].as_f64().unwrap_or(90.0),
                        })
                    })
                    .collect();
                let tags: Vec<String> = rows("music_soundscape_tags")
                    .iter()
                    .filter(|r| str_of(r, "soundscape_id") == sid)
                    .filter_map(|r| tag_names.get(&str_of(r, "tag_id")).cloned())
                    .collect();
                s.music_soundscape_write(
                    None,
                    &str_of(&sc, "name"),
                    camp(&sc).as_deref(),
                    &str_of(&sc, "notes"),
                    &tags,
                    &ch,
                    &sb,
                )?;
                rep.soundscapes += 1;
            }
            let existing_pl: Vec<String> =
                s.music_playlists(None)?.into_iter().map(|x| x.name).collect();
            for p in rows("music_playlists") {
                if existing_pl.contains(&str_of(&p, "name")) {
                    continue;
                }
                let pid = str_of(&p, "id");
                let mut items: Vec<(i64, String)> = rows("music_playlist_tracks")
                    .iter()
                    .filter(|r| str_of(r, "playlist_id") == pid)
                    .filter_map(|r| {
                        Some((
                            r["position"].as_i64().unwrap_or(0),
                            map.get(&str_of(r, "track_id"))?.clone(),
                        ))
                    })
                    .collect();
                items.sort_by_key(|i| i.0);
                let ids: Vec<String> = items.into_iter().map(|i| i.1).collect();
                s.music_playlist_save(None, &str_of(&p, "name"), camp(&p).as_deref(), &ids)?;
                rep.playlists += 1;
            }
            let existing_sb: Vec<String> =
                s.music_soundboards(None)?.into_iter().map(|x| x.name).collect();
            for b in rows("music_soundboards") {
                if existing_sb.contains(&str_of(&b, "name")) {
                    continue;
                }
                let bid = str_of(&b, "id");
                let mut buttons: Vec<MusicSoundboardButton> = rows("music_soundboard_buttons")
                    .iter()
                    .filter(|r| str_of(r, "soundboard_id") == bid)
                    .filter_map(|r| {
                        Some(MusicSoundboardButton {
                            id: String::new(),
                            track_id: Some(map.get(&str_of(r, "track_id"))?.clone()),
                            label: str_of(r, "label"),
                            icon: str_of(r, "icon"),
                            color: str_of(r, "color"),
                            position: r["position"].as_i64().unwrap_or(0),
                            loop_by_default: r["loop_by_default"].as_i64().unwrap_or(0) != 0,
                            volume: 80,
                            play_mode: Some(str_of(r, "play_mode"))
                                .filter(|m| !m.is_empty())
                                .unwrap_or_else(|| "oneshot".into()),
                            hotkey: None,
                        })
                    })
                    .collect();
                buttons.sort_by_key(|b| b.position);
                s.music_soundboard_save(
                    None,
                    &str_of(&b, "name"),
                    camp(&b).as_deref(),
                    &str_of(&b, "notes"),
                    &buttons,
                )?;
                rep.soundboards += 1;
            }
            Ok(rep)
        })
        .await
    }
}

fn push_request(s: &Store, r: Request) -> Result<()> {
    let mut q: Vec<Request> = s.workspace_get(REQ_KEY)?.unwrap_or_default();
    q.push(r);
    s.workspace_set(REQ_KEY, &q)?;
    s.record_change("music_live", "requests", "update")?;
    Ok(())
}

#[cfg(test)]
#[path = "music_tests.rs"]
mod tests;
