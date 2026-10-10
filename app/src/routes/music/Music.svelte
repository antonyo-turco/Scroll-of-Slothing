<script lang="ts">
  // DM Music: library, live mixer, soundscapes, playlists, soundboards.
  // Spec: docs/features/music/*.md. Audio runs in <MusicEngine> (always mounted).
  import { onMount } from 'svelte';
  import { onFileDrop } from '../../lib/api';
  import { DMIX_EXTENSION, music } from '../../lib/music';
  import { musicUi, type MusicTab } from '../../lib/music-ui.svelte';
  import { app } from '../../lib/state.svelte';
  import Library from './Library.svelte';
  import Mixer from './Mixer.svelte';
  import Playlists from './Playlists.svelte';
  import Soundboard from './Soundboard.svelte';
  import Soundscapes from './Soundscapes.svelte';

  const TABS: [MusicTab, string][] = [
    ['library', 'Library'],
    ['mixer', 'Mixer'],
    ['soundscapes', 'Soundscapes'],
    ['playlists', 'Playlists'],
    ['soundboard', 'Soundboard'],
  ];

  async function importDmix(path: string) {
    try {
      const r = await music.mixImport(path, true);
      musicUi.state = r.state;
      musicUi.tab = 'mixer';
      app.toast('success', `Loaded mix “${r.name}”`);
    } catch (e) {
      app.error('Could not open the mix', e);
    }
  }

  onMount(() => {
    void musicUi.refresh();
    let off: (() => void) | undefined;
    void onFileDrop((paths) => {
      const mixes = paths.filter((p) => p.toLowerCase().endsWith(`.${DMIX_EXTENSION}`));
      if (mixes.length) void importDmix(mixes[0]);
    }).then((f) => (off = f));
    return () => off?.();
  });

  async function stopMusic() {
    await musicUi.act('Could not stop the music', () => music.stopAll());
  }
</script>

<div class="music">
  <header class="tabs">
    <div class="segmented" role="tablist" aria-label="Music views">
      {#each TABS as [id, label] (id)}
        <button role="tab" aria-selected={musicUi.tab === id} onclick={() => (musicUi.tab = id)}>{label}</button>
      {/each}
    </div>
    <div class="spacer"></div>
    {#if musicUi.state?.playlist}
      <span class="muted now">♪ {musicUi.state.playlist.title}</span>
    {/if}
    <button class="btn danger" onclick={stopMusic} title="Stop everything and clear the live mix">⏹ Stop Music</button>
  </header>
  {#if musicUi.banner}
    <div class="banner" role="alert">
      {musicUi.banner}
      <button class="btn ghost" aria-label="Dismiss" onclick={() => (musicUi.banner = null)}>✕</button>
    </div>
  {/if}
  <div class="body">
    {#if musicUi.tab === 'library'}
      <Library />
    {:else if musicUi.tab === 'mixer'}
      <Mixer onImport={importDmix} />
    {:else if musicUi.tab === 'soundscapes'}
      <Soundscapes />
    {:else if musicUi.tab === 'playlists'}
      <Playlists />
    {:else}
      <Soundboard />
    {/if}
  </div>
</div>

<style>
  .music {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  .now {
    font-size: 13px;
    max-width: 30ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    background: color-mix(in srgb, var(--danger) 15%, var(--surface));
    color: var(--danger);
    font-size: 13px;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
</style>
