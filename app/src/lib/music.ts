// Music: typed contract between the UI, the audio engine and the Rust service.
// Spec: docs/features/music/*.md.
//
// Architecture
// - The Rust `service::music` owns all state (library, live mix, soundscapes,
//   playlists, soundboards, ducking, the SFX queue and the soundbite timers).
// - Audio is played by a hidden, always-mounted <MusicEngine> component in the
//   webview (Web Audio: one GainNode per channel, YouTube IFrame players for
//   YouTube tracks). The engine is dumb: it executes `EngineCommand`s and
//   reports `EngineEvent`s back with `music.engineEvent(...)`.
// - Commands arrive as service events `{ type: 'music_engine', command }` on the
//   regular event channel (`api.onEvent`), see `onEngineCommand` below.
// - Live state changes are announced as `store_changed` with entity
//   `music_live`; the UI then refetches `music.state()`. In the app, call
//   `music.sync()` on such events too (it applies changes made by another
//   process, e.g. the standalone MCP server, to the engine).
// - On mount the engine must call `music.engineEvent({ event: 'ready' })`; the
//   service then replays the full desired state (play_track for every playing
//   channel, …).

import { getBackend, isTauri } from './api';
import type { ServiceEvent } from './types';

// ── library ──────────────────────────────────────────────────────────────

export type TrackSource = 'youtube' | 'upload';

export interface Track {
  id: string;
  source: TrackSource;
  /** YouTube URL (null for uploads). */
  url: string | null;
  /** Upload blob hash (null for YouTube). Play it with `mediaUrl(blob_hash)`. */
  blob_hash: string | null;
  title: string;
  notes: string;
  loop_by_default: boolean;
  /** Seconds, if known (YouTube scrape, or reported by the engine). */
  duration: number | null;
  added_at: string;
  favorite: boolean;
  campaign_ids: string[];
  session_ids: string[];
  tags: string[];
}

/** Create/update payload. For uploads pass `file_path` (a new file) — when
 *  updating an upload without `file_path` the existing file is kept. */
export interface TrackInput {
  source: TrackSource;
  url?: string | null;
  file_path?: string | null;
  title: string;
  notes?: string;
  loop_by_default?: boolean;
  duration?: number | null;
  tags?: string[];
}

export interface TrackQuery {
  /** Tags combined with AND. */
  tags?: string[];
  /** Live title search. */
  text?: string;
  favorites_only?: boolean;
  /** true: only looping tracks; false: only one-shots. */
  looping?: boolean | null;
  /** Seconds, inclusive. Tracks of unknown length are left out when set. */
  min_duration?: number | null;
  max_duration?: number | null;
}

export interface TrackMeta {
  title: string | null;
  duration: number | null;
}

/** Media file extensions accepted for uploads. */
export const MEDIA_EXTENSIONS = [
  'mp3', 'm4a', 'aac', 'ogg', 'oga', 'opus', 'wav', 'flac', 'weba', 'webm', 'mp4', 'm4v', 'mov', 'mkv',
];

/** "M:SS" or "H:MM:SS" (Python track table). */
export function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds)) return '';
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

// ── live mix ─────────────────────────────────────────────────────────────

export type ChannelRole = 'music' | 'ambient' | 'sfx';
export type DuckMode = 'off' | 'soft' | 'strong';
/** Duck factors: soft ×0.5, strong ×0.25. */
export const DUCK_FACTORS: Record<DuckMode, number> = { off: 1, soft: 0.5, strong: 0.25 };

export interface Channel {
  id: string;
  track_id: string;
  title: string;
  /** 0–100, the base volume (before master and ducking). */
  volume: number;
  is_looping: boolean;
  is_playing: boolean;
  role: ChannelRole;
  position: number;
}

export interface ChannelPatch {
  volume?: number;
  is_looping?: boolean;
  is_playing?: boolean;
  role?: ChannelRole;
}

export interface Soundbite {
  id: string;
  track_id: string;
  title: string;
  volume: number;
  /** Seconds, 1–3600; min ≤ max (swapped if needed). */
  min_interval: number;
  max_interval: number;
}

export interface SoundbitePatch {
  volume?: number;
  min_interval?: number;
  max_interval?: number;
}

export interface PlaylistPlayback {
  playlist_id: string;
  name: string;
  /** 0-based index into `track_ids`. */
  index: number;
  track_ids: string[];
  title: string;
  paused: boolean;
}

