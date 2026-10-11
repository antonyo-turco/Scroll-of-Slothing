<script lang="ts">
  // Puts a view's file-level actions (open, save, export; primary last) or its
  // own tabs into the top bar, so every view keeps them in the same place.
  // Usage: <ViewActions>…buttons…</ViewActions> or <ViewActions slot="nav">.
  import type { Snippet } from 'svelte';

  let {
    children,
    slot = 'actions',
    end = false,
  }: {
    children: Snippet;
    slot?: 'actions' | 'nav';
    /** keep after any other view's actions (e.g. Music's Stop button after a sub-view's) */
    end?: boolean;
  } = $props();

  function teleport(node: HTMLElement, id: string) {
    document.getElementById(id)?.appendChild(node);
    return { destroy: () => node.remove() };
  }
</script>

<div class="view-{slot}" class:end use:teleport={`topbar-${slot}`}>{@render children()}</div>

<style>
  div {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .end {
    order: 1;
  }
</style>
