<script lang="ts">
  // Asset library (merges the old "Assets" and "Tagged Assets" tabs).
  // Spec: docs/features/library.md
  import { onMount } from 'svelte';
  import { api, IMAGE_EXTENSIONS, isTauri, onFileDrop, pickDirectory, pickFiles } from '../../lib/api';
  import Icon from '../../lib/components/Icon.svelte';
  import VirtualGrid from '../../lib/components/VirtualGrid.svelte';
  import { LibraryState, SORTS } from '../../lib/library.svelte';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import { app } from '../../lib/state.svelte';
  import ViewActions from '../../lib/components/ViewActions.svelte';
  import { ASSET_KINDS, KIND_LABELS, type Asset } from '../../lib/types';
  import AssetCard, { ROW_COLUMNS, VARIANT_SIZE, type CardVariant } from './AssetCard.svelte';
  import AssetInspector from './AssetInspector.svelte';
  import ImportDialog from './ImportDialog.svelte';
  import TagManager from './TagManager.svelte';

  const lib = new LibraryState();
  let importOpen = $state(false);
  let importPaths = $state<string[]>([]);
  let tagsOpen = $state(false);
  let bulkTag = $state('');
  let confirmBulkDelete = $state(false);
  let search: HTMLInputElement | undefined = $state();

  const selecting = $derived(lib.selected.size > 0);

  // View mode: big cards, a detail list, or compact pills. Remembered per user.
  const VIEW_KEY = 'library.view';
  const VIEWS: { id: CardVariant; label: string; icon: 'grid' | 'list' | 'pills' }[] = [
    { id: 'card', label: 'Cards', icon: 'grid' },
    { id: 'row', label: 'List', icon: 'list' },
    { id: 'pill', label: 'Pills', icon: 'pills' },
  ];
  let view = $state<CardVariant>('card');
  const size = $derived(VARIANT_SIZE[view]);
  function setView(v: CardVariant) {
    view = v;
    void api.workspaceSet(VIEW_KEY, v).catch(() => {});
  }
  const topTags = $derived(lib.allTags.filter((t) => t.count > 0).slice(0, 24));

  onMount(() => {
    void api.workspaceGet<CardVariant>(VIEW_KEY).then((v) => {
      if (v && v in VARIANT_SIZE) view = v;
    }).catch(() => {});
    void lib.loadReference().then(() => lib.refresh());
    let unlisten: (() => void) | undefined;
    void onFileDrop((paths) => {
      const imgs = paths.filter((p) => IMAGE_EXTENSIONS.includes(p.split('.').pop()?.toLowerCase() ?? ''));
      if (imgs.length) openImport(imgs);
    }).then((u) => (unlisten = u));
    return () => unlisten?.();
  });

  // Refetch when anything changes in the store (this or another process, e.g. MCP).
  let seenVersion = app.dataVersion;
  $effect(() => {
    if (app.dataVersion !== seenVersion) {
      seenVersion = app.dataVersion;
      void lib.loadReference();
      lib.schedule(0);
    }
  });

  async function chooseFiles() {
    const files = await pickFiles('Import images', IMAGE_EXTENSIONS);
    if (files?.length) openImport(files);
    else if (!isTauri()) app.toast('info', 'File dialogs are only available in the desktop app.');
  }

  function openImport(paths: string[]) {
    importPaths = paths;
    importOpen = true;
  }

  async function act(what: string, f: () => Promise<unknown>, done?: string) {
    try {
      await f();
      if (done) app.toast('success', done);
      await lib.loadReference();
      await lib.refresh();
    } catch (e) {
      app.error(what, e);
    }
  }

  const ids = () => [...lib.selected];

  // Like Python: tokens go to the token sheet, cut-outs to the object sheet, maps are skipped.
  async function bulkAddToSheet() {
    const pick = (kind: string) => lib.items.filter((a) => lib.selected.has(a.id) && a.kind === kind).map((a) => a.id);
    const [tokens, cutouts] = [pick('token'), pick('cutout')];
    const skipped = lib.selected.size - tokens.length - cutouts.length;
    if (!tokens.length && !cutouts.length) return app.toast('info', 'Maps cannot be added to a sheet.');
    try {
      if (tokens.length) await api.tokenSheetAddAssets(tokens);
      if (cutouts.length) await api.objectSheetAddAssets(cutouts);
      const parts = [
        tokens.length ? `${tokens.length} to the token sheet` : '',
        cutouts.length ? `${cutouts.length} to the object sheet` : '',
      ].filter(Boolean);
      app.toast('success', `Added ${parts.join(' and ')}` + (skipped ? ` · skipped ${skipped} map(s)` : ''));
    } catch (e) {
      app.error('Could not add to sheet', e);
    }
  }

  async function bulkExport() {
    const dir = await pickDirectory('Export to folder');
    if (!dir) return;
    await act('Export failed', () => api.libraryExportMany(ids(), dir), `Exported ${lib.selected.size} image(s)`);
  }

  async function togglePin(a: Asset) {
    try {
      lib.patchItem(await api.libraryUpdate(a.id, { pinned: !a.pinned }));
      if (lib.pinnedOnly) void lib.refresh();
    } catch (e) {
      app.error('Could not pin', e);
    }
  }

  async function setSize(a: Asset, size: string) {
    try {
      lib.patchItem(await api.libraryUpdate(a.id, { size_label: size }));
    } catch (e) {
      app.error('Could not change size', e);
    }
  }

  function onKey(e: KeyboardEvent) {
    const typing = (e.target as HTMLElement)?.matches?.('input, textarea, select');
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      search?.focus();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a' && !typing) {
      e.preventDefault();
      lib.selectAll();
    } else if (e.key === 'Escape' && typing) {
      // First Escape leaves the field (committing via blur); the next one closes.
      (e.target as HTMLElement).blur();
    } else if (e.key === 'Escape') {
      if (lib.selected.size) lib.clearSelection();
      else lib.activeId = null;
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<ViewActions>
  <button class="btn" onclick={() => (tagsOpen = true)}>Manage tags…</button>
  <button class="btn primary" onclick={chooseFiles}>+ Import</button>
</ViewActions>

<div class="view">
  <div class="work">
    <div class="filters">
    <div class="view-toolbar toolbar">
      <div class="search">
        <Icon name="search" size={16} />
        <input
          bind:this={search}
          bind:value={lib.q}
          oninput={() => lib.schedule()}
          placeholder="Search names, aliases, tags…"
          aria-label="Search library"
        />
      </div>
      <div class="segmented" role="group" aria-label="Kind">
        <button aria-pressed={lib.kinds.length === 0} onclick={() => lib.toggleKind(null)}>All</button>
        {#each ASSET_KINDS as k (k)}
          <button aria-pressed={lib.kinds.includes(k)} onclick={() => lib.toggleKind(k)}>{KIND_LABELS[k]}</button>
        {/each}
      </div>
      <label class="toggle">
        <input type="checkbox" bind:checked={lib.pinnedOnly} onchange={() => lib.refresh()} /> ★ Pinned
      </label>
      <InfoTip label="About the library" text="Search matches names, aliases and tags. Tags combine (all must match). Ctrl+click or Shift+click selects several assets for bulk actions. ★ pins an asset so you can find it quickly." />
      <select class="input" aria-label="Sort" bind:value={lib.sort} onchange={() => lib.refresh()}>
        {#each SORTS as s (s.id)}<option value={s.id}>{s.label}</option>{/each}
      </select>
      <div class="segmented views" role="group" aria-label="View">
        {#each VIEWS as v (v.id)}
          <button aria-pressed={view === v.id} aria-label="{v.label} view" title="{v.label} view" onclick={() => setView(v.id)}>
            <Icon name={v.icon} size={16} />
          </button>
        {/each}
      </div>
    </div>

    {#if topTags.length}
      <div class="tagbar" role="group" aria-label="Tag filter">
        {#each topTags as t (t.id)}
          <button
            class="tag"
            aria-pressed={lib.tags.some((x) => x.toLowerCase() === t.name.toLowerCase())}
            onclick={() => lib.toggleTag(t.name)}>{t.name} <span class="muted">{t.count}</span></button
          >
        {/each}
        {#if lib.filtersActive}<button class="btn ghost clear" onclick={() => lib.clearFilters()}>Clear filters</button>{/if}
      </div>
    {/if}
    </div>

    {#if selecting}
      <div class="bulk" role="toolbar" aria-label="Bulk actions">
        <strong>{lib.selected.size} selected</strong>
        <input class="input" placeholder="Tag…" aria-label="Tag to add" bind:value={bulkTag} />
        <button class="btn" disabled={!bulkTag.trim()} onclick={() => act('Tagging failed', () => api.libraryBulkTag(ids(), bulkTag.trim())).then(() => (bulkTag = ''))}>Add tag</button>
        <button class="btn" onclick={() => act('Pin failed', () => api.libraryBulkPin(ids(), true))}>Pin</button>
        <button class="btn" onclick={() => act('Unpin failed', () => api.libraryBulkPin(ids(), false))}>Unpin</button>
        <button class="btn" onclick={bulkAddToSheet}>Add to sheets</button>
        <button class="btn" onclick={bulkExport}>Export…</button>
        {#if confirmBulkDelete}
          <button class="btn danger" onclick={() => act('Delete failed', () => api.libraryDelete(ids()), 'Deleted').then(() => (confirmBulkDelete = false))}>
            Delete {lib.selected.size}
          </button>
          <button class="btn" onclick={() => (confirmBulkDelete = false)}>Cancel</button>
        {:else}
          <button class="btn ghost danger" onclick={() => (confirmBulkDelete = true)}>Delete…</button>
        {/if}
        <div class="spacer"></div>
        <button class="btn ghost" onclick={() => lib.selectAll()}>Select all</button>
        <button class="btn ghost" onclick={() => lib.clearSelection()}>Done</button>
      </div>
    {/if}

    <div class="grid-area" aria-busy={lib.loading}>
      {#if lib.loaded && lib.items.length === 0}
        <div class="empty">
          {#if lib.filtersActive}
            <p>No assets match these filters.</p>
            <button class="btn" onclick={() => lib.clearFilters()}>Clear filters</button>
          {:else}
            <p><strong>Your library is empty.</strong></p>
            <p class="muted">Import images, drop files here, or import your Orb of Many Functions data from Settings.</p>
            <button class="btn primary" onclick={chooseFiles}>+ Import images</button>
          {/if}
        </div>
      {:else}
        {#if view === 'row'}
          <div class="list-head" style:grid-template-columns={ROW_COLUMNS} aria-hidden="true">
            <span></span><span>Name</span><span>Kind</span><span>Size</span><span>Tags</span><span>Updated</span><span>★</span>
          </div>
        {/if}
        <VirtualGrid
          items={lib.items}
          itemWidth={size.w}
          itemHeight={size.h}
          gap={view === 'row' ? 0 : view === 'pill' ? 8 : 12}
          fill={view === 'row'}
          key={(a) => a.id}
        >
          {#snippet item(a: Asset)}
            <AssetCard
              variant={view}
              asset={a}
              selected={lib.selected.has(a.id)}
              active={lib.activeId === a.id}
              {selecting}
              onclick={(e) => lib.click(a.id, e)}
              onpin={() => togglePin(a)}
              onsize={(s) => setSize(a, s)}
            />
          {/snippet}
        </VirtualGrid>
      {/if}
    </div>
    <div class="statusbar" aria-live="polite">
      {lib.total} asset{lib.total === 1 ? '' : 's'}{lib.loading ? ' · loading…' : ''} · Ctrl+click to select several
    </div>
  </div>

  {#if lib.active}
    <AssetInspector {lib} asset={lib.active} />
  {:else}
    <aside class="inspector" aria-label="Asset details">
      <div class="empty-state">
        <strong>No asset selected</strong>
        <span>Click an asset to edit its name, tags and size, open it in its editor, or export it.</span>
        <span>Ctrl+click or Shift+click selects several for bulk actions.</span>
      </div>
    </aside>
  {/if}
</div>

<ImportDialog bind:open={importOpen} paths={importPaths} ondone={() => act('Refresh failed', async () => {})} />
<TagManager bind:open={tagsOpen} onchange={() => act('Refresh failed', async () => {})} />

<style>
  .filters {
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .filters .toolbar {
    border-bottom: 0;
  }
  .tagbar,
  .bulk {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    flex-wrap: wrap;
  }
  .search {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 160px;
    flex: 1 1 200px;
    max-width: 360px;
    min-height: 32px;
    padding: 0 var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text-muted);
  }
  .search input {
    flex: 1;
    border: 0;
    background: transparent;
    color: var(--text);
  }
  .search input:focus {
    outline: none;
  }
  .search:focus-within {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 4px;
    cursor: pointer;
    white-space: nowrap;
  }
  .toggle input {
    accent-color: var(--accent);
  }
  .tagbar {
    padding-top: 0;
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
  .bulk {
    margin: var(--space-2) var(--space-4) 0;
    border: 1px solid var(--focus);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .grid-area {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: var(--space-2) 0 0 var(--space-4);
  }
  /* The list header takes its share, so the scroller fills the rest. */
  .grid-area > :global(.viewport) {
    flex: 1;
    height: auto;
    min-height: 0;
    padding-right: var(--space-2);
  }
  .list-head {
    display: grid;
    gap: var(--space-3);
    padding: 0 calc(var(--space-2) + 1px) 6px;
    margin-right: var(--space-2);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .views button {
    display: inline-flex;
    align-items: center;
    padding: 0 var(--space-2);
  }
  .empty {
    height: 100%;
    display: grid;
    place-content: center;
    justify-items: center;
    text-align: center;
    gap: var(--space-2);
  }
  .empty p {
    margin: 0;
  }
</style>
