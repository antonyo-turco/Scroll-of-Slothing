<script lang="ts">
  // Clocks, filled by hand or one segment every N minutes of real time, in two
  // views: Board (a grid of cards) and Detail (a scrolling list, three rows to a
  // screen, each clock beside its events). The timer itself runs app-wide
  // (lib/clocks.svelte.ts). Spec: docs/features/clocks.md.
  import { onMount, tick } from 'svelte';
  import { api } from '../../lib/api';
  import Icon from '../../lib/components/Icon.svelte';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import ViewActions from '../../lib/components/ViewActions.svelte';
  import { clocks } from '../../lib/clocks.svelte';
  import { isFull, isRunning, progress, timerText } from '../../lib/clocks';
  import { app } from '../../lib/state.svelte';
  import type { Clock } from '../../lib/types';
  import ClockEvents from './ClockEvents.svelte';
  import ClockFace from './ClockFace.svelte';
  import ClockSettings from './ClockSettings.svelte';

  type ViewMode = 'board' | 'detail';
  const VIEW_KEY = 'clocks.view';
  const VIEWS: { id: ViewMode; label: string; icon: 'grid' | 'list' }[] = [
    { id: 'board', label: 'Board', icon: 'grid' },
    { id: 'detail', label: 'Detail', icon: 'list' },
  ];
  let view = $state<ViewMode>('board');
  function setView(v: ViewMode) {
    view = v;
    void api.workspaceSet(VIEW_KEY, v).catch(() => {});
  }

  let now = $state(Date.now());

  const list = $derived(clocks.list);
  const sel = $derived(list.find((c) => c.id === clocks.selected) ?? null);
  const timed = $derived(list.filter((c) => c.auto));
  const running = $derived(list.filter(isRunning));

  onMount(() => {
    void api.workspaceGet<ViewMode>(VIEW_KEY).then((v) => {
      if (v === 'board' || v === 'detail') view = v;
    }).catch(() => {});
    void clocks.refresh();
    const t = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(t);
  });

  function select(c: Clock) {
    clocks.selected = c.id;
  }
  /** Switches to Detail, scrolled to this clock. */
  async function openDetail(c: Clock) {
    clocks.selected = c.id;
    setView('detail');
    await tick();
    document.getElementById(`clock-row-${c.id}`)?.scrollIntoView({ block: 'nearest' });
  }

  function onCardKey(e: KeyboardEvent, c: Clock) {
    if (e.target !== e.currentTarget) return;
    if (e.key === '+' || e.key === '=' || e.key === 'ArrowRight' || e.key === 'ArrowUp') (e.preventDefault(), clocks.advance(c, 1));
    else if (e.key === '-' || e.key === 'ArrowLeft' || e.key === 'ArrowDown') (e.preventDefault(), clocks.advance(c, -1));
    else if (e.key === ' ' && c.auto) (e.preventDefault(), clocks.toggleRun(c));
    else if (e.key === 'Enter') (e.preventDefault(), openDetail(c));
  }
</script>

<ViewActions>
  <button class="btn primary" onclick={() => clocks.create()}>New clock</button>
</ViewActions>

