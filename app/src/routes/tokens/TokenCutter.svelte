<script lang="ts">
  // Token Cutter: frame an image inside a border ring.
  // Spec: docs/features/token-cutter.md. Rendering happens in Rust (token_render);
  // requests are gated so at most one is in flight and the latest state wins.
  import { onDestroy, onMount } from 'svelte';
  import { api, blobUrl, IMAGE_EXTENSIONS, isTauri, onFileDrop, pickFile, pickSavePath } from '../../lib/api';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import { app } from '../../lib/state.svelte';
  import { PinchTracker, wheelIntent } from '../../lib/gestures';
  import SaveAssetDialog from '../../lib/components/SaveAssetDialog.svelte';
  import ViewActions from '../../lib/components/ViewActions.svelte';
  import FiltersPanel from '../../lib/components/FiltersPanel.svelte';
  import Slider from '../../lib/components/Slider.svelte';
  import { type Border, type FilterCatalog, type SourceImage, type TokenParams, type SaveDetails } from '../../lib/types';

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
    params: { border, bg: null, ring_inset_pct: 0, pan: [0, 0], zoom: 1, filters: [] },
    name: '',
    size: 'Medium',
    asset_id: null,
  });

  let borders = $state<Border[]>([]);
  let catalog = $state<FilterCatalog | null>(null);
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
      catalog = await api.tokenFilterCatalog().catch(() => null);
      const saved = await api.workspaceGet<Draft>(DRAFT_KEY).catch(() => null);
      d = saved && borders.some((b) => b.id === saved.params.border) ? saved : blank(borders[0]?.id ?? '');
      d.params.filters ??= []; // drafts saved before token filters existed
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
    d = { ...draft, params: { ...draft.params, filters: draft.params.filters ?? [] } };
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
  const pinch = new PinchTracker();
  const setZoom = (f: number) => (d.params.zoom = Math.max(0.1, Math.min(10, d.params.zoom * f)));
  /** Moves the image by a screen delta (Python: pan -= delta / scale). */
  function moveImage(sx: number, sy: number) {
    if (!border || !stage) return;
    const scale = Math.min(stage.clientWidth, stage.clientHeight) / Math.max(border.width, border.height);
    d.params.pan = [d.params.pan[0] - sx / scale, d.params.pan[1] - sy / scale];
  }
  function onPointerDown(e: PointerEvent) {
    if (!d.source) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    if (pinch.down(e)) return void (drag = null);
    drag = { x: e.clientX, y: e.clientY };
  }
  function onPointerMove(e: PointerEvent) {
    const st = pinch.move(e);
    if (st) {
      setZoom(st.factor);
      moveImage(st.dx, st.dy);
      return;
    }
    if (pinch.active || !drag) return;
    // Dragging moves the image with the pointer.
    moveImage(e.clientX - drag.x, e.clientY - drag.y);
    drag = { x: e.clientX, y: e.clientY };
  }
  function onPointerUp(e: PointerEvent) {
    pinch.up(e);
    drag = null;
  }
  // Touchpad pinch / mouse wheel zoom the framing; two-finger scroll moves it.
  function wheel(node: HTMLElement) {
    const h = (e: WheelEvent) => {
      if (!d.source) return;
      e.preventDefault();
      const g = wheelIntent(e);
      if (g.kind === 'zoom') setZoom(g.factor);
      else moveImage(g.dx, g.dy);
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

  /** Saves with the name, size and tags chosen in the save dialog. */
  async function saveLibrary({ name, size, tags, update }: SaveDetails) {
    if (!d.source) return;
    d.name = name;
    d.size = size;
    busy = true;
    try {
      let a = await api.tokenSaveToLibrary(d.source.hash, params(), name, size, update ? d.asset_id : null);
      a = await api.librarySetTags(a.id, tags);
      d.asset_id = a.id;
      app.toast('success', update ? `“${a.name}” updated in the library` : `“${a.name}” saved to the library`);
    } catch (e) {
      app.error('Save failed', e);
    } finally {
      busy = false;
    }
  }

  function onSaveClick() {
    if (d.source) saveChoice = true;
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

<ViewActions>
  <button class="btn" onclick={browse} title="Load an image from disk">Open image…</button>
  <button class="btn" onclick={paste} title="Load the image on the clipboard (Ctrl+V)">Paste</button>
  <button class="btn" disabled={!d.source} onclick={savePng} title="Write the token as a PNG file">Save PNG…</button>
  <button class="btn" disabled={!d.source || busy} onclick={addToSheet} title="Put the token on the token sheet">Add to sheet</button>
  <button class="btn primary" disabled={!d.source || busy} onclick={onSaveClick} title="Store the token in the Library (Ctrl+S)">Save to library…</button>
</ViewActions>

<div class="view">
  <section class="work" aria-label="Token preview">
    <div
      class="stage-area stage"
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
        <div class="canvas-controls" role="group" aria-label="Zoom" onpointerdown={(e) => e.stopPropagation()}>
          <button class="btn icon" aria-label="Zoom out" onclick={() => setZoom(1 / 1.1)}>−</button>
          <span class="zoom-readout">{Math.round(d.params.zoom * 100)}%</span>
          <button class="btn icon" aria-label="Zoom in" onclick={() => setZoom(1.1)}>+</button>
          <button class="btn" onclick={() => (d.params = { ...d.params, pan: [0, 0], zoom: 1 })}>Reset view</button>
        </div>
      {:else if !d.source}
        <button class="drop-zone" onclick={browse} onpointerdown={(e) => e.stopPropagation()}>
          <strong>Drop an image here</strong>
          <span class="muted">or click to browse · Ctrl+V to paste · or “Open in Token Cutter” from the Library</span>
        </button>
      {/if}
    </div>
    <div class="statusbar">
      {#if d.source}
        <span>{d.source.width} × {d.source.height} px{rendering ? ' · rendering…' : ''}</span>
        <span class="spacer"></span>
        <span>Drag moves the image · pinch or wheel zooms</span>
      {:else}
        <span>Load an image to start.</span>
      {/if}
    </div>
  </section>

  <aside class="inspector" aria-label="Token settings">
    <section class="section" aria-label="Source image">
      <h3 class="section-title">Image</h3>
      {#if d.source}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
        <img
          bind:this={sourceImg}
          class="source"
          class:pipette
          src={blobUrl(d.source.hash)}
          crossorigin="anonymous"
          alt="Source"
          onclick={pickColour}
        />
        {#if pipette}<p class="note">Click the image to pick the background colour · Esc cancels.</p>{/if}
      {:else}
        <p class="note">No image yet. Drop one on the canvas, or use Open image… or Paste.</p>
      {/if}
    </section>

    <section class="section">
      <h3 class="section-title">Border <InfoTip label="About borders" text="The ring drawn around the token. The thin borders show more of the image. Ring thickness moves the cut inwards." /></h3>
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
      <Slider label="Ring thickness %" min={0} max={50} bind:value={d.params.ring_inset_pct} />
    </section>

    <section class="section">
      <h3 class="section-title">Filters <InfoTip label="About filters" text="Colour and tone filters change the image only, never the border. They run in order, top to bottom." /></h3>
      <FiltersPanel bind:pipeline={d.params.filters} {catalog} />
    </section>

    <section class="section">
      <h3 class="section-title">Background <InfoTip label="About the background" text="Fills transparent areas behind the image. Use the pipette to pick a colour from the image itself." /></h3>
      <div class="row">
        <div class="segmented" role="group" aria-label="Background">
          <button aria-pressed={d.params.bg === null} onclick={() => (d.params.bg = null)}>None</button>
          <button aria-pressed={d.params.bg !== null} onclick={() => (d.params.bg ??= [255, 255, 255])}>Colour</button>
        </div>
        {#if d.params.bg}
          <input type="color" class="swatch" aria-label="Background colour" value={bgHex} oninput={(e) => setBgHex(e.currentTarget.value)} />
        {/if}
        <button class="btn" aria-pressed={pipette} disabled={!d.source} title="Pick a colour from the source image" onclick={() => (pipette = !pipette)}>
          Pick from image
        </button>
      </div>
    </section>

    {#if d.asset_id !== null}<p class="note asset">Editing library asset #{d.asset_id}</p>{/if}
  </aside>
</div>

<SaveAssetDialog bind:open={saveChoice} name={d.name} kind="token" size={d.size} existing={d.asset_id} onsave={(s) => void saveLibrary(s)} />

<style>
  .stage {
    display: grid;
    place-items: center;
    padding: var(--space-4);
    background: repeating-conic-gradient(var(--surface-2) 0 25%, var(--surface) 0 50%) 0 0 / 20px 20px;
    cursor: grab;
    touch-action: none;
    user-select: none;
    container-type: size;
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
  .source {
    width: 100%;
    max-height: 180px;
    object-fit: contain;
    border-radius: var(--radius-sm);
    background: repeating-conic-gradient(var(--surface-2) 0 25%, var(--surface) 0 50%) 0 0 / 16px 16px;
  }
  .source.pipette {
    cursor: crosshair;
    outline: 2px dashed var(--accent);
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
  .asset {
    margin-top: auto;
    padding: var(--space-2) var(--space-4);
    border-top: 1px solid var(--border);
  }
</style>
