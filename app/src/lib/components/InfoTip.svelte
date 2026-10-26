<script lang="ts">
  // A small ⓘ that shows an explanation on hover or keyboard focus. The tip is
  // position: fixed, so scrolling panels (the inspector) don't clip it, and it
  // closes with Escape (WCAG 1.4.13).
  import Icon from './Icon.svelte';

  let { text, label = 'More information' }: { text: string; label?: string } = $props();

  const id = `tip-${Math.random().toString(36).slice(2, 10)}`;
  let btn = $state<HTMLButtonElement>();
  let open = $state(false);
  let pos = $state({ top: 0, left: 0 });

  const WIDTH = 260;

  function show() {
    if (!btn) return;
    const r = btn.getBoundingClientRect();
    const left = Math.min(Math.max(8, r.left + r.width / 2 - WIDTH / 2), window.innerWidth - WIDTH - 8);
    pos = { top: r.bottom + 6, left };
    open = true;
  }
  const hide = () => (open = false);
</script>

<button
  bind:this={btn}
  type="button"
  class="info-btn"
  aria-label={label}
  aria-describedby={id}
  onmouseenter={show}
  onmouseleave={hide}
  onfocus={show}
  onblur={hide}
  onclick={() => (open ? hide() : show())}
  onkeydown={(e) => e.key === 'Escape' && hide()}
>
  <Icon name="info" size={14} />
</button>
<span role="tooltip" {id} class="tip" class:open style="top: {pos.top}px; left: {pos.left}px; width: {WIDTH}px">{text}</span>

<style>
  .info-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    margin: -2px 0 -2px 2px;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-muted);
    vertical-align: middle;
    cursor: help;
  }
  .info-btn:hover,
  .info-btn:focus-visible {
    color: var(--text);
    background: var(--surface-2);
  }
  .tip {
    position: fixed;
    z-index: 1000;
    display: none;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.25);
    color: var(--text);
    font-size: 12px;
    font-weight: 400;
    line-height: 1.45;
    letter-spacing: normal;
    text-transform: none;
    white-space: normal;
    pointer-events: none;
  }
  .tip.open {
    display: block;
  }
</style>
