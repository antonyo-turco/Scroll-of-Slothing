<script lang="ts">
  // Renders one page of card drawing ops (bottom-left origin, PDF points) as SVG.
  // Text uses the same bundled font and the positions computed in Rust, with
  // kerning/ligatures off, so the preview matches the PDF.
  import { CARD_FONT } from '../../lib/fonts';
  import type { CardOp, Rgb } from '../../lib/types';

  let { w, h, ops, label }: { w: number; h: number; ops: CardOp[]; label: string } = $props();

  const rgb = (c: Rgb) => `rgb(${c.map((v) => Math.round(v * 255)).join(',')})`;
  const weight = (s: string) => (s.startsWith('bold') ? 700 : 400);
  const slant = (s: string) => (s.endsWith('italic') ? 'italic' : 'normal');
</script>

<svg class="card-page" viewBox="0 0 {w} {h}" role="img" aria-label={label}>
  <rect width={w} height={h} fill="#fff" />
  {#each ops as o, i (i)}
    {#if o.op === 'fill'}
      <rect x={o.x} y={h - o.y - o.h} width={o.w} height={o.h} fill={rgb(o.rgb)} />
    {:else if o.op === 'rect'}
      <rect x={o.x} y={h - o.y - o.h} width={o.w} height={o.h} fill="none" stroke={rgb(o.rgb)} stroke-width={o.width} />
    {:else if o.op === 'line'}
      <line x1={o.x1} y1={h - o.y1} x2={o.x2} y2={h - o.y2} stroke={rgb(o.rgb)} stroke-width={o.width} stroke-dasharray={o.dash ? o.dash.join(' ') : undefined} />
    {:else}
      <text
        x={o.x}
        y={h - o.y}
        font-family={CARD_FONT}
        font-size={o.size}
        font-weight={weight(o.style)}
        font-style={slant(o.style)}
        fill={rgb(o.rgb)}
        xml:space="preserve">{o.text}</text
      >
    {/if}
  {/each}
</svg>

<style>
  .card-page {
    display: block;
    width: 100%;
    height: auto;
    background: #fff;
    box-shadow: var(--shadow);
  }
  .card-page text {
    font-kerning: none;
    font-variant-ligatures: none;
    text-rendering: geometricPrecision;
  }
</style>
