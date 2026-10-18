<script lang="ts">
  // Rename (auto-merge), merge and delete tags. Spec: docs/features/tags.md.
  import { api } from '../../lib/api';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import Modal from '../../lib/components/Modal.svelte';
  import { app } from '../../lib/state.svelte';
  import type { TagCount } from '../../lib/types';

  let { open = $bindable(false), onchange }: { open: boolean; onchange: () => void } = $props();

  let tags = $state<TagCount[]>([]);
  let names = $state<Record<number, string>>({});
  let mergeSrc = $state<number | null>(null);
  let mergeDst = $state<number | null>(null);
  let confirmDelete = $state<number | null>(null);

  async function load() {
    tags = await api.tagsList();
    names = Object.fromEntries(tags.map((t) => [t.id, t.name]));
  }

  $effect(() => {
    if (open) void load();
  });

  async function act(what: string, f: () => Promise<unknown>) {
    try {
      await f();
      await load();
      onchange();
    } catch (e) {
      app.error(what, e);
    }
  }

  async function rename(t: TagCount) {
    const name = (names[t.id] ?? '').trim();
    if (!name || name === t.name) return;
    await act('Rename failed', async () => {
      const r = await api.tagsRename(t.id, name);
      if (r.merged_into) app.toast('info', `Merged “${t.name}” into “${name}”`);
    });
  }
</script>

<Modal title="Manage tags" bind:open width={560}>
  {#if tags.length === 0}
    <p class="muted">No tags yet. Add tags to assets from the details panel.</p>
  {:else}
    <div class="merge">
      <span>Merge <InfoTip label="About tags" text="Merge moves every asset from one tag to another. Renaming a tag to an existing name merges the two." /></span>
      <select class="input" aria-label="Tag to merge" bind:value={mergeSrc}>
        <option value={null}>tag…</option>
        {#each tags as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
      </select>
      <span>into</span>
      <select class="input" aria-label="Merge target" bind:value={mergeDst}>
        <option value={null}>tag…</option>
        {#each tags.filter((t) => t.id !== mergeSrc) as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
      </select>
      <button
        class="btn"
        disabled={mergeSrc === null || mergeDst === null}
        onclick={() => act('Merge failed', () => api.tagsMerge(mergeSrc!, mergeDst!)).then(() => (mergeSrc = mergeDst = null))}>Merge</button
      >
    </div>
    <ul>
      {#each tags as t (t.id)}
        <li>
          <input
            class="input"
            aria-label="Rename {t.name}"
            bind:value={names[t.id]}
            onkeydown={(e) => e.key === 'Enter' && rename(t)}
            onblur={() => rename(t)}
          />
          <span class="muted count">used by {t.count}</span>
          {#if confirmDelete === t.id}
            <button class="btn danger" onclick={() => act('Delete failed', () => api.tagsDelete(t.id))}>Delete</button>
            <button class="btn" onclick={() => (confirmDelete = null)}>Keep</button>
          {:else}
            <button class="btn ghost danger" aria-label="Delete {t.name}" onclick={() => (confirmDelete = t.id)}>Delete</button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</Modal>

<style>
  .merge {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin-bottom: var(--space-4);
    flex-wrap: wrap;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--space-2);
  }
  li {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  li .input {
    flex: 1;
  }
  .count {
    width: 80px;
    font-size: 12px;
  }
  .hint {
    font-size: 12px;
  }
</style>
