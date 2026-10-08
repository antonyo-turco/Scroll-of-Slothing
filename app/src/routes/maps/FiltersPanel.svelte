<script lang="ts">
  // Map filter pipeline editor (docs/features/map-filters.md): preset dropdown,
  // per-filter toggle + slider(s), drag to reorder, Clear all.
  import type { FilterCatalog, FilterStep } from '../../lib/types';

  let { pipeline = $bindable(), catalog }: { pipeline: FilterStep[]; catalog: FilterCatalog | null } = $props();

  let preset = $state('');
  let dragFrom = $state<number | null>(null);

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
        draggable="true"
        class:on={s.enabled}
        ondragstart={() => (dragFrom = i)}
        ondragover={(e) => e.preventDefault()}
        ondrop={() => drop(i)}
      >
        <div class="head">
          <span class="grip" aria-hidden="true">⋮⋮</span>
          <label class="name">
            <input type="checkbox" checked={s.enabled} onchange={(e) => toggle(i, e.currentTarget.checked)} />
            {d?.label ?? s.type}
          </label>
          <button class="btn icon ghost" aria-label="Move {d?.label ?? s.type} up" onclick={() => move(i, -1)}>▲</button>
          <button class="btn icon ghost" aria-label="Move {d?.label ?? s.type} down" onclick={() => move(i, 1)}>▼</button>
        </div>
        {#if s.enabled}
          {#each d?.params ?? [] as p (p.key)}
            <label class="param">
              <span class="muted">{p.label}</span>
              <input
                type="range"
                min={p.min}
                max={p.max}
                step={p.step}
                value={s.params[p.key] ?? p.default}
                oninput={(e) => setParam(i, p.key, Number(e.currentTarget.value))}
              />
              <span class="val">{s.params[p.key] ?? p.default}</span>
            </label>
          {/each}
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
    border-radius: var(--radius-sm);
    padding: 2px 4px;
  }
  li.on {
    background: var(--surface-2);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .grip {
    cursor: grab;
    color: var(--text-muted, #888);
    font-size: 10px;
  }
  .name {
    flex: 1;
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: 13px;
  }
  .head .btn {
    padding: 0 4px;
    font-size: 9px;
    min-height: 0;
  }
  .param {
    display: grid;
    grid-template-columns: 90px 1fr 34px;
    gap: 6px;
    align-items: center;
    font-size: 12px;
    padding-left: 20px;
  }
  .param input {
    accent-color: var(--accent);
  }
  .val {
    text-align: right;
  }
</style>