{#if clocks.loaded && !list.length}
  <div class="empty-state big-empty">
    <strong>Track looming trouble with clocks</strong>
    <span>A clock is a circle cut into segments (4, 6, 8 …). Fill them by hand as the danger grows, or let one fill on its own every few minutes of real time.</span>
    <button class="btn primary" onclick={() => clocks.create()}>New clock</button>
  </div>
{:else}
  <div class="view">
    <section class="work" aria-label="Clocks">
      <div class="view-toolbar">
        <button class="btn" disabled={!timed.some((c) => !isRunning(c) && !isFull(c))} onclick={() => clocks.runAll(true)}>Start all timers</button>
        <button class="btn" disabled={!running.length} onclick={() => clocks.runAll(false)}>Pause all timers</button>
        <div class="spacer"></div>
        <InfoTip label="How to fill a clock" text="Click a segment to fill up to it; click the last filled one to empty it. Double-click a clock on the board to open it in Detail." />
        <div class="segmented views" role="group" aria-label="View">
          {#each VIEWS as v (v.id)}
            <button aria-pressed={view === v.id} aria-label="{v.label} view" title="{v.label} view" onclick={() => setView(v.id)}>
              <Icon name={v.icon} size={16} />
            </button>
          {/each}
        </div>
      </div>

      {#if view === 'detail'}
        <div class="stage-area rows">
          {#each list as c (c.id)}
            <!-- Clicking anywhere in a row selects its clock; focusin does the same for the keyboard. -->
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div class="row" id="clock-row-{c.id}" data-testid="clock-row" onclick={() => select(c)} onfocusin={() => select(c)}>
              <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
              <article
                class="clock tile"
                class:active={c.id === clocks.selected}
                class:full={isFull(c)}
                tabindex="0"
                aria-label="{c.name}, {c.filled} of {c.segments}"
                onkeydown={(e) => onCardKey(e, c)}
              >
                <h2 class="name">{c.name}</h2>
                <div class="face-fit">
                  <ClockFace clock={c} size="fit" progress={progress(c, now)} onfill={(n) => (select(c), clocks.patch(c, { filled: n }))} />
                </div>
                <div class="fill-row">
                  <button class="btn icon big" aria-label="Empty a segment of {c.name}" disabled={c.filled === 0} onclick={() => clocks.advance(c, -1)}>−</button>
                  <span class="count">{c.filled}<span class="of">/{c.segments}</span></span>
                  <button class="btn icon big" aria-label="Fill a segment of {c.name}" disabled={isFull(c)} onclick={() => clocks.advance(c, 1)}>+</button>
                  {#if c.auto}
                    <button class="btn" disabled={isFull(c)} aria-pressed={isRunning(c)} onclick={() => clocks.toggleRun(c)}>{isRunning(c) ? 'Pause' : 'Start'}</button>
                    <span class="timer" class:on={isRunning(c)}>{timerText(c, now)}</span>
                  {/if}
                </div>
              </article>
              <ClockEvents clock={c} />
            </div>
          {/each}
        </div>
      {:else}
      <div class="stage-area tile-grid" style:--tile-min="260px">
        {#each list as c (c.id)}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
          <article
            class="clock tile"
            class:active={c.id === clocks.selected}
            class:full={isFull(c)}
            tabindex="0"
            aria-label="{c.name}, {c.filled} of {c.segments}"
            data-testid="clock"
            onclick={() => select(c)}
            ondblclick={() => openDetail(c)}
            onkeydown={(e) => onCardKey(e, c)}
            onfocus={() => select(c)}
          >
            <h2 class="name">{c.name}</h2>
            <ClockFace clock={c} progress={progress(c, now)} onfill={(n) => (select(c), clocks.patch(c, { filled: n }))} />
            <div class="fill-row">
              <button class="btn icon big" aria-label="Empty a segment of {c.name}" disabled={c.filled === 0} onclick={(e) => (e.stopPropagation(), clocks.advance(c, -1))}>−</button>
              <span class="count">{c.filled}<span class="of">/{c.segments}</span></span>
              <button class="btn icon big" aria-label="Fill a segment of {c.name}" disabled={isFull(c)} onclick={(e) => (e.stopPropagation(), clocks.advance(c, 1))}>+</button>
            </div>
            {#if c.auto}
              <div class="timer" class:on={isRunning(c)}>
                <button class="btn" disabled={isFull(c)} aria-pressed={isRunning(c)} onclick={(e) => (e.stopPropagation(), clocks.toggleRun(c))}>
                  {isRunning(c) ? 'Pause' : 'Start'}
                </button>
                <span>{timerText(c, now)}</span>
              </div>
            {/if}
            {#if c.notes}<p class="notes-preview">{c.notes}</p>{/if}
            <button class="btn ghost open" onclick={(e) => (e.stopPropagation(), openDetail(c))} title="Show this clock in the Detail view to write its events">
              {c.events.length ? `${c.events.length} event${c.events.length === 1 ? '' : 's'} · ` : ''}Details
            </button>
          </article>
        {/each}
      </div>
      {/if}

      <div class="statusbar">
        <span>{list.length} clock{list.length === 1 ? '' : 's'}{timed.length ? ` · ${running.length} of ${timed.length} timer${timed.length === 1 ? '' : 's'} running` : ''}</span>
      </div>
    </section>

    <ClockSettings clock={sel} {now} />
  </div>
{/if}

<style>
  .big-empty {
    height: 100%;
    gap: var(--space-3);
    font-size: var(--fs-md);
    max-width: 640px;
    margin: 0 auto;
    text-align: center;
  }
  .clock {
    display: grid;
    justify-items: center;
    gap: var(--space-3);
    padding: var(--space-4);
    cursor: default;
  }
  .clock:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .name {
    margin: 0;
    font-size: var(--fs-xl);
    text-align: center;
    overflow-wrap: anywhere;
  }
  .fill-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .big {
    width: 40px;
    height: 40px;
    font-size: 22px;
    line-height: 1;
  }
  .count {
    min-width: 64px;
    text-align: center;
    font-size: 26px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .of {
    font-size: var(--fs-lg);
    font-weight: 500;
    color: var(--text-muted);
  }
  .clock.full .count {
    color: var(--danger);
  }
  .timer {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--fs-md);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .timer.on span {
    color: var(--text);
  }
  .open {
    font-size: var(--fs-sm);
  }
  /* Detail: three rows fill the visible height; more scroll. */
  .rows {
    overflow: auto;
    padding: var(--space-4);
    container-type: size;
    display: grid;
    align-content: start;
    gap: var(--space-4);
  }
  .row {
    --row-h: max(260px, calc((100cqh - 2 * var(--space-4)) / 3));
    height: var(--row-h);
    display: grid;
    grid-template-columns: minmax(280px, 2fr) 3fr;
    gap: var(--space-4);
  }
  .row .clock {
    height: 100%;
    min-height: 0;
    grid-template-rows: auto minmax(0, 1fr) auto;
    gap: var(--space-2);
    padding: var(--space-3);
  }
  /* The face takes exactly the space between the name and the controls: a
     percentage height doesn't resolve in a grid track, so the SVG is placed
     absolutely in its box (the viewBox keeps the circle round and centred). */
  .face-fit {
    position: relative;
    width: 100%;
    min-height: 0;
  }
  .face-fit > :global(svg) {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  .row .fill-row {
    flex-wrap: wrap;
    justify-content: center;
    gap: var(--space-2);
  }
  .row .big {
    width: 36px;
    height: 36px;
  }
  .row .timer.on {
    color: var(--text);
  }
  .row .count {
    font-size: 22px;
    min-width: 56px;
  }
  .notes-preview {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--text-muted);
    text-align: center;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
</style>
