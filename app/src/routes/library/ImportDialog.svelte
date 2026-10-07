<script lang="ts">
  // Import images into the library: one row per file with name, kind and size.
  import { api } from '../../lib/api';
  import Modal from '../../lib/components/Modal.svelte';
  import { app } from '../../lib/state.svelte';
  import { ASSET_KINDS, KIND_LABELS, SIZE_LABELS, type AssetKind, type ImportReport } from '../../lib/types';

  interface Row {
    path: string;
    name: string;
    kind: AssetKind;
    size: string;
  }

  let { open = $bindable(false), paths = [], ondone }: { open: boolean; paths: string[]; ondone: (r: ImportReport) => void } = $props();

  let rows = $state<Row[]>([]);
  let busy = $state(false);
  let report = $state<ImportReport | null>(null);
  let allKind = $state<AssetKind>('token');

  const stem = (p: string) => p.split(/[\\/]/).pop()!.replace(/\.[^.]+$/, '');

  $effect(() => {
    if (open) {
      rows = paths.map((path) => ({ path, name: stem(path), kind: 'token' as AssetKind, size: 'Medium' }));
      report = null;
      allKind = 'token';
    }
  });

  function setAllKinds(k: AssetKind) {
    allKind = k;
    for (const r of rows) r.kind = k;
  }

  async function submit() {
    busy = true;
    try {
      report = await api.libraryImport(
        rows.map((r) => ({ path: r.path, name: r.name.trim() || null, kind: r.kind, size: r.kind === 'map' ? null : r.size })),
      );
      ondone(report);
      if (report.failed.length === 0) {
        app.toast('success', `Imported ${report.imported.length} asset${report.imported.length === 1 ? '' : 's'}`);
        open = false;
      }
    } catch (e) {
      app.error('Import failed', e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Import images" bind:open width={680}>
  {#if report && report.failed.length}
    <p>Imported {report.imported.length}. These files could not be imported:</p>
    <ul class="failed">
      {#each report.failed as f (f.path)}<li><strong>{stem(f.path)}</strong> — <span class="muted">{f.error}</span></li>{/each}
    </ul>
  {:else}
    <div class="bulk">
      <span class="muted">Set all to</span>
      <div class="segmented" role="group" aria-label="Kind for all files">
        {#each ASSET_KINDS as k (k)}
          <button aria-pressed={allKind === k} onclick={() => setAllKinds(k)}>{KIND_LABELS[k]}</button>
        {/each}
      </div>
    </div>
    <table>
      <thead><tr><th>Name</th><th>Kind</th><th>Size</th><th></th></tr></thead>
      <tbody>
        {#each rows as r, i (r.path)}
          <tr>
            <td><input class="input" aria-label="Name for {stem(r.path)}" bind:value={r.name} /></td>
            <td>
              <select class="input" aria-label="Kind for {stem(r.path)}" bind:value={r.kind}>
                {#each ASSET_KINDS as k (k)}<option value={k}>{KIND_LABELS[k]}</option>{/each}
              </select>
            </td>
            <td>
              <select class="input" aria-label="Size for {stem(r.path)}" bind:value={r.size} disabled={r.kind === 'map'}>
                {#each SIZE_LABELS as s (s)}<option value={s}>{s}</option>{/each}
              </select>
            </td>
            <td><button class="btn ghost icon" aria-label="Skip {stem(r.path)}" onclick={() => rows.splice(i, 1)}>×</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  {#snippet footer()}
    {#if report && report.failed.length}
      <button class="btn primary" onclick={() => (open = false)}>Close</button>
    {:else}
      <button class="btn" onclick={() => (open = false)}>Cancel</button>
      <button class="btn primary" disabled={busy || rows.length === 0} onclick={submit}>
        {busy ? 'Importing…' : `Import ${rows.length}`}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  .bulk {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin-bottom: var(--space-3);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    text-align: left;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-muted);
    padding-bottom: var(--space-1);
  }
  td {
    padding: 3px 4px 3px 0;
  }
  td .input {
    width: 100%;
  }
  .failed {
    padding-left: var(--space-4);
  }
</style>
