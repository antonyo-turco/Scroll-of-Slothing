<script lang="ts">
  // Token Sheet: tokens and cut-outs packed on the same pages, page preview, one
  // PDF. Specs: docs/features/token-sheet.md, docs/features/object-sheet.md.
  import { onMount } from 'svelte';
  import { api, blobUrl, IMAGE_EXTENSIONS, isTauri, onFileDrop, pickFiles, pickSavePath } from '../../lib/api';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import CreaturePicker from '../../lib/components/CreaturePicker.svelte';
  import { app } from '../../lib/state.svelte';
  import ViewActions from '../../lib/components/ViewActions.svelte';
  import { shapePath } from '../../lib/token-shapes';
  import { SIZE_LABELS, type ObjectItem, type ObjectSheet, type PageFormat, type SheetLayout, type SheetMode, type TokenSheet } from '../../lib/types';

  let sheet = $state<TokenSheet>({ items: [], format: 'Letter', cut_border: false });
  let objects = $state<ObjectSheet>({ items: [], format: 'Letter', mode: 'flat' });
  let layout = $state<SheetLayout | null>(null);
  let page = $state(0);
  let confirmClear = $state(false);
  let loaded = $state(false);

  const thumbs = $derived(new Map([...sheet.items, ...objects.items].map((i) => [i.id, i.thumb])));
  const labels = $derived(new Map([...sheet.items, ...objects.items].map((i) => [i.id, i.label])));
  const shapes = $derived(new Map(sheet.items.map((i) => [i.id, i.shape ?? 'circle'])));
  const cutouts = $derived(new Set(objects.items.map((i) => i.id)));
  const tokenCount = $derived(sheet.items.reduce((n, i) => n + i.copies, 0));
  const cutoutCount = $derived(objects.items.reduce((n, i) => n + i.copies, 0));
  const empty = $derived(!sheet.items.length && !objects.items.length);
  const pages = $derived(layout?.pages.length ?? 0);
  const skippedLabels = $derived((layout?.skipped ?? []).map((id) => labels.get(id) ?? id));
  const dash = $derived(
    app.settings.cut_line_style === 'Dashed' ? '2 3' : app.settings.cut_line_style === 'Dotted' ? '0.5 2' : '',
  );

  /** Runs a change to either list, then reloads both and the shared layout. */
  async function apply(p: Promise<unknown>, what = 'Update failed') {
    try {
      await p;
      [sheet, objects, layout] = await Promise.all([api.tokenSheetGet(), api.objectSheetGet(), api.tokenSheetLayout()]);
      if (page >= (layout?.pages.length ?? 0)) page = Math.max(0, (layout?.pages.length ?? 1) - 1);
    } catch (e) {
      app.error(what, e);
    }
  }

  let unlisten: (() => void) | undefined;
  onMount(() => {
    void apply(api.tokenSheetGet(), 'Could not load the sheet').then(() => (loaded = true));
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

  async function upload(kind: 'token' | 'cutout') {
    const files = await pickFiles(kind === 'token' ? 'Add images as tokens' : 'Add images as cut-outs', IMAGE_EXTENSIONS);
    if (files?.length) await apply(kind === 'token' ? api.tokenSheetAddFiles(files) : api.objectSheetAddFiles(files), 'Could not add files');
    else if (!isTauri()) app.toast('info', 'File dialogs are only available in the desktop app.');
  }

  function setCopies(id: string, raw: string) {
    const n = Math.round(Number(raw));
    if (!Number.isFinite(n) || n < 1 || n > 99) return app.toast('error', 'Copies must be between 1 and 99.');
    void apply(api.tokenSheetUpdate(id, { copies: n }));
  }
  function setCutout(it: ObjectItem, key: 'copies' | 'scale', raw: string) {
    const n = Math.round(Number(raw));
    const [min, max] = key === 'copies' ? [1, 99] : [10, 400];
    if (!Number.isFinite(n) || n < min || n > max) {
      return app.toast('error', key === 'copies' ? 'Copies must be between 1 and 99.' : 'Scale must be between 10 and 400 %.');
    }
    void apply(api.objectSheetUpdate(it.id, { [key]: n }));
  }
  async function clearAll() {
    await apply(Promise.all([api.tokenSheetClear(), api.objectSheetClear()]));
    confirmClear = false;
  }

  async function exportPdf() {
    if (empty) return;
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

  // ── tokens from 5etools creatures (sized from the creature) ──
  let pickOpen = $state(false);
  async function addCreatures(picked: { entry: { hash: string; name: string }; count: number }[]) {
    await apply(api.tokenSheetAddCreatures(picked.map((p) => ({ hash: p.entry.hash, count: p.count }))), 'Could not add the creature tokens');
  }
</script>

<svelte:window onkeydown={onKey} />

<ViewActions>
  <button class="btn" onclick={() => upload('token')} title="Add image files as tokens">Add tokens…</button>
  <button class="btn" onclick={() => upload('cutout')} title="Add image files as cut-outs">Add cut-outs…</button>
  <button class="btn" onclick={() => (pickOpen = true)} title="Add tokens of 5etools creatures, at each creature's size">From 5etools…</button>
  <button class="btn primary" disabled={empty || skippedLabels.length > 0} onclick={exportPdf} title="Ctrl+E">Export PDF</button>
</ViewActions>

<div class="view">
  <section class="work" aria-label="Page preview">
    <div class="view-toolbar">
      <div class="segmented" role="group" aria-label="Page format">
        {#each ['Letter', 'A4'] as f (f)}
          <button aria-pressed={sheet.format === f} onclick={() => apply(api.tokenSheetSetOptions(f as PageFormat, null))}>{f}</button>
        {/each}
      </div>
      <label class="check">
        <input type="checkbox" checked={sheet.cut_border} onchange={(e) => apply(api.tokenSheetSetOptions(null, e.currentTarget.checked))} />
        Cut border
      </label>
      <span class="tool-label">Cut-outs</span>
      <div class="segmented" role="group" aria-label="Cut-out mode">
        {#each [['flat', 'Flat'], ['standee', 'Standee']] as [m, label] (m)}
          <button aria-pressed={objects.mode === m} onclick={() => apply(api.objectSheetSetOptions(null, m as SheetMode))}>{label}</button>
        {/each}
      </div>
      <InfoTip label="About cut-out modes" text="Flat prints each cut-out once, to lay on the table. Standee prints it twice, mirrored, with a fold line, so it stands up when folded." />
      <div class="spacer"></div>
      <div class="pager">
        <button class="btn icon" disabled={page <= 0} onclick={() => page--} aria-label="Previous page">‹</button>
        <span>Page {pages ? page + 1 : 0} of {pages}</span>
        <button class="btn icon" disabled={page >= pages - 1} onclick={() => page++} aria-label="Next page">›</button>
      </div>
    </div>

    <div class="stage-area page-area">
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
            {@const href = blobUrl(thumbs.get(pl.id))}
            {#if cutouts.has(pl.id)}
              {#if objects.mode === 'standee'}
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
            {:else}
              <image {href} x={pl.x} y={top} width={pl.w} height={pl.h} preserveAspectRatio="xMidYMid meet" />
            {/if}
            {#if cutouts.has(pl.id)}
              <!-- cut-outs carry their own sticker outline: no cut line -->
            {:else if sheet.cut_border}
              {@const shape = shapes.get(pl.id) ?? 'circle'}
              <g fill="none" stroke={app.settings.cut_line_color} stroke-width={Math.max(0.1, app.settings.cut_line_width)} stroke-dasharray={dash} data-testid="cut-{shape}">
                {#if shape === 'circle'}
                  <circle cx={pl.x + pl.w / 2} cy={top + pl.h / 2} r={Math.min(pl.w, pl.h) / 2} />
                {:else}
                  <path d={shapePath(shape, pl.x, top, pl.w, pl.h)} />
                {/if}
              </g>
            {:else}
              <rect x={pl.x} y={top} width={pl.w} height={pl.h} fill="none" stroke="#ccc" stroke-width="0.5" />
            {/if}
          {/each}
        </svg>
      {:else}
        <div class="empty-state">
          <strong>Nothing on the sheet yet</strong>
          <span>Add images, use “Add to sheet” in either cutter, or select tokens and cut-outs in the Library.</span>
        </div>
      {/if}
    </div>

    <div class="statusbar">
      <span>{tokenCount} token{tokenCount === 1 ? '' : 's'} · {cutoutCount} cut-out{cutoutCount === 1 ? '' : 's'} · {pages} page{pages === 1 ? '' : 's'}</span>
      {#if skippedLabels.length}
        <span class="warn" role="alert">Too large for a page: {skippedLabels.join(', ')}</span>
      {/if}
    </div>
  </section>

  <aside class="inspector" aria-label="Sheet items">
    <section class="section items">
      <h3 class="section-title">Tokens <InfoTip label="About the sheet" text="Tokens and cut-outs print together on the same pages. Each token prints as many copies as you set, at its creature size (Tiny ½ in, Medium 1 in, Large 2 in…). Items too large for a page are reported in the status bar." /></h3>
      {#if loaded && sheet.items.length === 0}
        <p class="note">No tokens yet.</p>
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
              <div class="row nowrap">
                <select class="input" aria-label="Size of {it.label}" value={it.size} onchange={(e) => apply(api.tokenSheetUpdate(it.id, { size: e.currentTarget.value }))}>
                  {#each SIZE_LABELS as s (s)}<option value={s}>{s}</option>{/each}
                </select>
                <label class="copies" title="Copies">
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
    </section>
    <section class="section items">
      <h3 class="section-title">Cut-outs <InfoTip label="About cut-outs on the sheet" text="Each cut-out prints at its size and scale (on its longest side), as many copies as you set." /></h3>
      {#if loaded && objects.items.length === 0}
        <p class="note">No cut-outs yet.</p>
      {/if}
      <ul>
        {#each objects.items as it (it.id)}
          <li data-testid="object-item">
            <img src={blobUrl(it.thumb)} alt="" />
            <div class="meta">
              <input class="label-input" aria-label="Label" value={it.label} onchange={(e) => apply(api.objectSheetUpdate(it.id, { label: e.currentTarget.value }))} />
              <div class="row nowrap">
                <select class="input" aria-label="Size of {it.label}" value={it.size} onchange={(e) => apply(api.objectSheetUpdate(it.id, { size: e.currentTarget.value }))}>
                  {#each SIZE_LABELS as s (s)}<option value={s}>{s}</option>{/each}
                </select>
                <label class="copies" title="Scale">
                  <input class="input" type="number" min="10" max="400" step="10" aria-label="Scale of {it.label}" value={it.scale}
                    onchange={(e) => setCutout(it, 'scale', e.currentTarget.value)} />%
                </label>
                <label class="copies" title="Copies">
                  ×<input class="input" type="number" min="1" max="99" aria-label="Copies of {it.label}" value={it.copies}
                    onchange={(e) => setCutout(it, 'copies', e.currentTarget.value)} />
                </label>
              </div>
            </div>
            <button class="btn ghost icon" aria-label="Remove {it.label}" onclick={() => apply(api.objectSheetRemove(it.id))}>×</button>
          </li>
        {/each}
      </ul>
    </section>
    {#if !empty}
      <div class="inspector-footer">
        {#if confirmClear}
          <span class="note">Remove all items?</span>
          <div class="pair">
            <button class="btn" onclick={() => (confirmClear = false)}>Cancel</button>
            <button class="btn danger" onclick={clearAll}>Clear all</button>
          </div>
        {:else}
          <button class="btn ghost danger" onclick={() => (confirmClear = true)}>Clear all…</button>
        {/if}
      </div>
    {/if}
  </aside>
</div>

<style>
  .page-area {
    display: grid;
    place-items: center;
    padding: var(--space-4);
  }
  .page {
    max-width: 100%;
    max-height: 100%;
    height: 100%;
    box-shadow: var(--shadow);
  }
  .warn {
    color: var(--warning);
  }
  .tool-label {
    margin-left: var(--space-2);
    color: var(--text-muted);
  }
  .items {
    align-content: start;
  }
  ul {
    list-style: none;
    margin: 0 calc(-1 * var(--space-2));
    padding: 0;
    display: grid;
    gap: 2px;
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
    width: 48px;
    height: 48px;
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
  .nowrap {
    flex-wrap: nowrap;
  }
  .nowrap select {
    flex: 1;
  }
  .copies {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: none;
    font-size: 12px;
    color: var(--text-muted);
  }
  .copies .input {
    width: 56px;
  }
</style>

<CreaturePicker bind:open={pickOpen} title="Tokens from 5etools" multi action="Add to sheet" onPick={addCreatures} />
