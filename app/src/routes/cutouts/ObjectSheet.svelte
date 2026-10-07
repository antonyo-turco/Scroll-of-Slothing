<script lang="ts">
  // Object Sheet: cut-outs laid out flat or as fold-over standees. Spec: docs/features/object-sheet.md.
  import { onMount } from 'svelte';
  import { api, blobUrl, IMAGE_EXTENSIONS, isTauri, onFileDrop, pickFiles, pickSavePath } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { SIZE_LABELS, type ObjectSheet, type PageFormat, type SheetLayout, type SheetMode } from '../../lib/types';

  let sheet = $state<ObjectSheet>({ items: [], format: 'Letter', mode: 'flat' });
  let layout = $state<SheetLayout | null>(null);
  let page = $state(0);
  let confirmClear = $state(false);
  let loaded = $state(false);

  const byId = $derived(new Map(sheet.items.map((i) => [i.id, i])));
  const total = $derived(sheet.items.reduce((n, i) => n + i.copies, 0));
  const pages = $derived(layout?.pages.length ?? 0);
  const skipped = $derived((layout?.skipped ?? []).map((id) => byId.get(id)?.label ?? id));

  async function apply(p: Promise<ObjectSheet>, what = 'Update failed') {
    try {
      sheet = await p;
      layout = await api.objectSheetLayout();
      if (page >= (layout?.pages.length ?? 0)) page = Math.max(0, (layout?.pages.length ?? 1) - 1);
    } catch (e) {
      app.error(what, e);
    }
  }

  let unlisten: (() => void) | undefined;
  onMount(() => {
    void apply(api.objectSheetGet(), 'Could not load the object sheet').then(() => (loaded = true));
    void onFileDrop((paths) => {
      const imgs = paths.filter((p) => IMAGE_EXTENSIONS.includes(p.split('.').pop()?.toLowerCase() ?? ''));
      if (imgs.length) void apply(api.objectSheetAddFiles(imgs), 'Could not add files');
    }).then((u) => (unlisten = u));
    return () => unlisten?.();
  });

  let seen = app.dataVersion;
  $effect(() => {
    if (app.dataVersion !== seen) {
      seen = app.dataVersion;
      void apply(api.objectSheetGet());
    }
  });

  async function upload() {
    const files = await pickFiles('Add cut-out images', IMAGE_EXTENSIONS);
    if (files?.length) await apply(api.objectSheetAddFiles(files), 'Could not add files');
    else if (!isTauri()) app.toast('info', 'File dialogs are only available in the desktop app.');
  }

  function setNumber(id: string, key: 'copies' | 'scale', raw: string, min: number, max: number) {
    const n = Math.round(Number(raw));
    if (!Number.isFinite(n) || n < min || n > max) {
      return app.toast('error', key === 'copies' ? 'Copies must be between 1 and 99.' : 'Scale must be between 10 and 400 %.');
    }
    void apply(api.objectSheetUpdate(id, { [key]: n }));
  }

  async function exportPdf() {
    if (!sheet.items.length) return;
    const path = isTauri() ? await pickSavePath('Export object sheet', 'object-sheet.pdf', ['pdf']) : 'object-sheet.pdf';
    if (!path) return;
    try {
      await api.objectSheetExportPdf(path);
      app.toast('info', 'Exporting… progress is shown bottom-right.');
    } catch (e) {
      app.error('Export failed', e);
    }
  }
</script>

