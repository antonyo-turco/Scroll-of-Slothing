<script lang="ts">
  // Music library (Python ui/dm_music_library_panel.py).
  import { onDestroy, onMount } from 'svelte';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import PlayIndicator from '../../lib/components/PlayIndicator.svelte';
  import { formatDuration, music, type Track } from '../../lib/music';
  import { musicUi } from '../../lib/music-ui.svelte';
  import { app } from '../../lib/state.svelte';
  import TrackDialog from './TrackDialog.svelte';

  let tracks = $state<Track[]>([]);
  let allTracks = $state<Track[]>([]);
  let selectedTags = $state<string[]>([]);
  let text = $state('');
  let favoritesOnly = $state(false);
  /** Length filter, in seconds: [min, max]. */
  const LENGTHS: { id: string; label: string; min: number | null; max: number | null }[] = [
    { id: 'any', label: 'Any length', min: null, max: null },
    { id: 'short', label: 'Under 1 min', min: null, max: 59.999 },
    { id: 'mid', label: '1–5 min', min: 60, max: 300 },
    { id: 'long', label: '5–15 min', min: 300, max: 900 },
    { id: 'xl', label: 'Over 15 min', min: 900.001, max: null },
  ];
  let length = $state('any');
  /** 'any' | 'loop' | 'once' */
  let looping = $state<'any' | 'loop' | 'once'>('any');
  type SortKey = 'title' | 'duration' | 'loop' | 'added';
  let sort = $state<{ key: SortKey; asc: boolean }>({ key: 'added', asc: false });
  const filtersActive = $derived(
    !!text.trim() || selectedTags.length > 0 || favoritesOnly || length !== 'any' || looping !== 'any',
  );
  let selected = $state<string | null>(null);
  let dialogOpen = $state(false);
  let editing = $state<Track | null>(null);
  let flash = $state<string | null>(null);
  let seq = 0;

  const allTags = $derived([...new Set(allTracks.flatMap((t) => t.tags))].sort((a, b) => a.localeCompare(b)));

  async function load() {
    const my = ++seq;
    try {
      const len = LENGTHS.find((l) => l.id === length) ?? LENGTHS[0];
      const [list, all] = await Promise.all([
        music.tracksList({
          tags: selectedTags,
          text,
          favorites_only: favoritesOnly,
          looping: looping === 'any' ? null : looping === 'loop',
          min_duration: len.min,
          max_duration: len.max,
        }),
        music.tracksList({}),
      ]);
      if (my !== seq) return;
      tracks = list;
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

  // Sorted on the client: the list is already filtered and small.
  const sorted = $derived.by(() => {
    const dir = sort.asc ? 1 : -1;
    const by: Record<SortKey, (a: Track, b: Track) => number> = {
      title: (a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: 'base' }),
      // Unknown lengths go last either way.
      duration: (a, b) => (a.duration ?? Infinity) - (b.duration ?? Infinity),
      loop: (a, b) => Number(a.loop_by_default) - Number(b.loop_by_default),
      added: (a, b) => a.added_at.localeCompare(b.added_at),
    };
    const unknownLast = (a: Track, b: Track) =>
      sort.key === 'duration' ? Number(a.duration == null) - Number(b.duration == null) : 0;
    return [...tracks].sort((a, b) => unknownLast(a, b) || dir * by[sort.key](a, b) || b.added_at.localeCompare(a.added_at));
  });

  function sortBy(key: SortKey) {
    sort = sort.key === key ? { key, asc: !sort.asc } : { key, asc: key === 'title' };
  }
  const ariaSort = (key: SortKey) => (sort.key === key ? (sort.asc ? 'ascending' : 'descending') : 'none');

  async function toggleFavorite(t: Track) {
    try {
      const u = await music.trackSetFavorite(t.id, !t.favorite);
      tracks = tracks.map((x) => (x.id === u.id ? u : x));
      allTracks = allTracks.map((x) => (x.id === u.id ? u : x));
      if (favoritesOnly && !u.favorite) void load();
    } catch (e) {
      app.error('Could not change the favourite', e);
    }
  }

  function clearFilters() {
    text = '';
    selectedTags = [];
    favoritesOnly = false;
    length = 'any';
    looping = 'any';
    void load();
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
    <div class="filterbar" role="group" aria-label="Filters">
      <button class="chip" aria-pressed={favoritesOnly} onclick={() => ((favoritesOnly = !favoritesOnly), void load())}>★ Favourites</button>
      <select class="input compact" aria-label="Length" bind:value={length} onchange={() => void load()}>
        {#each LENGTHS as l (l.id)}<option value={l.id}>{l.label}</option>{/each}
      </select>
      <select class="input compact" aria-label="Loop" bind:value={looping} onchange={() => void load()}>
        <option value="any">Loop or once</option>
        <option value="loop">Loops</option>
        <option value="once">Plays once</option>
      </select>
      <InfoTip
        label="About the filters"
        text="Filters combine: tags must all match. Tracks whose length isn't known yet (YouTube links not played so far) are hidden while a length filter is on."
      />
      {#if filtersActive}<button class="btn ghost clear" onclick={clearFilters}>Clear filters</button>{/if}
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
      </div>
    {/if}
  </div>

  <div class="table-wrap">
    {#if tracks.length}
      <table>
        <thead>
          <tr>
            <th class="fav"><span class="visually-hidden">Favourite</span></th>
            <th aria-sort={ariaSort('title')}><button class="sort" onclick={() => sortBy('title')}>Title{sort.key === 'title' ? (sort.asc ? ' ▲' : ' ▼') : ''}</button></th>
            <th>Tags</th>
            <th aria-sort={ariaSort('duration')}><button class="sort" onclick={() => sortBy('duration')}>Length{sort.key === 'duration' ? (sort.asc ? ' ▲' : ' ▼') : ''}</button></th>
            <th aria-sort={ariaSort('loop')}><button class="sort" onclick={() => sortBy('loop')}>Loop{sort.key === 'loop' ? (sort.asc ? ' ▲' : ' ▼') : ''}</button></th>
            <th aria-sort={ariaSort('added')}><button class="sort" onclick={() => sortBy('added')}>Added{sort.key === 'added' ? (sort.asc ? ' ▲' : ' ▼') : ''}</button></th>
            <th class="actions"><span class="visually-hidden">Actions</span></th>
          </tr>
        </thead>
        <tbody>
          {#each sorted as t (t.id)}
            <tr
              class:selected={selected === t.id}
              onclick={() => (selected = t.id)}
              ondblclick={() => preview(t)}
              tabindex="0"
              onkeydown={(e) => e.key === 'Enter' && preview(t)}
            >
              <td class="fav">
                <button
                  class="star"
                  class:on={t.favorite}
                  aria-pressed={t.favorite}
                  aria-label={t.favorite ? `Remove ${t.title} from favourites` : `Add ${t.title} to favourites`}
                  title={t.favorite ? 'Favourite' : 'Add to favourites'}
                  onclick={(e) => (e.stopPropagation(), void toggleFavorite(t))}
                >{t.favorite ? '★' : '☆'}</button>
              </td>
              <td>
                {#if previewId === t.id && musicUi.state?.previewing}<PlayIndicator status={musicUi.previewStatus ?? 'loading'} />{/if}
                {t.source === 'upload' ? '⬆ ' : ''}{t.title}
              </td>
              <td class="muted">{t.tags.join(', ')}</td>
              <td>{formatDuration(t.duration)}</td>
              <td>{t.loop_by_default ? 'yes' : 'no'}</td>
              <td class="muted">{t.added_at.slice(0, 10)}</td>
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
        {#if filtersActive}
          <strong>No tracks match</strong>
          <span>Try another search, or clear the filters.</span>
          <button class="btn" onclick={clearFilters}>Clear filters</button>
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
    {:else}{tracks.length} track{tracks.length === 1 ? '' : 's'}{filtersActive ? ` of ${allTracks.length}` : ''} · double-click a row to preview{/if}
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
  .filterbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-4) var(--space-2);
  }
  .compact {
    width: auto;
    min-height: 28px;
    padding-block: 0;
    font-size: 12px;
  }
  .chip,
  .tag {
    border: 1px solid var(--border);
    background: var(--surface);
    border-radius: 999px;
    padding: 2px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .chip {
    background: var(--surface);
    border-radius: 999px;
    padding: 3px 12px;
    font-size: 12px;
    cursor: pointer;
  }
  .chip[aria-pressed='true'],
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
  .sort {
    border: 0;
    padding: 0;
    background: none;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .sort:hover {
    color: var(--text);
  }
  th.fav,
  td.fav {
    width: 28px;
    padding-inline: 4px;
  }
  .star {
    border: 0;
    background: none;
    padding: 0 2px;
    font-size: 16px;
    line-height: 1;
    color: var(--text-muted);
    cursor: pointer;
  }
  .star.on {
    color: #e0a400;
  }
  .actions .btn {
    padding: 2px 6px;
  }
</style>
