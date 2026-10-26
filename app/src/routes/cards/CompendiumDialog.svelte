<script lang="ts">
  // Spells & items: search the synced 5etools spells and items, tick the ones
  // you want and add them to the deck as cards. Spec: docs/features/spell-item-cards.md
  import { api } from '../../lib/api';
  import Modal from '../../lib/components/Modal.svelte';
  import { CLASS_NAMES } from '../../lib/dnd-classes';
  import { app } from '../../lib/state.svelte';
  import type { CompendiumEntry, CompendiumKind, CompendiumRef, CompendiumStatus } from '../../lib/types';

  let { open = $bindable(false), onAdd }: { open: boolean; onAdd: (markdown: string, count: number) => void } = $props();

  const SPELL_CLASSES = ['Artificer', 'Bard', 'Cleric', 'Druid', 'Paladin', 'Ranger', 'Sorcerer', 'Warlock', 'Wizard'].filter(
    (c) => CLASS_NAMES.includes(c),
  );
  const LEVELS: [string, string][] = [['', 'Any level'], ['0', 'Cantrip'], ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => [String(n), `Level ${n}`] as [string, string])];
  const RARITIES: [string, string][] = [
    ['', 'Any rarity'], ['none', 'Mundane'], ['common', 'Common'], ['uncommon', 'Uncommon'], ['rare', 'Rare'],
    ['very rare', 'Very rare'], ['legendary', 'Legendary'], ['artifact', 'Artifact'],
  ];
  const LIMIT = 300;

  let status = $state<CompendiumStatus | null>(null);
  let kind = $state<CompendiumKind>('spell');
  let query = $state('');
  let level = $state('');
  let klass = $state('');
  let rarity = $state('');
  let newestOnly = $state(true);
  let hits = $state<CompendiumEntry[]>([]);
  // picked across searches, in the order they were ticked
  let picked = $state<CompendiumEntry[]>([]);
  let focus = $state<CompendiumEntry | null>(null);
  let preview = $state('');
  let syncJobId = $state<string | null>(null);
  const syncJob = $derived(syncJobId ? app.jobs.find((j) => j.id === syncJobId) : undefined);
  const key = (e: CompendiumRef) => `${e.kind}:${e.id}`;
  const pickedKeys = $derived(new Set(picked.map(key)));

  $effect(() => {
    if (!open) return;
    void api.compendiumStatus().then((s) => (status = s)).catch((e) => app.error('Could not read the spell and item cache', e));
  });

  // search again whenever a filter changes (debounced, latest wins)
  let timer: ReturnType<typeof setTimeout> | null = null;
  let gen = 0;
  $effect(() => {
    const q = {
      kind, query, newest_only: newestOnly, limit: LIMIT,
      level: kind === 'spell' && level !== '' ? Number(level) : null,
      class: kind === 'spell' && klass ? klass : null,
      rarity: kind === 'item' && rarity ? rarity : null,
    };
    if (!open || !status?.synced) return;
    if (timer) clearTimeout(timer);
    timer = setTimeout(async () => {
      const g = ++gen;
      try {
        const h = await api.compendiumSearch(q);
        if (g === gen) hits = h;
      } catch (e) {
        app.error('Search failed', e);
      }
    }, 120);
  });

  $effect(() => {
    const job = syncJob;
    if (!job || job.state === 'running') return;
    syncJobId = null;
    if (job.state === 'done') void api.compendiumStatus().then((s) => (status = s));
    else if (job.state === 'failed') app.error('Could not download spells and items', job.error);
  });

  async function sync() {
    try {
      syncJobId = await api.compendiumSync();
    } catch (e) {
      app.error('Could not download spells and items', e);
    }
  }

  function toggle(e: CompendiumEntry) {
    picked = pickedKeys.has(key(e)) ? picked.filter((p) => key(p) !== key(e)) : [...picked, e];
  }

  async function show(e: CompendiumEntry) {
    focus = e;
    try {
      const out = await api.compendiumCards([{ kind: e.kind, id: e.id }]);
      if (focus === e) preview = out.markdown;
    } catch (err) {
      app.error('Could not build the card', err);
    }
  }

  async function add() {
    if (!picked.length) return;
    try {
      const out = await api.compendiumCards(picked.map((p) => ({ kind: p.kind, id: p.id })));
      if (out.missing.length) app.toast('error', `Not in the synced data: ${out.missing.join(', ')}`);
      if (out.cards) onAdd(out.markdown, out.cards);
      picked = [];
      open = false;
    } catch (e) {
      app.error('Could not build the cards', e);
    }
  }
</script>

