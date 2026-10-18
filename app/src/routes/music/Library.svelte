<script lang="ts">
  // Music library (Python ui/dm_music_library_panel.py). Rows share the look of
  // the asset Library's list view (.list-row in app.css).
  import { onDestroy, onMount } from 'svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import Modal from '../../lib/components/Modal.svelte';
  import PlayIndicator from '../../lib/components/PlayIndicator.svelte';
  import {
    TRACK_CATEGORIES, formatDuration, music, youtubeThumb, type ChannelRole, type TagSuggestion, type Track,
    type TrackCategory,
  } from '../../lib/music';
  import { musicUi } from '../../lib/music-ui.svelte';
  import { app } from '../../lib/state.svelte';
  import TrackDialog from './TrackDialog.svelte';

  /** thumb | title | category | tags | length | loop | added | actions | ★ */
  const COLUMNS = '40px minmax(0, 2fr) 104px minmax(0, 1.4fr) 64px 48px 92px 120px 32px';
  const CATEGORY_ICON: Record<TrackCategory, 'music' | 'wind' | 'bell'> = { music: 'music', ambient: 'wind', soundbite: 'bell' };
  const categoryLabel = (c: TrackCategory) => TRACK_CATEGORIES.find((x) => x.id === c)?.label ?? c;

  let tracks = $state<Track[]>([]);
  let allTracks = $state<Track[]>([]);
  let selectedTags = $state<string[]>([]);
  let text = $state('');
  let favoritesOnly = $state(false);
  let category = $state<TrackCategory | null>(null);
  /** Length filter, in seconds: [min, max]. */
  const LENGTHS: { id: string; label: string; min: number | null; max: number | null }[] = [
    { id: 'any', label: 'Any length', min: null, max: null },
    { id: 'short', label: 'Under 1 min', min: null, max: 59.999 },
    { id: 'mid', label: '1–5 min', min: 60, max: 300 },
    { id: 'long', label: '5–15 min', min: 300, max: 900 },
    { id: 'xl', label: 'Over 15 min', min: 900.001, max: null },
  ];
  let length = $state('any');
  let looping = $state<'any' | 'loop' | 'once'>('any');
  type SortKey = 'title' | 'category' | 'duration' | 'loop' | 'added';
  let sort = $state<{ key: SortKey; asc: boolean }>({ key: 'added', asc: false });
  const filtersActive = $derived(
    !!text.trim() || selectedTags.length > 0 || favoritesOnly || category !== null || length !== 'any' || looping !== 'any',
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
          category,
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

  // The category decides where a track goes: soundbites join the soundbite list
  // (random intervals), music and ambient tracks become a channel of that role.
  async function addToMix(t: Track) {
    if (t.category === 'soundbite') {
      await musicUi.act('Could not add the soundbite', () => music.soundbiteAdd(t.id));
    } else {
      const role: ChannelRole = t.category === 'ambient' ? 'ambient' : 'music';
      await musicUi.act('Could not add to the mix', () => music.channelAdd(t.id, { role, volume: 80, loop: t.loop_by_default, play: true }));
    }
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
    const order: TrackCategory[] = ['music', 'ambient', 'soundbite'];
    const by: Record<SortKey, (a: Track, b: Track) => number> = {
      title: (a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: 'base' }),
      category: (a, b) => order.indexOf(a.category) - order.indexOf(b.category),
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
    sort = sort.key === key ? { key, asc: !sort.asc } : { key, asc: key === 'title' || key === 'category' };
  }
  const ariaSort = (key: SortKey) => (sort.key === key ? (sort.asc ? 'ascending' : 'descending') : 'none');
  const arrow = (key: SortKey) => (sort.key === key ? (sort.asc ? ' ▲' : ' ▼') : '');

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
    category = null;
    length = 'any';
    looping = 'any';
    void load();
  }

  // ── tag suggestions (from the titles; nothing changes until Apply) ──
  let suggestOpen = $state(false);
  let suggestions = $state<TagSuggestion[]>([]);
  let suggestPick = $state<Record<string, boolean>>({});
  async function openSuggestions() {
    try {
      suggestions = await music.tagsSuggest();
      suggestPick = Object.fromEntries(suggestions.map((s) => [s.track_id, true]));
      if (!suggestions.length) return app.toast('info', 'No new tags to suggest');
      suggestOpen = true;
    } catch (e) {
      app.error('Could not suggest tags', e);
    }
  }
  async function applySuggestions() {
    try {
      const n = await music.tagsApply(suggestions.filter((s) => suggestPick[s.track_id]));
      suggestOpen = false;
      app.toast('success', `Tagged ${n} track${n === 1 ? '' : 's'}`);
      void load();
    } catch (e) {
      app.error('Could not apply the tags', e);
    }
  }

  const sel = $derived(tracks.find((t) => t.id === selected) ?? null);
