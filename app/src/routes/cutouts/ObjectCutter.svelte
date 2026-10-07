<script lang="ts">
  // Object Cutter: lasso an object, add a sticker outline. Spec: docs/features/object-cutter.md.
  // The trace lives in source-image pixels; zoom/pan is a CSS transform, and the
  // cut-out result is rendered in Rust (gated: one request in flight, latest wins).
  import { onDestroy, onMount } from 'svelte';
  import { api, blobUrl, IMAGE_EXTENSIONS, isTauri, onFileDrop, pickFile, pickSavePath } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { SIZE_LABELS, type CutoutParams, type LassoMode, type SourceImage } from '../../lib/types';

  const DRAFT_KEY = 'cutouts.cutter';

  interface Draft {
    source: SourceImage | null;
    params: CutoutParams;
    closed: boolean;
    name: string;
    size: string;
    asset_id: number | null;
  }

  const blankParams = (): CutoutParams => ({ mode: 'polygon', points: [], outline_rgb: [255, 255, 255], outline_px: 6 });
  const blank = (): Draft => ({ source: null, params: blankParams(), closed: false, name: '', size: 'Medium', asset_id: null });

  let d = $state<Draft>(blank());
  let loaded = $state(false);
  let resultUrl = $state<string | null>(null);
  let rendering = $state(false);
  let saveChoice = $state(false);
  let busy = $state(false);

  // View transform (screen px): image pixel (ix, iy) is drawn at (tx + ix·z, ty + iy·z).
  let stage: HTMLDivElement | undefined = $state();
  let z = $state(1);
  let tx = $state(0);
  let ty = $state(0);
  let spaceDown = $state(false);
  let panning = $state<{ x: number; y: number } | null>(null);
  let drawing = $state(false);

  const outlineHex = $derived('#' + d.params.outline_rgb.map((c) => c.toString(16).padStart(2, '0')).join(''));
  const pointsAttr = $derived(d.params.points.map(([x, y]) => `${x},${y}`).join(' '));
  const canCut = $derived(!!d.source && d.closed && d.params.points.length >= 3);

  function fit() {
    if (!stage || !d.source) return;
    const k = Math.min(stage.clientWidth / d.source.width, stage.clientHeight / d.source.height) * 0.95;
    z = Math.max(0.25, Math.min(10, k));
    tx = (stage.clientWidth - d.source.width * z) / 2;
    ty = (stage.clientHeight - d.source.height * z) / 2;
  }

  function zoomAt(factor: number, cx: number, cy: number) {
    const nz = Math.max(0.25, Math.min(10, z * factor));
    tx = cx - (cx - tx) * (nz / z);
    ty = cy - (cy - ty) * (nz / z);
    z = nz;
  }

  function zoomCentre(factor: number) {
    if (stage) zoomAt(factor, stage.clientWidth / 2, stage.clientHeight / 2);
  }

  function toImage(e: { clientX: number; clientY: number }): [number, number] {
    const r = stage!.getBoundingClientRect();
    return [(e.clientX - r.left - tx) / z, (e.clientY - r.top - ty) / z];
  }

  // ── rendering ──
  let inFlight = false;
  let dirty = false;
  let renderTimer: ReturnType<typeof setTimeout> | null = null;
  async function render() {
    if (!canCut || !d.source) {
      if (resultUrl) URL.revokeObjectURL(resultUrl);
      resultUrl = null;
      return;
    }
    if (inFlight) {
      dirty = true;
      return;
    }
    inFlight = true;
    rendering = true;
    try {
      do {
        dirty = false;
        const blob = await api.cutoutRender(d.source.hash, $state.snapshot(d.params) as CutoutParams, true);
        if (resultUrl) URL.revokeObjectURL(resultUrl);
        resultUrl = URL.createObjectURL(blob);
      } while (dirty && canCut);
    } catch (e) {
      app.error('Could not cut', e);
    } finally {
      inFlight = false;
      rendering = false;
    }
  }

  $effect(() => {
    void $state.snapshot(d.params);
    void d.closed;
    void d.source?.hash;
    if (renderTimer) clearTimeout(renderTimer);
    renderTimer = setTimeout(() => void render(), 80);
  });

  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const snap = $state.snapshot(d);
    if (!loaded) return;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void api.workspaceSet(DRAFT_KEY, snap).catch(() => {}), 400);
  });

  let unlisten: (() => void) | undefined;
  onMount(async () => {
    try {
      const saved = await api.workspaceGet<Draft>(DRAFT_KEY).catch(() => null);
      if (saved) d = saved;
      const req = app.openRequest;
      if (req && req.route === 'cutouts.cutter') {
        app.openRequest = null;
        const draft = await api.cutoutOpenAsset(req.assetId);
        d = { ...draft, closed: draft.params.points.length >= 3 };
      }
    } catch (e) {
      app.error('Could not load the object cutter', e);
    } finally {
      loaded = true;
      requestAnimationFrame(fit);
    }
    unlisten = await onFileDrop((paths) => {
      const img = paths.find((p) => IMAGE_EXTENSIONS.includes(p.split('.').pop()?.toLowerCase() ?? ''));
      if (img) void loadPath(img);
    });
  });
  onDestroy(() => {
    unlisten?.();
    if (resultUrl) URL.revokeObjectURL(resultUrl);
  });

  function setSource(src: SourceImage) {
    d = { ...d, source: src, asset_id: null, closed: false, params: { ...d.params, points: [] } };
    requestAnimationFrame(fit);
  }

  async function loadPath(path: string) {
    try {
      setSource(await api.imageSourceFromPath(path));
      if (!d.name) d.name = path.split(/[\\/]/).pop()!.replace(/\.[^.]+$/, '');
    } catch (e) {
      app.error('Could not load image', e);
    }
  }

  async function browse() {
    const p = await pickFile('Choose an image', IMAGE_EXTENSIONS, 'Images');
    if (p) await loadPath(p);
    else if (!isTauri()) app.toast('info', 'File dialogs are only available in the desktop app.');
  }

  async function paste() {
    try {
      setSource(await api.imageSourceFromClipboard());
    } catch (e) {
      app.error('Paste failed', e);
    }
  }

  // ── tracing ──
  function setMode(m: LassoMode) {
    d.params.mode = m;
    resetTrace();
  }
  function resetTrace() {
    d.params.points = [];
    d.closed = false;
  }
  function undoPoint() {
    if (d.closed) d.closed = false;
    else d.params.points = d.params.points.slice(0, -1);
  }
  function closeShape() {
    if (d.params.points.length >= 3) d.closed = true;
  }

  function onPointerDown(e: PointerEvent) {
    if (!d.source) return;
    if (e.button === 1 || (e.button === 0 && spaceDown)) {
      panning = { x: e.clientX, y: e.clientY };
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      e.preventDefault();
      return;
    }
    if (e.button !== 0 || d.closed) return;
    const p = toImage(e);
    if (d.params.mode === 'freehand') {
      d.params.points = [p];
      drawing = true;
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } else {
      d.params.points = [...d.params.points, p];
    }
  }

  function onPointerMove(e: PointerEvent) {
    if (panning) {
      tx += e.clientX - panning.x;
      ty += e.clientY - panning.y;
      panning = { x: e.clientX, y: e.clientY };
      return;
    }
    if (drawing) {
      const p = toImage(e);
      const last = d.params.points[d.params.points.length - 1];
      // Thin dense freehand input: keep points ≥ 2 screen px apart.
      if (!last || Math.hypot(p[0] - last[0], p[1] - last[1]) * z >= 2) d.params.points = [...d.params.points, p];
    }
  }

  function onPointerUp() {
    panning = null;
    if (drawing) {
      drawing = false;
      closeShape();
    }
  }

  function onDblClick(e: MouseEvent) {
    if (d.params.mode !== 'polygon' || d.closed) return;
    e.preventDefault();
    // The double-click's second click added a duplicate point; drop it.
    d.params.points = d.params.points.slice(0, -1);
    closeShape();
  }

  function onContextMenu(e: MouseEvent) {
    e.preventDefault();
    if (d.params.mode === 'polygon') closeShape();
  }

  function wheel(node: HTMLElement) {
    const h = (e: WheelEvent) => {
      if (!d.source) return;
      e.preventDefault();
      const r = node.getBoundingClientRect();
      zoomAt(e.deltaY < 0 ? 1.1 : 1 / 1.1, e.clientX - r.left, e.clientY - r.top);
    };
    node.addEventListener('wheel', h, { passive: false });
    return { destroy: () => node.removeEventListener('wheel', h) };
  }

  function onKey(e: KeyboardEvent) {
    // Ignore keys aimed at form fields and buttons (a focused button handles its own Enter/Space).
    const typing = (e.target as HTMLElement)?.matches?.('input, textarea, select, button');
    if (typing) return;
    if (e.key === ' ') {
      spaceDown = e.type === 'keydown';
      e.preventDefault();
      return;
    }
    if (e.type !== 'keydown') return;
    if (e.key === 'Enter') closeShape();
    else if (e.key === 'Escape') resetTrace();
    else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      undoPoint();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'v') {
      e.preventDefault();
      void paste();
    }
  }

  // ── actions ──
  const params = () => $state.snapshot(d.params) as CutoutParams;

  async function saveLibrary(update: boolean) {
    saveChoice = false;
    if (!canCut || !d.source) return;
    if (!d.name.trim()) return app.toast('error', 'Please enter a name first.');
    busy = true;
    try {
      const a = await api.cutoutSaveToLibrary(d.source.hash, params(), d.name, d.size, update ? d.asset_id : null);
      d.asset_id = a.id;
      app.toast('success', update ? `“${a.name}” updated in the library` : `“${a.name}” saved to the library`);
    } catch (e) {
      app.error('Save failed', e);
    } finally {
      busy = false;
    }
  }

  async function addToSheet() {
    if (!canCut || !d.source) return;
    busy = true;
    try {
      await api.objectSheetAddComposed(d.source.hash, params(), d.name, d.size);
      app.toast('success', `“${d.name || 'Cut-Out'}” added to the object sheet`);
    } catch (e) {
      app.error('Could not add to sheet', e);
    } finally {
      busy = false;
    }
  }

  async function exportPng() {
    if (!canCut || !d.source) return;
    const path = isTauri() ? await pickSavePath('Export cut-out', `${d.name || 'cutout'}.png`, ['png']) : `${d.name || 'cutout'}.png`;
    if (!path) return;
    try {
      app.toast('success', `Saved ${await api.cutoutSavePng(d.source.hash, params(), path)}`);
    } catch (e) {
      app.error('Export failed', e);
    }
  }
