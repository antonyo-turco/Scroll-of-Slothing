<script lang="ts">
  // "Save to library" for the editors (token cutter, object cutter, map
  // paginator): asks for the name, the print size (tokens and cut-outs) and
  // the tags, and, when the item was opened from the library, whether to
  // update that asset or save a new one. Tags start from the existing asset's.
  import { api } from '../api';
  import { SIZE_LABELS, type SaveDetails } from '../types';
  import Modal from './Modal.svelte';
  import TagInput from './TagInput.svelte';

  let {
    open = $bindable(false),
    name,
    size = null,
    kind,
    existing,
    onsave,
  }: {
    open: boolean;
    /** suggested name */
    name: string;
    /** suggested print size; null hides the field (maps) */
    size?: string | null;
    /** "token", "cut-out", "map": used in the texts */
    kind: string;
    /** the library asset id this item was opened from, if any */
    existing: number | null;
    onsave: (d: SaveDetails) => void;
  } = $props();

  let fName = $state('');
  let fSize = $state('Medium');
  let fTags = $state<string[]>([]);
  let suggestions = $state<string[]>([]);

  // Fill the form each time the dialog opens.
  let wasOpen = false;
  $effect(() => {
    if (open === wasOpen) return;
    wasOpen = open;
    if (!open) return;
    fName = name;
    fSize = size ?? 'Medium';
    fTags = [];
    api.tagsList().then((t) => (suggestions = t.map((x) => x.name)), () => {});
    if (existing !== null) {
      const id = existing;
      api.libraryGet(id).then((a) => open && existing === id && !fTags.length && (fTags = [...a.tags]), () => {});
    }
  });

  const valid = $derived(fName.trim().length > 0);

  function save(update: boolean) {
    if (!valid) return;
    open = false;
    onsave({ name: fName.trim(), size: fSize, tags: fTags, update });
  }
</script>

<Modal title="Save {kind} to library" bind:open width={440}>
  <form class="form" onsubmit={(e) => (e.preventDefault(), save(existing !== null))}>
    <div class="field">
      <label for="save-asset-name">Name</label>
      <input id="save-asset-name" class="input" bind:value={fName} placeholder={kind === 'token' ? 'e.g. Goblin Boss' : kind === 'cut-out' ? 'e.g. Brazier' : 'e.g. Crypt of the Lich'} />
    </div>
    {#if size !== null}
      <div class="field">
        <label for="save-asset-size">Size{kind === 'cut-out' ? ' (longest side)' : ''}</label>
        <select id="save-asset-size" class="input" bind:value={fSize}>
          {#each SIZE_LABELS as s (s)}<option value={s}>{s}</option>{/each}
        </select>
      </div>
    {/if}
    <div class="field">
      <span class="label">Tags</span>
      <TagInput values={fTags} {suggestions} onchange={(v) => (fTags = v)} />
    </div>
    {#if existing !== null}
      <p class="note">This {kind} was opened from the library (asset #{existing}).</p>
    {/if}
  </form>
  {#snippet footer()}
    <button class="btn ghost" onclick={() => (open = false)}>Cancel</button>
    {#if existing !== null}
      <button class="btn" disabled={!valid} onclick={() => save(false)}>Save as new</button>
      <button class="btn primary" disabled={!valid} onclick={() => save(true)}>Update existing</button>
    {:else}
      <button class="btn primary" disabled={!valid} onclick={() => save(false)}>Save</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .form {
    display: grid;
    gap: var(--space-3);
  }
</style>
