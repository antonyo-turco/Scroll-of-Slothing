<script lang="ts">
  // A Blades in the Dark style clock: a circle cut into wedges. Clicking a wedge
  // fills the clock up to it; clicking the last filled wedge empties it.
  // A running timer shades the next wedge as its segment counts up, and a dot
  // outside the rim marks each segment that has an event (hollow until the
  // segment fills, then solid in the clock's colour, like the legendary action
  // pips in Combat); hovering or focusing one shows its events.
  import { firesAt } from '../../lib/clocks';
  import { tooltip, type TipContent } from '../../lib/tooltip.svelte';
  import type { Clock } from '../../lib/types';

  let { clock, progress = 0, size = 200, onfill }: {
    clock: Clock;
    /** Share of the next segment counted by the timer, 0–1. */
    progress?: number;
    /** Pixels, or 'fit' to fill the parent (keeping the circle round). */
    size?: number | 'fit';
    onfill?: (filled: number) => void;
  } = $props();

  const R = 44;
  const color = $derived(clock.color ?? 'var(--accent)');
  const step = $derived((2 * Math.PI) / clock.segments);
  // Dividers thin out as segments crowd together: 1.6 at 4, ~1.1 at 8, ~0.9 at 12, ~0.65 at 24.
  const divider = $derived(1.6 * Math.sqrt(4 / clock.segments));
  // Past 8 segments the dividers stop short of the centre, so they don't merge into a blob.
  const hub = $derived(clock.segments > 8 ? (R * 0.14 * (clock.segments - 8)) / 16 : 0);
  /** Per segment, the events that fire when it fills. */
  const marks = $derived(
    Array.from({ length: clock.segments }, (_, i) => clock.events.filter((e) => firesAt(e, i + 1))),
  );
  /** The first segment ahead with an event. */
  const nextMark = $derived(marks.findIndex((m, i) => m.length && i >= clock.filled));

  function tipFor(i: number): TipContent {
    const evs = marks[i];
    const state = i < clock.filled ? 'reached' : i === nextMark ? 'next' : 'ahead';
    const repeats = evs.filter((e) => e.repeat).map((e) => `"${e.text}" repeats every ${e.repeat} from segment ${e.at}`);
    return {
      title: `Segment ${i + 1} · ${state}`,
      lines: evs.map((e) => e.text),
      note: repeats.length ? repeats.join('; ') : undefined,
      color,
    };
  }

  const pt = (a: number, r = R) => `${(r * Math.sin(a)).toFixed(3)} ${(-r * Math.cos(a)).toFixed(3)}`;
  const wedge = (a0: number, a1: number) => `M0 0L${pt(a0)}A${R} ${R} 0 0 1 ${pt(a1)}Z`;

  function pick(i: number) {
    onfill?.(clock.filled === i + 1 ? i : i + 1);
  }
</script>

<svg viewBox="-50 -50 100 100" width={size === 'fit' ? '100%' : size} height={size === 'fit' ? '100%' : size} role="img" aria-label="{clock.name}: {clock.filled} of {clock.segments} segments">
  {#each { length: clock.segments } as _, i (i)}
    {@const a0 = i * step}
    <path
      class="seg"
      class:on={i < clock.filled}
      d={wedge(a0, a0 + step)}
      style:fill={i < clock.filled ? color : undefined}
      role="button"
      tabindex="-1"
      aria-label="Fill to segment {i + 1}"
      onclick={() => pick(i)}
      onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), pick(i))}
    />
  {/each}
  {#if progress > 0 && clock.filled < clock.segments}
    {@const a0 = clock.filled * step}
    <path class="partial" d={wedge(a0, a0 + step * progress)} style:fill={color} />
  {/if}
  {#each { length: clock.segments } as _, i (i)}
    <path class="divider" d="M{pt(i * step, hub)}L{pt(i * step)}" style:stroke-width={divider} />
  {/each}
  <circle r={R} class="rim" />
  {#each marks as evs, i (i)}
    {#if evs.length}
      {@const a = (i + 0.5) * step}
      {@const x = (R + 5.5) * Math.sin(a)}
      {@const y = -(R + 5.5) * Math.cos(a)}
      <g class="mark" class:reached={i < clock.filled} class:next={i === nextMark} style:--c={color}
        tabindex="-1" role="img" aria-label="Segment {i + 1}: {evs.map((e) => e.text).join('; ')}" use:tooltip={tipFor(i)}>
        <circle class="hit" cx={x} cy={y} r="5.5" />
        <circle class="pip" cx={x} cy={y} r="2.7" />
      </g>
    {/if}
  {/each}
</svg>

<style>
  svg {
    display: block;
    overflow: visible;
  }
  .seg {
    fill: var(--surface-2);
    stroke: none;
    cursor: pointer;
    transition: fill 0.15s;
  }
  .seg:not(.on):hover {
    fill: var(--border);
  }
  .seg:focus {
    outline: none;
  }
  .partial {
    opacity: 0.3;
    pointer-events: none;
  }
  .divider {
    stroke: var(--text);
    stroke-linecap: round;
    pointer-events: none;
  }
  .rim {
    fill: none;
    stroke: var(--text);
    stroke-width: 2.6;
    pointer-events: none;
  }
  .mark {
    cursor: help;
    outline: none;
  }
  .hit {
    fill: transparent;
  }
  /* hollow until reached, like an available legendary action pip turned around */
  .pip {
    fill: var(--surface);
    stroke: var(--text);
    stroke-width: 1.3;
    transition: fill 0.15s, stroke 0.15s;
  }
  .mark.next .pip {
    stroke: var(--c);
    stroke-width: 1.6;
  }
  .mark.reached .pip {
    fill: var(--c);
    stroke: var(--c);
  }
  .mark:hover .pip,
  .mark:focus-visible .pip {
    stroke-width: 2.2;
  }
</style>
