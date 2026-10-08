<script lang="ts">
  // Hidden, always-mounted audio engine (docs/features/music/player-engine.md).
  // Executes service commands (`music_engine` events), reports engine events
  // back, keeps the shared mixer state fresh and handles .dmix double-clicks.
  import { onMount } from 'svelte';
  import { api, isTauri } from '../api';
  import { MusicEngine } from '../music-engine';
  import { mediaUrl, music, onDmixOpened, onEngineCommand, type EngineEvent } from '../music';
  import { musicUi } from '../music-ui.svelte';
  import { app } from '../state.svelte';
  import Icon from './Icon.svelte';

  let host: HTMLDivElement | undefined = $state();
  let videoVisible = $state(false);

  onMount(() => {
    const engine = new MusicEngine({
      videoHost: host!,
      mediaUrl,
      emit: (e: EngineEvent) => {
        if (e.event === 'channel_error') {
          const ch = musicUi.state?.channels.find((c) => c.id === e.channel_id);
          musicUi.showError(e.code, ch?.title);
        }
        void music.engineEvent(e).catch((err) => console.warn('[music] engine event', err));
      },
      onVideoToggle: (v) => (videoVisible = v),
    });
    const offs: (() => void)[] = [];
    let syncTimer: ReturnType<typeof setTimeout> | null = null;
    let alive = true;

    void (async () => {
      // The browser mock has no real media (fake blob URLs, and tests run
      // offline), so the engine stays silent there unless explicitly enabled.
      let audible = isTauri();
      try {
        audible ||= localStorage.getItem('sleuth.mockAudio') === '1';
      } catch {
        /* storage unavailable */
      }
      offs.push(await onEngineCommand((c) => audible && engine.handle(c)));
      // Live-state changes (also from the standalone MCP server): refresh the
      // mirror and let the service re-apply the desired state to the engine.
      offs.push(
        await api.onEvent((e) => {
          if (e.type !== 'store_changed' || !e.entity.startsWith('music')) return;
          if (syncTimer) clearTimeout(syncTimer);
          syncTimer = setTimeout(() => {
            void musicUi.refresh();
            if (isTauri()) void music.sync().catch(() => {});
          }, 200);
        }),
      );
      offs.push(
        await onDmixOpened((d) => {
          musicUi.openMixer();
          void musicUi.refresh();
          app.toast('success', `Loaded mix “${d.name}”`);
        }),
      );
      if (!alive) return offs.forEach((f) => f());
      await music.engineEvent({ event: 'ready' }).catch(() => {});
      void musicUi.refresh();
    })();

    // Autoplay policy: the AudioContext may need a user gesture to start.
    const unlock = () => engine.unlock();
    window.addEventListener('pointerdown', unlock, { once: true });

    return () => {
      alive = false;
      offs.forEach((f) => f());
      if (syncTimer) clearTimeout(syncTimer);
      window.removeEventListener('pointerdown', unlock);
      engine.dispose();
    };
  });
</script>

<div class="video" class:visible={videoVisible} aria-hidden={!videoVisible}>
  <div class="head">
    <span>YouTube</span>
    <button class="btn icon ghost" aria-label="Hide video" onclick={() => void music.toggleVideo()}>
      <Icon name="x" />
    </button>
  </div>
  <div class="players" bind:this={host}></div>
</div>

<style>
  /* Hidden players must stay rendered (a display:none iframe stops audio). */
  .video {
    position: fixed;
    left: -4000px;
    bottom: 16px;
    width: 336px;
    z-index: 50;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    opacity: 0;
    pointer-events: none;
  }
  .video.visible {
    left: auto;
    right: 16px;
    opacity: 1;
    pointer-events: auto;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 4px 4px 10px;
    font-size: 12px;
    color: var(--text-muted);
  }
  .players {
    display: grid;
    gap: 4px;
    padding: 0 8px 8px;
    max-height: 60vh;
    overflow: auto;
  }
  .players :global(iframe) {
    width: 320px;
    height: 180px;
    border: 0;
    display: block;
  }
</style>
