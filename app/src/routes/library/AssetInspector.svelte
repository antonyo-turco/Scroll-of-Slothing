<script lang="ts">
  // Right-hand inspector for one asset: metadata, tags, links, export, delete.
  import { api, blobUrl, isTauri, pickSavePath } from '../../lib/api';
  import Icon from '../../lib/components/Icon.svelte';
  import { EDITOR_ROUTE, type LibraryState } from '../../lib/library.svelte';
  import { findView } from '../../lib/nav';
  import { app } from '../../lib/state.svelte';
  import { KIND_LABELS, SIZE_LABELS, type Asset, type AssetKind, type ExportWhat } from '../../lib/types';

  let { lib, asset }: { lib: LibraryState; asset: Asset } = $props();

  let name = $state('');
  let aliases = $state('');
  let tagInput = $state('');
  let confirmDelete = $state(false);

  // Reset the form only when another asset is shown, not when this one is
  // updated: a save's reply must not wipe a tag being typed meanwhile.
  let shownId: number | null = null;
  $effect.pre(() => {
    if (asset.id === shownId) return;
    shownId = asset.id;
    name = asset.name;
    aliases = asset.aliases;
    confirmDelete = false;
    tagInput = '';
  });

  const editorView = $derived(findView(EDITOR_ROUTE[asset.kind] ?? '')?.view);

  async function run<T>(what: string, f: () => Promise<T>): Promise<T | undefined> {
    try {
      return await f();
    } catch (e) {
      app.error(what, e);
    }
  }

  async function saveMeta() {
    if (name.trim() === asset.name && aliases === asset.aliases) return;
    const a = await run('Could not save', () => api.libraryUpdate(asset.id, { name, aliases }));
    if (a) lib.patchItem(a);
  }

  async function setTags(values: string[]) {
    const a = await run('Could not save tags', () => api.librarySetTags(asset.id, values));
    if (a) {
      lib.patchItem(a);
      void lib.loadReference();
    }
  }

  function addTag() {
    const t = tagInput.trim().replace(/,$/, '');
    tagInput = '';
    if (t && !asset.tags.some((x) => x.toLowerCase() === t.toLowerCase())) void setTags([...asset.tags, t]);
  }

  async function exportFile(what: ExportWhat) {
    const ext = what === 'pdf' ? 'pdf' : 'png';
    const path = isTauri()
      ? await pickSavePath(`Export ${what}`, `${asset.name}_${what}.${ext}`, [ext])
      : `${asset.name}_${what}.${ext}`;
    if (!path) return;
    const out = await run('Export failed', () => api.libraryExport(asset.id, what, path));
    if (out) app.toast('success', `Saved ${out}`);
  }

  async function copy() {
    const ok = await run('Copy failed', () => api.libraryCopyToClipboard(asset.id));
    if (ok !== undefined) app.toast('success', 'Image copied to clipboard');
  }

  async function remove() {
    const n = await run('Delete failed', () => api.libraryDelete([asset.id]));
    if (n) {
      app.toast('success', `Deleted “${asset.name}”`);
      lib.activeId = null;
      void lib.refresh();
    }
  }

  function openEditor() {
    const route = EDITOR_ROUTE[asset.kind];
    if (!route) return;
    app.openRequest = { route, assetId: asset.id };
    app.navigate(route);
  }

  async function addToSheet() {
    const token = asset.kind === 'token';
    try {
      await (token ? api.tokenSheetAddAssets([asset.id]) : api.objectSheetAddAssets([asset.id]));
      app.toast('success', `Added “${asset.name}” to the ${token ? 'token' : 'object'} sheet`);
    } catch (e) {
      app.error('Could not add to sheet', e);
    }
  }
</script>

