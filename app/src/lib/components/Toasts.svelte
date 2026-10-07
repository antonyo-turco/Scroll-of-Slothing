<script lang="ts">
  import { app } from '../state.svelte';
  import Icon from './Icon.svelte';
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast card {t.kind}">
      <div class="row">
        <span class="text">{t.text}</span>
        <button class="btn ghost icon" aria-label="Dismiss" onclick={() => app.dismissToast(t.id)}>
          <Icon name="x" size={14} />
        </button>
      </div>
      {#if t.detail}
        <details>
          <summary class="muted">details</summary>
          <pre>{t.detail}</pre>
        </details>
      {/if}
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    left: 50%;
    bottom: var(--space-4);
    transform: translateX(-50%);
    display: grid;
    gap: var(--space-2);
    z-index: 45;
  }
  .toast {
    min-width: 280px;
    max-width: 480px;
    padding: var(--space-2) var(--space-3);
    box-shadow: var(--shadow);
    border-left: 4px solid var(--text-muted);
  }
  .toast.success {
    border-left-color: var(--success);
  }
  .toast.error {
    border-left-color: var(--danger);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .text {
    flex: 1;
  }
  pre {
    white-space: pre-wrap;
    font-size: 12px;
    margin: var(--space-1) 0 0;
  }
</style>
