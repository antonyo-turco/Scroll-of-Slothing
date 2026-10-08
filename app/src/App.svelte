<script lang="ts">
  import { onMount, type Component } from 'svelte';
  import CommandPalette from './lib/components/CommandPalette.svelte';
  import JobsTray from './lib/components/JobsTray.svelte';
  import MusicEngine from './lib/components/MusicEngine.svelte';
  import Rail from './lib/components/Rail.svelte';
  import Toasts from './lib/components/Toasts.svelte';
  import TopBar from './lib/components/TopBar.svelte';
  import { findView } from './lib/nav';
  import { app } from './lib/state.svelte';

  // Views are code-split and loaded on first visit, then cached.
  const cache = new Map<string, Component>();
  let View = $state<Component | null>(null);

  $effect(() => {
    const id = app.route;
    const found = findView(id);
    if (!found) return;
    const cached = cache.get(id);
    if (cached) {
      View = cached;
      return;
    }
    found.view.load().then((m) => {
      cache.set(id, m.default);
      if (app.route === id) View = m.default;
    });
  });

  onMount(() => {
    void app.init();
    return () => app.dispose();
  });

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      app.paletteOpen = !app.paletteOpen;
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="shell">
  <Rail />
  <div class="main">
    <TopBar />
    <main>
      {#if app.ready && View}
        <View />
      {/if}
    </main>
  </div>
</div>
<CommandPalette />
<JobsTray />
<Toasts />
{#if app.ready}<MusicEngine />{/if}

<style>
  .shell {
    display: flex;
    height: 100%;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  main {
    flex: 1;
    min-height: 0;
  }
</style>
