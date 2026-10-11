<script lang="ts">
  import { findView } from '../nav';
  import { app } from '../state.svelte';
  import Icon from './Icon.svelte';

  const found = $derived(findView(app.route));
</script>

<header class="topbar">
  <h1>{found?.dest.label ?? ''}</h1>
  {#if found && found.dest.views.length > 1}
    <div class="segmented" role="tablist" aria-label="{found.dest.label} views">
      {#each found.dest.views as v (v.id)}
        <button
          role="tab"
          aria-selected={v.id === app.route}
          onclick={() => app.navigate(v.id)}>{v.label}</button
        >
      {/each}
    </div>
  {/if}
  <!-- filled by the current view through <ViewActions> -->
  <div id="topbar-nav" class="slot"></div>
  <div class="spacer"></div>
  <div id="topbar-actions" class="slot"></div>
  <button class="btn ghost palette" onclick={() => (app.paletteOpen = true)} title="Command palette (Ctrl+K)" aria-label="Search actions (Ctrl+K)">
    <Icon name="search" size={16} />
    <span class="muted wide">Search…</span>
    <kbd class="wide">Ctrl K</kbd>
  </button>
</header>

<style>
  .topbar {
    height: var(--topbar-h);
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    container-type: inline-size;
  }
  h1 {
    font-size: 15px;
    margin: 0;
    font-weight: 650;
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
  h1,
  .segmented {
    flex: none;
  }
  .slot {
    display: contents;
  }
  .palette {
    border-color: var(--border);
    flex: none;
  }
  kbd {
    margin-left: var(--space-2);
    font-family: var(--font-mono);
    font-size: 11px;
    padding: 1px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-muted);
  }
  /* narrow windows: the palette shrinks to its icon to leave room for actions */
  @container (max-width: 980px) {
    .wide {
      display: none;
    }
    .palette {
      width: 32px;
      padding: 0;
      justify-content: center;
    }
  }
</style>
