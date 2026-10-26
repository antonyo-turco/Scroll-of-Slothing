<script lang="ts">
  // The floating card for `use:tooltip` (lib/tooltip.svelte.ts): a title with an
  // optional colour dot, bullet lines and a muted note, with an arrow pointing at
  // the element. Kept inside the window horizontally.
  import { tip } from '../tooltip.svelte';

  const MARGIN = 8;
  let width = $state(0);
  const left = $derived(Math.min(Math.max(MARGIN, tip.x - width / 2), window.innerWidth - width - MARGIN));
  // The arrow stays on the element even when the card is pushed sideways.
  const arrow = $derived(Math.min(Math.max(14, tip.x - left), width - 14));
</script>

{#if tip.content}
  <div
    class="tooltip {tip.place}"
    class:open={tip.open}
    role="tooltip"
    bind:clientWidth={width}
    style:left="{left}px"
    style:top="{tip.y}px"
    style:--arrow="{arrow}px"
  >
    <div class="title">
      {#if tip.content.color}<span class="swatch" style:background={tip.content.color}></span>{/if}
      {tip.content.title}
    </div>
    {#if tip.content.lines?.length}
      <ul>
        {#each tip.content.lines as line, i (i)}<li>{line}</li>{/each}
      </ul>
    {/if}
    {#if tip.content.note}<div class="note">{tip.content.note}</div>{/if}
  </div>
{/if}

<style>
  .tooltip {
    position: fixed;
    z-index: 1000;
    max-width: 320px;
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.35);
    color: var(--text);
    font-size: var(--fs-sm);
    line-height: 1.45;
    pointer-events: none;
    opacity: 0;
    visibility: hidden;
    transition: opacity 0.12s, transform 0.12s, visibility 0s 0.12s;
  }
  .tooltip.above {
    transform: translateY(calc(-100% - 10px)) translateY(4px);
  }
  .tooltip.below {
    transform: translateY(10px) translateY(-4px);
  }
  .tooltip.open {
    opacity: 1;
    visibility: visible;
    transition: opacity 0.12s, transform 0.12s;
  }
  .tooltip.open.above {
    transform: translateY(calc(-100% - 10px));
  }
  .tooltip.open.below {
    transform: translateY(10px);
  }
  /* the arrow: a rotated square sharing the card's border */
  .tooltip::after {
    content: '';
    position: absolute;
    left: calc(var(--arrow) - 6px);
    width: 10px;
    height: 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    transform: rotate(45deg);
  }
  .tooltip.above::after {
    bottom: -6px;
    border-top: 0;
    border-left: 0;
  }
  .tooltip.below::after {
    top: -6px;
    border-bottom: 0;
    border-right: 0;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 700;
    font-size: var(--fs-sm);
  }
  .swatch {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  ul {
    margin: 6px 0 0;
    padding-left: 18px;
  }
  li + li {
    margin-top: 2px;
  }
  .note {
    margin-top: 6px;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
</style>
