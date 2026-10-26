<script lang="ts">
  // A clock's events: things that happen when a given segment fills, once or
  // every N segments after. The app announces them when their segment fills,
  // by hand or by the timer. The list scrolls; the add form stays at the bottom.
  import Icon from '../../lib/components/Icon.svelte';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import { clocks } from '../../lib/clocks.svelte';
  import { MAX_SEGMENTS, firingSegments } from '../../lib/clocks';
  import { app } from '../../lib/state.svelte';
  import type { Clock, ClockEvent } from '../../lib/types';

  let { clock }: { clock: Clock } = $props();

  let newAt = $state(1);
  let newText = $state('');
  let newRepeat = $state<number | null>(null);

  const events = $derived([...clock.events].sort((a, b) => a.at - b.at));
  /** The next segment with an event, after the filled ones. */
  const nextSeg = $derived.by(() => {
    const segs = clock.events.flatMap((e) => firingSegments(e, clock.segments)).filter((s) => s > clock.filled);
    return segs.length ? Math.min(...segs) : null;
  });
  // A new event defaults to the next segment to fill.
  const nextFill = $derived(Math.min(clock.filled + 1, clock.segments));
  $effect(() => {
    newAt = nextFill;
  });

  const save = (list: ClockEvent[]) => clocks.patch(clock, { events: list });
  function change(id: string, p: Partial<ClockEvent>) {
    const list = clock.events.map((e) => (e.id === id ? { ...e, ...p } : e));
    if (!list.find((x) => x.id === id)?.text.trim()) return app.toast('error', 'An event needs some text');
    void save(list);
  }
  async function add() {
    if (!newText.trim()) return;
    await save([...clock.events, { id: '', at: newAt, repeat: newRepeat && newRepeat > 0 ? newRepeat : null, text: newText }]);
    newText = '';
    newRepeat = null;
  }

  function status(e: ClockEvent): 'next' | 'passed' | 'beyond' | 'later' {
    const segs = firingSegments(e, clock.segments);
    if (!segs.length) return 'beyond';
    if (segs.includes(nextSeg ?? -1)) return 'next';
    return segs.every((s) => s <= clock.filled) ? 'passed' : 'later';
  }
  function whenText(e: ClockEvent): string {
    const segs = firingSegments(e, clock.segments);
    if (!segs.length) return `past the last segment (${clock.segments})`;
    return segs.length > 1 ? `at ${segs.join(', ')}` : `at ${segs[0]}`;
  }
  const num = (v: string) => Math.max(1, Math.min(MAX_SEGMENTS, Math.round(Number(v) || 1)));
</script>

<section class="events card" aria-label="Events of {clock.name}">
  <header>
    <h3>Events <span class="muted-count">{clock.events.length}</span></h3>
    <InfoTip label="About events" text="What happens when a segment fills, once or every N segments after. You get a notice when it does, whether you fill it or the timer does. Dots around the clock mark these segments. For example: at 2 'the guards change shift', or at 3 every 2 'the ritual pulses'." />
    {#if nextSeg}<span class="next-at">next at segment {nextSeg}</span>{/if}
  </header>

  <div class="list">
    {#if events.length}
      <ul>
        {#each events as e (e.id)}
          {@const st = status(e)}
          <li class="event {st}" data-testid="clock-event">
            <input class="input at" type="number" min="1" max={MAX_SEGMENTS} value={e.at} aria-label="Segment of {e.text}" title="The segment whose filling triggers it"
              onchange={(ev) => change(e.id, { at: num(ev.currentTarget.value) })} />
            <div class="body">
              <input class="input text" value={e.text} aria-label="Event text" onchange={(ev) => change(e.id, { text: ev.currentTarget.value })} />
              <div class="meta">
                <label class="check repeat">
                  <input type="checkbox" checked={!!e.repeat} onchange={(ev) => change(e.id, { repeat: ev.currentTarget.checked ? 1 : null })} />
                  Repeat every
                </label>
                {#if e.repeat}
                  <input class="input small" type="number" min="1" max={MAX_SEGMENTS} value={e.repeat} aria-label="Repeat every N segments"
                    onchange={(ev) => change(e.id, { repeat: num(ev.currentTarget.value) })} />
                  <span>segment{e.repeat === 1 ? '' : 's'}</span>
                {/if}
                <span class="when">{whenText(e)}</span>
                {#if st === 'next'}<span class="tag next">next</span>{:else if st === 'passed'}<span class="tag">done</span>{/if}
              </div>
            </div>
            <button class="btn ghost icon" aria-label="Delete the event {e.text}" title="Delete the event" onclick={() => save(clock.events.filter((x) => x.id !== e.id))}>
              <Icon name="x" />
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="empty">No events yet.</p>
    {/if}
  </div>

  <form class="add" onsubmit={(ev) => (ev.preventDefault(), add())}>
    <input class="input at" type="number" min="1" max={MAX_SEGMENTS} aria-label="Segment of the new event" title="Segment" bind:value={newAt} />
    <input class="input text" placeholder="What happens…" aria-label="New event" bind:value={newText} />
    <input class="input small" type="number" min="1" max={MAX_SEGMENTS} placeholder="every" aria-label="Repeat the new event every N segments"
      title="Fire again every N segments after it (leave empty for once)" bind:value={newRepeat} />
    <button class="btn primary" disabled={!newText.trim()}>Add</button>
  </form>
</section>

<style>
  .events {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  h3 {
    margin: 0;
    font-size: var(--fs-lg);
  }
  .muted-count {
    margin-left: 4px;
    font-size: var(--fs-md);
    font-weight: 500;
    color: var(--text-muted);
  }
  .next-at {
    margin-left: auto;
    font-size: var(--fs-sm);
    color: var(--text-muted);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .empty {
    margin: 0;
    font-size: var(--fs-md);
    color: var(--text-muted);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--space-1);
  }
  .event {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    border-left: 4px solid transparent;
  }
  .event:hover {
    background: var(--surface-2);
  }
  .event.next {
    border-left-color: var(--accent);
    background: var(--surface-2);
  }
  .event.passed .text,
  .event.beyond .text {
    color: var(--text-muted);
  }
  .event.beyond .when {
    color: var(--danger);
  }
  .at {
    width: 64px;
    flex: none;
    font-size: var(--fs-md);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: var(--space-1);
  }
  .text {
    width: 100%;
    font-size: var(--fs-md);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--fs-sm);
    color: var(--text-muted);
  }
  .repeat {
    font-size: var(--fs-sm);
  }
  .small {
    width: 72px;
    flex: none;
  }
  .when {
    margin-left: auto;
  }
  .tag {
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--surface-2);
    font-weight: 600;
  }
  .tag.next {
    background: var(--accent);
    color: var(--accent-contrast, #1b1b1b);
  }
  .add {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding-top: var(--space-2);
    border-top: 1px solid var(--border);
  }
  .add .text {
    flex: 1;
    min-width: 0;
  }
</style>
