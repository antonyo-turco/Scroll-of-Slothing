// In-memory music backend for the browser mock (dev, vitest, Playwright).
// Mirrors the Rust `service::music` API shape; engine commands are emitted as
// `music_engine` events so a mounted <MusicEngine> sees them (it cannot play
// `mock-media:` URLs, which is fine for UI work).

import type {
  Channel, ChannelPatch, DuckMode, EngineCommand, EngineEvent, MixerState, Playlist, PlaylistAction,
  PlaylistInput, Soundbite, SoundbitePatch, Soundboard, SoundboardInput, Soundscape, SoundscapeInput,
  Track, TrackInput, TrackQuery,
} from './music';

type Emit = (e: Record<string, unknown>) => void;
type Handler = (args: Record<string, unknown>) => unknown;

const err = (code: string, message: string) => ({ code, message });

export function createMusicMockHandlers(emitRaw: Emit): Record<string, Handler> {
  let seq = 0;
  const id = (p: string) => `mock-${p}-${++seq}`;
  const now = () => new Date().toISOString();
  const tracks: Track[] = [
    { id: 'mock-t1', source: 'youtube', url: 'https://youtu.be/dQw4w9WgXcQ', blob_hash: null, title: 'Tavern Ambience', notes: '', loop_by_default: true, duration: 3600, added_at: now(), favorite: true, category: 'ambient', campaign_ids: [], session_ids: [], tags: ['tavern'] },
    { id: 'mock-t2', source: 'upload', url: null, blob_hash: 'mock-audio-1', title: 'Thunder Clap', notes: '', loop_by_default: false, duration: 4, added_at: now(), favorite: false, category: 'soundbite', campaign_ids: [], session_ids: [], tags: ['sfx'] },
    { id: 'mock-t3', source: 'youtube', url: 'https://youtu.be/abc', blob_hash: null, title: 'Battle Drums', notes: '', loop_by_default: true, duration: 245, added_at: now(), favorite: false, category: 'music', campaign_ids: [], session_ids: [], tags: ['battle'] },
  ];
  let channels: Channel[] = [];
  let soundbites: Soundbite[] = [];
  const soundscapes: Soundscape[] = [];
  const playlists: Playlist[] = [];
  const boards: Soundboard[] = [];
  const live = {
    soundbites_running: false, soundscape_id: null as string | null, duck: 'off' as DuckMode,
    master_volume: 100, playlist: null as MixerState['playlist'], toggles: [] as string[], previewing: false,
  };

  const changed = () => emitRaw({ type: 'store_changed', entity: 'music_live', entity_id: 'live', op: 'update' });
  const engine = (command: EngineCommand) => emitRaw({ type: 'music_engine', command });
  const track = (tid: string) => {
    const t = tracks.find((x) => x.id === tid);
    if (!t) throw err('not_found', `not found: track ${tid}`);
    return t;
  };
  const src = (t: Track) => (t.source === 'youtube' ? { kind: 'youtube' as const, src: t.url ?? '' } : { kind: 'file' as const, src: t.blob_hash ?? '' });
  const state = (): MixerState => ({
    channels, soundbites, soundbites_running: live.soundbites_running, soundscape_id: live.soundscape_id,
    soundscape_name: soundscapes.find((s) => s.id === live.soundscape_id)?.name ?? null,
    duck: live.duck, ducked: false, master_volume: live.master_volume, playlist: live.playlist,
    toggles_playing: live.toggles, previewing: live.previewing, sfx_queue: 0, engine_attached: false,
  });
  const mixChanged = () => { changed(); return state(); };
  const play = (c: Channel) => {
    const t = track(c.track_id);
    engine({ action: 'play_track', channel_id: c.id, ...src(t), volume: Math.round(c.volume * live.master_volume / 100), loop: c.is_looping });
  };

  return {
    music_tracks_list: ({ query }) => {
      const q = (query ?? {}) as TrackQuery;
      const bounded = q.min_duration != null || q.max_duration != null;
      return tracks
        .filter((t) => (q.tags ?? []).every((g) => t.tags.includes(g)))
        .filter((t) => !q.text || t.title.toLowerCase().includes(q.text.toLowerCase()))
        .filter((t) => !q.favorites_only || t.favorite)
        .filter((t) => !q.category || t.category === q.category)
        .filter((t) => q.looping == null || t.loop_by_default === q.looping)
        .filter((t) => !bounded || (t.duration != null
          && (q.min_duration == null || t.duration >= q.min_duration)
          && (q.max_duration == null || t.duration <= q.max_duration)))
        .sort((a, b) => b.added_at.localeCompare(a.added_at));
    },
    music_track_get: ({ id: tid }) => track(tid as string),
    music_track_create: ({ input }) => {
      const i = input as TrackInput;
      const t: Track = {
        id: id('t'), source: i.source, url: i.source === 'youtube' ? (i.url ?? '') : null,
        blob_hash: i.source === 'upload' ? `mock-audio-${seq}` : null, title: i.title, notes: i.notes ?? '',
        loop_by_default: i.loop_by_default ?? true, duration: i.duration ?? null, added_at: now(), favorite: false,
        category: i.category ?? (/sfx/i.test(i.title) || (i.loop_by_default === false && (i.duration ?? 999) <= 120) ? 'soundbite' : /ambien/i.test(i.title) ? 'ambient' : 'music'),
        campaign_ids: [], session_ids: [], tags: i.tags ?? [],
      };
      tracks.push(t);
      emitRaw({ type: 'store_changed', entity: 'music_track', entity_id: t.id, op: 'insert' });
      return t;
    },
    music_track_update: ({ id: tid, input }) => {
      const t = track(tid as string);
      const i = input as TrackInput;
      Object.assign(t, {
        title: i.title, notes: i.notes ?? t.notes, loop_by_default: i.loop_by_default ?? t.loop_by_default,
        url: i.source === 'youtube' ? (i.url ?? t.url) : t.url,
        tags: i.tags ?? t.tags, duration: i.duration ?? t.duration, category: i.category ?? t.category,
      });
      return t;
    },
    // A small keyword list standing in for sleuth_core::music::suggest_tags.
    music_tags_suggest: () => {
      const rules: [string, RegExp][] = [['tavern', /\btavern/i], ['combat', /\b(battle|drum|fight)/i], ['weather', /\b(thunder|rain|storm)/i]];
      return tracks
        .map((t) => ({ track_id: t.id, title: t.title, add: rules.filter(([g, re]) => re.test(t.title) && !t.tags.includes(g)).map(([g]) => g) }))
        .filter((s) => s.add.length);
    },
    music_tags_apply: ({ items }) => {
      let n = 0;
      for (const it of items as { track_id: string; add: string[] }[]) {
        const t = track(it.track_id);
        const before = t.tags.length;
        t.tags = [...new Set([...t.tags, ...it.add])];
        if (t.tags.length > before) n++;
      }
      return n;
    },
    music_track_set_favorite: ({ id: tid, favorite }) => {
      const t = track(tid as string);
      t.favorite = !!favorite;
      return t;
    },
    music_track_delete: ({ id: tid }) => {
      const i = tracks.findIndex((t) => t.id === tid);
      if (i < 0) throw err('not_found', `not found: track ${tid}`);
      tracks.splice(i, 1);
      channels = channels.filter((c) => c.track_id !== tid);
      soundbites = soundbites.filter((s) => s.track_id !== tid);
    },
    music_fetch_youtube_meta: () => ({ title: 'Mock YouTube Title', duration: 180 }),
    music_probe_file: ({ path }) => ({ title: String(path).split(/[\\/]/).pop()?.replace(/\.[^.]+$/, '') ?? '', duration: null }),

    music_state: () => state(),
    music_channel_add: ({ trackId, role, volume, looping, play: start }) => {
      const t = track(trackId as string);
      const r = (role as Channel['role']) ?? 'music';
      const c: Channel = {
        id: id('ch'), track_id: t.id, title: t.title, volume: (volume as number) ?? 80,
        is_looping: r === 'sfx' ? false : ((looping as boolean) ?? t.loop_by_default), is_playing: start !== false,
        role: r, position: channels.length,
      };
      channels.push(c);
      if (c.is_playing) play(c);
      return mixChanged();
    },
    music_channel_update: ({ id: cid, patch }) => {
      const c = channels.find((x) => x.id === cid);
      if (!c) throw err('not_found', `not found: channel ${cid}`);
      const p = patch as ChannelPatch;
      if (p.volume !== undefined) { c.volume = Math.max(0, Math.min(100, p.volume)); engine({ action: 'set_volume', channel_id: c.id, volume: c.volume, ramp_ms: 150 }); }
      if (p.is_looping !== undefined) { c.is_looping = p.is_looping; engine({ action: 'set_loop', channel_id: c.id, loop: c.is_looping }); }
      if (p.role !== undefined) c.role = p.role;
      if (p.is_playing !== undefined && p.is_playing !== c.is_playing) {
        c.is_playing = p.is_playing;
        engine(c.is_playing ? { action: 'resume_channel', channel_id: c.id } : { action: 'pause_channel', channel_id: c.id });
      }
      return mixChanged();
    },
    music_channel_remove: ({ id: cid }) => {
      channels = channels.filter((c) => c.id !== cid);
      engine({ action: 'stop_channel', channel_id: cid as string });
      return mixChanged();
    },
    music_set_all_playing: ({ playing }) => {
      for (const c of channels) c.is_playing = playing as boolean;
      if (!playing) live.soundbites_running = false;
      return mixChanged();
    },
    music_stop_all: () => {
      channels = []; soundbites = [];
      Object.assign(live, { soundbites_running: false, soundscape_id: null, playlist: null, toggles: [], previewing: false });
      engine({ action: 'stop_all' });
      return mixChanged();
    },
    music_soundbite_add: ({ trackId, minInterval, maxInterval, volume }) => {
      const t = track(trackId as string);
      let [lo, hi] = [Number(minInterval ?? 30), Number(maxInterval ?? 90)];
      if (lo > hi) [lo, hi] = [hi, lo];
      soundbites.push({ id: id('sb'), track_id: t.id, title: t.title, volume: Number(volume ?? 80), min_interval: lo, max_interval: hi });
      live.soundbites_running = true;
      return mixChanged();
    },
    music_soundbite_update: ({ id: sid, patch }) => {
      const s = soundbites.find((x) => x.id === sid);
      if (!s) throw err('not_found', `not found: soundbite ${sid}`);
      Object.assign(s, patch as SoundbitePatch);
      if (s.min_interval > s.max_interval) [s.min_interval, s.max_interval] = [s.max_interval, s.min_interval];
      return mixChanged();
    },
    music_soundbite_remove: ({ id: sid }) => { soundbites = soundbites.filter((s) => s.id !== sid); return mixChanged(); },
    music_soundbite_play: ({ id: sid }) => {
      const s = soundbites.find((x) => x.id === sid);
      if (!s) throw err('not_found', `not found: soundbite ${sid}`);
      engine({ action: 'play_oneshot', osid: id('os'), ...src(track(s.track_id)), volume: s.volume });
      return state();
    },
    music_set_duck: ({ mode }) => { live.duck = mode as DuckMode; return mixChanged(); },
    music_set_master: ({ volume }) => { live.master_volume = Number(volume); return mixChanged(); },
    music_preview: ({ trackId, seconds }) => {
      live.previewing = true;
      engine({ action: 'play_preview', ...src(track(trackId as string)), volume: 80, seconds: Number(seconds ?? 10) });
      changed();
    },
    music_stop_preview: () => { live.previewing = false; engine({ action: 'stop_preview' }); changed(); },
    music_toggle_video: () => engine({ action: 'toggle_video' }),
    music_engine_event: ({ event }) => {
      const e = event as EngineEvent;
      if (e.event === 'preview_ended') { live.previewing = false; changed(); }
    },
    music_sync: () => undefined,

    music_soundscapes_list: () =>
      [...soundscapes].sort((a, b) => Number(b.pinned) - Number(a.pinned) || a.name.localeCompare(b.name)),
    music_soundscape_set_pinned: ({ id: sid, pinned }) => {
      const s = soundscapes.find((x) => x.id === sid);
      if (!s) throw err('not_found', `not found: soundscape ${sid}`);
      s.pinned = !!pinned;
      return s;
    },
    music_soundscape_save: ({ input }) => {
      const i = input as SoundscapeInput;
      if (!i.name?.trim()) throw err('invalid_input', 'invalid input: name is required');
      const s: Soundscape = {
        id: id('ss'), name: i.name.trim(), pinned: false, campaign_id: null, notes: i.notes ?? '', created_at: now(), tags: i.tags ?? [],
        channels: channels.map((c) => ({ track_id: c.track_id, title: c.title, volume: c.volume, is_looping: c.is_looping, role: c.role })),
        soundbites: soundbites.map((b) => ({ track_id: b.track_id, title: b.title, volume: b.volume, min_interval: b.min_interval, max_interval: b.max_interval })),
      };
      soundscapes.push(s);
      live.soundscape_id = s.id;
      changed();
      return s;
    },
    music_soundscape_update: ({ id: sid }) => {
      const s = soundscapes.find((x) => x.id === sid);
      if (!s) throw err('not_found', `not found: soundscape ${sid}`);
      s.channels = channels.map((c) => ({ track_id: c.track_id, title: c.title, volume: c.volume, is_looping: c.is_looping, role: c.role }));
      s.soundbites = soundbites.map((b) => ({ track_id: b.track_id, title: b.title, volume: b.volume, min_interval: b.min_interval, max_interval: b.max_interval }));
      return s;
    },
    music_soundscape_load: ({ id: sid }) => {
      const s = soundscapes.find((x) => x.id === sid);
      if (!s) throw err('not_found', `not found: soundscape ${sid}`);
      engine({ action: 'stop_all' });
      channels = s.channels.map((c, i) => ({ ...c, id: id('ch'), is_playing: true, position: i }));
      soundbites = s.soundbites.map((b) => ({ ...b, id: id('sb') }));
      channels.forEach(play);
      Object.assign(live, { soundscape_id: s.id, soundbites_running: soundbites.length > 0 });
      return mixChanged();
    },
    music_soundscape_delete: ({ id: sid }) => {
      const i = soundscapes.findIndex((x) => x.id === sid);
      if (i >= 0) soundscapes.splice(i, 1);
    },

    music_playlists_list: () => playlists,
    music_playlist_save: ({ input }) => {
      const i = input as PlaylistInput;
      const items = i.track_ids.map((t) => ({ id: id('pi'), track_id: t, title: track(t).title }));
      let p = playlists.find((x) => x.id === i.id);
      if (p) Object.assign(p, { name: i.name, items });
      else playlists.push((p = { id: id('pl'), name: i.name, campaign_id: null, created_at: now(), items }));
      return p;
    },
    music_playlist_delete: ({ id: pid }) => {
      const i = playlists.findIndex((x) => x.id === pid);
      if (i >= 0) playlists.splice(i, 1);
    },
    music_playlist_play: ({ id: pid, index }) => {
      const p = playlists.find((x) => x.id === pid);
      if (!p || !p.items.length) throw err('invalid_input', 'invalid input: the playlist is empty');
      const idx = Number(index ?? 0);
      const t = track(p.items[idx].track_id);
      live.playlist = { playlist_id: p.id, name: p.name, index: idx, track_ids: p.items.map((x) => x.track_id), title: t.title, paused: false };
      engine({ action: 'play_track', channel_id: 'playlist', ...src(t), volume: 80, loop: false });
      return mixChanged();
    },
    music_playlist_control: ({ action }) => {
      const pl = live.playlist;
      if (!pl) return state();
      const a = action as PlaylistAction;
      if (a === 'stop') { live.playlist = null; engine({ action: 'stop_channel', channel_id: 'playlist' }); }
      else if (a === 'pause' || a === 'resume') { pl.paused = a === 'pause'; engine({ action: a === 'pause' ? 'pause_channel' : 'resume_channel', channel_id: 'playlist' }); }
      else {
        pl.index = Math.max(0, Math.min(pl.track_ids.length - 1, pl.index + (a === 'next' ? 1 : -1)));
        const t = track(pl.track_ids[pl.index]);
        pl.title = t.title;
        engine({ action: 'play_track', channel_id: 'playlist', ...src(t), volume: 80, loop: false });
      }
      return mixChanged();
    },

    music_soundboards_list: () => boards,
    music_soundboard_save: ({ input }) => {
      const i = input as SoundboardInput;
      if (i.buttons.some((b) => !b.track_id)) throw err('invalid_input', 'invalid input: every button needs a track');
      const buttons = i.buttons.map((b, n) => ({ ...b, id: b.id || id('btn'), position: n }));
      let b = boards.find((x) => x.id === i.id);
      if (b) Object.assign(b, { name: i.name, notes: i.notes ?? '', buttons });
      else boards.push((b = { id: id('sbd'), name: i.name, campaign_id: null, notes: i.notes ?? '', created_at: now(), buttons }));
      return b;
    },
    music_soundboard_delete: ({ id: bid }) => {
      const i = boards.findIndex((x) => x.id === bid);
      if (i >= 0) boards.splice(i, 1);
    },
    music_soundboard_press: ({ buttonId }) => {
      const btn = boards.flatMap((b) => b.buttons).find((b) => b.id === buttonId);
      if (!btn?.track_id) throw err('not_found', `not found: button ${buttonId}`);
      const t = track(btn.track_id);
      if (btn.play_mode === 'oneshot') engine({ action: 'play_oneshot', osid: id('os'), ...src(t), volume: btn.volume });
      else if (btn.play_mode === 'toggle' && live.toggles.includes(btn.id)) {
        live.toggles = live.toggles.filter((x) => x !== btn.id);
        engine({ action: 'stop_channel', channel_id: btn.id });
      } else {
        engine({ action: 'stop_channel', channel_id: btn.id });
        engine({ action: 'play_track', channel_id: btn.id, ...src(t), volume: btn.volume, loop: btn.loop_by_default });
        if (btn.play_mode === 'toggle') live.toggles = [...live.toggles, btn.id];
      }
      return mixChanged();
    },
    music_mix_export: ({ path }) => path,
    music_mix_import: ({ path, play: start }) => {
      const name = String(path).split(/[\\/]/).pop()?.replace(/\.dmix$/i, '') ?? 'Mix';
      const s: Soundscape = {
        id: id('ss'), name, campaign_id: null, notes: '', created_at: now(), tags: [],
        channels: [{ track_id: 'mock-t1', title: 'Tavern Ambience', volume: 70, is_looping: true, role: 'ambient' }], soundbites: [],
      };
      soundscapes.push(s);
      channels = s.channels.map((c, i) => ({ ...c, id: id('ch'), is_playing: start !== false, position: i }));
      if (start !== false) channels.forEach(play);
      live.soundscape_id = s.id;
      return { soundscape_id: s.id, name, tracks_created: 0, tracks_reused: 1, state: mixChanged() };
    },
    music_import_legacy: () => ({ tracks: 0, soundscapes: 0, playlists: 0, soundboards: 0, skipped: [] }),
  };
}
