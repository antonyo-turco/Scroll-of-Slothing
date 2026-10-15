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
  const uid = $props.id();

  const FOCUSABLE =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
  const focusables = () =>
    [...(dialog?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [])].filter((el) => el.offsetParent !== null || el === document.activeElement);

  // Focus the first field of the body (not the Close button) on open, and
  // give focus back to whatever opened the dialog when it closes.
  $effect(() => {
    if (!open) return;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    queueMicrotask(() => {
      const first = dialog?.querySelector<HTMLElement>(`.body :is(${FOCUSABLE})`) ?? focusables()[0];
      (first ?? dialog)?.focus();
    });
    return () => {
      if (opener?.isConnected) opener.focus();
    };
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      open = false;
    } else if (e.key === 'Tab') {
      // keep focus inside the dialog
      const els = focusables();
      if (!els.length) return e.preventDefault();
      const i = els.indexOf(document.activeElement as HTMLElement);
      const next = e.shiftKey ? (i <= 0 ? els.length - 1 : i - 1) : i === -1 || i === els.length - 1 ? 0 : i + 1;
      if ((e.shiftKey && i <= 0) || (!e.shiftKey && (i === -1 || i === els.length - 1))) {
        e.preventDefault();
        els[next].focus();
      }
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
      aria-labelledby="modal-title-{uid}"
      style:width="min({width}px, 94vw)"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKey}
    >
      <header>
        <h2 id="modal-title-{uid}">{title}</h2>
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