</script>

{#snippet thumb(t: Track)}
  {@const yt = t.source === 'youtube' ? youtubeThumb(t.url) : null}
  <span class="list-thumb cover cat-{t.category}" role="cell">
    {#if yt}
      <img src={yt} alt="" loading="lazy" decoding="async" referrerpolicy="no-referrer" />
    {:else}
      <Icon name={CATEGORY_ICON[t.category]} size={18} />
    {/if}
  </span>
{/snippet}

<div class="lib">
  <div class="filters">
    <div class="view-toolbar">
      <input class="input search" type="search" placeholder="Search titles…" bind:value={text} oninput={() => void load()} aria-label="Search tracks" />
      <button class="btn" disabled={!sel} onclick={() => sel && addToMix(sel)}>{sel && flash === sel.id ? 'Added!' : 'Add to Mix'}</button>
      <button class="btn" disabled={!sel} onclick={() => sel && preview(sel)}>▶ Preview</button>
      {#if musicUi.state?.previewing}<button class="btn" onclick={() => void music.stopPreview()}>Stop preview</button>{/if}
      <div class="spacer"></div>
      <button class="btn" onclick={openSuggestions} title="Suggest tags from the track titles">Suggest tags…</button>
      <button class="btn primary" onclick={() => openDialog(null)}>+ Add Track</button>
    </div>
    <div class="filterbar" role="group" aria-label="Filters">
      <div class="segmented" role="group" aria-label="Category">
        <button aria-pressed={category === null} onclick={() => ((category = null), void load())}>All</button>
        {#each TRACK_CATEGORIES as c (c.id)}
          <button aria-pressed={category === c.id} onclick={() => ((category = c.id), void load())}>{c.plural}</button>
        {/each}
      </div>
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
        text="The category decides where Add to Mix puts a track: music and ambient tracks become a channel of that kind, soundbites play at random intervals. Filters combine, and tags must all match. Tracks whose length isn't known yet are hidden while a length filter is on."
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

  <div class="list-wrap">
    {#if tracks.length}
      <div class="list" role="table" aria-label="Tracks">
        <div class="list-head" role="row" style:grid-template-columns={COLUMNS}>
          <span role="columnheader"><span class="visually-hidden">Thumbnail</span></span>
          <span role="columnheader" aria-sort={ariaSort('title')}><button class="sort" onclick={() => sortBy('title')}>Title{arrow('title')}</button></span>
          <span role="columnheader" aria-sort={ariaSort('category')}><button class="sort" onclick={() => sortBy('category')}>Category{arrow('category')}</button></span>
          <span role="columnheader">Tags</span>
          <span role="columnheader" aria-sort={ariaSort('duration')}><button class="sort" onclick={() => sortBy('duration')}>Length{arrow('duration')}</button></span>
          <span role="columnheader" aria-sort={ariaSort('loop')}><button class="sort" onclick={() => sortBy('loop')}>Loop{arrow('loop')}</button></span>
          <span role="columnheader" aria-sort={ariaSort('added')}><button class="sort" onclick={() => sortBy('added')}>Added{arrow('added')}</button></span>
          <span role="columnheader"><span class="visually-hidden">Actions</span></span>
          <span role="columnheader" title="Favourite">★</span>
        </div>
        {#each sorted as t (t.id)}
          <div
            class="list-row"
            class:selected={selected === t.id}
            role="row"
            tabindex="0"
            style:grid-template-columns={COLUMNS}
            onclick={() => (selected = t.id)}
            ondblclick={() => preview(t)}
            onkeydown={(e) => e.key === 'Enter' && preview(t)}
          >
            {@render thumb(t)}
            <span role="cell" class="title-cell">
              <span class="list-title" title={t.title}>
                {#if previewId === t.id && musicUi.state?.previewing}<PlayIndicator status={musicUi.previewStatus ?? 'loading'} />{/if}
                {t.title}
              </span>
              {#if t.source === 'upload'}<span class="list-sub">uploaded file</span>{/if}
            </span>
            <span role="cell"><span class="list-pill"><span class="dot cat-{t.category}"></span>{categoryLabel(t.category)}</span></span>
            <span role="cell" class="list-muted" title={t.tags.join(', ')}>{t.tags.join(' · ')}</span>
            <span role="cell" class="num">{formatDuration(t.duration)}</span>
            <span role="cell">{t.loop_by_default ? 'yes' : 'no'}</span>
            <span role="cell" class="list-muted num">{t.added_at.slice(0, 10)}</span>
            <span role="cell" class="actions">
              <button class="btn ghost" title="Preview 10 s" aria-label="Preview {t.title}" onclick={(e) => (e.stopPropagation(), void preview(t))}>▶</button>
              <button class="btn ghost" title={t.category === 'soundbite' ? 'Add to soundbites' : 'Add to mix'} aria-label="Add {t.title} to mix" onclick={(e) => (e.stopPropagation(), void addToMix(t))}>{flash === t.id ? '✓' : '+'}</button>
              <button class="btn ghost" title="Edit" aria-label="Edit {t.title}" onclick={(e) => (e.stopPropagation(), openDialog(t))}>✎</button>
              <button class="btn ghost" title="Delete" aria-label="Delete {t.title}" onclick={(e) => (e.stopPropagation(), void remove(t))}>✕</button>
            </span>
            <span role="cell">
              <button
                class="list-star"
                class:on={t.favorite}
                aria-pressed={t.favorite}
                aria-label={t.favorite ? `Remove ${t.title} from favourites` : `Add ${t.title} to favourites`}
                title={t.favorite ? 'Favourite' : 'Add to favourites'}
                onclick={(e) => (e.stopPropagation(), void toggleFavorite(t))}
              >{t.favorite ? '★' : '☆'}</button>
            </span>
          </div>
        {/each}
      </div>
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

<Modal title="Suggested tags" bind:open={suggestOpen} width={560}>
  <p class="muted intro">From the track titles. Existing tags are kept; untick a track to skip it.</p>
  <ul class="suggest" data-testid="tag-suggestions">
    {#each suggestions as s (s.track_id)}
      <li>
        <label class="check">
          <input type="checkbox" bind:checked={suggestPick[s.track_id]} />
          <span class="s-title">{s.title}</span>
        </label>
        <span class="s-tags">{#each s.add as g (g)}<span class="list-pill">+ {g}</span>{/each}</span>
      </li>
    {/each}
  </ul>
  {#snippet footer()}
    <button class="btn" onclick={() => (suggestOpen = false)}>Cancel</button>
    {@const picked = suggestions.filter((s) => suggestPick[s.track_id]).length}
    <button class="btn primary" disabled={!picked} onclick={applySuggestions}>
      Add tags to {picked} track{picked === 1 ? '' : 's'}
    </button>
  {/snippet}
</Modal>

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
  .filterbar,
  .tagbar {
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
    padding: 3px 12px;
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
  .list-wrap {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--space-2) var(--space-4) 0;
  }
  .list-head {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--surface-2);
  }
  .list-row {
    cursor: default;
  }
  .list-row:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .title-cell {
    display: grid;
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .actions {
    white-space: nowrap;
    text-align: right;
  }
  .actions .btn {
    padding: 2px 6px;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .dot.cat-music {
    background: var(--accent);
  }
  .dot.cat-ambient {
    background: var(--success);
  }
  .dot.cat-soundbite {
    background: #8b6fd6;
  }
  .list-thumb.cat-music {
    background: color-mix(in srgb, var(--accent) 18%, var(--surface));
  }
  .list-thumb.cat-ambient {
    background: color-mix(in srgb, var(--success) 18%, var(--surface));
  }
  .list-thumb.cat-soundbite {
    background: color-mix(in srgb, #8b6fd6 18%, var(--surface));
  }
  .intro {
    margin: 0 0 var(--space-2);
    font-size: 13px;
  }
  .suggest {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--space-2);
    max-height: 50vh;
    overflow: auto;
  }
  .suggest li {
    display: grid;
    gap: 4px;
    padding-bottom: var(--space-2);
    border-bottom: 1px solid var(--border);
  }
  .s-title {
    font-weight: 600;
  }
  .s-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding-left: 24px;
  }
</style>
