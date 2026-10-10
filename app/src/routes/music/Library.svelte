<script lang="ts">
  // Music library (Python ui/dm_music_library_panel.py).
  import { onDestroy, onMount } from 'svelte';
  import { api } from '../../lib/api';
  import PlayIndicator from '../../lib/components/PlayIndicator.svelte';
  import { formatDuration, music, type Track } from '../../lib/music';
  import { musicUi } from '../../lib/music-ui.svelte';
  import { app } from '../../lib/state.svelte';
  import type { Campaign, Session } from '../../lib/types';
  import TrackDialog from './TrackDialog.svelte';

  let { campaigns, sessions, onCampaignsChanged }: { campaigns: Campaign[]; sessions: Session[]; onCampaignsChanged: () => void } = $props();

  let tracks = $state<Track[]>([]);
  let allTracks = $state<Track[]>([]);
  let campaignId = $state<string | null>(app.campaignId);
  let sessionId = $state<string | null>(null);
  let tagFilter = $state('');
  let selectedTags = $state<string[]>([]);
  let text = $state('');
  let selected = $state<string | null>(null);
  let dialogOpen = $state(false);
  let editing = $state<Track | null>(null);
  let newCampaign = $state('');
  let addingCampaign = $state(false);
  let flash = $state<string | null>(null);
  let seq = 0;

  const allTags = $derived([...new Set(allTracks.flatMap((t) => t.tags))].sort((a, b) => a.localeCompare(b)));
  const campaignSessions = $derived(campaignId ? sessions.filter((s) => s.campaign_id === campaignId) : []);
  const campaignNames = (t: Track) => t.campaign_ids.map((id) => campaigns.find((c) => c.id === id)?.name ?? '?').join(', ');

  async function load() {
    const my = ++seq;
    try {
      const [list, all] = await Promise.all([
        music.tracksList({ campaign_id: campaignId, session_id: sessionId, tags: selectedTags, text }),
        music.tracksList({}),
      ]);
      if (my !== seq) return;
      tracks = [...list].sort((a, b) => b.added_at.localeCompare(a.added_at));
      allTracks = all;
    } catch (e) {
      app.error('Could not load tracks', e);
    }
  }

  onMount(() => void load());
  let seen = app.dataVersion;
  $effect(() => {
    if (app.dataVersion !== seen) {
      seen = app.dataVersion;
      void load();
    }
  });
  // Leaving the Library stops the preview (Python behaviour).
  onDestroy(() => {
    if (musicUi.state?.previewing) void music.stopPreview().catch(() => {});
  });

  function setCampaign(id: string | null) {
    campaignId = id;
    sessionId = null;
    void load();
  }

  async function addCampaign() {
    const name = newCampaign.trim();
    if (!name) return;
    try {
      const c = await api.campaignsCreate(name);
      newCampaign = '';
      addingCampaign = false;
      onCampaignsChanged();
      setCampaign(c.id);
    } catch (e) {
      app.error('Could not create the campaign', e);
    }
  }

  /** Track being previewed (the engine has a single preview slot). */
  let previewId = $state<string | null>(null);
  $effect(() => {
    if (!musicUi.state?.previewing) previewId = null;
  });

  async function preview(t: Track) {
    try {
      previewId = t.id;
      await music.preview(t.id, 10);
      void musicUi.refresh();
    } catch (e) {
      app.error('Could not preview', e);
    }
  }

  async function addToMix(t: Track) {
    await musicUi.act('Could not add to the mix', () => music.channelAdd(t.id, { role: 'music', volume: 80, loop: t.loop_by_default, play: true }));
    flash = t.id;
    setTimeout(() => (flash = null), 1200);
  }

  async function remove(t: Track) {
    if (!confirm(`Delete “${t.title}”?`)) return;
    try {
      await music.trackDelete(t.id);
      if (selected === t.id) selected = null;
      void load();
    } catch (e) {
      app.error('Could not delete the track', e);
    }
  }

  function openDialog(t: Track | null) {
    if (!t && !campaigns.length) return app.toast('info', 'Create a campaign first');
    editing = t;
    dialogOpen = true;
  }

  const sel = $derived(tracks.find((t) => t.id === selected) ?? null);
</script>

