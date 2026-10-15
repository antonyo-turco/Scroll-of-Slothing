<script lang="ts">
  // Music library (Python ui/dm_music_library_panel.py).
  import { onDestroy, onMount } from 'svelte';
  import PlayIndicator from '../../lib/components/PlayIndicator.svelte';
  import { formatDuration, music, type Track } from '../../lib/music';
  import { musicUi } from '../../lib/music-ui.svelte';
  import { app } from '../../lib/state.svelte';
  import TrackDialog from './TrackDialog.svelte';

  let tracks = $state<Track[]>([]);
  let allTracks = $state<Track[]>([]);
  let selectedTags = $state<string[]>([]);
  let text = $state('');
  let selected = $state<string | null>(null);
  let dialogOpen = $state(false);
  let editing = $state<Track | null>(null);
  let flash = $state<string | null>(null);
  let seq = 0;

  const allTags = $derived([...new Set(allTracks.flatMap((t) => t.tags))].sort((a, b) => a.localeCompare(b)));

  async function load() {
    const my = ++seq;
    try {
      const [list, all] = await Promise.all([
        music.tracksList({ tags: selectedTags, text }),
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
    editing = t;
    dialogOpen = true;
  }

  const sel = $derived(tracks.find((t) => t.id === selected) ?? null);
</script>

<div class="lib">
  <div class="filters">
    <div class="view-toolbar">
      <input class="input search" type="search" placeholder="Search titles…" bind:value={text} oninput={() => void load()} aria-label="Search tracks" />
      <button class="btn" disabled={!sel} onclick={() => sel && addToMix(sel)}>{sel && flash === sel.id ? 'Added!' : 'Add to Mix'}</button>
      <button class="btn" disabled={!sel} onclick={() => sel && preview(sel)}>▶ Preview</button>
      {#if musicUi.state?.previewing}<button class="btn" onclick={() => void music.stopPreview()}>Stop preview</button>{/if}
      <div class="spacer"></div>
      <button class="btn primary" onclick={() => openDialog(null)}>+ Add Track</button>
    </div>
    {#if allTags.length}
      <div class="tagbar" role="group" aria-label="Tag filter">
        {#each allTags as t (t)}
          <button
            class="tag"
            aria-pressed={selectedTags.includes(t)}
            onclick={() => ((selectedTags = selectedTags.includes(t) ? selectedTags.filter((x) => x !== t) : [...selectedTags, t]), void load())}
          >{t}</button>
        {/each}
        {#if selectedTags.length}
          <button class="btn ghost clear" onclick={() => ((selectedTags = []), void load())}>Clear filters</button>
        {/if}
      </div>
    {/if}
  </div>

  <div class="table-wrap">
    {#if tracks.length}
      <table>
        <thead>
          <tr><th>Title</th><th>Tags</th><th>Length</th><th>Loop</th><th class="actions"><span class="visually-hidden">Actions</span></th></tr>
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
              <td class="actions">
                <button class="btn ghost" title="Preview 10 s" aria-label="Preview {t.title}" onclick={(e) => (e.stopPropagation(), void preview(t))}>▶</button>
                <button class="btn ghost" title="Add to mix" aria-label="Add {t.title} to mix" onclick={(e) => (e.stopPropagation(), void addToMix(t))}>{flash === t.id ? '✓' : '+'}</button>
                <button class="btn ghost" title="Edit" aria-label="Edit {t.title}" onclick={(e) => (e.stopPropagation(), openDialog(t))}>✎</button>
                <button class="btn ghost" title="Delete" aria-label="Delete {t.title}" onclick={(e) => (e.stopPropagation(), void remove(t))}>✕</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <div class="empty-state">
        {#if text || selectedTags.length}
          <strong>No tracks match</strong>
          <span>Try another search, or clear the tag filters.</span>
        {:else}
          <strong>No tracks yet</strong>
          <span>Add a YouTube link or upload an audio file.</span>
          <button class="btn primary" onclick={() => openDialog(null)}>+ Add Track</button>
        {/if}
      </div>
    {/if}
  </div>
  <div class="statusbar">
    {#if musicUi.state?.previewing}
      <PlayIndicator status={musicUi.previewStatus ?? 'loading'} />
      {musicUi.previewStatus === 'playing' ? 'Previewing…' : 'Loading preview…'}
    {:else}{tracks.length} track{tracks.length === 1 ? '' : 's'} · double-click a row to preview{/if}
  </div>
</div>

<TrackDialog bind:open={dialogOpen} track={editing} {allTags} onSaved={() => void load()} />

<style>
  .lib {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .filters {
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .filters .view-toolbar {
    border-bottom: 0;
  }
  .tagbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-4) var(--space-2);
  }
  .tag {
    border: 1px solid var(--border);
    background: var(--surface);
    border-radius: 999px;
    padding: 2px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .tag[aria-pressed='true'] {
    background: var(--accent-soft);
    border-color: var(--accent);
    font-weight: 600;
  }
  .clear {
    font-size: 12px;
    min-height: 26px;
  }
  .search {
    flex: 1 1 200px;
    max-width: 320px;
  }
  .table-wrap {
    flex: 1;
    min-height: 0;
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
</style>
