<script lang="ts" module>
  type Tab = 'scale' | 'pages' | 'look';
  // the inspector tab survives leaving and re-entering the view
  let lastTab: Tab = 'scale';
</script>

<script lang="ts">
  // Map Paginator: grid sizing, grid removal, filters, de-warp and true-scale
  // page tiling. Spec: docs/features/map-paginator.md.
  // The whole state is a MapDoc (full-res image px). Zoom/pan, the grid, tiles
  // and tools are client-side vectors over the Rust-processed proxy image.
  import { onDestroy, onMount } from 'svelte';
  import { api, IMAGE_EXTENSIONS, isTauri, onFileDrop, pickFile, pickSavePath } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { PinchTracker, wheelIntent } from '../../lib/gestures';
  import type { FilterCatalog, MapDoc, MapLayout, Pt, SaveDetails, SizingMode } from '../../lib/types';
  import SaveAssetDialog from '../../lib/components/SaveAssetDialog.svelte';
  import ViewActions from '../../lib/components/ViewActions.svelte';
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

  const TABS: [Tab, string][] = [
    ['scale', 'Scale'],
    ['pages', 'Pages'],
    ['look', 'Look'],
  ];
  // Tools that edit something in a tab bring that tab up.
  const TOOL_TAB: Partial<Record<Tool, Tab>> = { region: 'pages', layout: 'pages', reference: 'scale', nudge: 'scale' };

  let doc = $state<MapDoc | null>(null);
  let layout = $state<MapLayout | null>(null);
  let catalog = $state<FilterCatalog | null>(null);
  let previewUrl = $state<string | null>(null);
  let pageUrl = $state<string | null>(null);
  let rendering = $state(false);
  let busy = $state(false);
  let loaded = false;

  let tool = $state<Tool>('none');
  let tab = $state<Tab>(lastTab);
  $effect(() => {
    lastTab = tab;
  });
  function setTool(t: Tool) {
    tool = t;
    tab = TOOL_TAB[t] ?? tab;
  }
  let showTiles = $state(true);
  let page = $state(0);
  let drag = $state<Drag | null>(null);
  let shift = $state(false);
  let spaceDown = $state(false);
  const pinch = new PinchTracker();

  let saveOpen = $state(false);
  let saveName = $state('');
  let saveJob = $state<string | null>(null);
  /** tags chosen in the save dialog, applied once the save job has the asset id */
  let saveTags: string[] = [];

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
  // Touchpad pinch / mouse wheel zoom at the cursor; two-finger scroll pans.
  function wheel(node: HTMLElement) {
    const h = (e: WheelEvent) => {
      if (!doc?.source) return;
      e.preventDefault();
      const r = node.getBoundingClientRect();
      const g = wheelIntent(e);
      if (g.kind === 'zoom') zoomAt(g.factor, e.clientX - r.left, e.clientY - r.top);
      else {
        tx += g.dx;
        ty += g.dy;
      }
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
      void api.librarySetTags(a.id, saveTags).catch((e) => app.error('Could not save the tags', e));
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
    // a second finger turns any drag into a pinch
    if (pinch.down(e)) return void (drag = null);
    const p = toImage(e);
    if (e.button === 1 || spaceDown || (e.button === 0 && tool === 'none')) {
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
    const st = pinch.move(e);
    if (st && stage) {
      const r = stage.getBoundingClientRect();
      tx += st.dx;
      ty += st.dy;
      zoomAt(st.factor, st.cx - r.left, st.cy - r.top);
      return;
    }
    if (pinch.active || !drag || !doc) return;
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
    pinch.up(e);
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
    if (e.key === ' ') {
      // Space + drag pans with any tool
      spaceDown = e.type === 'keydown';
      e.preventDefault();
      return;
    }
    if (e.type !== 'keydown') return;
    if (e.key === 'Escape') tool = 'none';
    else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'v') {
      e.preventDefault();
      void paste();
    }
  }

  // ── actions ──
  /** Detects the embedded grid and adopts its square size and offset. */
  async function detectGrid() {
    if (!doc) return;
    busy = true;
    try {
      const d = await api.mapDetectGrid(snap());
      doc = d;
      app.toast('success', `Grid detected · 1 square = ${d.side} px`);
    } catch (e) {
      app.error('Grid not detected', e);
    } finally {
      busy = false;
    }
  }

  /** Print region = content inside uniform borders, snapped to grid lines. */
  async function fitRegion() {
    if (!doc) return;
    busy = true;
    try {
      const before = JSON.stringify(doc.region);
      const d = await api.mapFitRegion(snap());
      doc = d;
      if (JSON.stringify(d.region) === before) app.toast('info', 'The print area already fits the map');
      else if (!d.region) app.toast('success', 'The map fills the image · whole map');
      else app.toast('success', `Print area fitted · ${Math.round(d.region[2] / (layout?.grid.cell_w || 1))}×${Math.round(d.region[3] / (layout?.grid.cell_h || 1))} squares`);
    } catch (e) {
      app.error('Could not fit the print area', e);
    } finally {
      busy = false;
    }
  }

  /** Applies the orientation and page shift with the fewest pages. */
  async function fewestPages() {
    if (!doc || !layout?.fewest) return;
    const before = layout.tiles.length;
    const f = layout.fewest;
    try {
      adopt(await api.mapFewestPages(snap()));
      app.toast('success', `${before} → ${f.pages} pages · ${f.landscape ? 'landscape' : 'portrait'}`);
    } catch (e) {
      app.error('Could not rearrange the pages', e);
    }
  }

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
  async function save({ name, tags, update }: SaveDetails) {
    if (!doc) return;
    saveTags = tags;
    busy = true;
    try {
      saveJob = await api.mapSaveAsset(snap(), name, update ? doc.asset_id : null);
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


<svelte:window onkeydown={onKey} onkeyup={onKey} onblur={() => (spaceDown = false)} />

<ViewActions>
  <button class="btn" onclick={browse} title="Load a map image from disk">Open map…</button>
  <button class="btn" onclick={paste} title="Load the image on the clipboard (Ctrl+V)">Paste</button>
  <button class="btn" disabled={!doc?.source || busy} onclick={openSave} title="Store the map and these settings in the Library">Save to library…</button>
  <button class="btn primary" disabled={!doc?.source || !layout?.selected} onclick={exportPdf} title="Write the selected pages as a true-scale PDF">Export PDF</button>
</ViewActions>

<div class="view">
  <section class="work" aria-label="Map">
    <div class="view-toolbar">
      <div class="segmented tools" role="group" aria-label="Canvas tool">
        {#each TOOLS as t (t.id)}
          <button aria-pressed={tool === t.id} aria-label={t.label} onclick={() => setTool(t.id)} title="{t.label}: {t.help}">
            <span class="glyph" aria-hidden="true">{t.glyph}</span><span class="tool-label" aria-hidden="true">{t.label}</span>
          </button>
        {/each}
      </div>
      <div class="spacer"></div>
      <label class="check" title="Show the printed pages over the map"><input type="checkbox" bind:checked={showTiles} /> Show pages</label>
    </div>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="stage-area stage tool-{tool}"
      class:panning={drag?.kind === 'pan'}
      bind:this={stage}
      use:wheel
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={(e) => (pinch.up(e), (drag = null))}
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
        <p class="toolhint" data-testid="tool-hint">
          <strong>{activeTool.label}:</strong>
          {activeTool.help}{refFocus ? ' Grid and pages are hidden while this tool is active.' : ''}
        </p>
        {#if rendering}<span class="badge">processing…</span>{/if}
        <div class="canvas-controls" role="group" aria-label="Zoom" onpointerdown={(e) => e.stopPropagation()}>
          <button class="btn icon" aria-label="Zoom out" onclick={() => zoomCentre(1 / 1.25)}>−</button>
          <span class="zoom-readout">{Math.round(z * 100)}%</span>
          <button class="btn icon" aria-label="Zoom in" onclick={() => zoomCentre(1.25)}>+</button>
          <button class="btn" onclick={fit} title="Fit the whole map in view">Fit</button>
        </div>
      {:else}
        <button class="drop-zone" onclick={browse}>
          <strong>Drop a battle map here</strong>
          <span class="muted">or click to browse · Ctrl+V to paste · or “Open in Map Paginator” from the Library</span>
        </button>
      {/if}
    </div>
    <div class="statusbar">
      {#if !doc?.source}
        <span>Load a map to start.</span>
      {:else}
        <span>{W} × {H} px{doc.dpi_detected ? ` · ${Math.round(doc.detected_dpi)} dpi` : ''}</span>
        {#if layout?.summary}<span>{layout.summary}</span>{/if}
        <span class="spacer"></span>
        <span>Pinch or wheel zooms · two-finger scroll, Space+drag or middle-drag pans</span>
      {/if}
    </div>
  </section>

  <aside class="inspector" aria-label="Map settings">
    {#if doc}
      <div class="inspector-tabs">
        <div class="segmented" role="tablist" aria-label="Settings">
          {#each TABS as [id, label] (id)}
            <button role="tab" id="map-tab-{id}" aria-selected={tab === id} aria-controls="map-panel" onclick={() => (tab = id)}>{label}</button>
          {/each}
        </div>
      </div>

      <div id="map-panel" role="tabpanel" aria-labelledby="map-tab-{tab}">
        {#if tab === 'scale'}
          <section class="section">
            <h3 class="section-title">Square size</h3>
            <button class="btn block" disabled={busy} onclick={detectGrid} title="Find the grid drawn in the map and set the square size and line offset from it">⌗ Detect grid</button>
            <p class="note">Reads the grid drawn on the map. Or set it by hand:</p>
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
                <p class="note ok">✓ Map de-warped to a flat, even grid.</p>
                <label class="field">Square size (px) <input class="input" type="number" min="2" max="2000" step="0.1" bind:value={doc.side} /></label>
                <button class="btn" disabled={busy} onclick={undoDewarp}>↺ Undo de-warp</button>
              {:else}
                <label class="field">Block is N×N squares <input class="input" type="number" min="1" max="99" bind:value={doc.ref_n} /></label>
                <p class="note">Box a block of known size with the Reference square tool, then drag its corners onto the block's real corners to de-warp the whole map.</p>
                <div class="pair">
                  <button class="btn" disabled={busy} onclick={() => setTool('reference')}>{doc.ref_quad ? 'Edit block' : 'Draw block'}</button>
                  <button class="btn primary" disabled={!doc.ref_quad || busy} onclick={dewarp}>▦ De-warp</button>
                </div>
              {/if}
            {:else if doc.mode === 'ppi'}
              <label class="field">Pixels per inch <input class="input" type="number" min="10" max="1200" step="0.1" bind:value={doc.ppi} /></label>
            {:else}
              <label class="field">Cell side (px) <input class="input" type="number" min="2" max="2000" step="0.1" bind:value={doc.side} /></label>
            {/if}
            {#if grid}<p class="note">1 square = {grid.cell_w.toFixed(2)} × {grid.cell_h.toFixed(2)} px</p>{/if}
          </section>

          <section class="section">
            <h3 class="section-title">Grid position</h3>
            <div class="pair">
              <label class="field">Offset X <input class="input" type="number" step="0.5" bind:value={doc.offset_x} /></label>
              <label class="field">Offset Y <input class="input" type="number" step="0.5" bind:value={doc.offset_y} /></label>
            </div>
            <button class="btn" aria-pressed={tool === 'nudge'} onclick={() => setTool(tool === 'nudge' ? 'none' : 'nudge')}>⌖ Align on the map…</button>
          </section>
        {:else if tab === 'pages'}
          {#if layout?.fewest}
            <section class="section">
              <div class="callout" data-testid="fewest-pages">
                <p>
                  {layout.fewest.landscape !== doc.landscape ? (layout.fewest.landscape ? 'Landscape' : 'Portrait') : 'A shifted layout'}
                  prints this on <strong>{layout.fewest.pages}</strong> page{layout.fewest.pages === 1 ? '' : 's'} instead of {layout.tiles.length}.
                </p>
                <button class="btn primary" onclick={fewestPages} title="Switch orientation and shift the pages; page selections are cleared">Use fewest pages</button>
              </div>
            </section>
          {/if}

          <section class="section">
            <h3 class="section-title">Paper</h3>
            <div class="prop">
              <span>Format</span>
              <div class="segmented" role="group" aria-label="Page format">
                <button aria-pressed={doc.format === 'A4'} onclick={() => (doc!.format = 'A4')}>A4</button>
                <button aria-pressed={doc.format === 'Letter'} onclick={() => (doc!.format = 'Letter')}>Letter</button>
              </div>
            </div>
            <div class="prop">
              <span>Orientation</span>
              <div class="segmented" role="group" aria-label="Page orientation">
                <button aria-pressed={!doc.landscape} onclick={() => (doc!.landscape = false)}>Portrait</button>
                <button aria-pressed={doc.landscape} onclick={() => (doc!.landscape = true)}>Landscape</button>
              </div>
            </div>
            <label class="prop">
              <span>1 square =</span>
              <span class="with-unit"><input class="input" type="number" min="0.05" step="0.05" aria-label="1 square = (in)" bind:value={doc.square_in} /><span>in</span></span>
            </label>
            <label class="check" title="Lines along every edge of each page's map, out to the paper edge, to trim the white margin">
              <input type="checkbox" bind:checked={doc.cut_border} /> Cut lines (trim the white margin)
            </label>
            {#if doc.cut_border && (doc.margin_in ?? 0) < 0.2}
              <div class="note warn" data-testid="margin-hint">
                Most printers can't print within about 0.2 in (5 mm) of the paper edge, so the map's edges and the cut lines get clipped.
                <button class="btn ghost linkish" onclick={() => (doc!.margin_in = 0.2)}>Set margin to 0.2 in</button>
              </div>
            {/if}
          </section>

          <section class="section">
            <h3 class="section-title">Print area</h3>
            <div class="pair">
              <button class="btn" disabled={busy} onclick={fitRegion} title="Trim blank or solid borders and snap the print area to whole squares">Fit to map</button>
              <button class="btn" onclick={() => doc && ((doc.region = null), (doc.excluded = []))} disabled={!doc.region}>Whole map</button>
            </div>
            <div class="pair">
              <button class="btn" aria-pressed={tool === 'region'} onclick={() => setTool(tool === 'region' ? 'none' : 'region')}>▭ Draw…</button>
              <button class="btn" aria-pressed={tool === 'layout'} onclick={() => setTool(tool === 'layout' ? 'none' : 'layout')}>⇲ Move pages…</button>
            </div>
            {#if doc.page_offset[0] || doc.page_offset[1]}
              <button class="btn ghost" onclick={() => doc && (doc.page_offset = [0, 0])}>Reset page position</button>
            {/if}
          </section>

          <section class="section">
            <h3 class="section-title">Pages</h3>
            <p class="note" data-testid="tiling-summary">{layout?.summary || 'No pages.'}</p>
            {#if tiles.length}
              <div class="pager">
                <button class="btn icon" aria-label="Previous page" disabled={page <= 0} onclick={() => page--}>‹</button>
                <span>Page {Math.min(page, tiles.length - 1) + 1} of {tiles.length}</span>
                <button class="btn icon" aria-label="Next page" disabled={page >= tiles.length - 1} onclick={() => page++}>›</button>
              </div>
              <div class="page">
                {#if pageUrl}<img src={pageUrl} alt="Page preview" data-testid="page-preview" />{/if}
                {#if curTile && isExcluded(curTile.col, curTile.row)}<span class="excluded">EXCLUDED</span>{/if}
              </div>
              <div class="row">
                {#if curTile}
                  <label class="check grow">
                    <input type="checkbox" checked={!isExcluded(curTile.col, curTile.row)} onchange={() => toggleTile(curTile.col, curTile.row)} /> Print this page
                  </label>
                {/if}
                <button class="btn ghost" onclick={() => doc && (doc.excluded = [])} disabled={!doc.excluded.length}>Include all</button>
              </div>
              <p class="note">Click a page on the map to leave it out.</p>
            {/if}
          </section>

          <details class="section">
            <summary>Advanced</summary>
            <div class="pair">
              <label class="field">Margin (in) <input class="input" type="number" min="0" step="0.05" bind:value={doc.margin_in} /></label>
              <label class="field">Overlap (in) <input class="input" type="number" min="0" step="0.05" bind:value={doc.overlap_in} /></label>
            </div>
            <label class="field">1 in prints as (mm) <input class="input" type="number" min="1" step="0.1" bind:value={doc.calibration_mm} /></label>
            <p class="note">Printer calibration: measure a printed inch and enter it here.</p>
            <label class="check"><input type="checkbox" bind:checked={doc.align_to_grid} /> Align pages to grid</label>
            <div class="field">
              <span class="label">Check it fits a table of</span>
              <div class="with-unit">
                <input class="input" type="number" min="0" step="0.5" aria-label="Fit width (ft)" bind:value={doc.fit_w_ft} />
                <span>×</span>
                <input class="input" type="number" min="0" step="0.5" aria-label="Fit height (ft)" bind:value={doc.fit_h_ft} />
                <span>ft</span>
              </div>
            </div>
            {#if fitLine}<p class="note" class:ok={fitLine.ok} class:bad={!fitLine.ok} data-testid="fit-check">{fitLine.text}</p>{/if}
          </details>
        {:else}
          <section class="section">
            <h3 class="section-title">Grid lines</h3>
            <label class="check"><input type="checkbox" bind:checked={doc.visible} /> Draw the grid</label>
            <div class="prop">
              <span>Line</span>
              <div class="row">
                <input type="color" class="swatch" aria-label="Grid colour" value={hex(doc.color)} oninput={(e) => (doc!.color = fromHex(e.currentTarget.value))} />
                <select class="input grow" aria-label="Line style" bind:value={doc.style}>
                  <option value="solid">Solid</option><option value="dashed">Dashed</option>
                  <option value="dotted">Dotted</option><option value="broken">Broken</option>
                </select>
              </div>
            </div>
            <label class="range"><span>Thickness</span><input type="range" min="1" max="10" bind:value={doc.thickness} /><span>{doc.thickness}</span></label>
            <label class="range"><span>Opacity</span><input type="range" min="0" max="255" bind:value={doc.opacity} /><span>{doc.opacity}</span></label>
          </section>

          <section class="section">
            <h3 class="section-title">Grid printed in the map</h3>
            <div class="segmented stretch" role="group" aria-label="Embedded grid">
              <button aria-pressed={!doc.remove_grid} onclick={() => (doc!.remove_grid = false)}>Keep</button>
              <button aria-pressed={doc.remove_grid} onclick={() => (doc!.remove_grid = true)}>Remove</button>
            </div>
            <p class="note">Removing it paints over the map's own lines, using the square size.</p>
          </section>

          <section class="section">
            <h3 class="section-title">Filters</h3>
            <FiltersPanel bind:pipeline={doc.filters} {catalog} />
          </section>
        {/if}
      </div>
    {:else}
      <div class="empty-state">
        <strong>No map loaded</strong>
        <span>Open a map, paste one, or drop it on the canvas.</span>
      </div>
    {/if}
    {#if doc?.asset_id != null}<p class="note asset">Editing library asset #{doc.asset_id}</p>{/if}
  </aside>
</div>

<SaveAssetDialog bind:open={saveOpen} name={saveName} kind="map" existing={doc?.asset_id ?? null} onsave={save} />

<style>
  .tools {
    flex-wrap: wrap;
  }
  .glyph {
    display: inline-block;
    min-width: 1em;
    text-align: center;
  }
  .tool-label {
    margin-left: 6px;
  }
  /* narrow canvas: tools show their glyph only (the name stays in the tooltip) */
  @container (max-width: 680px) {
    .tool-label {
      display: none;
    }
  }
  .stage {
    overflow: hidden;
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
  /* the active tool's help floats over the top of the canvas */
  .toolhint {
    position: absolute;
    top: var(--space-2);
    left: var(--space-2);
    right: var(--space-2);
    max-width: max-content;
    margin: 0;
    padding: 6px var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--surface) 92%, transparent);
    box-shadow: var(--shadow);
    font-size: 12px;
    pointer-events: none;
  }
  .badge {
    position: absolute;
    bottom: var(--space-2);
    left: var(--space-2);
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text-muted);
    font-size: 11px;
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
  .segmented.stretch {
    display: flex;
  }
  .segmented.stretch button {
    flex: 1;
  }
  .field {
    font-size: 12px;
  }
  .page {
    position: relative;
    display: grid;
    place-items: center;
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    min-height: 120px;
  }
  .page img {
    max-width: 100%;
    max-height: 260px;
    box-shadow: var(--shadow);
    background: #fff;
  }
  .excluded {
    position: absolute;
    color: #d32f2f;
    font-weight: 700;
  }
  .linkish {
    display: block;
    min-height: 0;
    margin-top: 4px;
    padding: 0;
    color: var(--text);
    text-decoration: underline;
    font-size: 12px;
  }
  .asset {
    margin-top: auto;
    padding: var(--space-2) var(--space-4);
    border-top: 1px solid var(--border);
  }
</style>
