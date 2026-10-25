<script lang="ts">
  // Add creature: search the cached bestiary with CR and source filters, a stat
  // block preview, count, custom name, side, rolled HP and a CR to scale to.
  import { api } from '../../lib/api';
  import Modal from '../../lib/components/Modal.svelte';
  import { CRS, crOfMon } from '../../lib/scale';
  import { app } from '../../lib/state.svelte';
  import type { BestiaryEntry, BestiaryStatus } from '../../lib/types';
  import type { Mon } from './combat-ui';
  import StatBlock from './StatBlock.svelte';

  type Side = 'enemy' | 'ally' | 'neutral';
  export type AddRequest = { entry: BestiaryEntry; mon: Mon | null; count: number; rollHp: boolean; customName: string | null; side: Side; cr: string };

  let { open = $bindable(false), status, onAdd }: { open: boolean; status: BestiaryStatus | null; onAdd: (q: AddRequest) => Promise<void> } = $props();

  const SIDES: [Side, string][] = [['enemy', 'Enemies'], ['ally', 'Allies'], ['neutral', 'Neutral']];
  const CR_BANDS: [string, string][] = [['low', 'CR 0–1'], ['mid', 'CR 2–5'], ['high', 'CR 6–10'], ['epic', 'CR 11+']];

  let query = $state('');
  let hits = $state<BestiaryEntry[]>([]);
  let pick = $state<BestiaryEntry | null>(null);
  let pickMon = $state<Mon | null>(null);
  let count = $state(1);
  let rollHp = $state(false);
  let customName = $state('');
  let side = $state<Side>('enemy');
  let cr = $state('');
  let crFilter = $state('');
  let srcFilter = $state('');
  let timer: ReturnType<typeof setTimeout> | null = null;

  // fresh options each time it opens
  $effect(() => {
    if (!open) return;
    rollHp = !!app.settings.combat_roll_hp;
    search();
  });
  function search() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(async () => {
      try {
        hits = await api.bestiarySearch(query, 200);
        if (pick && !hits.some((h) => h.hash === pick!.hash)) pick = null;
      } catch (e) {
        app.error('Search failed', e);
      }
    }, 120);
  }
  const crNum = (c: string | null) => {
    if (!c) return NaN;
    const [a, b] = c.split('/').map(Number);
    return b ? a / b : a;
  };
  const sources = $derived([...new Set(hits.map((h) => h.source))].sort());
  const shown = $derived(
    hits.filter((h) => {
      if (srcFilter && h.source !== srcFilter) return false;
      const n = crNum(h.cr);
      if (crFilter === 'low') return n <= 1;
      if (crFilter === 'mid') return n >= 2 && n <= 5;
      if (crFilter === 'high') return n >= 6 && n <= 10;
      if (crFilter === 'epic') return n >= 11;
      return true;
    }),
  );
  function choose(h: BestiaryEntry) {
    pick = h;
    pickMon = null;
    void api.bestiaryGet(h.hash).then((m) => {
      if (pick?.hash === h.hash) pickMon = m;
    });
  }
  async function add() {
    if (!pick) return;
    await onAdd({ entry: pick, mon: pickMon, count: Math.max(1, Math.min(100, count || 1)), rollHp, customName: customName.trim() || null, side, cr });
    open = false;
    [pick, count, customName, cr] = [null, 1, '', ''];
  }
</script>

