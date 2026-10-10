// Engine playback status (loading / playing / paused) and the indicator.
import { render } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import PlayIndicator from './components/PlayIndicator.svelte';
import { MusicEngine, PREVIEW_SLOT, youtubeId, type PlayStatus } from './music-engine';
import type { EngineEvent } from './music';

/** Minimal HTMLAudioElement stand-in (jsdom cannot play media). */
class FakeAudio {
  static last: FakeAudio;
  src = '';
  loop = false;
  volume = 1;
  crossOrigin: string | null = null;
  preload = '';
  duration = 42;
  onended: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onloadedmetadata: (() => void) | null = null;
  onplaying: (() => void) | null = null;
  onwaiting: (() => void) | null = null;
  onpause: (() => void) | null = null;
  constructor() {
    FakeAudio.last = this;
  }
  play() {
    return Promise.resolve();
  }
  pause() {
    this.onpause?.();
  }
  load() {}
  removeAttribute() {}
}

/** Fake YT IFrame API: records the last player's event handlers. */
class FakeYtPlayer {
  static last: FakeYtPlayer;
  events: Record<string, (e: unknown) => void>;
  constructor(_el: unknown, opts: { events: Record<string, (e: unknown) => void> }) {
    this.events = opts.events;
    FakeYtPlayer.last = this;
  }
  setVolume() {}
  playVideo() {}
  pauseVideo() {}
  getDuration() {
    return 99;
  }
  getPlayerState() {
    return 1;
  }
  seekTo() {}
  destroy() {}
}

function setup() {
  const statuses: [string, PlayStatus | null][] = [];
  const events: EngineEvent[] = [];
  const engine = new MusicEngine({
    videoHost: document.createElement('div'),
    mediaUrl: (h) => `media/${h}`,
    emit: (e) => events.push(e),
    onStatus: (slot, s) => statuses.push([slot, s]),
  });
  const last = () => statuses.at(-1);
  return { engine, statuses, events, last };
}

beforeEach(() => {
  vi.stubGlobal('Audio', FakeAudio);
  (window as unknown as { YT: unknown }).YT = { Player: FakeYtPlayer, PlayerState: { ENDED: 0 } };
});
afterEach(() => vi.unstubAllGlobals());

describe('youtubeId', () => {
  it('extracts ids from the usual URL shapes', () => {
    expect(youtubeId('https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=3')).toBe('dQw4w9WgXcQ');
    expect(youtubeId('https://youtu.be/dQw4w9WgXcQ')).toBe('dQw4w9WgXcQ');
    expect(youtubeId('https://www.youtube.com/shorts/dQw4w9WgXcQ')).toBe('dQw4w9WgXcQ');
    expect(youtubeId('dQw4w9WgXcQ')).toBe('dQw4w9WgXcQ');
    expect(youtubeId('not a video')).toBeNull();
  });
});

describe('engine playback status', () => {
  it('file channel: loading until the media plays, then paused, buffering, stopped', () => {
    const { engine, last } = setup();
    engine.handle({ action: 'play_track', channel_id: 'c1', kind: 'file', src: 'h', volume: 50, loop: true });
    expect(last()).toEqual(['c1', 'loading']);
    FakeAudio.last.onplaying?.();
    expect(last()).toEqual(['c1', 'playing']);
    expect(engine.statusOf('c1')).toBe('playing');
    engine.handle({ action: 'pause_channel', channel_id: 'c1' });
    expect(last()).toEqual(['c1', 'paused']);
    engine.handle({ action: 'resume_channel', channel_id: 'c1' });
    expect(last()).toEqual(['c1', 'loading']);
    FakeAudio.last.onplaying?.();
    FakeAudio.last.onwaiting?.(); // buffering
    expect(last()).toEqual(['c1', 'loading']);
    engine.handle({ action: 'stop_channel', channel_id: 'c1' });
    expect(last()).toEqual(['c1', null]);
    expect(engine.statusOf('c1')).toBeUndefined();
  });

  it('a replaced slot never reports for the old media', () => {
    const { engine, statuses } = setup();
    engine.handle({ action: 'play_track', channel_id: 'c1', kind: 'file', src: 'a', volume: 50, loop: true });
    const old = FakeAudio.last;
    engine.handle({ action: 'play_track', channel_id: 'c1', kind: 'file', src: 'b', volume: 50, loop: true });
    const n = statuses.length;
    old.onplaying?.();
    expect(statuses.length).toBe(n);
    expect(engine.statusOf('c1')).toBe('loading');
  });

  it('youtube channel follows the player state; errors clear the slot', async () => {
    const { engine, last, events } = setup();
    engine.handle({ action: 'play_track', channel_id: 'yt', kind: 'youtube', src: 'https://youtu.be/dQw4w9WgXcQ', volume: 80, loop: false });
    expect(last()).toEqual(['yt', 'loading']);
    await vi.waitFor(() => expect(FakeYtPlayer.last).toBeDefined());
    const p = FakeYtPlayer.last;
    p.events.onReady({ target: p });
    p.events.onStateChange({ data: 3, target: p }); // buffering
    expect(last()).toEqual(['yt', 'loading']);
    p.events.onStateChange({ data: 1, target: p });
    expect(last()).toEqual(['yt', 'playing']);
    p.events.onStateChange({ data: 2, target: p });
    expect(last()).toEqual(['yt', 'paused']);
    p.events.onError({ data: 150 });
    expect(last()).toEqual(['yt', null]);
    expect(events).toContainEqual({ event: 'channel_error', channel_id: 'yt', code: 150 });
  });

  it('preview and stop_all', () => {
    const { engine, last } = setup();
    engine.handle({ action: 'play_preview', kind: 'file', src: 'h', volume: 80, seconds: 10 });
    expect(last()).toEqual([PREVIEW_SLOT, 'loading']);
    FakeAudio.last.onplaying?.();
    expect(last()).toEqual([PREVIEW_SLOT, 'playing']);
    engine.handle({ action: 'stop_all' });
    expect(last()).toEqual([PREVIEW_SLOT, null]);
  });
});

describe('PlayIndicator', () => {
  it('renders a labelled glyph per status and nothing when idle', () => {
    for (const [status, label] of [
      ['playing', 'Playing'],
      ['loading', 'Loading…'],
      ['paused', 'Paused'],
    ] as const) {
      const { getByRole, unmount } = render(PlayIndicator, { status });
      const el = getByRole('img');
      expect(el.getAttribute('aria-label')).toBe(label);
      expect(el.dataset.status).toBe(status);
      unmount();
    }
    const { container } = render(PlayIndicator, { status: null });
    expect(container.querySelector('[role="img"]')).toBeNull();
  });
});
