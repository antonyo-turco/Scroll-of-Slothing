<script lang="ts">
  // Map filter pipeline editor (docs/features/map-filters.md): preset dropdown,
  // per-filter toggle + slider(s), drag to reorder, Clear all.
  import Slider from './Slider.svelte';
  import type { FilterCatalog, FilterStep } from '../types';

  let { pipeline = $bindable(), catalog }: { pipeline: FilterStep[]; catalog: FilterCatalog | null } = $props();

  let preset = $state('');
  let dragFrom = $state<number | null>(null);
  /** Row whose grip is held: only it is draggable, so sliders inside rows still slide. */
  let armed = $state<number | null>(null);

  const def = (key: string) => catalog?.defs.find((d) => d.key === key);

  /** Every known filter appears once; unknown/missing ones are appended disabled. */
  const steps = $derived.by(() => {
    const out = [...pipeline];
    for (const d of catalog?.defs ?? []) {
      if (!out.some((s) => s.type === d.key)) {
        out.push({ type: d.key, enabled: false, params: Object.fromEntries(d.params.map((p) => [p.key, p.default])) });
      }
    }
    return out;
  });

  function commit(next: FilterStep[]) {
    pipeline = next;
  }

  function applyPreset(name: string) {
    const p = catalog?.presets.find((x) => x.name === name);
    if (p) commit(structuredClone($state.snapshot(p.pipeline)) as FilterStep[]);
  }

  function toggle(i: number, on: boolean) {
    const next = steps.map((s) => ({ ...s }));
    next[i].enabled = on;
    commit(next);
    preset = '';
  }

  function setParam(i: number, key: string, v: number) {
    const next = steps.map((s) => ({ ...s, params: { ...s.params } }));
    next[i].params[key] = v;
    commit(next);
    preset = '';
  }

  function clearAll() {
    commit(steps.map((s) => ({ ...s, enabled: false })));
    preset = '';
  }

  function drop(to: number) {
    if (dragFrom === null || dragFrom === to) return;
    const next = [...steps];
    const [m] = next.splice(dragFrom, 1);
    next.splice(to, 0, m);
    dragFrom = null;
    armed = null;
    commit(next);
  }

  function move(i: number, by: number) {
    const j = i + by;
    if (j < 0 || j >= steps.length) return;
    const next = [...steps];
    [next[i], next[j]] = [next[j], next[i]];
    commit(next);
  }
</script>

<div class="filters">
  <div class="row">
    <select class="input grow" aria-label="Filter preset" bind:value={preset} onchange={() => applyPreset(preset)}>
      <option value="">Preset…</option>
      {#each catalog?.presets ?? [] as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
    </select>
    <button class="btn" onclick={clearAll}>Clear all</button>
  </div>
  <ul>
    {#each steps as s, i (s.type)}
      {@const d = def(s.type)}
      <li
        draggable={armed === i}
        class:on={s.enabled}
        class:dragging={dragFrom === i}
        ondragstart={() => (dragFrom = i)}
        ondragend={() => ((dragFrom = null), (armed = null))}
        ondragover={(e) => e.preventDefault()}
        ondrop={() => drop(i)}
      >
        <div class="head">
          <span
            class="grip"
            title="Drag to reorder"
            aria-hidden="true"
            onpointerdown={() => (armed = i)}
            onpointerup={() => (armed = null)}>⋮⋮</span
          >
          <label class="name">
            <input type="checkbox" checked={s.enabled} onchange={(e) => toggle(i, e.currentTarget.checked)} />
            <span>{d?.label ?? s.type}</span>
          </label>
          <button class="btn icon ghost" aria-label="Move {d?.label ?? s.type} up" disabled={i === 0} onclick={() => move(i, -1)}>▲</button>
          <button class="btn icon ghost" aria-label="Move {d?.label ?? s.type} down" disabled={i === steps.length - 1} onclick={() => move(i, 1)}>▼</button>
        </div>
        {#if s.enabled && d?.params.length}
          <div class="params">
            {#each d.params as p (p.key)}
              <Slider
                label={p.label}
                min={p.min}
                max={p.max}
                step={p.step}
                value={s.params[p.key] ?? p.default}
                oninput={(v) => setParam(i, p.key, v)}
              />
            {/each}
          </div>
        {/if}
      </li>
    {/each}
  </ul>
</div>

<style>
  .filters {
    display: grid;
    gap: var(--space-2);
  }
  .row {
    display: flex;
    gap: var(--space-2);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
  }
  li {
    min-width: 0;
    border-radius: var(--radius-sm);
    padding: 2px 4px;
  }
  li.on {
    background: var(--surface-2);
  }
  li.dragging {
    opacity: 0.5;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
  }
  .grip {
    flex: none;
    width: 18px;
    padding: 6px 0;
    text-align: center;
    cursor: grab;
    color: var(--text-muted);
    font-size: 14px;
    letter-spacing: -3px;
    touch-action: none;
  }
  .name {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 6px;
    align-items: center;
    min-height: 30px;
    font-size: 13px;
    cursor: pointer;
  }
  .name span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .head .btn {
    flex: none;
    width: 28px;
    min-height: 28px;
    font-size: 11px;
  }
  .params {
    display: grid;
    gap: var(--space-2);
    padding: 2px 4px 6px 20px;
  }
</style>