<div class="lib">
  <aside class="side">
    <h3>Campaigns</h3>
    <button class="item" aria-pressed={campaignId === null} onclick={() => setCampaign(null)}>All</button>
    {#each campaigns as c (c.id)}
      <button class="item" aria-pressed={campaignId === c.id} onclick={() => setCampaign(c.id)}>{c.name}</button>
    {/each}
    {#if addingCampaign}
      <form class="inline" onsubmit={(e) => (e.preventDefault(), void addCampaign())}>
        <input class="input" bind:value={newCampaign} placeholder="Campaign name" aria-label="New campaign name" />
      </form>
    {:else}
      <button class="item add" onclick={() => (addingCampaign = true)}>+ Campaign</button>
    {/if}

    <h3>Sessions</h3>
    {#if !campaignId}
      <p class="muted hint">Select a campaign to filter by session.</p>
    {:else}
      <button class="item" aria-pressed={sessionId === null} onclick={() => ((sessionId = null), void load())}>All</button>
      {#each campaignSessions as s (s.id)}
        <button class="item" aria-pressed={sessionId === s.id} onclick={() => ((sessionId = s.id), void load())}>{s.name}</button>
      {/each}
    {/if}

    <h3>Tags</h3>
    <div class="tagfilter">
      <input class="input" bind:value={tagFilter} placeholder="filter" aria-label="Filter tags" />
      {#if tagFilter}<button class="btn ghost" aria-label="Clear filter" onclick={() => (tagFilter = '')}>✕</button>{/if}
    </div>
    {#each allTags.filter((t) => t.toLowerCase().includes(tagFilter.toLowerCase())) as t (t)}
      <label class="tag">
        <input
          type="checkbox"
          checked={selectedTags.includes(t)}
          onchange={() => ((selectedTags = selectedTags.includes(t) ? selectedTags.filter((x) => x !== t) : [...selectedTags, t]), void load())}
        />
        {t}
      </label>
    {/each}
    {#if selectedTags.length}
      <button class="btn ghost" onclick={() => ((selectedTags = []), void load())}>Clear</button>
    {/if}
  </aside>

  <section class="main">
    <div class="bar">
      <input class="input search" type="search" placeholder="Search titles…" bind:value={text} oninput={() => void load()} aria-label="Search tracks" />
      <button class="btn primary" onclick={() => openDialog(null)}>+ Add Track</button>
      <button class="btn" disabled={!sel} onclick={() => sel && addToMix(sel)}>{sel && flash === sel.id ? 'Added!' : 'Add to Mix'}</button>
      <button class="btn" disabled={!sel} onclick={() => sel && preview(sel)}>▶ Preview</button>
      {#if musicUi.state?.previewing}<button class="btn" onclick={() => void music.stopPreview()}>Stop preview</button>{/if}
    </div>
    <div class="table-wrap">
      <table>
        <thead>
          <tr><th>Title</th><th>Tags</th><th>Length</th><th>Loop</th><th>Campaign(s)</th><th class="actions"></th></tr>
        </thead>
        <tbody>
          {#each tracks as t (t.id)}
            <tr
              class:selected={selected === t.id}
              onclick={() => (selected = t.id)}
              ondblclick={() => preview(t)}
              tabindex="0"
              onkeydown={(e) => e.key === 'Enter' && preview(t)}
            >
              <td>
                {#if previewId === t.id && musicUi.state?.previewing}<PlayIndicator status={musicUi.previewStatus ?? 'loading'} />{/if}
                {t.source === 'upload' ? '⬆ ' : ''}{t.title}
              </td>
              <td class="muted">{t.tags.join(', ')}</td>
              <td>{formatDuration(t.duration)}</td>
              <td>{t.loop_by_default ? 'yes' : 'no'}</td>
              <td class="muted">{campaignNames(t)}</td>
              <td class="actions">
                <button class="btn ghost" title="Preview 10 s" aria-label="Preview {t.title}" onclick={(e) => (e.stopPropagation(), void preview(t))}>▶</button>
                <button class="btn ghost" title="Add to mix" aria-label="Add {t.title} to mix" onclick={(e) => (e.stopPropagation(), void addToMix(t))}>{flash === t.id ? '✓' : '+'}</button>
                <button class="btn ghost" title="Edit" aria-label="Edit {t.title}" onclick={(e) => (e.stopPropagation(), openDialog(t))}>✎</button>
                <button class="btn ghost" title="Delete" aria-label="Delete {t.title}" onclick={(e) => (e.stopPropagation(), void remove(t))}>✕</button>
              </td>
            </tr>
          {:else}
            <tr><td colspan="6" class="muted empty">No tracks. Add a YouTube link or upload an audio file.</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    <div class="status muted">
      {#if musicUi.state?.previewing}
        <PlayIndicator status={musicUi.previewStatus ?? 'loading'} />
        {musicUi.previewStatus === 'playing' ? 'Previewing…' : 'Loading preview…'}
      {:else}{tracks.length} track(s){/if}
    </div>
  </section>
</div>

<TrackDialog bind:open={dialogOpen} track={editing} {campaigns} {sessions} {allTags} onSaved={() => void load()} />

<style>
  .lib {
    display: grid;
    grid-template-columns: 200px 1fr;
    height: 100%;
    min-height: 0;
  }
  .side {
    border-right: 1px solid var(--border);
    padding: var(--space-3);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  h3 {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    margin: var(--space-3) 0 4px;
  }
  h3:first-child {
    margin-top: 0;
  }
  .item {
    text-align: left;
    border: 0;
    background: transparent;
    color: var(--text);
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font: inherit;
    font-size: 13px;
  }
  .item:hover {
    background: var(--surface-2);
  }
  .item[aria-pressed='true'] {
    background: var(--accent-soft);
  }
  .item.add {
    color: var(--text-muted);
  }
  .hint {
    font-size: 12px;
    margin: 0;
  }
  .tagfilter {
    display: flex;
    gap: 4px;
  }
  .tag {
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: 13px;
    padding: 2px 8px;
  }
  .main {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .bar {
    display: flex;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    align-items: center;
  }
  .search {
    flex: 1;
    max-width: 320px;
  }
  .table-wrap {
    flex: 1;
    overflow: auto;
    padding: 0 var(--space-4);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  th {
    text-align: left;
    font-weight: 600;
    color: var(--text-muted);
    font-size: 12px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: var(--bg);
  }
  td {
    padding: 4px 8px;
  }
  tbody tr:nth-child(even) {
    background: var(--surface);
  }
  tbody tr {
    cursor: default;
  }
  tbody tr.selected {
    background: var(--accent-soft);
  }
  .actions {
    white-space: nowrap;
    text-align: right;
  }
  .actions .btn {
    padding: 2px 6px;
  }
  .empty {
    padding: var(--space-4);
  }
  .status {
    padding: var(--space-2) var(--space-4);
    font-size: 12px;
    border-top: 1px solid var(--border);
  }
</style>
