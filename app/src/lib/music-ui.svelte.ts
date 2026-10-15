// Shared Music UI state: active tab, live mixer state, error banner.
// The service owns the real state; this mirrors `music.state()` and refreshes
// on `store_changed` (bumped through `app.dataVersion`) and after each action.

import { music, youtubeErrorMessage, type MixerState } from './music';
import { ONESHOT_PREFIX, PREVIEW_SLOT, type PlayStatus } from './music-engine';
import { app } from './state.svelte';

export type MusicTab = 'library' | 'mixer' | 'soundscapes' | 'playlists' | 'soundboard';

class MusicUi {
  tab = $state<MusicTab>('library');
  state = $state<MixerState | null>(null);
  /** YouTube/playback error banner (auto-hides after 6 s). */
  banner = $state<string | null>(null);
  #bannerTimer: ReturnType<typeof setTimeout> | null = null;
  /** What the engine is actually doing, per slot (channel id, button id,
   *  `playlist`, preview, one-shots). Reported by <MusicEngine>. */
  playback = $state<Record<string, PlayStatus>>({});

  setPlayback(slot: string, status: PlayStatus | null): void {
    if (status) this.playback[slot] = status;
    else delete this.playback[slot];
  }

  /** Indicator status for a slot the service wants `playing` (or paused).
   *  A playing slot the engine has not reported on yet is still loading. */
  statusFor(slot: string, playing: boolean): PlayStatus | null {
    const s = this.playback[slot];
    if (s) return playing || s !== 'playing' ? s : 'paused';
    if (!playing) return 'paused';
    return this.state?.engine_attached ? 'loading' : null;
  }

  get previewStatus(): PlayStatus | null {
    return this.playback[PREVIEW_SLOT] ?? null;
  }

  /** Status of soundbite / one-shot clips (the busiest one wins). */
  get oneshotStatus(): PlayStatus | null {
    let out: PlayStatus | null = null;
    for (const [k, v] of Object.entries(this.playback)) {
      if (!k.startsWith(ONESHOT_PREFIX)) continue;
      if (v === 'playing') return v;
      if (v === 'loading') out = v;
    }
    return out;
  }

  /** Anything audible or about to be (for the navigation rail). */
  get activity(): PlayStatus | null {
    const all = Object.values(this.playback);
    if (all.includes('playing')) return 'playing';
    if (all.includes('loading')) return 'loading';
    return null;
  }

  async refresh(): Promise<void> {
    try {
      this.state = await music.state();
    } catch (e) {
      app.error('Could not load the music mixer', e);
    }
  }

  /** Runs an action that returns the new mixer state. */
  async act(text: string, f: () => Promise<MixerState | unknown>): Promise<void> {
    try {
      const r = await f();
      if (r && typeof r === 'object' && 'channels' in (r as object)) this.state = r as MixerState;
      else await this.refresh();
    } catch (e) {
      app.error(text, e);
    }
  }

  showError(code: number, title?: string): void {
    this.banner = `${title ? `${title}: ` : ''}${youtubeErrorMessage(code)} (code ${code})`;
    if (this.#bannerTimer) clearTimeout(this.#bannerTimer);
    this.#bannerTimer = setTimeout(() => (this.banner = null), 6000);
  }

  openMixer(): void {
    this.tab = 'mixer';
    app.navigate('music');
  }
}

export const musicUi = new MusicUi();