<svelte:window onkeydown={(e) => (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'e' && (e.preventDefault(), exportPdf())} />

<div class="sheet">
  <section class="items" aria-label="Sheet items">
    <div class="bar">
      <button class="btn" onclick={upload}>+ Add images…</button>
      <span class="muted count">{sheet.items.length} item{sheet.items.length === 1 ? '' : 's'} · {total} cut-out{total === 1 ? '' : 's'}</span>
    </div>
    {#if loaded && sheet.items.length === 0}
      <div class="empty muted">
        <p>No cut-outs yet.</p>
        <p>Use “Add to object sheet” in the Object Cutter, add images here, or select cut-outs in the Library.</p>
      </div>
    {/if}
    <ul>
      {#each sheet.items as it (it.id)}
        <li data-testid="object-item">
          <img src={blobUrl(it.thumb)} alt="" />
          <div class="meta">
            <input class="label-input" aria-label="Label" value={it.label} onchange={(e) => apply(api.objectSheetUpdate(it.id, { label: e.currentTarget.value }))} />
            <div class="row">
              <select class="input" aria-label="Size of {it.label}" value={it.size} onchange={(e) => apply(api.objectSheetUpdate(it.id, { size: e.currentTarget.value }))}>
                {#each SIZE_LABELS as s (s)}<option value={s}>{s}</option>{/each}
              </select>
              <label class="num">
                <input class="input" type="number" min="10" max="400" step="10" aria-label="Scale of {it.label}" value={it.scale}
                  onchange={(e) => setNumber(it.id, 'scale', e.currentTarget.value, 10, 400)} />%
              </label>
              <label class="num">
                ×<input class="input" type="number" min="1" max="99" aria-label="Copies of {it.label}" value={it.copies}
                  onchange={(e) => setNumber(it.id, 'copies', e.currentTarget.value, 1, 99)} />
              </label>
            </div>
          </div>
          <button class="btn ghost icon" aria-label="Remove {it.label}" onclick={() => apply(api.objectSheetRemove(it.id))}>×</button>
        </li>
      {/each}
    </ul>
    {#if sheet.items.length}
      <div class="bar bottom">
        {#if confirmClear}
          <span>Remove all items?</span>
          <button class="btn danger" onclick={() => apply(api.objectSheetClear()).then(() => (confirmClear = false))}>Clear all</button>
          <button class="btn" onclick={() => (confirmClear = false)}>Cancel</button>
        {:else}
          <button class="btn ghost danger" onclick={() => (confirmClear = true)}>Clear all…</button>
        {/if}
      </div>
    {/if}
  </section>

  <section class="preview" aria-label="Page preview">
    <div class="options">
      <div class="segmented" role="group" aria-label="Page format">
        {#each ['Letter', 'A4'] as f (f)}
          <button aria-pressed={sheet.format === f} onclick={() => apply(api.objectSheetSetOptions(f as PageFormat, null))}>{f}</button>
        {/each}
      </div>
      <div class="segmented" role="group" aria-label="Mode">
        {#each [['flat', 'Flat'], ['standee', 'Standee']] as [m, label] (m)}
          <button aria-pressed={sheet.mode === m} onclick={() => apply(api.objectSheetSetOptions(null, m as SheetMode))}>{label}</button>
        {/each}
      </div>
      <div class="spacer"></div>
      <button class="btn primary" disabled={!sheet.items.length || skipped.length > 0} onclick={exportPdf} title="Ctrl+E">Export PDF</button>
    </div>
    {#if skipped.length}<p class="warn" role="alert">Too large for a page: {skipped.join(', ')}</p>{/if}

    <div class="page-area">
      {#if layout && pages > 0}
        <svg class="page" viewBox="0 0 {layout.page_w} {layout.page_h}" role="img" aria-label="Page {page + 1} of {pages}" data-testid="object-page">
          <rect width={layout.page_w} height={layout.page_h} fill="#fff" />
          {#each layout.pages[page] as pl, i (i)}
            {@const top = layout.page_h - pl.y - pl.h}
            {@const href = blobUrl(byId.get(pl.id)?.thumb)}
            {#if sheet.mode === 'standee'}
              {@const half = pl.h / 2}
              <!-- mirrored top half, upright bottom half, dashed fold line -->
              <g transform="translate(0 {top + half}) scale(1 -1)">
                <image {href} x={pl.x} y={0} width={pl.w} height={half} preserveAspectRatio="none" />
              </g>
              <image {href} x={pl.x} y={top + half} width={pl.w} height={half} preserveAspectRatio="none" />
              <line x1={pl.x} x2={pl.x + pl.w} y1={top + half} y2={top + half} stroke="#787878" stroke-width="0.6" stroke-dasharray="3 3" class="fold" />
            {:else}
              <image {href} x={pl.x} y={top} width={pl.w} height={pl.h} preserveAspectRatio="none" />
            {/if}
          {/each}
        </svg>
      {:else}
        <p class="muted">The page preview appears here.</p>
      {/if}
    </div>
    <div class="pager">
      <button class="btn" disabled={page <= 0} onclick={() => page--} aria-label="Previous page">◀</button>
      <span>Page {pages ? page + 1 : 0} of {pages}</span>
      <button class="btn" disabled={page >= pages - 1} onclick={() => page++} aria-label="Next page">▶</button>
    </div>
  </section>
</div>

<style>
  .sheet {
    display: grid;
    grid-template-columns: 380px 1fr;
    height: 100%;
    min-height: 0;
  }
  .items {
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .bar {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    padding: var(--space-3) var(--space-4);
  }
  .bar.bottom {
    border-top: 1px solid var(--border);
    margin-top: auto;
  }
  .count {
    font-size: 12px;
  }
  .empty {
    padding: 0 var(--space-4);
    font-size: 13px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0 var(--space-2);
    overflow-y: auto;
  }
  li {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    padding: var(--space-2);
    border-radius: var(--radius-sm);
  }
  li:hover {
    background: var(--surface-2);
  }
  li img {
    width: 52px;
    height: 52px;
    object-fit: contain;
    flex: none;
  }
  .meta {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 4px;
  }
  .label-input {
    border: 1px solid transparent;
    background: transparent;
    font-weight: 600;
    padding: 2px 4px;
    border-radius: 4px;
    min-width: 0;
  }
  .label-input:hover,
  .label-input:focus {
    border-color: var(--border);
    background: var(--surface);
  }
  .row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .num {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .num .input {
    width: 58px;
  }
  .preview {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    padding: var(--space-3) var(--space-4);
    gap: var(--space-2);
  }
  .options {
    display: flex;
    gap: var(--space-3);
    align-items: center;
  }
  .spacer {
    flex: 1;
  }
  .warn {
    margin: 0;
    color: var(--warning);
    font-size: 13px;
  }
  .page-area {
    flex: 1;
    min-height: 0;
    display: grid;
    place-items: center;
    background: var(--surface-2);
    border-radius: var(--radius);
    padding: var(--space-3);
  }
  .page {
    max-width: 100%;
    max-height: 100%;
    height: 100%;
    box-shadow: var(--shadow);
  }
  .pager {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    justify-content: center;
  }
</style>
