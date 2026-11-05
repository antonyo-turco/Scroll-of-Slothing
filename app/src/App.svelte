<script lang="ts">
  import { onMount, type Component } from 'svelte';
  import ClockWatcher from './lib/components/ClockWatcher.svelte';
  import CommandPalette from './lib/components/CommandPalette.svelte';
  import DiceRoller from './lib/components/DiceRoller.svelte';
  import { dice } from './lib/dice.svelte';
  import JobsTray from './lib/components/JobsTray.svelte';
  import MusicEngine from './lib/components/MusicEngine.svelte';
  import Rail from './lib/components/Rail.svelte';
  import Toasts from './lib/components/Toasts.svelte';
  import TooltipHost from './lib/components/TooltipHost.svelte';
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

  /** True while the user types into a field (single-key shortcuts stay off). */
  const typing = (t: EventTarget | null) =>
    t instanceof HTMLElement && (t.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(t.tagName));

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      app.paletteOpen = !app.paletteOpen;
    } else if (e.key.toLowerCase() === 'd' && !e.ctrlKey && !e.metaKey && !e.altKey && !e.repeat && !typing(e.target)) {
      e.preventDefault();
      dice.toggle();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<a class="skip" href="#content" onclick={(e) => (e.preventDefault(), document.getElementById('content')?.focus())}>Skip to content</a>
<div class="shell">
  <Rail />
  <div class="main">
    <TopBar />
    <main id="content" tabindex="-1">
      {#if app.ready && View}
        <View />
      {/if}
    </main>
  </div>
</div>
<CommandPalette />
<JobsTray />
<Toasts />
<TooltipHost />
{#if app.ready}<MusicEngine /><ClockWatcher /><DiceRoller />{/if}

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
  main:focus {
    outline: none;
  }
  /* visible only while focused (first Tab stop) */
  .skip {
    position: fixed;
    left: 8px;
    top: -48px;
    z-index: 100;
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    background: var(--accent);
    color: var(--accent-contrast, #fff);
    font-weight: 600;
    text-decoration: none;
  }
  .skip:focus {
    top: 8px;
  }
</style>
