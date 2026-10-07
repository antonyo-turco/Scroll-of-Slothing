<script lang="ts">
  import { blobUrl } from '../../lib/api';
  import { KIND_LABELS, SIZE_LABELS, type Asset, type AssetKind } from '../../lib/types';

  let {
    asset,
    selected = false,
    active = false,
    selecting = false,
    onclick,
    onpin,
    onsize,
  }: {
    asset: Asset;
    selected?: boolean;
    active?: boolean;
    selecting?: boolean;
    onclick: (e: MouseEvent) => void;
    onpin: () => void;
    onsize: (size: string) => void;
  } = $props();

  let hover = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;

  function enter() {
    timer = setTimeout(() => (hover = true), 350);
  }
  function leave() {
    if (timer) clearTimeout(timer);
    hover = false;
  }
</script>

<article class="card asset" class:selected class:active data-testid="asset-card">
  <button class="hit" aria-label="{asset.name} ({KIND_LABELS[asset.kind as AssetKind] ?? asset.kind})" aria-pressed={selected || active} {onclick} onmouseenter={enter} onmouseleave={leave}>
    <div class="thumb">
      <img src={blobUrl(asset.thumb_blob ?? asset.rendered_blob)} alt="" loading="lazy" decoding="async" draggable="false" />
      {#if selecting}<span class="check" aria-hidden="true">{selected ? '✓' : ''}</span>{/if}
    </div>
  </button>
  <button
    class="pin"
    class:on={asset.pinned}
    aria-label={asset.pinned ? 'Unpin' : 'Pin'}
    aria-pressed={asset.pinned}
    title={asset.pinned ? 'Pinned' : 'Pin'}
    onclick={onpin}>{asset.pinned ? '★' : '☆'}</button
  >
  <div class="meta">
    <div class="name" title={asset.name}>{asset.name}</div>
    <div class="row">
      <span class="kind kind-{asset.kind}">{KIND_LABELS[asset.kind as AssetKind] ?? asset.kind}</span>
      {#if asset.kind !== 'map'}
        <select class="size" aria-label="Print size of {asset.name}" value={asset.size_label} onchange={(e) => onsize(e.currentTarget.value)}>
          {#each SIZE_LABELS as s (s)}<option value={s}>{s}</option>{/each}
        </select>
      {/if}
    </div>
    {#if asset.tags.length}
      <div class="tags" title={asset.tags.join(', ')}>{asset.tags.slice(0, 3).join(' · ')}{asset.tags.length > 3 ? ' …' : ''}</div>
    {/if}
  </div>
  {#if hover && asset.preview_blob}
    <div class="preview card" aria-hidden="true">
      <img src={blobUrl(asset.preview_blob)} alt="" />
      {#if asset.aliases}<div class="muted aka">aka {asset.aliases.split('\n').join(', ')}</div>{/if}
    </div>
  {/if}
</article>

<style>
  .asset {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: visible;
    transition: border-color 80ms, box-shadow 80ms;
  }
  .asset.active {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .asset.selected {
    border-color: var(--focus);
    box-shadow: 0 0 0 2px var(--focus);
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
  .asset:hover .pin,
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
  .preview {
    position: absolute;
    z-index: 20;
    left: 50%;
    top: -8px;
    transform: translate(-50%, -100%);
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