export interface MixerState {
  channels: Channel[];
  soundbites: Soundbite[];
  soundbites_running: boolean;
  /** The loaded soundscape, if any. */
  soundscape_id: string | null;
  soundscape_name: string | null;
  duck: DuckMode;
  /** True while an SFX clip from the queue is playing (music is ducked). */
  ducked: boolean;
  /** 0–100, applied to every channel. */
  master_volume: number;
  playlist: PlaylistPlayback | null;
  /** Soundboard toggle buttons currently playing (button ids). */
  toggles_playing: string[];
  previewing: boolean;
  sfx_queue: number;
  /** False when no audio engine is attached (e.g. the standalone MCP server). */
  engine_attached: boolean;
}

// ── soundscapes, playlists, soundboards ──────────────────────────────────

export interface SoundscapeChannel {
  track_id: string;
  title: string;
  volume: number;
  is_looping: boolean;
  role: ChannelRole;
}

export interface SoundscapeSoundbite {
  track_id: string;
  title: string;
  volume: number;
  min_interval: number;
  max_interval: number;
}

export interface Soundscape {
  id: string;
  name: string;
  /** Pinned soundscapes are listed first. */
  pinned: boolean;
  campaign_id: string | null;
  notes: string;
  created_at: string;
  tags: string[];
  channels: SoundscapeChannel[];
  soundbites: SoundscapeSoundbite[];
}

export interface SoundscapeInput {
  name: string;
  notes?: string;
  tags?: string[];
}

export interface PlaylistItem {
  /** Item id (a track may appear more than once). */
  id: string;
  track_id: string;
  title: string;
}

export interface Playlist {
  id: string;
  name: string;
  campaign_id: string | null;
  created_at: string;
  items: PlaylistItem[];
}

export interface PlaylistInput {
  /** Omit to create. */
  id?: string | null;
  name: string;
  /** Ordered; duplicates allowed. */
  track_ids: string[];
}

export type PlaylistAction = 'prev' | 'next' | 'pause' | 'resume' | 'stop';

export type PlayMode = 'oneshot' | 'restart' | 'toggle';
export const PLAY_MODE_SYMBOLS: Record<PlayMode, string> = { oneshot: '1×', restart: '↺→', toggle: '↔' };
/** Python `BUTTON_COLORS` swatches. */
export const BUTTON_COLORS = ['#4a4a8a', '#8a4a4a', '#4a8a4a', '#8a7a3a', '#3a7a8a', '#7a3a8a'];

export interface SoundboardButton {
  /** Stable id (omit/empty for a new button). Also the channel id for restart/toggle. */
  id: string;
  track_id: string | null;
  label: string;
  icon: string;
  color: string;
  position: number;
  loop_by_default: boolean;
  /** 0–100, default 80. */
  volume: number;
  play_mode: PlayMode;
  /** Optional key, e.g. "1".."9". */
  hotkey: string | null;
}

export interface Soundboard {
  id: string;
  name: string;
  campaign_id: string | null;
  notes: string;
  created_at: string;
  buttons: SoundboardButton[];
}

export interface SoundboardInput {
  id?: string | null;
  name: string;
  notes?: string;
  /** Rows without a track are rejected. */
  buttons: SoundboardButton[];
}

/** Result of importing a `.dmix` file (see docs/features/music/dmix-format.md). */
export interface MixImportResult {
  soundscape_id: string;
  name: string;
  tracks_created: number;
  tracks_reused: number;
  state: MixerState;
}

/** Payload of the Tauri event `music://dmix-opened` (a .dmix was double-clicked;
 *  it is already imported and playing — navigate to the Mixer). */
export interface DmixOpened {
  soundscape_id: string;
  name: string;
}
export const DMIX_OPENED_EVENT = 'music://dmix-opened';
export const DMIX_EXTENSION = 'dmix';

export interface MusicImportReport {
  tracks: number;
  soundscapes: number;
  playlists: number;
  soundboards: number;
  skipped: string[];
}

// ── engine protocol ──────────────────────────────────────────────────────

/** `kind: 'youtube'` → `src` is a YouTube URL; `kind: 'file'` → `src` is a blob
 *  hash, play `mediaUrl(src)`. Volumes are effective 0–100 (base × master ×
 *  duck), ramp them over `ramp_ms` with the channel's GainNode. */
