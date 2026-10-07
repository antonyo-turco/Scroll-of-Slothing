<script lang="ts">
  // Token Cutter: frame an image inside a border ring.
  // Spec: docs/features/token-cutter.md. Rendering happens in Rust (token_render);
  // requests are gated so at most one is in flight and the latest state wins.
  import { onDestroy, onMount } from 'svelte';
  import { api, blobUrl, IMAGE_EXTENSIONS, isTauri, onFileDrop, pickFile, pickSavePath } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { SIZE_LABELS, type Border, type SourceImage, type TokenParams } from '../../lib/types';

  const DRAFT_KEY = 'tokens.cutter';

  interface Draft {
    source: SourceImage | null;
    params: TokenParams;
    name: string;
    size: string;
    asset_id: number | null;
  }

  const blank = (border = ''): Draft => ({
    source: null,
    params: { border, bg: null, ring_inset_pct: 0, pan: [0, 0], zoom: 1 },
    name: '',
    size: 'Medium',
    asset_id: null,
  });

  let borders = $state<Border[]>([]);
  let d = $state<Draft>(blank());
  let previewUrl = $state<string | null>(null);
  let rendering = $state(false);
  let pipette = $state(false);
  let saveChoice = $state(false);
  let busy = $state(false);
  let loaded = $state(false);
  let stage: HTMLDivElement | undefined = $state();
  let sourceImg: HTMLImageElement | undefined = $state();

  const border = $derived(borders.find((b) => b.id === d.params.border) ?? null);
  const bgHex = $derived(
    d.params.bg ? '#' + d.params.bg.map((c) => c.toString(16).padStart(2, '0')).join('') : '#ffffff',
  );

  // ── rendering (in-flight gating) ──
  let inFlight = false;
  let dirty = false;
  async function requestRender() {
    if (!d.source || !d.params.border) return;
    if (inFlight) {
      dirty = true;
      return;
    }
    inFlight = true;
    rendering = true;
    try {
      do {
        dirty = false;
        const blob = await api.tokenRender(d.source.hash, $state.snapshot(d.params) as TokenParams, true);
        const url = URL.createObjectURL(blob);
        if (previewUrl) URL.revokeObjectURL(previewUrl);
        previewUrl = url;
      } while (dirty && d.source);
    } catch (e) {
      app.error('Could not render the token', e);
    } finally {
      inFlight = false;
      rendering = false;
    }
  }

  // Re-render whenever the source or any parameter changes (not on name/size edits).
  $effect(() => {
    void $state.snapshot(d.params);
    void d.source?.hash;
    void requestRender();
  });

  // Autosave the whole draft (survives restarts).
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
      borders = await api.tokenBordersList();
      const saved = await api.workspaceGet<Draft>(DRAFT_KEY).catch(() => null);
      d = saved && borders.some((b) => b.id === saved.params.border) ? saved : blank(borders[0]?.id ?? '');
      const req = app.openRequest;
      if (req && req.route === 'tokens.cutter') {
        app.openRequest = null;
        await openAsset(req.assetId);
      }
    } catch (e) {
      app.error('Could not load the token cutter', e);
    } finally {
      loaded = true;
    }
    unlisten = await onFileDrop((paths) => {
      const img = paths.find((p) => IMAGE_EXTENSIONS.includes(p.split('.').pop()?.toLowerCase() ?? ''));
      if (img) void loadPath(img);
    });
  });
  onDestroy(() => {
    unlisten?.();
    if (previewUrl) URL.revokeObjectURL(previewUrl);
  });

  async function openAsset(id: number) {
    const draft = await api.tokenOpenAsset(id);
    d = { ...draft };
  }

  function setSource(src: SourceImage) {
    // A fresh image is not a re-edit; framing resets (Python `_set_base_image`).
    d = { ...d, source: src, asset_id: null, params: { ...d.params, pan: [0, 0], zoom: 1 } };
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

  async function addBorder() {
    const p = await pickFile('Add a border (PNG with transparent centre)', ['png', 'webp'], 'Images');
    if (!p) return;
    try {
      const b = await api.tokenBordersAdd(p);
      borders = await api.tokenBordersList();
      d.params.border = b.id;
    } catch (e) {
      app.error('Could not add border', e);
    }
  }

  // ── pan & zoom on the preview ──
  let drag = $state<{ x: number; y: number } | null>(null);
  function onPointerDown(e: PointerEvent) {
    if (!d.source) return;
    drag = { x: e.clientX, y: e.clientY };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onPointerMove(e: PointerEvent) {
    if (!drag || !border || !stage) return;
    const scale = Math.min(stage.clientWidth, stage.clientHeight) / Math.max(border.width, border.height);
    const dx = (e.clientX - drag.x) / scale;
    const dy = (e.clientY - drag.y) / scale;
    drag = { x: e.clientX, y: e.clientY };
    // Dragging moves the image with the pointer (Python: pan -= delta / scale).
    d.params.pan = [d.params.pan[0] - dx, d.params.pan[1] - dy];
  }
  function onPointerUp() {
    drag = null;
  }
  function wheel(node: HTMLElement) {
    const h = (e: WheelEvent) => {
      if (!d.source) return;
      e.preventDefault();
      const f = e.deltaY < 0 ? 1.1 : 1 / 1.1;
      d.params.zoom = Math.max(0.1, Math.min(10, d.params.zoom * f));
    };
    node.addEventListener('wheel', h, { passive: false });
    return { destroy: () => node.removeEventListener('wheel', h) };
  }

  // ── eyedropper on the source image ──
  function pickColour(e: MouseEvent) {
    if (!pipette || !sourceImg) return;
    const r = sourceImg.getBoundingClientRect();
    const x = Math.floor(((e.clientX - r.left) / r.width) * sourceImg.naturalWidth);
    const y = Math.floor(((e.clientY - r.top) / r.height) * sourceImg.naturalHeight);
    const c = document.createElement('canvas');
    c.width = 1;
    c.height = 1;
    const ctx = c.getContext('2d');
    if (!ctx) return;
    try {
      ctx.drawImage(sourceImg, x, y, 1, 1, 0, 0, 1, 1);
      const [rr, gg, bb] = ctx.getImageData(0, 0, 1, 1).data;
      d.params.bg = [rr, gg, bb];
    } catch (err) {
      app.error('Could not sample colour', err);
    }
    pipette = false;
  }

  function setBgHex(hex: string) {
    d.params.bg = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) as [number, number, number];
  }

  // ── actions ──
  const params = () => $state.snapshot(d.params) as TokenParams;

  async function saveLibrary(update: boolean) {
    saveChoice = false;
    if (!d.source) return;
    if (!d.name.trim()) return app.toast('error', 'Please enter a name first.');
    busy = true;
    try {
      const a = await api.tokenSaveToLibrary(d.source.hash, params(), d.name, d.size, update ? d.asset_id : null);
      d.asset_id = a.id;
      app.toast('success', update ? `“${a.name}” updated in the library` : `“${a.name}” saved to the library`);
    } catch (e) {
      app.error('Save failed', e);
    } finally {
      busy = false;
    }
  }

  function onSaveClick() {
    if (d.asset_id !== null) saveChoice = true;
    else void saveLibrary(false);
  }

  async function addToSheet() {
    if (!d.source) return;
    busy = true;
    try {
      await api.tokenSheetAddComposed(d.source.hash, params(), d.name || 'Token', d.size);
      app.toast('success', `“${d.name || 'Token'}” added to the token sheet`);
    } catch (e) {
      app.error('Could not add to sheet', e);
    } finally {
      busy = false;
    }
  }

  async function savePng() {
    if (!d.source) return;
    const path = isTauri() ? await pickSavePath('Save token', `${d.name || 'token'}.png`, ['png']) : `${d.name || 'token'}.png`;
    if (!path) return;
    try {
      app.toast('success', `Saved ${await api.tokenSavePng(d.source.hash, params(), path)}`);
    } catch (e) {
      app.error('Save failed', e);
    }
  }

  function onKey(e: KeyboardEvent) {
    const typing = (e.target as HTMLElement)?.matches?.('input, textarea, select');
    if (typing) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'v') {
      e.preventDefault();
      void paste();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      onSaveClick();
    } else if (e.key === 'Escape') pipette = false;
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="cutter">
  <section class="source" aria-label="Source image">
    <h2>Source</h2>
    {#if d.source}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
      <img
        bind:this={sourceImg}
        class:pipette
        src={blobUrl(d.source.hash)}
        crossorigin="anonymous"
        alt="Source"
        onclick={pickColour}
      />
      <p class="muted small">{d.source.width} × {d.source.height}px{pipette ? ' · click to pick a colour' : ''}</p>
    {:else}
      <button class="drop" onclick={browse}>
        <strong>Drop an image here</strong>
        <span class="muted">or click to browse · Ctrl+V to paste</span>
      </button>
    {/if}
    <div class="row">
      <button class="btn" onclick={browse}>Choose file…</button>
      <button class="btn" onclick={paste}>Paste</button>
    </div>
  </section>

  <section class="stage-wrap" aria-label="Token preview">
    <div
      class="stage"
      class:grabbing={drag !== null}
      bind:this={stage}
      use:wheel
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      role="application"
      aria-label="Drag to pan, scroll to zoom"
    >
      {#if d.source && previewUrl}
        <img src={previewUrl} alt="Token preview" draggable="false" data-testid="token-preview" />
      {:else}
        <p class="muted">Load an image to start.</p>
      {/if}
    </div>
    <div class="stage-bar muted">
      <span>Drag to pan · scroll to zoom · {Math.round(d.params.zoom * 100)}%{rendering ? ' · rendering…' : ''}</span>
      <button class="btn ghost" disabled={!d.source} onclick={() => (d.params = { ...d.params, pan: [0, 0], zoom: 1 })}>Reset view</button>
    </div>
  </section>

  <aside class="inspector" aria-label="Token settings">
    <div class="field">
      <span class="label">Border</span>
      <div class="borders" role="radiogroup" aria-label="Border">
        {#each borders as b (b.id)}
          <button
            role="radio"
            aria-checked={d.params.border === b.id}
            class="border"
            class:sel={d.params.border === b.id}
            title={b.name}
            onclick={() => (d.params.border = b.id)}
          >
            <img src={blobUrl(b.blob)} alt={b.name} />
          </button>
        {/each}
        <button class="border add" title="Add border…" aria-label="Add border" onclick={addBorder}>+</button>
      </div>
    </div>

    <div class="field">
      <span class="label">Background</span>
      <div class="row">
        <div class="segmented" role="group" aria-label="Background">
          <button aria-pressed={d.params.bg === null} onclick={() => (d.params.bg = null)}>None</button>
          <button aria-pressed={d.params.bg !== null} onclick={() => (d.params.bg ??= [255, 255, 255])}>Colour</button>
        </div>
        {#if d.params.bg}
          <input type="color" class="swatch" aria-label="Background colour" value={bgHex} oninput={(e) => setBgHex(e.currentTarget.value)} />
        {/if}
        <button class="btn" class:active={pipette} aria-pressed={pipette} disabled={!d.source} title="Pick a colour from the source image" onclick={() => (pipette = !pipette)}>
          Pick
        </button>
      </div>
    </div>

    <div class="field">
      <label for="ring">Ring thickness <span class="muted">{Math.round(d.params.ring_inset_pct)}%</span></label>
      <input id="ring" type="range" min="0" max="50" step="1" bind:value={d.params.ring_inset_pct} />
    </div>

    <div class="field">
      <label for="token-name">Name</label>
      <input id="token-name" class="input" bind:value={d.name} placeholder="e.g. Goblin Boss" />
    </div>
    <div class="field">
      <label for="token-size">Size</label>
      <select id="token-size" class="input" bind:value={d.size}>
        {#each SIZE_LABELS as s (s)}<option value={s}>{s}</option>{/each}
      </select>
    </div>

    <div class="actions">
      {#if saveChoice}
        <div class="choice card" role="group" aria-label="Save choice">
          <p>This token was opened from the library.</p>
          <button class="btn primary" onclick={() => saveLibrary(true)}>Update existing</button>
          <button class="btn" onclick={() => saveLibrary(false)}>Save as new</button>
          <button class="btn ghost" onclick={() => (saveChoice = false)}>Cancel</button>
        </div>
      {:else}
        <button class="btn primary" disabled={!d.source || busy} onclick={onSaveClick}>Save to library</button>
      {/if}
      <button class="btn" disabled={!d.source || busy} onclick={addToSheet}>Add to token sheet</button>
      <button class="btn" disabled={!d.source} onclick={savePng}>Save PNG…</button>
      {#if d.asset_id !== null}<p class="muted small">Editing library asset #{d.asset_id}</p>{/if}
    </div>
  </aside>
</div>

<style>
  .cutter {
    display: grid;
    grid-template-columns: 260px 1fr var(--inspector-w);
    height: 100%;
    min-height: 0;
  }
  h2 {
    font-size: 13px;
    margin: 0;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .source {
    padding: var(--space-4);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    overflow: auto;
  }
  .source img {
    width: 100%;
    border-radius: var(--radius-sm);
    background: repeating-conic-gradient(var(--surface-2) 0 25%, var(--surface) 0 50%) 0 0 / 16px 16px;
  }
  .source img.pipette {
    cursor: crosshair;
    outline: 2px dashed var(--accent);
  }
  .drop {
    display: grid;
    gap: 4px;
    place-items: center;
    padding: var(--space-6) var(--space-3);
    border: 2px dashed var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    cursor: pointer;
    text-align: center;
  }
  .drop:hover {
    border-color: var(--accent);
  }
  .row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  .small {
    font-size: 12px;
    margin: 0;
  }
  .stage-wrap {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: var(--space-4);
    gap: var(--space-2);
  }
  .stage {
    flex: 1;
    min-height: 0;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
    background: repeating-conic-gradient(var(--surface-2) 0 25%, var(--surface) 0 50%) 0 0 / 20px 20px;
    cursor: grab;
    touch-action: none;
    user-select: none;
  }
  .stage.grabbing {
    cursor: grabbing;
  }
  .stage img {
    width: min(100%, 100cqh);
    max-height: 100%;
    aspect-ratio: 1;
    object-fit: contain;
    pointer-events: none;
  }
  .stage {
    container-type: size;
  }
  .stage-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
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
  .borders {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .border {
    width: 56px;
    height: 56px;
    padding: 4px;
    border: 2px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    cursor: pointer;
  }
  .border img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .border.sel {
    border-color: var(--accent);
  }
  .border.add {
    font-size: 22px;
    color: var(--text-muted);
  }
  .swatch {
    width: 40px;
    height: 32px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
  }
  .btn.active {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  input[type='range'] {
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
  .choice p {
    margin: 0;
    font-size: 13px;
  }
</style>