<Modal title="Add creature" bind:open width={1000}>
  {#if !status?.synced}
    <p class="note warn">The bestiary is not synced: Settings → Combat → Sync from mirror ({status?.base_url}).</p>
  {/if}
  <div class="picker">
    <div class="pick-left">
      <div class="pick-filters">
        <input id="combat-search" class="input grow" type="search" aria-label="Search the bestiary" placeholder="Search creatures…" bind:value={query} oninput={search} />
        <select class="input compact" aria-label="Challenge rating" bind:value={crFilter}>
          <option value="">Any CR</option>
          {#each CR_BANDS as [id, l] (id)}<option value={id}>{l}</option>{/each}
        </select>
        <select class="input compact" aria-label="Source" bind:value={srcFilter}>
          <option value="">Any source</option>
          {#each sources as src (src)}<option value={src}>{src}</option>{/each}
        </select>
      </div>
      <div class="pick-list" role="listbox" aria-label="Creatures">
        {#each shown as h (h.hash)}
          <button class="list-row pick-row" role="option" aria-selected={pick?.hash === h.hash} class:selected={pick?.hash === h.hash}
            onclick={() => choose(h)} ondblclick={add}>
            <span class="list-thumb">{#if h.token_url}<img src={h.token_url} alt="" loading="lazy" />{/if}</span>
            <span class="title-cell"><span class="list-title">{h.name}</span><span class="list-sub">{h.source}</span></span>
            <span class="list-pill">CR {h.cr ?? '—'}</span>
            <span class="list-muted hp">HP {h.hp ?? '—'}</span>
          </button>
        {:else}
          <p class="muted pad">{status?.synced ? 'No matches.' : ''}</p>
        {/each}
      </div>
    </div>
    <div class="pick-right">
      {#if pick}
        <StatBlock mon={pickMon} hero={null} name={pick.name} />
      {:else}
        <p class="muted pad">Pick a creature to see its stat block.</p>
      {/if}
    </div>
  </div>
  {#snippet footer()}
    <div class="pick-foot">
      <span class="stepper" role="group" aria-label="Count">
        <button class="btn ghost icon" aria-label="One fewer" onclick={() => (count = Math.max(1, count - 1))}>−</button>
        <input class="input num" type="number" min="1" max="100" aria-label="Count" bind:value={count} />
        <button class="btn ghost icon" aria-label="One more" onclick={() => (count = Math.min(100, count + 1))}>+</button>
      </span>
      <input class="input" aria-label="Custom name" placeholder="Custom name (optional)" bind:value={customName} />
      <div class="segmented side-pick" role="group" aria-label="Side">
        {#each SIDES as [id, l] (id)}
          <button class="side-{id}" aria-pressed={side === id} onclick={() => (side = id)}>{l}</button>
        {/each}
      </div>
      <label class="check nowrap"><input type="checkbox" bind:checked={rollHp} /> Roll HP</label>
      <label class="cr-pick">CR
        <select class="input compact" aria-label="Scale to CR" bind:value={cr} disabled={!pickMon}>
          <option value="">{pickMon ? `${crOfMon(pickMon) ?? '—'} (as is)` : 'as is'}</option>
          {#each CRS as c (c)}<option value={c}>{c}</option>{/each}
        </select>
      </label>
      <span class="grow"></span>
      <button class="btn primary" disabled={!pick} onclick={add}>Add {count > 1 ? `${count} × ` : ''}{pick?.name ?? 'creature'}</button>
    </div>
  {/snippet}
</Modal>

<style>
  .picker {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--space-4);
    height: 60vh;
  }
  .pick-left {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-height: 0;
  }
  .pick-filters {
    display: flex;
    gap: var(--space-2);
  }
  .pick-list {
    flex: 1;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .pick-row {
    width: 100%;
    grid-template-columns: 40px minmax(0, 1fr) auto 64px;
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
    font: inherit;
  }
  .hp {
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .pick-right {
    overflow: auto;
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  .pick-foot {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
  }
  .side-pick button::before {
    content: '';
    display: inline-block;
    width: 10px;
    height: 10px;
    margin-right: 6px;
    border-radius: 50%;
    vertical-align: -1px;
  }
  .side-pick .side-enemy::before {
    background: #d9381e;
  }
  .side-pick .side-ally::before {
    background: #f2f2f2;
    box-shadow: 0 0 0 1px #888;
  }
  .side-pick .side-neutral::before {
    background: #000;
    box-shadow: 0 0 0 1px #888;
  }
  .stepper {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .num {
    width: 52px;
    text-align: center;
  }
  .pad {
    padding: var(--space-3);
  }
  .title-cell {
    display: grid;
  }
  .nowrap {
    white-space: nowrap;
    flex: none;
  }
  .cr-pick {
    white-space: nowrap;
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 14px;
  }
  .grow {
    flex: 1;
  }
</style>
