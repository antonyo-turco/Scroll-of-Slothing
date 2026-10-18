<script lang="ts" generics="T">
  // Windowed grid: only rows intersecting the viewport (± overscan) are in the
  // DOM, so libraries with thousands of assets scroll smoothly.
  import type { Snippet } from 'svelte';

  let {
    items,
    itemWidth,
    itemHeight,
    gap = 12,
    overscan = 2,
    fill = false,
    key,
    item,
  }: {
    items: T[];
    itemWidth: number;
    itemHeight: number;
    gap?: number;
    /** One full-width column (a list); `itemWidth` is ignored. */
    fill?: boolean;
    overscan?: number;
    key: (t: T) => string | number;
    item: Snippet<[T, number]>;
  } = $props();

  let viewport: HTMLDivElement | undefined = $state();
  let width = $state(0);
  let height = $state(0);
  let scrollTop = $state(0);

  const cols = $derived(fill ? 1 : Math.max(1, Math.floor((width + gap) / (itemWidth + gap))));
  const rowH = $derived(itemHeight + gap);
  const rows = $derived(Math.ceil(items.length / cols));
  const first = $derived(Math.max(0, Math.floor(scrollTop / rowH) - overscan));
  const last = $derived(Math.min(rows, Math.ceil((scrollTop + height) / rowH) + overscan));
  const visible = $derived(items.slice(first * cols, last * cols).map((it, i) => ({ it, i: first * cols + i })));

  /** Scrolls back to the top (e.g. after the query changes). */
  export function reset() {
    if (viewport) viewport.scrollTop = 0;
  }
</script>

<div
  class="viewport"
  bind:this={viewport}
  bind:clientWidth={width}
  bind:clientHeight={height}
  onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
>
  <div class="spacer" style:height="{Math.max(0, rows * rowH - gap)}px">
    <div
      class="grid"
      style:transform="translateY({first * rowH}px)"
      style:grid-template-columns={fill ? 'minmax(0, 1fr)' : `repeat(${cols}, ${itemWidth}px)`}
      style:gap="{gap}px"
    >
      {#each visible as v (key(v.it))}
        <div style:height="{itemHeight}px">{@render item(v.it, v.i)}</div>
      {/each}
    </div>
  </div>
</div>

<style>
  .viewport {
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    contain: strict;
  }
  .spacer {
    position: relative;
  }
  .grid {
    display: grid;
    justify-content: start;
    will-change: transform;
  }
</style>
