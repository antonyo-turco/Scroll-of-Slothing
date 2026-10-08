<script lang="ts">
  // Map Paginator: grid sizing, grid removal, filters, de-warp and true-scale
  // page tiling. Spec: docs/features/map-paginator.md.
  // The whole state is a MapDoc (full-res image px). Zoom/pan, the grid, tiles
  // and tools are client-side vectors over the Rust-processed proxy image.
  import { onDestroy, onMount } from 'svelte';
  import { api, IMAGE_EXTENSIONS, isTauri, onFileDrop, pickFile, pickSavePath } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import type { FilterCatalog, MapDoc, MapLayout, Pt, SizingMode } from '../../lib/types';
  import FiltersPanel from './FiltersPanel.svelte';
  import {
    HANDLES,
    handlePoint,
    linePositions,
    moveQuadEdge,
    orderQuad,
    quadEdges,
    pmod,
    rectFrom,
    resizeRect,
    snapLine,
    snapQuarter,
    squareFrom,
    stylePattern,
    type Handle,
    type Rect,
  } from './geometry';

  type Tool = 'none' | 'region' | 'reference' | 'nudge' | 'layout';
  type Drag =
    | { kind: 'pan'; x: number; y: number; sx: number; sy: number; moved: boolean; button: number }
    | { kind: 'new'; x0: number; y0: number; x1: number; y1: number }
    | { kind: 'resize'; hd: Handle; orig: Rect }
    | { kind: 'move'; x0: number; y0: number; orig: Rect }
    | { kind: 'corner'; i: number }
    | { kind: 'side'; a: number; b: number; x0: number; y0: number; orig: Pt[] }
    | { kind: 'layout'; x0: number; y0: number; orig: [number, number] };

  const TOOLS: { id: Tool; glyph: string; label: string; help: string }[] = [
    { id: 'none', glyph: '✋', label: 'Pan', help: 'Drag to move around the map. Click a page to include or exclude it.' },
    { id: 'region', glyph: '▭', label: 'Print area', help: 'Drag the part of the map to print (snaps to the grid; Shift = free). Drag the handles to resize, inside to move.' },
    { id: 'reference', glyph: '◰', label: 'Reference square', help: 'Drag a box over a block of known size (N×N squares; Shift = not square). Then drag its corners or sides onto the block’s real edges.' },
    { id: 'nudge', glyph: '⌖', label: 'Align grid', help: 'Drag over one real grid cell of the map to line the grid up with it.' },
    { id: 'layout', glyph: '⇲', label: 'Move pages', help: 'Drag the page layout over the map (snaps to ¼ page; Shift = free).' },
  ];

  const MODES: [SizingMode, string][] = [
    ['rows_cols', 'Rows×Cols'],
    ['reference', 'Reference'],
    ['ppi', 'PPI'],
    ['cell_side', 'Cell side'],
  ];

  let doc = $state<MapDoc | null>(null);
  let layout = $state<MapLayout | null>(null);
  let catalog = $state<FilterCatalog | null>(null);
  let previewUrl = $state<string | null>(null);
  let pageUrl = $state<string | null>(null);
  let rendering = $state(false);
  let busy = $state(false);
  let loaded = false;

  let tool = $state<Tool>('none');
  let showTiles = $state(true);
  let page = $state(0);
  let drag = $state<Drag | null>(null);
  let shift = $state(false);

  let saveOpen = $state(false);
  let saveName = $state('');
  let saveJob = $state<string | null>(null);

  // View transform: image px (ix, iy) → screen (tx + ix·z, ty + iy·z).
  let stage: HTMLDivElement | undefined = $state();
  let z = $state(1);
  let tx = $state(0);
  let ty = $state(0);

  const W = $derived(doc?.source?.width ?? 0);
  const H = $derived(doc?.source?.height ?? 0);
  const grid = $derived(layout?.grid ?? null);
  const tiles = $derived(layout?.tiles ?? []);
  const isExcluded = (c: number, r: number) => !!doc?.excluded.some(([a, b]) => a === c && b === r);

  // ── view ──
  function fit() {
    if (!stage || !W) return;
    const k = Math.min(stage.clientWidth / W, stage.clientHeight / H) * 0.95;
    z = Math.max(0.1, Math.min(12, k));
    tx = (stage.clientWidth - W * z) / 2;
    ty = (stage.clientHeight - H * z) / 2;
  }
  function zoomAt(f: number, cx: number, cy: number) {
    const nz = Math.max(0.1, Math.min(12, z * f));
    tx = cx - (cx - tx) * (nz / z);
    ty = cy - (cy - ty) * (nz / z);
    z = nz;
  }
  const zoomCentre = (f: number) => stage && zoomAt(f, stage.clientWidth / 2, stage.clientHeight / 2);
  function toImage(e: { clientX: number; clientY: number }): Pt {
    const r = stage!.getBoundingClientRect();
    return [(e.clientX - r.left - tx) / z, (e.clientY - r.top - ty) / z];
  }
  function wheel(node: HTMLElement) {
    const h = (e: WheelEvent) => {
      if (!doc?.source) return;
      e.preventDefault();
      const r = node.getBoundingClientRect();
      zoomAt(e.deltaY < 0 ? 1.1 : 1 / 1.1, e.clientX - r.left, e.clientY - r.top);
    };
    node.addEventListener('wheel', h, { passive: false });
    return { destroy: () => node.removeEventListener('wheel', h) };
  }

  // ── backend sync: layout (cheap, every change), preview (debounced), autosave ──
  const snap = () => $state.snapshot(doc) as MapDoc;

  let layoutBusy = false;
  let layoutDirty = false;
  async function refreshLayout() {
    if (!doc?.source) return void (layout = null);
    if (layoutBusy) return void (layoutDirty = true);
    layoutBusy = true;
    try {
      do {
        layoutDirty = false;
        layout = await api.mapLayout(snap());
      } while (layoutDirty);
      if (page >= tiles.length) page = Math.max(0, tiles.length - 1);
    } catch (e) {
      app.error('Layout failed', e);
    } finally {
      layoutBusy = false;
    }
  }

  let prevBusy = false;
  let prevDirty = false;
  async function refreshPreview() {
    if (!doc?.source) return;
    if (prevBusy) return void (prevDirty = true);
    prevBusy = true;
    rendering = true;
    try {
      do {
        prevDirty = false;
        const blob = await api.mapPreview(snap());
        if (previewUrl) URL.revokeObjectURL(previewUrl);
        previewUrl = URL.createObjectURL(blob);
      } while (prevDirty);
    } catch (e) {
      app.error('Preview failed', e);
    } finally {
      prevBusy = false;
      rendering = false;
    }
  }

  let pageBusy = false;
  let pageDirty = false;
  async function refreshPage() {
    if (!doc?.source || !tiles.length) {
      if (pageUrl) URL.revokeObjectURL(pageUrl);
      pageUrl = null;
      return;
    }
    if (pageBusy) return void (pageDirty = true);
    pageBusy = true;
    try {
      do {
        pageDirty = false;
        const blob = await api.mapPreviewPage(snap(), Math.min(page, tiles.length - 1), 0.6);
        if (pageUrl) URL.revokeObjectURL(pageUrl);
        pageUrl = URL.createObjectURL(blob);
      } while (pageDirty);
    } catch {
      /* page preview is best-effort */
    } finally {
      pageBusy = false;
    }
  }

  // Preview depends only on the source, removal (and its cell size) and filters.
  const previewKey = $derived(
    doc?.source
      ? JSON.stringify([
          doc.source.hash,
          doc.remove_grid,
          doc.remove_grid && grid ? [grid.cell_w, grid.cell_h] : null,
          $state.snapshot(doc.filters),
        ])
      : '',
  );
  let prevTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    if (!previewKey) return;
    if (prevTimer) clearTimeout(prevTimer);
    prevTimer = setTimeout(() => void refreshPreview(), 120);
  });

  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let pageTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const s = $state.snapshot(doc);
    if (!s || !loaded) return;
    void refreshLayout();
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void api.mapSet(s as MapDoc).catch(() => {}), 500);
  });
  $effect(() => {
    void previewKey;
    void page;
    void layout;
    if (pageTimer) clearTimeout(pageTimer);
    pageTimer = setTimeout(() => void refreshPage(), 250);
  });

  // Save-as-asset job: adopt the new asset id when it finishes.
  $effect(() => {
    const j = saveJob && app.jobs.find((x) => x.id === saveJob);
    if (!j || j.state === 'running') return;
    saveJob = null;
    busy = false;
    const a = j.result as { id?: number; name?: string } | null;
    if (j.state === 'done' && doc && a?.id != null) {
      doc.asset_id = a.id;
      doc.name = a.name ?? doc.name;
    }
  });

  // ── loading ──
  function adopt(d: MapDoc) {
    doc = d;
    tool = 'none';
    page = 0;
    requestAnimationFrame(fit);
  }
  async function loadPath(path: string) {
    try {
      adopt(await api.mapOpenFile(path));
    } catch (e) {
      app.error('Could not load map', e);
    }
  }
  async function browse() {
    const p = await pickFile('Choose a map image', IMAGE_EXTENSIONS, 'Images');
    if (p) await loadPath(p);
    else if (!isTauri()) app.toast('info', 'File dialogs are only available in the desktop app.');
  }
  async function paste() {
    try {
      const src = await api.imageSourceFromClipboard();
      adopt(await api.mapOpenSource(src.hash, 'Map'));
    } catch (e) {
      app.error('Paste failed', e);
    }
  }

  let unlisten: (() => void) | undefined;
  onMount(async () => {
    try {
      catalog = await api.mapFilterCatalog().catch(() => null);
      const req = app.openRequest;
      if (req && req.route === 'maps.paginator') {
        app.openRequest = null;
        adopt(await api.mapOpenAsset(req.assetId));
      } else {
        const saved = await api.mapGet().catch(() => null);
        if (saved) adopt(saved);
      }
    } catch (e) {
      app.error('Could not load the map paginator', e);
    } finally {
      loaded = true;
      if (doc) void refreshLayout();
    }
    unlisten = await onFileDrop((paths) => {
      const img = paths.find((p) => IMAGE_EXTENSIONS.includes(p.split('.').pop()?.toLowerCase() ?? ''));
      if (img) void loadPath(img);
    });
  });
  onDestroy(() => {
    unlisten?.();
    if (previewUrl) URL.revokeObjectURL(previewUrl);
    if (pageUrl) URL.revokeObjectURL(pageUrl);
  });

  // ── tools ──
  const snapX = (v: number) => (grid && !shift ? snapLine(v, grid.offset_x, grid.cell_w) : v);
  const snapY = (v: number) => (grid && !shift ? snapLine(v, grid.offset_y, grid.cell_h) : v);
  const near = (a: Pt, b: Pt, px: number) => Math.hypot((a[0] - b[0]) * z, (a[1] - b[1]) * z) <= px;
  const mid = (a: Pt, b: Pt): Pt => [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];

  function onPointerDown(e: PointerEvent) {
    if (!doc?.source) return;
    shift = e.shiftKey;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const p = toImage(e);
    if (e.button === 1 || (e.button === 0 && tool === 'none')) {
      drag = { kind: 'pan', x: e.clientX, y: e.clientY, sx: e.clientX, sy: e.clientY, moved: false, button: e.button };
      e.preventDefault();
      return;
    }
    if (e.button !== 0) return;
    if (tool === 'region' && doc.region) {
      const r = doc.region;
      const hd = HANDLES.find((h) => near(handlePoint(r, h), p, 8));
      if (hd) return void (drag = { kind: 'resize', hd, orig: [...r] });
      if (p[0] > r[0] && p[0] < r[0] + r[2] && p[1] > r[1] && p[1] < r[1] + r[3])
        return void (drag = { kind: 'move', x0: p[0], y0: p[1], orig: [...r] });
    }
    if (tool === 'reference' && doc.ref_quad) {
      const q = doc.ref_quad;
      const i = q.findIndex((c) => near(c, p, 10));
      if (i >= 0) return void (drag = { kind: 'corner', i });
      const side = quadEdges(q).find(([a, b]) => near(mid(q[a], q[b]), p, 10));
      if (side) return void (drag = { kind: 'side', a: side[0], b: side[1], x0: p[0], y0: p[1], orig: q.map((c) => [...c] as Pt) });
    }
    if (tool === 'layout') return void (drag = { kind: 'layout', x0: p[0], y0: p[1], orig: [...doc.page_offset] });
    const [x0, y0] = tool === 'region' ? [snapX(p[0]), snapY(p[1])] : p;
    drag = { kind: 'new', x0, y0, x1: x0, y1: y0 };
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag || !doc) return;
    shift = e.shiftKey;
    const p = toImage(e);
    switch (drag.kind) {
      case 'pan':
        tx += e.clientX - drag.x;
        ty += e.clientY - drag.y;
        drag.x = e.clientX;
        drag.y = e.clientY;
        if (Math.hypot(e.clientX - drag.sx, e.clientY - drag.sy) > 3) drag.moved = true;
        break;
      case 'new':
        if (tool === 'region') [drag.x1, drag.y1] = [snapX(p[0]), snapY(p[1])];
        else if (tool === 'reference' && !e.shiftKey) [drag.x1, drag.y1] = squareFrom(drag.x0, drag.y0, p[0], p[1]);
        else [drag.x1, drag.y1] = p;
        break;
      case 'resize':
        doc.region = resizeRect(drag.orig, drag.hd, snapX(p[0]), snapY(p[1]));
        break;
      case 'move': {
        const nx = snapX(drag.orig[0] + p[0] - drag.x0);
        const ny = snapY(drag.orig[1] + p[1] - drag.y0);
        doc.region = [nx, ny, drag.orig[2], drag.orig[3]];
        break;
      }
      case 'corner':
        if (doc.ref_quad) doc.ref_quad[drag.i] = p;
        break;
      case 'side':
        doc.ref_quad = moveQuadEdge(drag.orig, drag.a, drag.b, p[0] - drag.x0, p[1] - drag.y0) as [Pt, Pt, Pt, Pt];
        break;
      case 'layout': {
        const t = tiles[0]?.page_rect;
        const pw = t ? t[2] - t[0] : 0;
        const ph = t ? t[3] - t[1] : 0;
        let ox = drag.orig[0] + p[0] - drag.x0;
        let oy = drag.orig[1] + p[1] - drag.y0;
        if (!e.shiftKey) {
          ox = snapQuarter(ox, pw, 10 / z);
          oy = snapQuarter(oy, ph, 10 / z);
        }
        doc.page_offset = [ox, oy];
        break;
      }
    }
  }

  function onPointerUp(e: PointerEvent) {
    const d = drag;
    drag = null;
    if (!d || !doc) return;
    if (d.kind === 'pan') {
      if (!d.moved && d.button === 0) clickAt(toImage(e));
      return;
    }
    if (d.kind !== 'new') return;
    const r = rectFrom(d.x0, d.y0, d.x1, d.y1);
    if (r[2] * z < 4 || r[3] * z < 4) return;
    if (tool === 'region') {
      doc.region = r;
      doc.excluded = [];
    } else if (tool === 'reference') {
      const [x0, y0, w, h] = r;
      doc.ref_quad = [[x0, y0], [x0 + w, y0], [x0 + w, y0 + h], [x0, y0 + h]];
      doc.mode = 'reference';
    } else if (tool === 'nudge' && layout && grid) {
      // drag over one real cell: offset = (left % cw) − base offset
      doc.offset_x = Math.round((pmod(r[0], grid.cell_w) - layout.base_offset[0]) * 10) / 10;
      doc.offset_y = Math.round((pmod(r[1], grid.cell_h) - layout.base_offset[1]) * 10) / 10;
      tool = 'none';
    }
  }

  function clickAt(p: Pt) {
    if (!doc || !showTiles) return;
    const t = tiles.find((t) => p[0] >= t.page_rect[0] && p[0] < t.page_rect[2] && p[1] >= t.page_rect[1] && p[1] < t.page_rect[3]);
    if (t) toggleTile(t.col, t.row);
  }

  function toggleTile(c: number, r: number) {
    if (!doc) return;
    doc.excluded = isExcluded(c, r) ? doc.excluded.filter(([a, b]) => a !== c || b !== r) : [...doc.excluded, [c, r]];
  }

  function onKey(e: KeyboardEvent) {
    if ((e.target as HTMLElement)?.matches?.('input, textarea, select, button')) return;
    if (e.type !== 'keydown') return;
    if (e.key === 'Escape') tool = 'none';
    else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'v') {
      e.preventDefault();
      void paste();
    }
  }

  // ── actions ──
  async function dewarp() {
    if (!doc) return;
    busy = true;
    try {
      adopt(await api.mapDewarp(snap()));
      doc!.mode = 'reference';
      app.toast('success', `De-warped · ${doc!.source?.width} × ${doc!.source?.height} px · 1 square = ${doc!.side}px`);
    } catch (e) {
      app.error('De-warp failed', e);
    } finally {
      busy = false;
    }
  }
  async function undoDewarp() {
    if (!doc) return;
    busy = true;
    try {
      adopt(await api.mapUndoDewarp(snap()));
      tool = 'reference';
    } catch (e) {
      app.error('Undo failed', e);
    } finally {
      busy = false;
    }
  }
  async function exportPdf() {
    if (!doc?.source) return;
    const name = `${doc.name || 'map'}.pdf`;
    const path = isTauri() ? await pickSavePath('Export map PDF', name, ['pdf']) : name;
    if (!path) return;
    try {
      await api.mapExportPdf(snap(), path);
    } catch (e) {
      app.error('Export failed', e);
    }
  }
  function openSave() {
    saveName = doc?.name || 'Map';
    saveOpen = true;
  }
  async function save(update: boolean) {
    if (!doc) return;
    saveOpen = false;
    busy = true;
    try {
      saveJob = await api.mapSaveAsset(snap(), saveName.trim() || 'Map', update ? doc.asset_id : null);
    } catch (e) {
      busy = false;
      app.error('Save failed', e);
    }
  }

  // ── derived overlay geometry ──
  const lines = $derived.by(() => {
    if (!grid || !grid.visible || !W) return { v: [] as number[], h: [] as number[] };
    return { v: linePositions(grid.offset_x, grid.cell_w, W), h: linePositions(grid.offset_y, grid.cell_h, H) };
  });
  const dashV = $derived(grid ? stylePattern(grid.style, grid.cell_h, grid.thickness)?.join(' ') : undefined);
  const dashH = $derived(grid ? stylePattern(grid.style, grid.cell_w, grid.thickness)?.join(' ') : undefined);
  const gridColor = $derived(grid ? `rgb(${grid.color.join(',')})` : '#000');
  const hex = (c: number[]) => '#' + c.map((v) => v.toString(16).padStart(2, '0')).join('');
  const fromHex = (s: string) => [1, 3, 5].map((i) => parseInt(s.slice(i, i + 2), 16)) as [number, number, number];
  const dragRect = $derived(drag?.kind === 'new' ? rectFrom(drag.x0, drag.y0, drag.x1, drag.y1) : null);
  const quadPts = $derived(doc?.ref_quad ? orderQuad(doc.ref_quad).map((p) => p.join(',')).join(' ') : '');
  const fitLine = $derived.by(() => {
    const f = layout?.fit;
    if (!f || !doc) return null;
    const [w, h] = f.printed_in;
    const size = `${f.squares[0]}×${f.squares[1]} squares · ${w.toFixed(1)}×${h.toFixed(1)}" (${(w / 12).toFixed(1)}×${(h / 12).toFixed(1)} ft)`;
    const target = `${doc.fit_w_ft}×${doc.fit_h_ft} ft`;
    return { ok: f.fits, text: f.fits ? `✓ ${size} · fits ${target}` : `✗ ${size} · exceeds ${target}` };
  });
  const curTile = $derived(tiles[Math.min(page, tiles.length - 1)]);
  const sw = $derived(1 / z); // one screen pixel in image px
  // The Reference tool hides the grid and page tiles so the block underneath is clear.
  const refFocus = $derived(tool === 'reference');
  const activeTool = $derived(TOOLS.find((t) => t.id === tool) ?? TOOLS[0]);
