<script lang="ts">
  // AoE templates: true-scale area-of-effect shapes, single or packed on a sheet.
  // Spec: docs/features/aoe-templates.md (Python ui/aoe_template_panel.py).
  import { onDestroy, onMount } from 'svelte';
  import { api, isTauri, pickSavePath } from '../../lib/api';
  import Modal from '../../lib/components/Modal.svelte';
  import { app } from '../../lib/state.svelte';
  import ViewActions from '../../lib/components/ViewActions.svelte';
  import type { AoeConfig, AoeInfo, AoePreset, AoeSheet, AoeSheetLayout, PageFormat } from '../../lib/types';

  const EDITOR_KEY = 'aoe.editor';
  type Shape = AoeConfig['shape'];
  const SHAPES: [Shape, string][] = [
    ['circle', 'Circle / Sphere (radius)'],
    ['cone', 'Cone'],
    ['line', 'Line'],
    ['square', 'Square / Cube'],
  ];
  const PRIMARY: Record<Shape, string> = { circle: 'Radius (ft)', cone: 'Length (ft)', line: 'Length (ft)', square: 'Side (ft)' };

  // Python f"{v:g}" for the values a DM types.
  function fmtG(v: number): string {
    if (!Number.isFinite(v) || v === 0) return '0';
    const s = Number(v.toPrecision(6)).toString();
    return s;
  }
  function defaultLabel(shape: Shape, a: number, b: number): string {
    const A = fmtG(a);
    if (shape === 'circle') return `${A} ft radius`;
    if (shape === 'cone') return `${A} ft cone`;
    if (shape === 'line') return `${A} x ${fmtG(b)} ft line`;
    return `${A} ft cube`;
  }

  function defaults(): AoeConfig {
    return {
      shape: 'circle', a_ft: 20, b_ft: 5, feet_per_square: 5,
      fill_rgb: [220, 70, 40], fill_alpha: 80,
      line_rgb: [150, 25, 0], line_width: 4,
      grid_on: true, grid_rgb: [60, 60, 60], grid_alpha: 110,
      label_on: true, label: '20 ft radius',
      inch: 1.0, page_format: (app.settings.map_page_format ?? 'A4') as PageFormat, name: '',
    };
  }

  let cfg = $state<AoeConfig>(defaults());
  let labelEdited = $state(false);
  let presets = $state<AoePreset[]>([]);
  let presetSel = $state('');
  let mode = $state<'current' | 'sheet'>('current');
  let info = $state<AoeInfo | null>(null);
  let previewUrl = $state<string | null>(null);
  let rendering = $state(false);
  let sheet = $state<AoeSheet>({ items: [], gap_in: 0.2 });
  let sheetLayout = $state<AoeSheetLayout | null>(null);
  let sheetUrls = $state<string[]>([]);
  let selected = $state<string[]>([]);
  let loaded = false;

  let saveOpen = $state(false);
  let saveName = $state('');
  let confirmOverwrite = $state(false);
  let confirmDelete = $state(false);
  let confirmClear = $state(false);

  const hex = (c: number[]) => '#' + c.map((v) => v.toString(16).padStart(2, '0')).join('');
  const rgb = (h: string): [number, number, number] => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];

  // auto-filled caption until the user types their own
  $effect(() => {
    const l = defaultLabel(cfg.shape, Math.max(0, cfg.a_ft || 0), Math.max(0, cfg.b_ft || 0));
    if (!labelEdited && cfg.label !== l) cfg.label = l;
  });

  // ── render current template (debounced 120 ms, in-flight gating) ──
  let inFlight = false;
  let dirty = false;
  let timer: ReturnType<typeof setTimeout> | null = null;
  function schedule() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void render(), 120);
  }
  async function render() {
    if (inFlight) {
      dirty = true;
      return;
    }
    inFlight = true;
    rendering = true;
    try {
      do {
        dirty = false;
        const snap = $state.snapshot(cfg) as AoeConfig;
        info = await api.aoeInfo(snap);
        if (snap.a_ft > 0) {
          const blob = await api.aoeRender(snap);
          const url = URL.createObjectURL(blob);
          if (previewUrl) URL.revokeObjectURL(previewUrl);
          previewUrl = url;
        } else if (previewUrl) {
          URL.revokeObjectURL(previewUrl);
          previewUrl = null;
        }
      } while (dirty);
    } catch (e) {
      app.error('Could not render the template', e);
    } finally {
      inFlight = false;
      rendering = false;
    }
    if (mode === 'sheet') void refreshSheet();
  }

  $effect(() => {
    void $state.snapshot(cfg);
    schedule();
  });

  // autosave the editor
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const snap = { cfg: $state.snapshot(cfg), labelEdited };
    if (!loaded) return;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void api.workspaceSet(EDITOR_KEY, snap).catch(() => {}), 400);
  });

  // ── sheet ──
  async function refreshSheet() {
    try {
      sheet = await api.aoeSheetGet();
      sheetLayout = await api.aoeSheetLayout(cfg.inch, cfg.page_format);
      const urls: string[] = [];
      if (mode === 'sheet') {
        for (let p = 0; p < sheetLayout.pages; p++) {
          urls.push(URL.createObjectURL(await api.aoeSheetPreview(cfg.inch, cfg.page_format, p, 900)));
        }
      }
      sheetUrls.forEach((u) => URL.revokeObjectURL(u));
      sheetUrls = urls;
      selected = selected.filter((id) => sheet.items.some((i) => i.id === id));
    } catch (e) {
      app.error('Could not update the sheet', e);
    }
  }

  async function sheetOp(p: Promise<AoeSheet>, toSheet = false) {
    try {
      sheet = await p;
      if (toSheet) mode = 'sheet';
      await refreshSheet();
    } catch (e) {
      app.error('Sheet update failed', e);
    }
  }

  function addToSheet() {
    if (!(cfg.a_ft > 0)) return app.toast('info', 'Set a template size first.');
    void sheetOp(api.aoeSheetAdd($state.snapshot(cfg) as AoeConfig), true);
  }

  // ── presets ──
  async function loadPresets() {
    try {
      presets = await api.aoePresetsList();
    } catch (e) {
      app.error('Could not load presets', e);
    }
  }

  function applyPreset(key: string) {
    presetSel = key;
    const p = presets.find((x) => (x.builtin ? x.name : '★ ' + x.name) === key);
    if (!p) return;
    if (p.builtin) {
      cfg.shape = p.config.shape;
      cfg.a_ft = p.config.a_ft;
      cfg.b_ft = p.config.b_ft;
      labelEdited = false;
    } else {
      cfg = { ...defaults(), ...p.config };
      labelEdited = true;
    }
  }

  function openSave() {
    saveName = cfg.label || 'My template';
    confirmOverwrite = false;
    saveOpen = true;
  }

  async function savePreset(overwrite: boolean) {
    const name = saveName.trim();
    if (!name) return;
    if (!overwrite && presets.some((p) => !p.builtin && p.name === name)) {
      confirmOverwrite = true;
      return;
    }
    try {
      await api.aoePresetsSave(name, $state.snapshot(cfg) as AoeConfig, overwrite);
      saveOpen = false;
      await loadPresets();
      presetSel = '★ ' + name;
      app.toast('info', `Preset “${name}” saved`);
    } catch (e) {
      if (!overwrite && String(e).includes('exists')) confirmOverwrite = true;
      else app.error('Could not save the preset', e);
    }
  }

  async function deletePreset() {
    const name = presetSel.replace(/^★ /, '');
    try {
      await api.aoePresetsDelete(name);
      confirmDelete = false;
      presetSel = '';
      await loadPresets();
    } catch (e) {
      app.error('Could not delete the preset', e);
    }
  }

  // ── exports ──
  async function exportTemplate() {
    if (!(cfg.a_ft > 0)) return app.toast('info', 'Set a template size first.');
    const path = isTauri() ? await pickSavePath('Export template', `${cfg.label || 'aoe'}.pdf`, ['pdf']) : 'aoe.pdf';
    if (!path) return;
    try {
      await api.aoeExportTemplatePdf($state.snapshot(cfg) as AoeConfig, path);
      app.toast('info', 'Exporting… print at Actual Size / 100% for true scale.');
    } catch (e) {
      app.error('Export failed', e);
    }
  }

  async function exportSheet() {
    if (!sheet.items.length) return app.toast('info', 'Add at least one template to the sheet.');
    const path = isTauri() ? await pickSavePath('Export AoE sheet', 'aoe-sheet.pdf', ['pdf']) : 'aoe-sheet.pdf';
    if (!path) return;
    try {
      await api.aoeSheetExportPdf(cfg.inch, cfg.page_format, path);
      app.toast('info', 'Exporting… print at Actual Size / 100% for true scale.');
    } catch (e) {
      app.error('Export failed', e);
    }
  }

  async function savePng() {
    if (!(cfg.a_ft > 0)) return app.toast('info', 'Set a template size first.');
    const path = isTauri() ? await pickSavePath('Save template image', `${cfg.label || 'aoe'}.png`, ['png']) : 'aoe.png';
    if (!path) return;
    try {
      await api.aoeExportPng($state.snapshot(cfg) as AoeConfig, path);
      app.toast('info', `Template image saved to ${path}`);
    } catch (e) {
      app.error('Could not save the image', e);
    }
  }

  function setMode(m: 'current' | 'sheet') {
    mode = m;
    if (m === 'sheet') void refreshSheet();
  }

  let seen = app.dataVersion;
  $effect(() => {
    if (app.dataVersion !== seen) {
      seen = app.dataVersion;
      void loadPresets();
      void refreshSheet();
    }
  });

  onMount(async () => {
    try {
      const saved = await api.workspaceGet<{ cfg: AoeConfig; labelEdited: boolean }>(EDITOR_KEY);
      if (saved?.cfg) {
        cfg = { ...defaults(), ...saved.cfg };
        labelEdited = !!saved.labelEdited;
      }
    } catch {
      /* fresh editor */
    }
    loaded = true;
    await loadPresets();
    await refreshSheet();
  });

  onDestroy(() => {
    if (timer) clearTimeout(timer);
    if (previewUrl) URL.revokeObjectURL(previewUrl);
    sheetUrls.forEach((u) => URL.revokeObjectURL(u));
  });

  const nameOf = (id: string) => sheet.items.find((i) => i.id === id)?.config.name ?? 'template';
  const isUserPreset = $derived(presetSel.startsWith('★ '));
