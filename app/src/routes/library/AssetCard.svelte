<script lang="ts" module>
  export type CardVariant = 'card' | 'pill' | 'row';

  /** Columns of a list row; the list header in Library.svelte uses the same template. */
  export const ROW_COLUMNS = '40px minmax(0, 2fr) 84px 104px minmax(0, 1.5fr) 92px 32px';

  /** Item sizes for the virtualized views (width is ignored for rows: they fill). */
  export const VARIANT_SIZE: Record<CardVariant, { w: number; h: number }> = {
    card: { w: 176, h: 258 },
    pill: { w: 112, h: 142 },
    row: { w: 0, h: 48 },
  };
</script>

<script lang="ts">
  import { blobUrl } from '../../lib/api';
  import { portal } from '../../lib/portal';
  import { KIND_LABELS, SIZE_LABELS, type Asset, type AssetKind } from '../../lib/types';

  let {
    asset,
    variant = 'card',
    selected = false,
    active = false,
    selecting = false,
    onclick,
    onpin,
    onsize,
  }: {
    asset: Asset;
    variant?: CardVariant;
    selected?: boolean;
    active?: boolean;
    selecting?: boolean;
    onclick: (e: MouseEvent) => void;
    onpin: () => void;
    onsize: (size: string) => void;
  } = $props();

  const kind = $derived(KIND_LABELS[asset.kind as AssetKind] ?? asset.kind);
  const date = $derived(asset.updated.slice(0, 10));

  let hover = $state(false);
  let pos = $state({ left: 0, top: 0 });
  let timer: ReturnType<typeof setTimeout> | null = null;
  const PREVIEW_W = 336;
  const PREVIEW_H = 360;

  // Fixed-position popover beside the item (right if it fits, else left), so it
  // is never clipped by the scrolling grid and never covers the hovered item.
  function enter(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    timer = setTimeout(() => {
      const right = r.right + 8 + PREVIEW_W <= window.innerWidth;
      pos = {
        left: right ? r.right + 8 : Math.max(8, r.left - 8 - PREVIEW_W),
        top: Math.min(Math.max(8, r.top), window.innerHeight - PREVIEW_H - 8),
      };
      hover = true;
    }, 350);
  }
  function leave() {
    if (timer) clearTimeout(timer);
    hover = false;
  }
</script>