</script>

<svelte:window onkeydown={onKey} onkeyup={onKey} />

<div class="cutter">
  <section class="work">
    <div class="toolbar">
      <div class="segmented" role="group" aria-label="Selection mode">
        <button aria-pressed={d.params.mode === 'polygon'} onclick={() => setMode('polygon')}>Polygon</button>
        <button aria-pressed={d.params.mode === 'freehand'} onclick={() => setMode('freehand')}>Freehand</button>
      </div>
      <button class="btn" disabled={!d.params.points.length} onclick={undoPoint} title="Ctrl+Z">Undo point</button>
      <button class="btn" disabled={!d.params.points.length} onclick={resetTrace} title="Esc">Reset</button>
      {#if d.params.mode === 'polygon' && !d.closed}
        <button class="btn" disabled={d.params.points.length < 3} onclick={closeShape} title="Enter, double-click or right-click">Close shape</button>
      {/if}
      <div class="spacer"></div>
      <button class="btn icon" aria-label="Zoom out" onclick={() => zoomCentre(1 / 1.25)}>−</button>
      <span class="zoom muted">{Math.round(z * 100)}%</span>
      <button class="btn icon" aria-label="Zoom in" onclick={() => zoomCentre(1.25)}>+</button>
      <button class="btn" onclick={fit}>Fit</button>
      <button class="btn" onclick={browse}>Choose file…</button>
      <button class="btn" onclick={paste}>Paste</button>
    </div>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="stage"
      class:pan={spaceDown || panning}
      class:closed={d.closed}
      bind:this={stage}
      use:wheel
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      ondblclick={onDblClick}
      oncontextmenu={onContextMenu}
      data-testid="cutter-stage"
    >
      {#if d.source}
        <div class="layer" style:transform="translate({tx}px, {ty}px) scale({z})" style:width="{d.source.width}px" style:height="{d.source.height}px">
          <img src={blobUrl(d.source.hash)} alt="Source" draggable="false" />
          <svg viewBox="0 0 {d.source.width} {d.source.height}" width={d.source.width} height={d.source.height}>
            {#if d.params.points.length}
              {#if d.closed}
                <polygon points={pointsAttr} class="sel closed" />
              {:else}
                <polyline points={pointsAttr} class="sel" />
              {/if}
              {#if d.params.mode === 'polygon'}
                {#each d.params.points as [x, y], i (i)}
                  <circle cx={x} cy={y} r={3 / z} class="vertex" />
                {/each}
              {/if}
            {/if}
          </svg>
        </div>
      {:else}
        <button class="drop" onclick={browse}>
          <strong>Drop an image here</strong>
          <span class="muted">or click to browse · Ctrl+V to paste</span>
        </button>
      {/if}
    </div>
    <p class="hint muted">
      {#if !d.source}
        Load an image to start.
      {:else if d.closed}
        Shape closed: adjust the outline or save. Undo point re-opens it.
      {:else if d.params.mode === 'polygon'}
        Click to add points · double-click, right-click or Enter to close · {d.params.points.length} point{d.params.points.length === 1 ? '' : 's'}
      {:else}
        Drag around the object; release to close.
      {/if}
      · wheel zooms, middle-drag or Space+drag pans
    </p>
  </section>

  <aside class="inspector" aria-label="Cut-out settings">
    <div class="result">
      {#if resultUrl}
        <img src={resultUrl} alt="Cut-out result" data-testid="cutout-result" />
      {:else}
        <p class="muted">{d.source ? 'Close the selection to see the cut-out.' : 'No image yet.'}</p>
      {/if}
      {#if rendering}<span class="badge muted">rendering…</span>{/if}
    </div>

    <div class="field">
      <span class="label">Outline</span>
      <div class="row">
        <input type="color" class="swatch" aria-label="Outline colour" value={outlineHex}
          oninput={(e) => (d.params.outline_rgb = [1, 3, 5].map((i) => parseInt(e.currentTarget.value.slice(i, i + 2), 16)) as [number, number, number])} />
        <label class="grow">
          <span class="visually-hidden">Outline thickness</span>
          <input type="range" min="0" max="40" step="1" aria-label="Outline thickness" bind:value={d.params.outline_px} />
        </label>
        <span class="muted">{d.params.outline_px}px</span>
      </div>
    </div>

    <div class="field">
      <label for="cut-name">Name</label>
      <input id="cut-name" class="input" bind:value={d.name} placeholder="e.g. Brazier" />
    </div>
    <div class="field">
      <label for="cut-size">Size <span class="muted">(longest side)</span></label>
      <select id="cut-size" class="input" bind:value={d.size}>
        {#each SIZE_LABELS as s (s)}<option value={s}>{s}</option>{/each}
      </select>
    </div>

    <div class="actions">
      {#if saveChoice}
        <div class="choice card" role="group" aria-label="Save choice">
          <p>This cut-out was opened from the library.</p>
          <button class="btn primary" onclick={() => saveLibrary(true)}>Update existing</button>
          <button class="btn" onclick={() => saveLibrary(false)}>Save as new</button>
          <button class="btn ghost" onclick={() => (saveChoice = false)}>Cancel</button>
        </div>
      {:else}
        <button class="btn primary" disabled={!canCut || busy} onclick={() => (d.asset_id !== null ? (saveChoice = true) : saveLibrary(false))}>
          Save to library
        </button>
      {/if}
      <button class="btn" disabled={!canCut || busy} onclick={addToSheet}>Add to object sheet</button>
      <button class="btn" disabled={!canCut} onclick={exportPng}>Export PNG…</button>
      {#if d.asset_id !== null}<p class="muted small">Editing library asset #{d.asset_id}</p>{/if}
    </div>
  </aside>
</div>

<style>
  .cutter {
    display: grid;
    grid-template-columns: 1fr var(--inspector-w);
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
  .spacer {
    flex: 1;
  }
  .zoom {
    min-width: 44px;
    text-align: center;
    font-size: 12px;
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    border-radius: var(--radius);
    background: repeating-conic-gradient(var(--surface-2) 0 25%, var(--surface) 0 50%) 0 0 / 20px 20px;
    cursor: crosshair;
    touch-action: none;
    user-select: none;
  }
  .stage.pan {
    cursor: grab;
  }
  .stage.closed {
    cursor: default;
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
  .layer img {
    image-rendering: auto;
  }
  .sel {
    fill: none;
    stroke: #ff3b3b;
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
  }
  .sel.closed {
    fill: rgb(255 59 59 / 0.12);
  }
  .vertex {
    fill: #ff3b3b;
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
  .hint {
    margin: 0;
    font-size: 12px;
  }
  .inspector {
    border-left: 1px solid var(--border);
    background: var(--surface);
    padding: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    overflow-y: auto;
  }
  .result {
    position: relative;
    aspect-ratio: 1;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
    background: repeating-conic-gradient(var(--surface-2) 0 25%, var(--surface) 0 50%) 0 0 / 16px 16px;
    text-align: center;
    padding: var(--space-2);
  }
  .result img {
    max-width: 100%;
    max-height: 100%;
  }
  .badge {
    position: absolute;
    bottom: 6px;
    right: 8px;
    font-size: 11px;
  }
  .swatch {
    width: 40px;
    height: 32px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
  }
  .grow {
    flex: 1;
    display: flex;
  }
  .grow input {
    width: 100%;
    accent-color: var(--accent);
  }
  .actions {
    display: grid;
    gap: var(--space-2);
    margin-top: auto;
  }
  .actions > .btn {
    justify-content: center;
  }
  .choice {
    padding: var(--space-3);
    display: grid;
    gap: var(--space-2);
  }
  .choice p,
  .small {
    margin: 0;
    font-size: 13px;
  }
</style>
