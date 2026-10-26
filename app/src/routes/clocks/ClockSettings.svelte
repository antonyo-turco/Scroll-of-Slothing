<script lang="ts">
  // The clock inspector shared by the Board and Detail views: name, segments,
  // colour, timer, notes, reset and delete.
  import Icon from '../../lib/components/Icon.svelte';
  import { clocks } from '../../lib/clocks.svelte';
  import { CLOCK_COLORS, MAX_SEGMENTS, MIN_SEGMENTS, SEGMENT_CHOICES, elapsedMs, isFull, isRunning } from '../../lib/clocks';
  import type { Clock } from '../../lib/types';

  let { clock, now }: { clock: Clock | null; now: number } = $props();

  type Unit = 'sec' | 'min' | 'h';
  const UNIT_SECS: Record<Unit, number> = { sec: 1, min: 60, h: 3600 };

  let confirmDelete = $state(false);
  $effect(() => {
    void clock?.id;
    confirmDelete = false;
  });

  // The timer period is edited as a number and a unit; the unit follows the stored value.
  function unitOf(secs: number): Unit {
    if (secs % 3600 === 0) return 'h';
    if (secs % 60 === 0) return 'min';
    return 'sec';
  }
  let unit = $state<Unit>('min');
  $effect(() => {
    if (clock?.auto) unit = unitOf(clock.auto.every_secs);
  });
  function setEvery(c: Clock, value: string, u: Unit) {
    const n = Number(value);
    if (!Number.isFinite(n) || n <= 0) return;
    void clocks.patch(c, { every_secs: Math.round(n * UNIT_SECS[u]) });
  }
</script>

<aside class="inspector" aria-label="Clock settings">
  {#if clock}
    {@const sel = clock}
    {#key sel.id}
      <section class="section">
        <h3 class="section-title">Clock</h3>
        <label class="field"><span class="label">Name</span>
          <input class="input" value={sel.name} onchange={(e) => clocks.patch(sel, { name: e.currentTarget.value })} />
        </label>
        <div class="field">
          <span class="label">Segments</span>
          <div class="seg-pick">
            <div class="segmented" role="group" aria-label="Segments">
              {#each SEGMENT_CHOICES as n (n)}
                <button aria-pressed={sel.segments === n} onclick={() => clocks.patch(sel, { segments: n })}>{n}</button>
              {/each}
            </div>
            <input class="input num" type="number" min={MIN_SEGMENTS} max={MAX_SEGMENTS} aria-label="Number of segments" value={sel.segments}
              onchange={(e) => clocks.patch(sel, { segments: Number(e.currentTarget.value) })} />
          </div>
        </div>
        <div class="field">
          <span class="label">Colour</span>
          <div class="swatches" role="group" aria-label="Colour">
            {#each CLOCK_COLORS as col (col ?? 'accent')}
              <button class="swatch" style:background={col ?? 'var(--accent)'} aria-pressed={sel.color === col}
                aria-label={col ?? 'Accent colour'} title={col ?? 'Accent colour'} onclick={() => clocks.patch(sel, { color: col ?? '' })}></button>
            {/each}
          </div>
        </div>
      </section>

      <section class="section">
        <h3 class="section-title">Timer</h3>
        <label class="check">
          <input type="checkbox" checked={!!sel.auto} onchange={(e) => clocks.patch(sel, { every_secs: e.currentTarget.checked ? 600 : 0 })} />
          Fill a segment on its own
        </label>
        {#if sel.auto}
          <div class="field">
            <span class="label">Every</span>
            <div class="every">
              <input class="input num" type="number" min="1" step="any" aria-label="Timer period"
                value={sel.auto.every_secs / UNIT_SECS[unit]}
                onchange={(e) => setEvery(sel, e.currentTarget.value, unit)} />
              <select class="input" aria-label="Timer unit" value={unit}
                onchange={(e) => { const u = e.currentTarget.value as Unit; if (sel.auto) setEvery(sel, String(sel.auto.every_secs / UNIT_SECS[unit]), u); unit = u; }}>
                <option value="sec">seconds</option>
                <option value="min">minutes</option>
                <option value="h">hours</option>
              </select>
            </div>
          </div>
          <button class="btn" disabled={isFull(sel)} onclick={() => clocks.toggleRun(sel)}>{isRunning(sel) ? 'Pause timer' : 'Start timer'}</button>
          <p class="note">You get a notice and a chime on every segment, for each event and when the clock fills, on any page of the app. A full clock stops its timer; a paused one keeps its partial segment.</p>
        {/if}
      </section>

      <section class="section">
        <h3 class="section-title">Notes</h3>
        <textarea class="input notes" aria-label="Clock notes" placeholder="What happens when it fills…"
          value={sel.notes} onchange={(e) => clocks.patch(sel, { notes: e.currentTarget.value })}></textarea>
      </section>

      <div class="inspector-footer">
        {#if confirmDelete}
          <span class="note">Delete "{sel.name}"?</span>
          <div class="pair">
            <button class="btn" onclick={() => (confirmDelete = false)}>Cancel</button>
            <button class="btn danger" onclick={() => (confirmDelete = false, clocks.remove(sel))}>Delete</button>
          </div>
        {:else}
          <div class="pair">
            <button class="btn" disabled={sel.filled === 0 && !(sel.auto && elapsedMs(sel, now) > 0)} onclick={() => clocks.reset(sel)}>
              <Icon name="undo" /> Reset
            </button>
            <button class="btn ghost danger" onclick={() => (confirmDelete = true)}>Delete…</button>
          </div>
        {/if}
      </div>
    {/key}
  {:else}
    <p class="note pad">Select a clock to change it.</p>
  {/if}
</aside>

<style>
  .seg-pick,
  .every {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .num {
    width: 80px;
  }
  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .swatch {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    border: 2px solid var(--border);
    cursor: pointer;
    padding: 0;
  }
  .swatch[aria-pressed='true'] {
    border-color: var(--text);
    box-shadow: 0 0 0 2px var(--surface), 0 0 0 4px var(--text);
  }
  .notes {
    min-height: 140px;
    resize: vertical;
    font-size: var(--fs-md);
    line-height: 1.5;
    padding: var(--space-2);
  }
  .pad {
    padding: var(--space-4);
  }
</style>