<aside class="inspector" aria-label="Asset details">
  <section class="section">
    <header>
      <h3 class="section-title">{KIND_LABELS[asset.kind as AssetKind] ?? asset.kind}</h3>
      <button class="btn ghost icon" aria-label="Close details" onclick={() => (lib.activeId = null)}><Icon name="x" /></button>
    </header>
    <div class="preview">
      <img src={blobUrl(asset.preview_blob ?? asset.rendered_blob)} alt={asset.name} />
    </div>
    <button class="btn primary block" onclick={openEditor} title={editorView ? `Available in phase ${editorView.phase}` : ''}>
      Open in {asset.kind === 'map' ? 'Map Paginator' : asset.kind === 'cutout' ? 'Object Cutter' : 'Token Cutter'}
    </button>
    {#if asset.kind === 'token' || asset.kind === 'cutout'}
      <button class="btn block" onclick={addToSheet}>Add to {asset.kind === 'token' ? 'token' : 'object'} sheet</button>
    {/if}
  </section>

  <section class="section">
    <h3 class="section-title">Details</h3>
    <div class="field">
      <label for="asset-name">Name</label>
      <input id="asset-name" class="input" bind:value={name} onblur={saveMeta} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} />
    </div>

    {#if asset.kind !== 'map'}
      <div class="field">
        <label for="asset-size">Print size</label>
        <select
          id="asset-size"
          class="input"
          value={asset.size_label}
          onchange={async (e) => {
            const a = await run('Could not save', () => api.libraryUpdate(asset.id, { size_label: e.currentTarget.value }));
            if (a) lib.patchItem(a);
          }}
        >
          {#each SIZE_LABELS as s (s)}<option value={s}>{s}</option>{/each}
        </select>
      </div>
    {/if}

    <div class="field">
      <label for="asset-aliases">Aliases <span class="muted">(one per line)</span></label>
      <textarea id="asset-aliases" class="input area" rows="2" bind:value={aliases} onblur={saveMeta}></textarea>
    </div>

    <div class="field">
      <span class="label">Tags</span>
      <div class="chips">
        {#each asset.tags as t (t)}
          <span class="chip">{t}<button aria-label="Remove tag {t}" onclick={() => setTags(asset.tags.filter((x) => x !== t))}>×</button></span>
        {/each}
        <input
          class="chip-input"
          placeholder="Add tag…"
          aria-label="Add tag"
          list="tag-suggestions"
          bind:value={tagInput}
          onkeydown={(e) => (e.key === 'Enter' || e.key === ',') && (e.preventDefault(), addTag())}
          onblur={addTag}
        />
        <datalist id="tag-suggestions">
          {#each lib.allTags as t (t.id)}<option value={t.name}></option>{/each}
        </datalist>
      </div>
    </div>
  </section>

  <section class="section">
    <h3 class="section-title">Export</h3>
    <div class="pair">
      <button class="btn" onclick={() => exportFile('rendered')}>Save image…</button>
      <button class="btn" onclick={() => exportFile('original')}>Save original…</button>
      {#if asset.pdf_blob}<button class="btn" onclick={() => exportFile('pdf')}>Save PDF…</button>{/if}
      <button class="btn" onclick={copy}>Copy image</button>
    </div>
  </section>

  <div class="inspector-footer">
    {#if confirmDelete}
      <span class="note">Delete permanently?</span>
      <div class="pair">
        <button class="btn" onclick={() => (confirmDelete = false)}>Cancel</button>
        <button class="btn danger" onclick={remove}>Delete</button>
      </div>
    {:else}
      <button class="btn ghost danger" onclick={() => (confirmDelete = true)}>Delete asset…</button>
    {/if}
    <p class="note">Added {asset.created.slice(0, 10)}{asset.legacy_id ? ' · imported from Orb' : ''}</p>
  </div>
</aside>

<style>
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin: -4px 0;
  }
  .preview {
    border-radius: var(--radius-sm);
    background: repeating-conic-gradient(var(--surface-2) 0 25%, var(--surface) 0 50%) 0 0 / 16px 16px;
    display: grid;
    place-items: center;
    aspect-ratio: 4 / 3;
    overflow: hidden;
  }
  .preview img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .field {
    font-size: 12px;
  }
  .area {
    min-height: 52px;
    padding: var(--space-2);
    resize: vertical;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px 4px 2px 8px;
    border-radius: 999px;
    background: var(--accent-soft);
    font-size: 12px;
  }
  .chip button {
    border: 0;
    background: none;
    cursor: pointer;
    padding: 0 4px;
    color: var(--text-muted);
  }
  .chip-input {
    flex: 1;
    min-width: 80px;
    border: 0;
    background: transparent;
    padding: 2px 4px;
  }
  .chip-input:focus {
    outline: none;
  }
  .pair .btn {
    justify-content: center;
  }
</style>
