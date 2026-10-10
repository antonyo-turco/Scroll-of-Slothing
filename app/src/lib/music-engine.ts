// Music playback engine (runs inside the main webview; replaces Python's
// pywebview subprocess). Spec: docs/features/music/player-engine.md.
// Protocol: `EngineCommand` / `EngineEvent` in ./music.ts. The service owns
// all state (master volume, ducking, queue); volumes arrive already effective.
//
// - Web Audio graph: every file channel -> its own GainNode -> master node.
//   Volume changes ramp smoothly (setTargetAtTime over `ramp_ms`).
// - Uploaded files play through <audio> + MediaElementAudioSourceNode.
// - YouTube plays through IFrame API players (volume via setVolume; Web Audio
//   cannot reach a cross-origin iframe). Looping is manual (seekTo(0)).
// - One-shots, a single preview slot (auto-stops after N s), video toggle.
// - Reports a per-slot playback status (loading / playing / paused) through
//   `onStatus`, so the UI can tell a track that is still loading or buffering
//   from one that is audible.

import type { EngineCommand, EngineEvent } from './music';

/** Extracts a YouTube video id (?v=, youtu.be/, /embed/, /shorts/, regex fallback). */
export function youtubeId(url: string): string | null {
  const u = url.trim();
  if (!u) return null;
  try {
    const p = new URL(u);
    if (p.hostname.includes('youtu.be')) return p.pathname.slice(1).split('/')[0] || null;
    const v = p.searchParams.get('v');
    if (v) return v;
    const m = p.pathname.match(/\/(?:embed|shorts|v|live)\/([\w-]{6,})/);
    if (m) return m[1];
  } catch {
    /* not a URL: fall through */
  }
  const m = u.match(/(?:v=|\/)([\w-]{11})(?:[?&#/]|$)/) ?? u.match(/^([\w-]{11})$/);
  return m ? m[1] : null;
}

// ── YouTube IFrame API loader (commands before it is ready are queued) ──

/* eslint-disable @typescript-eslint/no-explicit-any */
type YTPlayer = any;
let ytReady: Promise<any> | null = null;

function loadYoutubeApi(): Promise<any> {
  if (ytReady) return ytReady;
  ytReady = new Promise((resolve, reject) => {
    const w = window as any;
    if (w.YT?.Player) return resolve(w.YT);
    const prev = w.onYouTubeIframeAPIReady;
    w.onYouTubeIframeAPIReady = () => {
      prev?.();
      resolve(w.YT);
    };
    const s = document.createElement('script');
    s.src = 'https://www.youtube.com/iframe_api';
    s.async = true;
    s.onerror = () => {
      ytReady = null;
      reject(new Error('YouTube API unavailable (offline?)'));
    };
    document.head.appendChild(s);
  });
  return ytReady;
}

type Kind = 'youtube' | 'file';

/** What a slot is actually doing (the service only knows what it asked for). */
export type PlayStatus = 'loading' | 'playing' | 'paused';

interface Slot {
  id: string;
  kind: Kind;
  loop: boolean;
  /** Effective 0..100. */
  volume: number;
  audio?: HTMLAudioElement;
  node?: MediaElementAudioSourceNode;
  gain?: GainNode;
  yt?: YTPlayer;
  ytHost?: HTMLDivElement;
  /** Actions waiting for the YouTube player to be ready. */
  pending: ((p: YTPlayer) => void)[];
  paused: boolean;
  status?: PlayStatus;
  timer?: ReturnType<typeof setTimeout>;
}

export interface EngineOptions {
  /** Where YouTube players are mounted (hidden normally, shown by toggle_video). */
  videoHost: HTMLElement;
  /** URL for an uploaded media blob hash. */
  mediaUrl: (hash: string) => string;
  emit: (e: EngineEvent) => void;
  onVideoToggle?: (visible: boolean) => void;
  /** Slot status changes; `null` when the slot is gone. Slots are channel ids,
   *  `PREVIEW_SLOT` and `ONESHOT_PREFIX + osid`. */
  onStatus?: (slot: string, status: PlayStatus | null) => void;
}

export const PREVIEW_SLOT = '__preview';
export const ONESHOT_PREFIX = '__os_';
const PREVIEW = PREVIEW_SLOT;
const OS = ONESHOT_PREFIX;

// YT.PlayerState values (the API object may not be loaded yet).
const YT_ENDED = 0;
const YT_PLAYING = 1;
const YT_PAUSED = 2;
const YT_BUFFERING = 3;

export class MusicEngine {
  #ctx: AudioContext | null = null;
  #master: GainNode | null = null;
  #slots = new Map<string, Slot>();
  videoVisible = false;

  constructor(private opts: EngineOptions) {}

  #audio(): { ctx: AudioContext; master: GainNode } | null {
    try {
      if (!this.#ctx) {
        this.#ctx = new AudioContext();
        this.#master = this.#ctx.createGain();
        this.#master.connect(this.#ctx.destination);
      }
      if (this.#ctx.state === 'suspended') void this.#ctx.resume();
      return { ctx: this.#ctx, master: this.#master! };
    } catch {
      return null;
    }
  }

  /** Creates/resumes the AudioContext (call from a user gesture: autoplay policy). */
  unlock(): void {
    this.#audio();
  }

  /** Channels known to the engine (debug / status). */
  status(): { id: string; kind: Kind; paused: boolean; volume: number }[] {
    return [...this.#slots.values()].map((s) => ({ id: s.id, kind: s.kind, paused: s.paused, volume: s.volume }));
  }

  handle(cmd: EngineCommand): void {
    try {
      switch (cmd.action) {
        case 'play_track':
          this.#play(cmd.channel_id, cmd.kind, cmd.src, cmd.volume, cmd.loop);
          break;
        case 'pause_channel':
          this.#with(cmd.channel_id, (s) => {
            s.paused = true;
            this.#status(s, 'paused');
            if (s.audio) s.audio.pause();
            else this.#yt(s, (p) => p.pauseVideo());
          });
          break;
        case 'resume_channel':
          this.#with(cmd.channel_id, (s) => {
            s.paused = false;
            this.#status(s, 'loading'); // until the media reports it is playing
            if (s.audio) void s.audio.play().catch(() => {});
            else this.#yt(s, (p) => p.playVideo());
          });
          break;
        case 'stop_channel':
          this.#stop(cmd.channel_id);
          break;
        case 'set_volume':
          this.#with(cmd.channel_id, (s) => {
            s.volume = clamp(cmd.volume);
            this.#applyVolume(s, cmd.ramp_ms);
          });
          break;
        case 'set_loop':
          this.#with(cmd.channel_id, (s) => {
            s.loop = cmd.loop;
            if (s.audio) s.audio.loop = cmd.loop;
          });
          break;
        case 'stop_all':
          for (const id of [...this.#slots.keys()]) this.#stop(id);
          break;
        case 'play_oneshot':
          this.#play(OS + cmd.osid, cmd.kind, cmd.src, cmd.volume, false, cmd.osid);
          break;
        case 'play_preview': {
          this.#stop(PREVIEW, true);
          this.#play(PREVIEW, cmd.kind, cmd.src, cmd.volume, false);
          const s = this.#slots.get(PREVIEW);
          if (s) s.timer = setTimeout(() => {
            this.#stop(PREVIEW);
            this.opts.emit({ event: 'preview_ended' });
          }, Math.max(1, cmd.seconds) * 1000);
          break;
        }
        case 'stop_preview':
          this.#stop(PREVIEW);
          break;
        case 'toggle_video':
          this.videoVisible = !this.videoVisible;
          this.opts.onVideoToggle?.(this.videoVisible);
          break;
      }
    } catch (e) {
      console.error('[music] command failed', cmd, e);
    }
  }

  dispose(): void {
    for (const id of [...this.#slots.keys()]) this.#stop(id, true);
    void this.#ctx?.close();
    this.#ctx = null;
  }

  /** Status of one slot (`undefined` when it is not loaded). */
  statusOf(id: string): PlayStatus | undefined {
    return this.#slots.get(id)?.status;
  }

  #status(s: Slot, st: PlayStatus) {
    if (this.#slots.get(s.id) !== s || s.status === st) return; // stale or unchanged
    s.status = st;
    this.opts.onStatus?.(s.id, st);
  }

  #with(id: string, f: (s: Slot) => void) {
    const s = this.#slots.get(id);
    if (s) f(s);
  }

  #yt(s: Slot, f: (p: YTPlayer) => void) {
    if (s.yt && !s.pending.length && s.yt.getPlayerState) f(s.yt);
    else s.pending.push(f);
  }

  #applyVolume(s: Slot, rampMs = 150) {
    if (s.gain && this.#ctx) ramp(s.gain.gain, s.volume / 100, this.#ctx.currentTime, rampMs / 1000);
    else if (s.audio) s.audio.volume = s.volume / 100;
    else this.#yt(s, (p) => p.setVolume(Math.round(s.volume)));
  }

  #play(id: string, kind: Kind, src: string, volume: number, loop: boolean, osid?: string) {
    this.#stop(id, true);
    const s: Slot = { id, kind, loop, volume: clamp(volume), paused: false, pending: [] };
    this.#slots.set(id, s);
    this.#status(s, 'loading');
    const isChannel = !osid && id !== PREVIEW;
    let done = false;
    const ended = () => {
      if (done) return;
      if (osid) {
        done = true;
        this.opts.emit({ event: 'oneshot_ended', osid });
        this.#stop(id, true);
      } else if (id === PREVIEW) {
        done = true;
        this.#stop(id, true);
        this.opts.emit({ event: 'preview_ended' });
      } else if (!s.loop) {
        this.opts.emit({ event: 'channel_ended', channel_id: id });
      }
    };
    const failed = (code: number) => {
      if (done) return;
      done = true;
      if (osid) this.opts.emit({ event: 'oneshot_ended', osid });
      else if (isChannel) this.opts.emit({ event: 'channel_error', channel_id: id, code });
      else this.opts.emit({ event: 'preview_ended' });
      this.#stop(id, true);
    };
    const duration = (seconds: number) => {
      if (!Number.isFinite(seconds) || seconds <= 0) return;
      if (osid) this.opts.emit({ event: 'oneshot_duration', osid, seconds });
      else if (isChannel) this.opts.emit({ event: 'track_duration', channel_id: id, seconds });
    };

    if (kind === 'youtube') {
      const vid = youtubeId(src);
      if (!vid) return failed(2);
      this.#playYoutube(s, vid, ended, failed, duration, osid !== undefined);
      return;
    }
    const audio = new Audio();
    audio.crossOrigin = 'anonymous';
    audio.preload = 'auto';
    audio.loop = loop;
    audio.src = this.opts.mediaUrl(src);
    s.audio = audio;
    const a = this.#audio();
    if (a) {
      try {
        s.node = a.ctx.createMediaElementSource(audio);
        s.gain = a.ctx.createGain();
        s.gain.gain.value = 0;
        s.node.connect(s.gain).connect(a.master);
        ramp(s.gain.gain, s.volume / 100, a.ctx.currentTime, 0.15); // fade in
      } catch {
        s.gain = undefined;
        audio.volume = s.volume / 100;
      }
    } else audio.volume = s.volume / 100;
    audio.onended = ended;
    audio.onerror = () => failed(5);
    audio.onloadedmetadata = () => duration(audio.duration);
    audio.onplaying = () => this.#status(s, 'playing');
    audio.onwaiting = () => !s.paused && this.#status(s, 'loading');
    audio.onpause = () => s.paused && this.#status(s, 'paused');
    void audio.play().catch((e) => {
      if ((e as DOMException)?.name !== 'AbortError') failed(5);
    });
  }

  #playYoutube(
    s: Slot,
    videoId: string,
    ended: () => void,
    failed: (code: number) => void,
    duration: (sec: number) => void,
    oneshot: boolean,
  ) {
    const host = document.createElement('div');
    host.className = 'yt-slot';
    host.dataset.channel = s.id;
    const inner = document.createElement('div');
    host.appendChild(inner);
    this.opts.videoHost.appendChild(host);
    s.ytHost = host;
    loadYoutubeApi().then(
      (YT) => {
        if (this.#slots.get(s.id) !== s) return; // stopped meanwhile
        s.yt = new YT.Player(inner, {
          width: 320,
          height: 180,
          videoId,
          playerVars: {
            autoplay: 1, controls: 0, disablekb: 1, fs: 0, iv_load_policy: 3, modestbranding: 1, rel: 0,
            playsinline: 1, origin: window.location.origin,
          },
          events: {
            onReady: (ev: any) => {
              const p = ev.target;
              p.setVolume(Math.round(s.volume));
              if (!s.paused) p.playVideo();
              duration(p.getDuration?.() ?? 0);
              const queued = s.pending;
              s.pending = [];
              for (const f of queued) f(p);
            },
            onStateChange: (ev: any) => {
              if (ev.data === YT_PLAYING) this.#status(s, 'playing');
              else if (ev.data === YT_PAUSED) this.#status(s, 'paused');
              else if (ev.data === YT_BUFFERING) this.#status(s, 'loading');
              if (ev.data === YT_ENDED) {
                if (s.loop && !oneshot) {
                  ev.target.seekTo(0);
                  ev.target.playVideo();
                } else ended();
              }
            },
            onError: (ev: any) => failed(Number(ev.data) || 5),
          },
        });
      },
      () => failed(5),
    );
  }

  #stop(id: string, immediate = false) {
    const s = this.#slots.get(id);
    if (!s) return;
    this.#slots.delete(id);
    if (s.status) this.opts.onStatus?.(id, null);
    if (s.timer) clearTimeout(s.timer);
    if (s.audio) {
      const a = s.audio;
      const finish = () => {
        a.onended = a.onerror = a.onloadedmetadata = a.onplaying = a.onwaiting = a.onpause = null;
        a.pause();
        a.removeAttribute('src');
        a.load();
        s.node?.disconnect();
        s.gain?.disconnect();
      };
      if (s.gain && !immediate && this.#ctx) {
        ramp(s.gain.gain, 0, this.#ctx.currentTime, 0.08); // short fade-out, no click
        setTimeout(finish, 120);
      } else finish();
    }
    if (s.yt) {
      try {
        s.yt.destroy();
      } catch {
        /* ignore */
      }
    }
    s.ytHost?.remove();
  }
}

function clamp(v: number): number {
  return Math.max(0, Math.min(100, Number.isFinite(v) ? v : 80));
}

function ramp(p: AudioParam, target: number, now: number, seconds: number) {
  p.cancelScheduledValues(now);
  p.setValueAtTime(p.value, now);
  // time constant ≈ 1/3 of the ramp so it settles within `seconds`
  p.setTargetAtTime(target, now, Math.max(0.005, seconds / 3));
}
