<script lang="ts">
  // Saved parties of heroes (name, species, class, level, HP, AC, …), added
  // to the tracker as player rows. Styled like the music library.
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import Modal from '../../lib/components/Modal.svelte';
  import { app } from '../../lib/state.svelte';
  import { CLASSES, CLASS_NAMES } from '../../lib/dnd-classes';
  import type { Hero, Party, Tracker } from '../../lib/types';

  let { onAdded }: { onAdded: (t: Tracker) => void } = $props();

  let list = $state<Party[]>([]);
  let selected = $state<string | null>(null);

  async function load() {
    try {
      list = await api.partiesList();
    } catch (e) {
      app.error('Could not load parties', e);
    }
  }
  onMount(() => void load());
  let seen = app.dataVersion;
  $effect(() => {
    if (app.dataVersion !== seen) {
      seen = app.dataVersion;
      void load();
    }
  });

  const COLUMNS = '1fr 80px 90px 150px';
  const heroLine = (h: Hero) =>
    [h.name || 'Hero', [h.species, h.class].filter(Boolean).join(' '), h.subclass, h.level ? `lv ${h.level}` : ''].filter(Boolean).join(' · ');
  const avgLevel = (p: Party) => {
    const l = p.heroes.map((h) => h.level).filter((x): x is number => !!x);
    return l.length ? Math.round((l.reduce((a, b) => a + b, 0) / l.length) * 10) / 10 : null;
  };

  async function add(p: Party, asDefault: boolean) {
    try {
      const t = await api.partyAddToTracker(p.id, null, asDefault);
      app.toast('success', asDefault ? `“${p.name}” added and set as the default party` : `“${p.name}” added to the tracker`);
      onAdded(t);
    } catch (e) {
      app.error('Could not add the party', e);
    }
  }

  let confirmDelete = $state<string | null>(null);
  async function remove(p: Party) {
    if (confirmDelete !== p.id) return void (confirmDelete = p.id);
    confirmDelete = null;
    try {
      await api.partyDelete(p.id);
      await load();
    } catch (e) {
      app.error('Could not delete the party', e);
    }
  }

  // ── editor ──
  const blankHero = (): Hero => ({
    id: '', name: '', species: '', class: '', subclass: '', level: null, hp_max: null, ac: null, initiative_bonus: null,
    passive_perception: null, speed: 30, spell_dc: null, player: '', notes: '',
  });
  let editOpen = $state(false);
  let editId = $state<string | null>(null);
  let editName = $state('');
  let editNotes = $state('');
  let heroes = $state<Hero[]>([]);

  function openEditor(p: Party | null) {
    editId = p?.id ?? null;
    editName = p?.name ?? '';
    editNotes = p?.notes ?? '';
    heroes = p ? structuredClone($state.snapshot(p.heroes)).map((h) => ({ ...h, subclass: h.subclass ?? '' })) : [blankHero()];
    custom = {};
    editOpen = true;
  }
  async function save() {
    try {
      const p = await api.partySave(editId, editName, editNotes, $state.snapshot(heroes).filter((h) => h.name.trim()));
      editOpen = false;
      await load();
      selected = p.id;
    } catch (e) {
      app.error('Could not save the party', e);
    }
  }
  /** Rows whose class / subclass is typed by hand ("Custom…"). */
  let custom = $state<Record<string, boolean>>({});
  const isCustomClass = (h: Hero, i: number) => custom[`c${i}`] || (!!h.class && !CLASSES[h.class]);
  const isCustomSub = (h: Hero, i: number) => custom[`s${i}`] || (!!h.subclass && !(CLASSES[h.class] ?? []).includes(h.subclass));
  function pickClass(h: Hero, i: number, v: string) {
    if (v === '__custom') return void (custom = { ...custom, [`c${i}`]: true, [`s${i}`]: true }, (h.class = ''), (h.subclass = ''));
    h.class = v;
    h.subclass = '';
    custom = { ...custom, [`c${i}`]: false, [`s${i}`]: false };
  }
  function pickSub(h: Hero, i: number, v: string) {
    if (v === '__custom') return void ((custom = { ...custom, [`s${i}`]: true }), (h.subclass = ''));
    h.subclass = v;
  }

  /** Number fields: empty is unknown. */
  const num = (v: string) => (v.trim() === '' ? null : Math.round(Number(v)) || 0);

  const SHORT: Record<string, string> = {
    name: 'Name', species: 'Species', class: 'Class', subclass: 'Subclass', lvl: 'Lvl', hp: 'HP', ac: 'AC', init: 'Init', pp: 'PP',
    spd: 'Speed', dc: 'DC', player: 'Player',
  };
  const FIELDS: [keyof Hero, string, 'text' | 'num', string][] = [
    ['name', 'Name', 'text', 'name'],
    ['species', 'Species', 'text', 'species'],
    ['class', 'Class', 'text', 'class'],
    ['subclass', 'Subclass', 'text', 'subclass'],
    ['level', 'Level', 'num', 'lvl'],
    ['hp_max', 'Hit points', 'num', 'hp'],
    ['ac', 'Armor class', 'num', 'ac'],
    ['initiative_bonus', 'Initiative bonus', 'num', 'init'],
    ['passive_perception', 'Passive Perception', 'num', 'pp'],
    ['speed', 'Speed', 'num', 'spd'],
    ['spell_dc', 'Spell save DC', 'num', 'dc'],
    ['player', 'Player', 'text', 'player'],
  ];