<Modal title="Spells and items" bind:open width={1100}>
  {#if status && !status.synced}
    <div class="empty-state sync">
      <strong>Spells and items aren’t downloaded yet</strong>
      <span>They come from the 5etools mirror at {status.base_url} (Settings → Combat). Download them once; search then works offline.</span>
      <button class="btn primary" onclick={sync} disabled={!!syncJobId}>
        {syncJobId ? `Downloading… ${Math.round((syncJob?.progress ?? 0) * 100)}%` : 'Download spells and items'}
      </button>
    </div>
  {:else}
    <div class="picker">
      <div class="pick-left">
        <div class="pick-filters">
          <div class="segmented" role="group" aria-label="Spells or items">
            <button aria-pressed={kind === 'spell'} onclick={() => (kind = 'spell')}>Spells</button>
            <button aria-pressed={kind === 'item'} onclick={() => (kind = 'item')}>Items</button>
          </div>
          <input class="input grow" type="search" aria-label="Search by name" placeholder={kind === 'spell' ? 'Search spells…' : 'Search items…'} bind:value={query} />
        </div>
        <div class="pick-filters">
          {#if kind === 'spell'}
            <select class="input" aria-label="Spell level" bind:value={level}>
              {#each LEVELS as [v, l] (v)}<option value={v}>{l}</option>{/each}
            </select>
            <select class="input" aria-label="Class" bind:value={klass}>
              <option value="">Any class</option>
              {#each SPELL_CLASSES as c (c)}<option value={c}>{c}</option>{/each}
            </select>
          {:else}
            <select class="input" aria-label="Rarity" bind:value={rarity}>
              {#each RARITIES as [v, l] (v)}<option value={v}>{l}</option>{/each}
            </select>
          {/if}
          <label class="check nowrap" title="Leave out entries reprinted in a newer book, such as PHB spells that are in the 2024 Player’s Handbook">
            <input type="checkbox" bind:checked={newestOnly} /> Newest printings only
          </label>
        </div>
        <div class="list-box" role="listbox" aria-label={kind === 'spell' ? 'Spells' : 'Items'} aria-multiselectable="true">
          {#each hits as h (key(h))}
            <div class="list-row pick-row" class:selected={focus === h}>
              <input type="checkbox" aria-label="Add {h.name} ({h.source})" checked={pickedKeys.has(key(h))} onchange={() => toggle(h)} />
              <button class="row-btn" role="option" aria-selected={pickedKeys.has(key(h))} onclick={() => show(h)} ondblclick={() => toggle(h)}>
                <span class="list-title">{h.name}</span>
                <span class="list-sub">{h.detail}{h.concentration ? ' · concentration' : ''}</span>
              </button>
              <span class="list-pill">{h.source}</span>
            </div>
          {:else}
            <p class="muted pad">{status ? 'No matches.' : 'Loading…'}</p>
          {/each}
          {#if hits.length >= LIMIT}<p class="muted pad">Showing the first {LIMIT}; type more of the name to narrow it down.</p>{/if}
        </div>
      </div>
      <div class="pick-right">
        {#if focus}
          <h3 class="prev-title">{focus.name} <span class="muted">({focus.source})</span></h3>
          <pre class="prev">{preview}</pre>
        {:else}
          <p class="muted pad">Click a {kind} to see its card text. Tick the ones to add; picks are kept when you search again.</p>
        {/if}
      </div>
    </div>
  {/if}
  {#snippet footer()}
    <span class="muted grow">
      {#if picked.length}{picked.length} picked: {picked.slice(0, 4).map((p) => p.name).join(', ')}{picked.length > 4 ? '…' : ''}{/if}
    </span>
    {#if picked.length}<button class="btn" onclick={() => (picked = [])}>Clear</button>{/if}
    <button class="btn primary" disabled={!picked.length} onclick={add}>Add {picked.length || ''} card{picked.length === 1 ? '' : 's'}</button>
  {/snippet}
</Modal>

<style>
  .picker {
    display: grid;
    grid-template-columns: minmax(0, 1.1fr) minmax(0, 1fr);
    gap: var(--space-4);
    height: 62vh;
  }
  .pick-left {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-height: 0;
  }
  .pick-filters {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .pick-filters select {
    width: auto;
  }
  .pick-row {
    grid-template-columns: 28px minmax(0, 1fr) auto;
    align-items: center;
  }
  .row-btn {
    display: grid;
    border: 0;
    padding: 0;
    background: none;
    color: var(--text);
    text-align: left;
    font: inherit;
    cursor: pointer;
    min-width: 0;
  }
  .pick-right {
    overflow: auto;
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  .prev-title {
    margin: 0 0 var(--space-2);
  }
  .prev {
    white-space: pre-wrap;
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    line-height: 1.5;
    margin: 0;
  }
  .pad {
    padding: var(--space-3);
  }
  .grow {
    flex: 1;
  }
  .nowrap {
    white-space: nowrap;
  }
  .sync {
    display: grid;
    gap: var(--space-3);
    justify-items: start;
    padding: var(--space-4);
  }
</style>