export type EngineCommand =
  | { action: 'play_track'; channel_id: string; kind: 'youtube' | 'file'; src: string; volume: number; loop: boolean }
  | { action: 'pause_channel'; channel_id: string }
  | { action: 'resume_channel'; channel_id: string }
  | { action: 'stop_channel'; channel_id: string }
  | { action: 'set_volume'; channel_id: string; volume: number; ramp_ms: number }
  | { action: 'set_loop'; channel_id: string; loop: boolean }
  /** Stops and destroys every channel, one-shot and the preview. */
  | { action: 'stop_all' }
  /** Fire-and-forget clip; must answer `oneshot_ended` (also on error). */
  | { action: 'play_oneshot'; osid: string; kind: 'youtube' | 'file'; src: string; volume: number }
  /** Single preview slot; the engine stops it itself after `seconds`. */
  | { action: 'play_preview'; kind: 'youtube' | 'file'; src: string; volume: number; seconds: number }
  | { action: 'stop_preview' }
  /** Show/hide the YouTube video panel. */
  | { action: 'toggle_video' };

export type EngineEvent =
  | { event: 'ready' }
  /** Only for non-looping channels. */
  | { event: 'channel_ended'; channel_id: string }
  /** YouTube codes: 2 bad URL, 5 HTML5 error, 100 not found/private, 101/150 embedding disallowed. */
  | { event: 'channel_error'; channel_id: string; code: number }
  | { event: 'oneshot_ended'; osid: string }
  | { event: 'oneshot_duration'; osid: string; seconds: number }
  /** Media duration discovered while loading a channel (stored if unknown). */
  | { event: 'track_duration'; channel_id: string; seconds: number }
  | { event: 'preview_ended' };

/** User-facing message for a YouTube error code (Python `app.py:282`). */
export function youtubeErrorMessage(code: number): string {
  switch (code) {
    case 2: return 'Invalid parameter or bad URL';
    case 5: return 'HTML5 player error';
    case 100: return 'Video not found or private';
    case 101:
    case 150: return 'The owner does not allow embedding';
    default: return `Playback error ${code}`;
  }
}

/** URL of an uploaded media blob (Range-capable `sleuth://media/<hash>`). */
export function mediaUrl(hash: string): string {
  if (!isTauri()) return `mock-media:${hash}`;
  return navigator.userAgent.includes('Windows')
    ? `http://sleuth.localhost/media/${hash}`
    : `sleuth://localhost/media/${hash}`;
}

/** Subscribes the engine to commands from the service. */
export async function onEngineCommand(handler: (c: EngineCommand) => void): Promise<() => void> {
  return getBackend().listen((e: ServiceEvent) => {
    const ev = e as unknown as { type: string; command?: EngineCommand };
    if (ev.type === 'music_engine' && ev.command) handler(ev.command);
  });
}

/** Fires when the OS opened a .dmix with the app (startup or second instance). */
export async function onDmixOpened(handler: (e: DmixOpened) => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const { listen } = await import('@tauri-apps/api/event');
  return listen<DmixOpened>(DMIX_OPENED_EVENT, (e) => handler(e.payload));
}

// ── service calls ────────────────────────────────────────────────────────

const call = <T>(cmd: string, args?: Record<string, unknown>) => getBackend().invoke<T>(cmd, args);

