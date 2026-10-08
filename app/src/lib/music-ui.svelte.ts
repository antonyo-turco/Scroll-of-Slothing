// Shared Music UI state: active tab, live mixer state, error banner.
// The service owns the real state; this mirrors `music.state()` and refreshes
// on `store_changed` (bumped through `app.dataVersion`) and after each action.

import { music, youtubeErrorMessage, type MixerState } from './music';
import { app } from './state.svelte';

export type MusicTab = 'library' | 'mixer' | 'soundscapes' | 'playlists' | 'soundboard';

class MusicUi {
  tab = $state<MusicTab>('library');
  state = $state<MixerState | null>(null);
  /** YouTube/playback error banner (auto-hides after 6 s). */
  banner = $state<string | null>(null);
  #bannerTimer: ReturnType<typeof setTimeout> | null = null;

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