</script>

<ViewActions>
  <button class="btn" onclick={savePng} title="Write the current template as a PNG image">Save PNG…</button>
  <button class="btn" onclick={exportSheet} disabled={!sheet.items.length} title="Every template on the sheet, packed onto pages">Export sheet PDF</button>
  <button class="btn primary" onclick={exportTemplate} title="The current template at true scale">Export template PDF</button>
</ViewActions>

<div class="view">
  <section class="work" aria-label="AoE preview">
    <div class="view-toolbar">
      <div class="segmented" role="group" aria-label="Preview">
        <button aria-pressed={mode === 'current'} onclick={() => setMode('current')}>Current template</button>
        <button aria-pressed={mode === 'sheet'} onclick={() => setMode('sheet')}>Full sheet ({sheet.items.length})</button>
      </div>
    </div>

    <div class="stage-area stage">
      {#if mode === 'current'}
        {#if previewUrl && info && cfg.a_ft > 0}
          <svg class="tpl" viewBox="0 0 {info.image_w} {info.image_h}" role="img" aria-label="Template preview" data-testid="aoe-preview">
            <image href={previewUrl} width={info.image_w} height={info.image_h} />
            <rect width={info.image_w} height={info.image_h} fill="none" stroke="#bbb" vector-effect="non-scaling-stroke" />
            {#each info.page_rects as r, i (i)}
              <rect x={r[0]} y={r[1]} width={r[2] - r[0]} height={r[3] - r[1]} fill="none" stroke="#FF9800" stroke-width="2" vector-effect="non-scaling-stroke" />
              {#if info.page_rects.length > 1}
                <text x={(r[0] + r[2]) / 2} y={(r[1] + r[3]) / 2} fill="#FF9800" font-weight="700" font-size={Math.max(16, info.image_w / 25)} text-anchor="middle" dominant-baseline="middle">{i + 1}</text>
              {/if}
            {/each}
          </svg>
        {:else}
          <div class="empty-state"><span>Set a size greater than 0.</span></div>
        {/if}
      {:else if sheet.items.length === 0}
        <div class="empty-state">
          <strong>No templates on the sheet yet</strong>
          <span>Set up a template, then “Add to sheet” in the Sheet section.</span>
        </div>
      {:else if sheetUrls.length === 0}
        <div class="empty-state"><span>None of these templates fit on a single sheet.</span></div>
      {:else}
        <div class="strip" data-testid="aoe-sheet">
          {#each sheetUrls as u, i (u)}
            <figure>
              {#if sheetUrls.length > 1}<figcaption>Page {i + 1}</figcaption>{/if}
              <img src={u} alt="Sheet page {i + 1}" />
            </figure>
          {/each}
        </div>
      {/if}
    </div>
    <div class="statusbar" data-testid="aoe-info">
      <span>{#if mode === 'sheet'}{sheetLayout?.summary ?? ''}{:else}{info?.summary ?? ''}{/if}</span>
      {#if rendering}<span aria-live="polite">rendering…</span>{/if}
    </div>
  </section>

  <aside class="inspector" aria-label="Template settings">
    <section class="section">
      <h3 class="section-title">Preset</h3>
      <select class="input" aria-label="Preset" value={presetSel} onchange={(e) => applyPreset(e.currentTarget.value)}>
        <option value="">— choose —</option>
        {#each presets as p (p.builtin + p.name)}
          {@const key = p.builtin ? p.name : '★ ' + p.name}
          <option value={key}>{key}</option>
        {/each}
      </select>
      <div class="row">
        <button class="btn" onclick={openSave}>Save preset…</button>
        {#if confirmDelete}
          <button class="btn danger" onclick={deletePreset}>Delete “{presetSel.slice(2)}”</button>
          <button class="btn" onclick={() => (confirmDelete = false)}>Cancel</button>
        {:else}
          <button class="btn ghost" disabled={!isUserPreset} title="Only your ★ presets can be deleted" onclick={() => (confirmDelete = true)}>Delete</button>
        {/if}
      </div>
    </section>

    <section class="section">
      <h3 class="section-title">Shape &amp; size</h3>
      <label class="prop"><span>Shape</span>
        <select class="input" bind:value={cfg.shape}>
          {#each SHAPES as [k, l] (k)}<option value={k}>{l}</option>{/each}
        </select>
      </label>
      <label class="prop"><span>{PRIMARY[cfg.shape]}</span>
        <input class="input" type="number" min="1" max="2000" step="5" bind:value={cfg.a_ft} />
      </label>
      {#if cfg.shape === 'line'}
        <label class="prop"><span>Width (ft)</span>
          <input class="input" type="number" min="1" max="200" step="5" bind:value={cfg.b_ft} />
        </label>
      {/if}
    </section>

    <section class="section">
      <h3 class="section-title">Scale &amp; paper</h3>
      <div class="pair">
        <label class="field">Feet per square <input class="input" type="number" min="1" max="100" bind:value={cfg.feet_per_square} /></label>
        <label class="field">Inches per square <input class="input" type="number" min="0.05" step="0.05" bind:value={cfg.inch} /></label>
      </div>
      <div class="prop">
        <span>Paper</span>
        <div class="segmented" role="group" aria-label="Page format">
          {#each ['A4', 'Letter', 'A3'] as f (f)}
            <button aria-pressed={cfg.page_format === f} onclick={() => (cfg.page_format = f as PageFormat)}>{f}</button>
          {/each}
        </div>
      </div>
    </section>

    <section class="section">
      <h3 class="section-title">Style</h3>
      <div class="prop">
        <span>Fill</span>
        <div class="row nowrap">
          <input type="color" class="swatch" aria-label="Fill colour" value={hex(cfg.fill_rgb)} oninput={(e) => (cfg.fill_rgb = rgb(e.currentTarget.value))} />
          <input class="grow" type="range" min="0" max="255" aria-label="Fill opacity" bind:value={cfg.fill_alpha} />
        </div>
      </div>
      <div class="prop">
        <span>Outline</span>
        <div class="row nowrap">
          <input type="color" class="swatch" aria-label="Outline colour" value={hex(cfg.line_rgb)} oninput={(e) => (cfg.line_rgb = rgb(e.currentTarget.value))} />
          <input class="input" type="number" min="1" max="12" aria-label="Outline width" bind:value={cfg.line_width} />
        </div>
      </div>
      <div class="prop">
        <label class="check"><input type="checkbox" bind:checked={cfg.grid_on} /> Grid</label>
        <div class="row nowrap">
          <input type="color" class="swatch" aria-label="Grid colour" value={hex(cfg.grid_rgb)} oninput={(e) => (cfg.grid_rgb = rgb(e.currentTarget.value))} />
          <input class="grow" type="range" min="0" max="255" aria-label="Grid opacity" bind:value={cfg.grid_alpha} />
        </div>
      </div>
      <div class="prop">
        <label class="check"><input type="checkbox" bind:checked={cfg.label_on} /> Label</label>
        <input class="input" aria-label="Label text" value={cfg.label}
          oninput={(e) => { labelEdited = true; cfg.label = e.currentTarget.value; }} />
      </div>
      {#if labelEdited}
        <button class="btn ghost" onclick={() => (labelEdited = false)}>Reset label</button>
      {/if}
    </section>

    <section class="section">
      <h3 class="section-title">Sheet</h3>
      <div class="row nowrap">
        <button class="btn grow" onclick={addToSheet}>+ Add to sheet</button>
        <label class="with-unit gap" title="Gap between templates on the sheet">Gap
          <input class="input" type="number" min="0" step="0.05" aria-label="Gap (in)" value={sheet.gap_in}
            onchange={(e) => sheetOp(api.aoeSheetSetGap(Math.max(0, Number(e.currentTarget.value) || 0)))} /><span>in</span>
        </label>
      </div>
      {#if sheet.items.length}
        <select class="input list" multiple aria-label="Sheet templates" bind:value={selected} size={Math.min(8, Math.max(3, sheet.items.length))}>
          {#each sheet.items as it (it.id)}<option value={it.id}>{it.config.name || 'template'}</option>{/each}
        </select>
        <div class="row">
          <button class="btn" disabled={!selected.length} onclick={() => sheetOp(api.aoeSheetRemove(selected))}>Remove selected</button>
          {#if confirmClear}
            <button class="btn danger" onclick={() => sheetOp(api.aoeSheetClear()).then(() => (confirmClear = false))}>Clear all</button>
            <button class="btn" onclick={() => (confirmClear = false)}>Cancel</button>
          {:else}
            <button class="btn ghost danger" onclick={() => (confirmClear = true)}>Clear sheet…</button>
          {/if}
        </div>
        {#if selected.length === 1}<p class="note">{nameOf(selected[0])}</p>{/if}
      {:else}
        <p class="note">The sheet packs several templates onto pages to print together.</p>
      {/if}
      {#if sheetLayout?.skipped.length}
        <p class="note warn" role="alert">Too big for one sheet (export individually): {sheetLayout.skipped.join(', ')}</p>
      {/if}
    </section>
  </aside>
</div>

<Modal title="Save preset" bind:open={saveOpen} width={420}>
  <div class="field">
    <label for="aoe-preset-name">Preset name</label>
    <input id="aoe-preset-name" class="input" bind:value={saveName} oninput={() => (confirmOverwrite = false)}
      onkeydown={(e) => e.key === 'Enter' && savePreset(false)} />
  </div>
  {#if confirmOverwrite}<p class="note warn overwrite">A preset named “{saveName.trim()}” exists. Overwrite it?</p>{/if}
  {#snippet footer()}
    {#if confirmOverwrite}
      <button class="btn danger" onclick={() => savePreset(true)}>Overwrite</button>
    {:else}
      <button class="btn primary" onclick={() => savePreset(false)}>Save preset</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .stage {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-4);
    overflow: auto;
  }
  .tpl {
    max-width: 100%;
    max-height: 100%;
    background: #fff;
    box-shadow: var(--shadow);
  }
  .strip {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    max-height: 100%;
  }
  .strip figure {
    margin: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }
  .strip figcaption {
    font-size: 12px;
    font-weight: 600;
  }
  .strip img {
    max-height: 70vh;
    max-width: 100%;
    background: #fff;
    box-shadow: var(--shadow);
  }
  .field {
    font-size: 12px;
  }
  .nowrap {
    flex-wrap: nowrap;
  }
  .gap {
    flex: none;
    font-size: 12px;
    color: var(--text-muted);
  }
  .gap .input {
    width: 64px;
  }
  .list {
    height: auto;
    padding: 4px;
  }
  .overwrite {
    margin-top: var(--space-3);
  }
</style>
