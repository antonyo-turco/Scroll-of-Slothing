<script lang="ts">
  // Token Sheet: items, packed page preview, PDF export. Spec: docs/features/token-sheet.md.
  import { onMount } from 'svelte';
  import { api, blobUrl, IMAGE_EXTENSIONS, isTauri, onFileDrop, pickFiles, pickSavePath } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { SIZE_LABELS, type PageFormat, type SheetLayout, type TokenSheet } from '../../lib/types';

  let sheet = $state<TokenSheet>({ items: [], format: 'Letter', cut_border: false });
  let layout = $state<SheetLayout | null>(null);
  let page = $state(0);
  let confirmClear = $state(false);
  let loaded = $state(false);

  const thumbs = $derived(new Map(sheet.items.map((i) => [i.id, i.thumb])));
  const labels = $derived(new Map(sheet.items.map((i) => [i.id, i.label])));
  const total = $derived(sheet.items.reduce((n, i) => n + i.copies, 0));
  const pages = $derived(layout?.pages.length ?? 0);
  const skippedLabels = $derived((layout?.skipped ?? []).map((id) => labels.get(id) ?? id));
  const dash = $derived(
    app.settings.cut_line_style === 'Dashed' ? '2 3' : app.settings.cut_line_style === 'Dotted' ? '0.5 2' : '',
  );

  async function apply(p: Promise<TokenSheet>, what = 'Update failed') {
    try {
      sheet = await p;
      layout = await api.tokenSheetLayout();
      if (page >= (layout?.pages.length ?? 0)) page = Math.max(0, (layout?.pages.length ?? 1) - 1);
    } catch (e) {
      app.error(what, e);
    }
  }

  let unlisten: (() => void) | undefined;
  onMount(() => {
    void apply(api.tokenSheetGet(), 'Could not load the token sheet').then(() => (loaded = true));
    void onFileDrop((paths) => {
      const imgs = paths.filter((p) => IMAGE_EXTENSIONS.includes(p.split('.').pop()?.toLowerCase() ?? ''));
      if (imgs.length) void apply(api.tokenSheetAddFiles(imgs), 'Could not add files');
    }).then((u) => (unlisten = u));
    return () => unlisten?.();
  });

  // Sheet edits from elsewhere (Cutter, Library, MCP) show up live.
  let seen = app.dataVersion;
  $effect(() => {
    if (app.dataVersion !== seen) {
      seen = app.dataVersion;
      void apply(api.tokenSheetGet());
    }
  });

  async function upload() {
    const files = await pickFiles('Add images to the sheet', IMAGE_EXTENSIONS);
    if (files?.length) await apply(api.tokenSheetAddFiles(files), 'Could not add files');
    else if (!isTauri()) app.toast('info', 'File dialogs are only available in the desktop app.');
  }

  function setCopies(id: string, raw: string) {
    const n = Math.round(Number(raw));
    if (!Number.isFinite(n) || n < 1 || n > 99) return app.toast('error', 'Copies must be between 1 and 99.');
    void apply(api.tokenSheetUpdate(id, { copies: n }));
  }

  async function exportPdf() {
    if (!sheet.items.length) return;
    const path = isTauri()
      ? await pickSavePath('Export token sheet', 'token-sheet.pdf', ['pdf'])
      : 'token-sheet.pdf';
    if (!path) return;
    try {
      await api.tokenSheetExportPdf(path);
      app.toast('info', 'Exporting… progress is shown bottom-right.');
    } catch (e) {
      app.error('Export failed', e);
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'e') {
      e.preventDefault();
      void exportPdf();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="sheet">
  <section class="items" aria-label="Sheet items">
    <div class="bar">
      <button class="btn" onclick={upload}>+ Add images…</button>
      <span class="muted count">{sheet.items.length} item{sheet.items.length === 1 ? '' : 's'} · {total} token{total === 1 ? '' : 's'}</span>
    </div>
    {#if loaded && sheet.items.length === 0}
      <div class="empty muted">
        <p>No tokens yet.</p>
        <p>Add images here, use “Add to token sheet” in the Token Cutter, or select tokens in the Library.</p>
      </div>
    {/if}
    <ul>
      {#each sheet.items as it (it.id)}
        <li data-testid="sheet-item">
          <img src={blobUrl(it.thumb)} alt="" />
          <div class="meta">
            <input
              class="label-input"
              aria-label="Label"
              value={it.label}
              onchange={(e) => apply(api.tokenSheetUpdate(it.id, { label: e.currentTarget.value }))}
            />
            <div class="row">
              <select class="input" aria-label="Size of {it.label}" value={it.size} onchange={(e) => apply(api.tokenSheetUpdate(it.id, { size: e.currentTarget.value }))}>
                {#each SIZE_LABELS as s (s)}<option value={s}>{s}</option>{/each}
              </select>
              <label class="copies">
                ×
                <input
                  class="input"
                  type="number"
                  min="1"
                  max="99"
                  aria-label="Copies of {it.label}"
                  value={it.copies}
                  onchange={(e) => setCopies(it.id, e.currentTarget.value)}
                />
              </label>
            </div>
          </div>
          <button class="btn ghost icon" aria-label="Remove {it.label}" onclick={() => apply(api.tokenSheetRemove(it.id))}>×</button>
        </li>
      {/each}
    </ul>
    {#if sheet.items.length}
      <div class="bar bottom">
        {#if confirmClear}
          <span>Remove all items?</span>
          <button class="btn danger" onclick={() => apply(api.tokenSheetClear()).then(() => (confirmClear = false))}>Clear all</button>
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
          <button aria-pressed={sheet.format === f} onclick={() => apply(api.tokenSheetSetOptions(f as PageFormat, null))}>{f}</button>
        {/each}
      </div>
      <label class="toggle">
        <input type="checkbox" checked={sheet.cut_border} onchange={(e) => apply(api.tokenSheetSetOptions(null, e.currentTarget.checked))} />
        Cut border
      </label>
      <div class="spacer"></div>
      <button class="btn primary" disabled={!sheet.items.length || skippedLabels.length > 0} onclick={exportPdf} title="Ctrl+E">Export PDF</button>
    </div>

    {#if skippedLabels.length}
      <p class="warn" role="alert">Too large for a page: {skippedLabels.join(', ')}</p>
    {/if}

    <div class="page-area">
      {#if layout && pages > 0}
        <svg
          class="page"
          viewBox="0 0 {layout.page_w} {layout.page_h}"
          role="img"
          aria-label="Page {page + 1} of {pages}"
          data-testid="sheet-page"
        >
          <rect width={layout.page_w} height={layout.page_h} fill="#fff" />
          {#each layout.pages[page] as pl, i (i)}
            {@const top = layout.page_h - pl.y - pl.h}
            <image href={blobUrl(thumbs.get(pl.id))} x={pl.x} y={top} width={pl.w} height={pl.h} preserveAspectRatio="xMidYMid meet" />
            {#if sheet.cut_border}
              <circle
                cx={pl.x + pl.w / 2}
                cy={top + pl.h / 2}
                r={Math.min(pl.w, pl.h) / 2}
                fill="none"
                stroke={app.settings.cut_line_color}
                stroke-width={Math.max(0.1, app.settings.cut_line_width)}
                stroke-dasharray={dash}
              />
            {:else}
              <rect x={pl.x} y={top} width={pl.w} height={pl.h} fill="none" stroke="#ccc" stroke-width="0.5" />
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
    grid-template-columns: 340px 1fr;
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
  .copies {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .copies .input {
    width: 60px;
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
  .toggle {
    display: flex;
    gap: 6px;
    align-items: center;
    cursor: pointer;
  }
  .toggle input {
    accent-color: var(--accent);
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
