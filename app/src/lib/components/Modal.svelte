<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    title,
    open = $bindable(false),
    width = 520,
    children,
    footer,
  }: { title: string; open: boolean; width?: number; children: Snippet; footer?: Snippet } = $props();

  let dialog: HTMLDivElement | undefined = $state();

  $effect(() => {
    if (open) queueMicrotask(() => dialog?.querySelector<HTMLElement>('input, select, textarea, button')?.focus());
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      open = false;
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={() => (open = false)}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      bind:this={dialog}
      class="modal card"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label={title}
      style:width="min({width}px, 94vw)"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKey}
    >
      <header>
        <h2>{title}</h2>
        <button class="btn ghost icon" aria-label="Close" onclick={() => (open = false)}><Icon name="x" /></button>
      </header>
      <div class="body">{@render children()}</div>
      {#if footer}<footer>{@render footer()}</footer>{/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.3);
    display: grid;
    place-items: center;
    z-index: 60;
  }
  .modal {
    max-height: 86vh;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border);
  }
  h2 {
    margin: 0;
    font-size: 15px;
  }
  .body {
    padding: var(--space-4);
    overflow: auto;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--border);
  }
</style>
