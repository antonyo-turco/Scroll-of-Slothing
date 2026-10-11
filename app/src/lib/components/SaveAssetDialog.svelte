<script lang="ts">
  // "Save to library" for the editors (token cutter, object cutter, map
  // paginator): asks for the name and, when the item was opened from the
  // library, whether to update that asset or save a new one.
  import Modal from './Modal.svelte';

  let {
    open = $bindable(false),
    name = $bindable(''),
    kind,
    existing,
    onsave,
  }: {
    open: boolean;
    name: string;
    /** "token", "cut-out", "map": used in the texts */
    kind: string;
    /** the library asset id this item was opened from, if any */
    existing: number | null;
    onsave: (update: boolean) => void;
  } = $props();

  function save(update: boolean) {
    open = false;
    onsave(update);
  }
</script>

<Modal title="Save {kind} to library" bind:open width={420}>
  <div class="field">
    <label for="save-asset-name">Name</label>
    <input id="save-asset-name" class="input" bind:value={name} onkeydown={(e) => e.key === 'Enter' && save(existing !== null)} />
  </div>
  {#if existing !== null}
    <p class="note">This {kind} was opened from the library (asset #{existing}).</p>
  {/if}
  {#snippet footer()}
    <button class="btn ghost" onclick={() => (open = false)}>Cancel</button>
    {#if existing !== null}
      <button class="btn" onclick={() => save(false)}>Save as new</button>
      <button class="btn primary" onclick={() => save(true)}>Update existing</button>
    {:else}
      <button class="btn primary" onclick={() => save(false)}>Save</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .note {
    margin-top: var(--space-3);
  }
</style>