{#snippet thumb()}
  <div class="thumb">
    <img src={blobUrl(asset.thumb_blob ?? asset.rendered_blob)} alt="" loading="lazy" decoding="async" draggable="false" />
    {#if selecting}<span class="check" aria-hidden="true">{selected ? '✓' : ''}</span>{/if}
  </div>
{/snippet}

{#snippet pin()}
  <button
    class="pin"
    class:on={asset.pinned}
    aria-label={asset.pinned ? `Unpin ${asset.name}` : `Pin ${asset.name}`}
    aria-pressed={asset.pinned}
    title={asset.pinned ? 'Pinned' : 'Pin'}
    onclick={onpin}>{asset.pinned ? '★' : '☆'}</button
  >
{/snippet}

{#snippet size()}
  {#if asset.kind !== 'map'}
    <select class="size" aria-label="Print size of {asset.name}" value={asset.size_label} onchange={(e) => onsize(e.currentTarget.value)}>
      {#each SIZE_LABELS as s (s)}<option value={s}>{s}</option>{/each}
    </select>
  {/if}
{/snippet}

{#if variant === 'row'}
  <div class="item row-item" class:selected class:active data-testid="asset-card" style:grid-template-columns={ROW_COLUMNS}>
    <!-- The whole row is the hit target; the controls sit above it. -->
    <button class="hit row-hit" aria-label="{asset.name} ({kind})" aria-pressed={selected || active} {onclick} onmouseenter={enter} onmouseleave={leave}></button>
    {@render thumb()}
    <div class="cell name-cell">
      <span class="name" title={asset.name}>{asset.name}</span>
      {#if asset.aliases}<span class="aka-inline">{asset.aliases.split('\n').join(', ')}</span>{/if}
    </div>
    <div class="cell"><span class="kind kind-{asset.kind}">{kind}</span></div>
    <div class="cell control">{@render size()}</div>
    <div class="cell tags" title={asset.tags.join(', ')}>{asset.tags.join(' · ')}</div>
    <div class="cell date">{date}</div>
    <div class="cell control">{@render pin()}</div>
  </div>
{:else}
  <article class="card item asset" class:pill={variant === 'pill'} class:selected class:active data-testid="asset-card">
    <button class="hit" aria-label="{asset.name} ({kind})" aria-pressed={selected || active} {onclick} onmouseenter={enter} onmouseleave={leave}>
      {@render thumb()}
    </button>
    {@render pin()}
    <div class="meta">
      <div class="name" title={asset.name}>{asset.name}</div>
      {#if variant === 'card'}
        <div class="row">
          <span class="kind kind-{asset.kind}">{kind}</span>
          {@render size()}
        </div>
        {#if asset.tags.length}
          <div class="tags" title={asset.tags.join(', ')}>{asset.tags.slice(0, 3).join(' · ')}{asset.tags.length > 3 ? ' …' : ''}</div>
        {/if}
      {/if}
    </div>
  </article>
{/if}

{#if hover && asset.preview_blob}
  <div use:portal class="preview card" aria-hidden="true" style:left="{pos.left}px" style:top="{pos.top}px">
    <img src={blobUrl(asset.preview_blob)} alt="" />
    {#if asset.aliases}<div class="muted aka">aka {asset.aliases.split('\n').join(', ')}</div>{/if}
  </div>
{/if}

<style>
  .item {
    transition: border-color 80ms, box-shadow 80ms;
  }
  .item.active {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .item.selected {
    border-color: var(--focus);
    box-shadow: 0 0 0 2px var(--focus);
  }

  /* ── card and pill ── */
  .asset {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: visible;
  }
  .hit {
    border: 0;
    padding: 0;
    background: none;
    cursor: pointer;
  }
  .thumb {
    position: relative;
    aspect-ratio: 1;
    border-radius: var(--radius) var(--radius) 0 0;
    background:
      repeating-conic-gradient(var(--surface-2) 0 25%, var(--surface) 0 50%) 0 0 / 16px 16px;
    display: grid;
    place-items: center;
    overflow: hidden;
  }
  .thumb img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .check {
    position: absolute;
    top: 6px;
    left: 6px;
    width: 20px;
    height: 20px;
    border-radius: 4px;
    border: 2px solid var(--focus);
    background: var(--surface);
    color: var(--focus);
    font-size: 13px;
    font-weight: 700;
    display: grid;
    place-items: center;
  }
  .pin {
    position: absolute;
    top: 4px;
    right: 4px;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 50%;
    background: rgb(0 0 0 / 0.35);
    color: #fff;
    cursor: pointer;
    font-size: 15px;
    opacity: 0;
    transition: opacity 80ms;
  }
  .item:hover .pin,
  .pin.on,
  .pin:focus-visible {
    opacity: 1;
  }
  .pin.on {
    color: #ffc94a;
  }
  .meta {
    padding: var(--space-2);
    display: grid;
    gap: 4px;
    min-width: 0;
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .kind {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--text-muted);
  }
  .size {
    margin-left: auto;
    font-size: 12px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface);
    padding: 1px 2px;
  }
  .tags {
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* pill: smaller image, name only */
  .pill .meta {
    padding: 4px var(--space-2);
  }
  .pill .name {
    font-size: 12px;
    font-weight: 500;
  }
  .pill .pin {
    width: 24px;
    height: 24px;
    font-size: 13px;
  }
  .pill .check {
    width: 18px;
    height: 18px;
    top: 4px;
    left: 4px;
  }

  /* ── row ── */
  .row-item {
    position: relative;
    display: grid;
    align-items: center;
    gap: var(--space-3);
    height: 100%;
    padding: 0 var(--space-2);
    border: 1px solid transparent;
    border-bottom-color: var(--border);
    border-radius: var(--radius-sm);
  }
  .row-item:hover {
    background: var(--surface-2);
  }
  .row-item.active,
  .row-item.selected {
    background: var(--accent-soft);
  }
  .row-hit {
    position: absolute;
    inset: 0;
    width: 100%;
    border-radius: inherit;
  }
  .row-hit:focus-visible {
    outline-offset: -2px;
  }
  .row-item .thumb {
    width: 40px;
    height: 40px;
    border-radius: var(--radius-sm);
    pointer-events: none;
  }
  .row-item .check {
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    font-size: 11px;
  }
  .cell {
    min-width: 0;
    pointer-events: none;
    font-size: 13px;
  }
  .cell.control {
    position: relative;
    z-index: 1;
    pointer-events: auto;
  }
  .name-cell {
    display: grid;
  }
  .aka-inline {
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row-item .size {
    margin-left: 0;
  }
  .row-item .tags {
    font-size: 12px;
  }
  .date {
    color: var(--text-muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .row-item .pin {
    position: static;
    opacity: 1;
    background: transparent;
    color: var(--text-muted);
  }
  .row-item .pin.on {
    color: #e0a400;
  }

  .preview {
    position: fixed;
    z-index: 30;
    width: 336px;
    padding: 8px;
    box-shadow: var(--shadow);
    pointer-events: none;
  }
  .preview img {
    width: 320px;
    max-height: 320px;
    object-fit: contain;
    display: block;
  }
  .aka {
    font-size: 12px;
    margin-top: 4px;
  }
</style>