</script>

<svelte:window onkeydown={onKey} />

<div class="paginator">
  <section class="work">
    <div class="toolbar">
      <button class="btn" onclick={browse} title="Load a map image from disk">Open map…</button>
      <button class="btn" onclick={paste} title="Load the image on the clipboard (Ctrl+V)">Paste</button>
      <div class="spacer"></div>
      <button class="btn" disabled={!doc?.source || busy} onclick={openSave} title="Store the map and these settings in the Library">Save to library…</button>
      <button class="btn primary" disabled={!doc?.source || !layout?.selected} onclick={exportPdf} title="Write the selected pages as a true-scale PDF">Export PDF</button>
    </div>
    <div class="toolbar">
      <div class="segmented tools" role="group" aria-label="Canvas tool">
        {#each TOOLS as t (t.id)}
          <button aria-pressed={tool === t.id} onclick={() => (tool = t.id)} title={t.help}>
            <span class="glyph" aria-hidden="true">{t.glyph}</span>{t.label}
          </button>
        {/each}
      </div>
      <div class="spacer"></div>
      <div class="viewctl" role="group" aria-label="View">
        <label class="check" title="Show the printed pages over the map"><input type="checkbox" bind:checked={showTiles} /> Pages</label>
        <button class="btn icon" aria-label="Zoom out" onclick={() => zoomCentre(1 / 1.25)}>−</button>
        <span class="zoom muted">{Math.round(z * 100)}%</span>
        <button class="btn icon" aria-label="Zoom in" onclick={() => zoomCentre(1.25)}>+</button>
        <button class="btn" onclick={fit} title="Fit the whole map in view">Fit</button>
      </div>
    </div>
    {#if doc?.source}
      <p class="toolhint" data-testid="tool-hint">
        <strong>{activeTool.label}:</strong>
        {activeTool.help}{refFocus ? ' Grid and pages are hidden while this tool is active.' : ''}
      </p>
    {/if}

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="stage tool-{tool}"
      class:panning={drag?.kind === 'pan'}
      bind:this={stage}
      use:wheel
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={() => (drag = null)}
      data-testid="map-stage"
    >
      {#if doc?.source}
        <div class="layer" style:transform="translate({tx}px, {ty}px) scale({z})" style:width="{W}px" style:height="{H}px">
          {#if previewUrl}<img src={previewUrl} alt="Map" draggable="false" />{/if}
          <svg viewBox="0 0 {W} {H}" width={W} height={H}>
            {#if grid?.visible && !refFocus}
              <g stroke={gridColor} stroke-opacity={grid.opacity / 255} stroke-width={Math.max(grid.thickness, sw)}>
                {#each lines.v as x (x)}<line x1={x} y1="0" x2={x} y2={H} stroke-dasharray={dashV} />{/each}
                {#each lines.h as y (y)}<line x1="0" y1={y} x2={W} y2={y} stroke-dasharray={dashH} />{/each}
              </g>
            {/if}
            {#if doc.region}
              {@const r = doc.region}
              <path class="outside" fill-rule="evenodd" d="M0 0H{W}V{H}H0Z M{r[0]} {r[1]}h{r[2]}v{r[3]}h{-r[2]}Z" />
              <rect class="region" x={r[0]} y={r[1]} width={r[2]} height={r[3]} stroke-width={2 * sw} />
              {#if tool === 'region'}
                {#each HANDLES as h (h)}
                  {@const [hx, hy] = handlePoint(r, h)}
                  <rect class="handle" x={hx - 4 * sw} y={hy - 4 * sw} width={8 * sw} height={8 * sw} stroke-width={sw} />
                {/each}
              {/if}
            {/if}
            {#if showTiles && !refFocus}
              {#each tiles as t, i (t.col + ':' + t.row)}
                {@const pr = t.page_rect}
                {@const ex = isExcluded(t.col, t.row)}
                <rect class="tile" class:ex class:cur={i === page} x={pr[0]} y={pr[1]} width={pr[2] - pr[0]} height={pr[3] - pr[1]} stroke-width={2 * sw} />
                {#if ex}
                  <text class="x" x={(pr[0] + pr[2]) / 2} y={(pr[1] + pr[3]) / 2} font-size={36 * sw}>✕</text>
                {:else}
                  <text class="num" x={pr[0] + 6 * sw} y={pr[1] + 18 * sw} font-size={14 * sw}>{i + 1}</text>
                {/if}
              {/each}
            {/if}
            {#if doc.ref_quad && !doc.pre_warp && !(tool === 'reference' && drag?.kind === 'new')}
              {@const q = doc.ref_quad}
              <polygon class="quad" points={quadPts} stroke-width={2 * sw} />
              {#if tool === 'reference'}
                {#each quadEdges(q) as [a, b] (a + ':' + b)}
                  {@const [mx, my] = mid(q[a], q[b])}
                  <rect class="side" x={mx - 5 * sw} y={my - 5 * sw} width={10 * sw} height={10 * sw} stroke-width={sw} />
                {/each}
              {/if}
              {#each q as [qx, qy], i (i)}
                <circle class="corner" cx={qx} cy={qy} r={6 * sw} stroke-width={sw} />
              {/each}
            {/if}
            {#if dragRect}
              <rect class="drag" x={dragRect[0]} y={dragRect[1]} width={dragRect[2]} height={dragRect[3]} stroke-width={2 * sw} />
            {/if}
          </svg>
        </div>
        {#if rendering}<span class="badge muted">processing…</span>{/if}
      {:else}
        <button class="drop" onclick={browse}>
          <strong>Drop a battle map here</strong>
          <span class="muted">or click to browse · Ctrl+V to paste · or “Open in editor” from the Library</span>
        </button>
      {/if}
    </div>
    <p class="hint muted">
      {#if !doc?.source}Load a map to start.
      {:else}
        {W} × {H} px{doc.dpi_detected ? ` · ${Math.round(doc.detected_dpi)} dpi` : ''} · wheel zooms, middle-drag pans
      {/if}
    </p>
  </section>

  <aside class="inspector" aria-label="Map settings">
    {#if doc}
      {#if saveOpen}
        <div class="card choice" role="group" aria-label="Save map as asset">
          <label class="field">Asset name <input class="input" bind:value={saveName} /></label>
          {#if doc.asset_id !== null}
            <p class="small">This map was opened from the library.</p>
            <button class="btn primary" onclick={() => save(true)}>Update existing</button>
            <button class="btn" onclick={() => save(false)}>Save as new</button>
          {:else}
            <button class="btn primary" onclick={() => save(false)}>Save</button>
          {/if}
          <button class="btn ghost" onclick={() => (saveOpen = false)}>Cancel</button>
        </div>
      {/if}

      <details open>
        <summary>Grid</summary>
        <div class="segmented modes" role="group" aria-label="Sizing mode">
          {#each MODES as [m, label] (m)}
            <button aria-pressed={doc.mode === m} onclick={() => (doc!.mode = m)}>{label}</button>
          {/each}
        </div>
        {#if doc.mode === 'rows_cols'}
          <div class="pair">
            <label class="field">Rows <input class="input" type="number" min="1" max="999" bind:value={doc.rows} /></label>
            <label class="field">Cols <input class="input" type="number" min="1" max="999" bind:value={doc.cols} /></label>
          </div>
          <label class="check"><input type="checkbox" bind:checked={doc.square_cells} /> Square cells</label>
        {:else if doc.mode === 'reference'}
          {#if doc.pre_warp}
            <p class="small ok">✓ Map de-warped to a flat, even grid.</p>
            <label class="field">Square size (px) <input class="input" type="number" min="2" max="2000" step="0.1" bind:value={doc.side} /></label>
            <button class="btn" disabled={busy} onclick={undoDewarp}>↺ Undo de-warp</button>
          {:else}
            <label class="field">Block is N×N squares <input class="input" type="number" min="1" max="99" bind:value={doc.ref_n} /></label>
            <p class="small muted">Use the Reference tool to box a known block, then drag its corners onto the block's real corners to de-warp the whole map.</p>
            <button class="btn" disabled={!doc.ref_quad || busy} onclick={() => (tool = 'reference')}>Edit block</button>
            <button class="btn primary" disabled={!doc.ref_quad || busy} onclick={dewarp}>▦ De-warp map from this square</button>
          {/if}
        {:else if doc.mode === 'ppi'}
          <label class="field">Pixels per inch <input class="input" type="number" min="10" max="1200" step="0.1" bind:value={doc.ppi} /></label>
        {:else}
          <label class="field">Cell side (px) <input class="input" type="number" min="2" max="2000" step="0.1" bind:value={doc.side} /></label>
        {/if}
        <div class="pair">
          <label class="field">Offset X <input class="input" type="number" step="0.5" bind:value={doc.offset_x} /></label>
          <label class="field">Offset Y <input class="input" type="number" step="0.5" bind:value={doc.offset_y} /></label>
        </div>
        {#if grid}<p class="small muted">Cell {grid.cell_w.toFixed(2)} × {grid.cell_h.toFixed(2)} px</p>{/if}
        <div class="row">
          <input type="color" class="swatch" aria-label="Grid colour" value={hex(doc.color)} oninput={(e) => (doc!.color = fromHex(e.currentTarget.value))} />
          <select class="input grow" aria-label="Line style" bind:value={doc.style}>
            <option value="solid">Solid</option><option value="dashed">Dashed</option>
            <option value="dotted">Dotted</option><option value="broken">Broken</option>
          </select>
        </div>
        <label class="slider">Thickness <input type="range" min="1" max="10" bind:value={doc.thickness} /><span>{doc.thickness}</span></label>
        <label class="slider">Opacity <input type="range" min="0" max="255" bind:value={doc.opacity} /><span>{doc.opacity}</span></label>
        <label class="check"><input type="checkbox" bind:checked={doc.visible} /> Show grid</label>
      </details>

      <details open>
        <summary>Embedded grid</summary>
        <div class="segmented" role="group" aria-label="Embedded grid">
          <button aria-pressed={!doc.remove_grid} onclick={() => (doc!.remove_grid = false)}>Keep</button>
          <button aria-pressed={doc.remove_grid} onclick={() => (doc!.remove_grid = true)}>Remove embedded grid</button>
        </div>
      </details>

      <details>
        <summary>Filters</summary>
        <FiltersPanel bind:pipeline={doc.filters} {catalog} />
      </details>

      <details open>
        <summary>Print</summary>
        <div class="segmented" role="group" aria-label="Page format">
          <button aria-pressed={doc.format === 'A4'} onclick={() => (doc!.format = 'A4')}>A4</button>
          <button aria-pressed={doc.format === 'Letter'} onclick={() => (doc!.format = 'Letter')}>Letter</button>
        </div>
        <div class="pair">
          <label class="field">1 square = (in) <input class="input" type="number" min="0.05" step="0.05" bind:value={doc.square_in} /></label>
          <label class="field">1 in prints as (mm) <input class="input" type="number" min="1" step="0.1" bind:value={doc.calibration_mm} /></label>
          <label class="field">Margin (in) <input class="input" type="number" min="0" step="0.05" bind:value={doc.margin_in} /></label>
          <label class="field">Overlap (in) <input class="input" type="number" min="0" step="0.05" bind:value={doc.overlap_in} /></label>
        </div>
        <label class="check"><input type="checkbox" bind:checked={doc.align_to_grid} /> Align pages to grid</label>
        <label class="check"><input type="checkbox" bind:checked={doc.cut_border} /> Cut border</label>
        <div class="row fitrow">
          <span class="small">Fit area</span>
          <input class="input tiny" type="number" min="0" step="0.5" aria-label="Fit width (ft)" bind:value={doc.fit_w_ft} />
          <span class="small">×</span>
          <input class="input tiny" type="number" min="0" step="0.5" aria-label="Fit height (ft)" bind:value={doc.fit_h_ft} />
          <span class="small">ft</span>
        </div>
        {#if fitLine}<p class="small" class:ok={fitLine.ok} class:bad={!fitLine.ok} data-testid="fit-check">{fitLine.text}</p>{/if}
      </details>

      <details open>
        <summary>Pages</summary>
        <div class="row">
          <button class="btn" onclick={() => doc && ((doc.region = null), (doc.excluded = []))} disabled={!doc.region}>Whole map</button>
          <button class="btn" onclick={() => doc && (doc.excluded = [])} disabled={!doc.excluded.length}>Include all</button>
          <button class="btn" onclick={() => doc && (doc.page_offset = [0, 0])} disabled={!doc.page_offset[0] && !doc.page_offset[1]}>Reset layout</button>
        </div>
        <p class="small" data-testid="tiling-summary">{layout?.summary || 'No pages.'}</p>
        {#if tiles.length}
          <div class="row">
            <button class="btn icon" aria-label="Previous page" disabled={page <= 0} onclick={() => page--}>‹</button>
            <span class="small grow center">Page {Math.min(page, tiles.length - 1) + 1} of {tiles.length}</span>
            <button class="btn icon" aria-label="Next page" disabled={page >= tiles.length - 1} onclick={() => page++}>›</button>
          </div>
          {#if curTile}
            <label class="check">
              <input type="checkbox" checked={!isExcluded(curTile.col, curTile.row)} onchange={() => toggleTile(curTile.col, curTile.row)} /> Include this page
            </label>
          {/if}
          <div class="page">
            {#if pageUrl}<img src={pageUrl} alt="Page preview" data-testid="page-preview" />{/if}
            {#if curTile && isExcluded(curTile.col, curTile.row)}<span class="excluded">EXCLUDED</span>{/if}
          </div>
        {/if}
      </details>
      {#if doc.asset_id !== null}<p class="small muted">Editing library asset #{doc.asset_id}</p>{/if}
    {:else}
      <p class="muted">No map loaded.</p>
    {/if}
  </aside>
</div>

<style>
  .paginator {
    display: grid;
    /* Wider than other views: the paginator has the most settings. */
    grid-template-columns: minmax(0, 1fr) clamp(320px, 30vw, 440px);
    height: 100%;
    min-height: 0;
  }
  .work {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    padding: var(--space-3) var(--space-4);
    gap: var(--space-2);
  }
  .toolbar,
  .row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  .viewctl {
    display: flex;
    flex: none;
    gap: var(--space-2);
    align-items: center;
  }
  .spacer,
  .grow {
    flex: 1;
  }
  .center {
    text-align: center;
  }
  .zoom {
    min-width: 44px;
    text-align: center;
    font-size: 12px;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: 13px;
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    border-radius: var(--radius);
    background: var(--surface-2);
    cursor: grab;
    touch-action: none;
    user-select: none;
  }
  .stage.panning {
    cursor: grabbing;
  }
  .stage.tool-region,
  .stage.tool-reference,
  .stage.tool-nudge {
    cursor: crosshair;
  }
  .stage.tool-layout {
    cursor: move;
  }
  .layer {
    position: absolute;
    left: 0;
    top: 0;
    transform-origin: 0 0;
  }
  .layer img,
  .layer svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  .layer svg {
    overflow: visible;
  }
  .outside {
    fill: rgb(0 0 0 / 0.35);
  }
  .region {
    fill: none;
    stroke: #2196f3;
  }
  .handle {
    fill: #fff;
    stroke: #2196f3;
  }
  .tile {
    fill: transparent;
    stroke: #ff9800;
    cursor: pointer;
  }
  .tile.cur {
    stroke: #e65100;
  }
  .tile.ex {
    fill: rgb(0 0 0 / 0.45);
    stroke: #9e9e9e;
  }
  .x {
    fill: #d32f2f;
    text-anchor: middle;
    dominant-baseline: middle;
    pointer-events: none;
  }
  .num {
    fill: #ff9800;
    font-weight: 700;
    pointer-events: none;
  }
  .quad {
    fill: rgb(76 175 80 / 0.15);
    stroke: #4caf50;
  }
  .corner {
    fill: #fff;
    stroke: #2e7d32;
  }
  .drag {
    fill: rgb(33 150 243 / 0.12);
    stroke: #2196f3;
    stroke-dasharray: 6 4;
    vector-effect: non-scaling-stroke;
  }
  .drop {
    position: absolute;
    inset: var(--space-6);
    display: grid;
    place-content: center;
    gap: 4px;
    border: 2px dashed var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    cursor: pointer;
  }
  .badge {
    position: absolute;
    bottom: 6px;
    right: 8px;
    font-size: 11px;
  }
  .hint {
    margin: 0;
    font-size: 12px;
  }
  .inspector {
    border-left: 1px solid var(--border);
    background: var(--surface);
    padding: var(--space-3) var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    overflow-x: hidden;
    overflow-y: auto;
    min-width: 0;
  }
  .inspector :global(.input) {
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
  }
  .inspector .tiny {
    width: 64px;
    flex: none;
  }
  details {
    display: grid;
    gap: var(--space-2);
    border-bottom: 1px solid var(--border);
    padding-bottom: var(--space-2);
  }
  details > :global(*:not(summary)) {
    margin-top: var(--space-2);
  }
  summary {
    font-weight: 600;
    cursor: pointer;
  }
  .segmented.modes {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  .segmented.modes button:nth-child(odd) {
    border-left: 0;
  }
  .segmented.modes button:nth-child(n + 3) {
    border-top: 1px solid var(--border);
  }
  .segmented.tools {
    flex-wrap: wrap;
  }
  .glyph {
    margin-right: 6px;
  }
  .toolhint {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    font-size: 13px;
  }
  .field {
    display: grid;
    gap: 2px;
    font-size: 12px;
  }
  .pair {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--space-2);
  }
  .slider {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) 32px;
    gap: 6px;
    align-items: center;
    font-size: 12px;
  }
  .slider input {
    accent-color: var(--accent);
    width: 100%;
    min-width: 0;
  }
  .swatch {
    width: 40px;
    height: 32px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
  }
  .tiny {
    width: 64px;
  }
  .small {
    margin: 0;
    font-size: 12px;
  }
  .ok {
    color: #2e7d32;
  }
  .bad {
    color: #c62828;
  }
  .choice {
    padding: var(--space-3);
    display: grid;
    gap: var(--space-2);
  }
  .page {
    position: relative;
    display: grid;
    place-items: center;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    padding: var(--space-2);
    min-height: 120px;
  }
  .page img {
    max-width: 100%;
    box-shadow: var(--shadow);
    background: #fff;
  }
  .excluded {
    position: absolute;
    color: #d32f2f;
    font-weight: 700;
  }
</style>