export const music = {
  // library
  tracksList: (query: TrackQuery = {}) => call<Track[]>('music_tracks_list', { query }),
  trackGet: (id: string) => call<Track>('music_track_get', { id }),
  trackCreate: (input: TrackInput) => call<Track>('music_track_create', { input }),
  trackUpdate: (id: string, input: TrackInput) => call<Track>('music_track_update', { id, input }),
  trackSetFavorite: (id: string, favorite: boolean) => call<Track>('music_track_set_favorite', { id, favorite }),
  trackDelete: (id: string) => call<void>('music_track_delete', { id }),
  /** YouTube title (oEmbed) + duration (watch page), best effort. */
  fetchYoutubeMeta: (url: string) => call<TrackMeta>('music_fetch_youtube_meta', { url }),
  /** Title from the file name; duration unknown until played. */
  probeFile: (path: string) => call<TrackMeta>('music_probe_file', { path }),

  // live mix
  state: () => call<MixerState>('music_state'),
  channelAdd: (trackId: string, opts: { role?: ChannelRole; volume?: number; loop?: boolean; play?: boolean } = {}) =>
    call<MixerState>('music_channel_add', { trackId, role: opts.role ?? null, volume: opts.volume ?? null, looping: opts.loop ?? null, play: opts.play ?? true }),
  channelUpdate: (id: string, patch: ChannelPatch) => call<MixerState>('music_channel_update', { id, patch }),
  channelRemove: (id: string) => call<MixerState>('music_channel_remove', { id }),
  /** ▶ All / ⏸ All (pausing also stops the soundbite timers). */
  setAllPlaying: (playing: boolean) => call<MixerState>('music_set_all_playing', { playing }),
  /** Global stop: everything stops, live mix cleared, soundscape unloaded. */
  stopAll: () => call<MixerState>('music_stop_all'),
  soundbiteAdd: (trackId: string, minInterval = 30, maxInterval = 90, volume = 80) =>
    call<MixerState>('music_soundbite_add', { trackId, minInterval, maxInterval, volume }),
  soundbiteUpdate: (id: string, patch: SoundbitePatch) => call<MixerState>('music_soundbite_update', { id, patch }),
  soundbiteRemove: (id: string) => call<MixerState>('music_soundbite_remove', { id }),
  /** Plays a soundbite now through the SFX queue (ducks the music). */
  soundbitePlay: (id: string) => call<MixerState>('music_soundbite_play', { id }),
  setDuck: (mode: DuckMode) => call<MixerState>('music_set_duck', { mode }),
  setMaster: (volume: number) => call<MixerState>('music_set_master', { volume }),
  preview: (trackId: string, seconds = 10) => call<void>('music_preview', { trackId, seconds }),
  stopPreview: () => call<void>('music_stop_preview'),
  toggleVideo: () => call<void>('music_toggle_video'),

  // engine bridge (app only)
  engineEvent: (event: EngineEvent) => call<void>('music_engine_event', { event }),
  sync: () => call<void>('music_sync'),

  // soundscapes
  soundscapesList: () => call<Soundscape[]>('music_soundscapes_list'),
  soundscapeSave: (input: SoundscapeInput) => call<Soundscape>('music_soundscape_save', { input }),
  /** Overwrites the soundscape with the current live mix. */
  soundscapeUpdate: (id: string) => call<Soundscape>('music_soundscape_update', { id }),
  soundscapeLoad: (id: string) => call<MixerState>('music_soundscape_load', { id }),
  soundscapeSetPinned: (id: string, pinned: boolean) => call<Soundscape>('music_soundscape_set_pinned', { id, pinned }),
  soundscapeDelete: (id: string) => call<void>('music_soundscape_delete', { id }),

  // playlists
  playlistsList: () => call<Playlist[]>('music_playlists_list'),
  playlistSave: (input: PlaylistInput) => call<Playlist>('music_playlist_save', { input }),
  playlistDelete: (id: string) => call<void>('music_playlist_delete', { id }),
  playlistPlay: (id: string, index = 0) => call<MixerState>('music_playlist_play', { id, index }),
  playlistControl: (action: PlaylistAction) => call<MixerState>('music_playlist_control', { action }),

  // soundboards
  soundboardsList: () => call<Soundboard[]>('music_soundboards_list'),
  soundboardSave: (input: SoundboardInput) => call<Soundboard>('music_soundboard_save', { input }),
  soundboardDelete: (id: string) => call<void>('music_soundboard_delete', { id }),
  soundboardPress: (buttonId: string) => call<MixerState>('music_soundboard_press', { buttonId }),

  // .dmix mix files (portable: uploads embedded, YouTube kept as URLs)
  /** Writes the live mix (or a saved soundscape) to `path`. */
  mixExport: (path: string, soundscapeId: string | null = null) => call<string>('music_mix_export', { path, soundscapeId }),
  /** Imports a .dmix: reuses/creates tracks, saves a soundscape named after the
   *  file, loads it into the mixer and starts it when `play`. */
  mixImport: (path: string, play = true) => call<MixImportResult>('music_mix_import', { path, play }),

  // legacy
  importLegacy: () => call<MusicImportReport>('music_import_legacy'),
};

export const musicMixExport = music.mixExport;
export const musicMixImport = music.mixImport;
