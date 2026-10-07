<script lang="ts">
  import { api } from '../api';
  import { findView } from '../nav';
  import { app } from '../state.svelte';
  import type { Campaign } from '../types';
  import Icon from './Icon.svelte';

  const found = $derived(findView(app.route));
  let campaigns = $state<Campaign[]>([]);

  // Reload the campaign list whenever data changes (also on first render).
  $effect(() => {
    void app.dataVersion;
    void api.campaignsList().then((c) => (campaigns = c)).catch(() => {});
  });
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
  <div class="spacer"></div>
  {#if campaigns.length}
    <select
      class="input campaign"
      aria-label="Active campaign"
      title="Active campaign: default filter for the Library"
      value={app.campaignId ?? ''}
      onchange={(e) => app.setCampaign(e.currentTarget.value || null)}
    >
      <option value="">No active campaign</option>
      {#each campaigns as c (c.id)}<option value={c.id}>{c.name}</option>{/each}
    </select>
  {/if}
  <button class="btn ghost palette" onclick={() => (app.paletteOpen = true)} title="Command palette (Ctrl+K)">
    <Icon name="search" size={16} />
    <span class="muted">Search actions…</span>
    <kbd>Ctrl K</kbd>
  </button>
</header>

<style>
  .topbar {
    height: var(--topbar-h);
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--space-4);
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  h1 {
    font-size: 15px;
    margin: 0;
    font-weight: 650;
  }
  .spacer {
    flex: 1;
  }
  .campaign {
    max-width: 220px;
  }
  .palette {
    border-color: var(--border);
    min-width: 220px;
  }
  kbd {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 11px;
    padding: 1px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-muted);
  }
</style>