</script>

<div class="parties">
  <div class="view-toolbar">
    <span class="muted hint">Heroes join the tracker as player rows with full HP, rolled initiative and their AC, PP, speed and spell DC in matching columns.</span>
    <span class="grow"></span>
    <button class="btn primary" onclick={() => openEditor(null)}>+ New party</button>
  </div>

  <div class="list-wrap">
    {#if list.length}
      <div class="list" role="table" aria-label="Parties">
        <div class="list-head" role="row" style:grid-template-columns={COLUMNS}>
          <span role="columnheader">Party</span>
          <span role="columnheader">Heroes</span>
          <span role="columnheader">Avg. level</span>
          <span role="columnheader"><span class="visually-hidden">Actions</span></span>
        </div>
        {#each list as p (p.id)}
          <div class="list-row" class:selected={selected === p.id} role="row" tabindex="0" style:grid-template-columns={COLUMNS}
            onclick={() => (selected = p.id)} ondblclick={() => openEditor(p)} onkeydown={(e) => e.key === 'Enter' && openEditor(p)}>
            <span role="cell" class="title-cell">
              <span class="list-title">{p.name}</span>
              <span class="list-sub" title={p.heroes.map(heroLine).join('\n')}>{p.heroes.map(heroLine).join(', ') || 'No heroes'}</span>
            </span>
            <span role="cell" class="num">{p.heroes.length}</span>
            <span role="cell" class="num list-muted">{avgLevel(p) ?? '—'}</span>
            <span role="cell" class="actions">
              <button class="btn ghost" title="Add to the tracker" aria-label="Add {p.name} to the tracker" onclick={(e) => (e.stopPropagation(), void add(p, false))}>▶</button>
              <button class="btn ghost" title="Add and make it the default party (restored on Reset)" aria-label="Make {p.name} the default party"
                onclick={(e) => (e.stopPropagation(), void add(p, true))}>⚑</button>
              <button class="btn ghost" title="Edit" aria-label="Edit {p.name}" onclick={(e) => (e.stopPropagation(), openEditor(p))}>✎</button>
              <button class="btn ghost" class:danger={confirmDelete === p.id} title={confirmDelete === p.id ? 'Click again to delete' : 'Delete'} aria-label="Delete {p.name}"
                onclick={(e) => (e.stopPropagation(), void remove(p))}>{confirmDelete === p.id ? 'Delete?' : '✕'}</button>
            </span>
          </div>
        {/each}
      </div>
    {:else}
      <div class="empty-state">
        <strong>No parties yet</strong>
        <span>Save your players' heroes once, then add them to any combat.</span>
        <button class="btn primary" onclick={() => openEditor(null)}>+ New party</button>
      </div>
    {/if}
  </div>
</div>

<Modal title={editId ? 'Edit party' : 'New party'} bind:open={editOpen} width={980}>
  <div class="row nowrap">
    <label class="field grow">Party name <input class="input" bind:value={editName} placeholder="Vox Machina" /></label>
    <label class="field grow">Notes <input class="input" bind:value={editNotes} /></label>
  </div>
  <div class="heroes" role="table" aria-label="Heroes">
    <div class="hrow head" role="row">
      {#each FIELDS as [, label, , cls] (cls)}<span role="columnheader" class={cls} title={label}>{SHORT[cls]}</span>{/each}
      <span role="columnheader"><span class="visually-hidden">Remove</span></span>
    </div>
    {#each heroes as h, i (i)}
      <div class="hrow" role="row">
        {#each FIELDS as [key, label, kind, cls] (cls)}
          <span role="cell" class={cls}>
            {#if key === 'class' && !isCustomClass(h, i)}
              <select class="input" aria-label="Class of hero {i + 1}" value={h.class} onchange={(e) => pickClass(h, i, e.currentTarget.value)}>
                <option value="">—</option>
                {#each CLASS_NAMES as c (c)}<option value={c}>{c}</option>{/each}
                <option value="__custom">Custom…</option>
              </select>
            {:else if key === 'subclass' && !isCustomSub(h, i)}
              <select class="input" aria-label="Subclass of hero {i + 1}" value={h.subclass} disabled={!h.class} onchange={(e) => pickSub(h, i, e.currentTarget.value)}>
                <option value="">—</option>
                {#each CLASSES[h.class] ?? [] as c (c)}<option value={c}>{c}</option>{/each}
                <option value="__custom">Custom…</option>
              </select>
            {:else if kind === 'num'}
              <input class="input" type="number" aria-label="{label} of hero {i + 1}" value={h[key] ?? ''}
                onchange={(e) => ((h as unknown as Record<string, unknown>)[key] = num(e.currentTarget.value))} />
            {:else}
              <input class="input" aria-label="{label} of hero {i + 1}" value={h[key] as string}
                oninput={(e) => ((h as unknown as Record<string, unknown>)[key] = e.currentTarget.value)} />
            {/if}
          </span>
        {/each}
        <span role="cell"><button class="btn ghost icon danger" aria-label="Remove hero {i + 1}" onclick={() => heroes.splice(i, 1)}>✕</button></span>
      </div>
    {/each}
  </div>
  <button class="btn add-hero" onclick={() => heroes.push(blankHero())}>+ Hero</button>
  {#snippet footer()}
    <button class="btn primary" disabled={!editName.trim()} onclick={save}>Save party</button>
  {/snippet}
</Modal>

<style>
  .parties {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .hint {
    font-size: 12px;
  }
  .grow {
    flex: 1;
  }
  .list-wrap {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--space-2) var(--space-4) 0;
  }
  .list-head {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--surface-2);
  }
  .list-row:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .title-cell {
    display: grid;
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .actions {
    white-space: nowrap;
  }
  .field {
    font-size: 12px;
  }
  .nowrap {
    flex-wrap: nowrap;
  }
  .heroes {
    margin-top: var(--space-3);
    max-height: 50vh;
    overflow: auto;
  }
  .hrow {
    display: grid;
    grid-template-columns: 1.3fr 1fr 1.1fr 1.4fr 52px 60px 52px 52px 52px 60px 52px 1fr 32px;
    gap: 6px;
    align-items: center;
    padding: 3px 0;
  }
  .hrow.head {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .hrow .input {
    width: 100%;
    min-width: 0;
  }
  .add-hero {
    margin-top: var(--space-2);
  }
</style>
