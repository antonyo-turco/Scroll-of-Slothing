<script lang="ts">
  // Combat (initiative) tracker: a port of the 5etools DM Screen Initiative Tracker.
  // Spec: docs/features/combat-tracker.md. Creatures come from the cached bestiary
  // of the 5etools mirror set in Settings (fivetools_base_url).
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import type {
    BestiaryEntry, BestiaryStatus, CombatCatalog, CombatCell, CombatColumn, CombatRow, CombatSort, Tracker,
  } from '../../lib/types';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import Modal from '../../lib/components/Modal.svelte';
  import ViewActions from '../../lib/components/ViewActions.svelte';

  let t = $state<Tracker | null>(null);
  let catalog = $state<CombatCatalog>({ conditions: [], columns: [] });
  let status = $state<BestiaryStatus | null>(null);
  let syncJobId = $state<string | null>(null);
  const syncJob = $derived(syncJobId ? app.jobs.find((j) => j.id === syncJobId) : undefined);
  let openNotes = $state<Set<string>>(new Set());
  let confirmReset = $state(false);

  // ── loading and saving ──
  async function load() {
    try {
      const [tr, cat, st] = await Promise.all([api.combatGet(), api.combatCatalog(), api.bestiaryStatus()]);
      [catalog, status] = [cat, st];
      setTracker(tr);
    } catch (e) {
      app.error('Could not load the combat tracker', e);
    }
  }
  onMount(load);

  // refetch after changes from elsewhere (MCP, a finished sync)
  let seenVersion = app.dataVersion;
  $effect(() => {
    if (app.dataVersion !== seenVersion) {
      seenVersion = app.dataVersion;
      if (!saveTimer) void load();
    }
  });
  $effect(() => {
    const job = syncJob;
    if (!job || job.state === 'running') return;
    syncJobId = null;
    if (job.state === 'done') {
      app.toast('success', 'Bestiary synced', `${(job.result as { creatures?: number } | null)?.creatures ?? 0} creatures`);
      void api.bestiaryStatus().then((s) => (status = s));
    } else if (job.state === 'failed') app.error('Could not sync the bestiary', job.error);
  });

  /** Every row gets a cell per column, so inputs can bind to them (the
   *  service fills new columns' cells; this only covers old data). */
  function setTracker(x: Tracker) {
    for (const r of x.rows) {
      for (const c of x.stats_cols) if (!r.cells.some((cell) => cell.id === c.id)) r.cells.push({ id: c.id, value: null });
    }
    t = x;
  }

  async function op(p: Promise<Tracker>, what = 'Could not update the tracker') {
    try {
      setTracker(await p);
    } catch (e) {
      app.error(what, e);
    }
  }

  // field edits are saved as a whole tracker, debounced
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  function commit() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      saveTimer = null;
      if (t) await op(api.combatSet($state.snapshot(t) as Tracker), 'Could not save the tracker');
    }, 250);
  }

  // ── rows ──
  const locked = $derived(!!t?.is_locked);
  const similar = (r: CombatRow) =>
    t?.rows.filter((x) => x.source && x.name === r.name && x.source === r.source && x.custom_name === r.custom_name).length ?? 0;
  const label = (r: CombatRow) => {
    const n = r.custom_name || r.display_name || r.name || (r.source ? 'Creature' : 'Player');
    return r.source && similar(r) > 1 ? `${n} (${r.ordinal})` : n;
  };

  /** 5etools getWoundLevel (inverted when HP is tracked as damage). */
  function wound(r: CombatRow): { text: string; color: string } {
    if (r.hp_current === null || r.hp_max === null || r.hp_max <= 0) return { text: 'Unknown', color: '#a5a5a5' };
    const cur = t?.is_invert_wound_direction ? r.hp_max - r.hp_current : r.hp_current;
    const pct = Math.round(Math.min(100, Math.max(0, (cur / r.hp_max) * 100)));
    if (pct === 100) return { text: 'Healthy', color: '#00bb20' };
    if (pct > 50) return { text: 'Hurt', color: '#c5ca00' };
    if (pct > 0) return { text: 'Bloodied', color: '#f7a100' };
    return { text: 'Defeated', color: '#cc0000' };
  }

  /** HP entry: a number sets it, "+5" / "-7" adds or subtracts (5etools). */
  function setHp(r: CombatRow, field: 'hp_current' | 'hp_max', raw: string) {
    const s = raw.trim();
    if (!s) r[field] = null;
    else if (/^[+-]\d+$/.test(s)) r[field] = (r[field] ?? 0) + Number(s);
    else if (/^\d+$/.test(s)) r[field] = Number(s);
    commit();
  }

  function setInit(r: CombatRow, raw: string) {
    r.initiative = raw.trim() === '' ? null : Math.round(Number(raw)) || 0;
    commit();
  }

  const cellFor = (r: CombatRow, c: CombatColumn): CombatCell => r.cells.find((x) => x.id === c.id) ?? { id: c.id, value: null };
  const isCheckbox = (c: CombatColumn) => c.populate_with.startsWith('cb');

  function toggleNotes(id: string) {
    const s = new Set(openNotes);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    openNotes = s;
  }

  // ── add creature ──
  let addOpen = $state(false);
  let query = $state('');
  let hits = $state<BestiaryEntry[]>([]);
  let pick = $state<BestiaryEntry | null>(null);
  let count = $state(1);
  let rollHp = $state(false);
  let customName = $state('');
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  function openAdd() {
    addOpen = true;
    rollHp = !!t?.is_roll_hp;
    void search();
  }
  function search() {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(async () => {
      try {
        hits = await api.bestiarySearch(query, 40);
        if (pick && !hits.some((h) => h.hash === pick!.hash)) pick = null;
      } catch (e) {
        app.error('Search failed', e);
      }
    }, 120);
  }
  async function addCreature() {
    if (!pick) return;
    await op(api.combatAddCreature(pick.hash, Math.max(1, Math.min(100, count || 1)), rollHp, customName.trim() || null), 'Could not add the creature');
    addOpen = false;
    [pick, count, customName] = [null, 1, ''];
  }

  // ── conditions ──
  let condRow = $state<CombatRow | null>(null);
  let condTurns = $state<number | null>(null);
  let customCond = $state({ name: '', color: '#8a2be2', turns: null as number | null });
  const condOpen = {
    get value() {
      return condRow !== null;
    },
    set value(v: boolean) {
      if (!v) condRow = null;
    },
  };
  async function addCondition(name: string, color: string, turns: number | null) {
    if (!condRow) return;
    await op(api.combatAddCondition(condRow.id, name, color, turns && turns > 0 ? turns : null), 'Could not add the condition');
    condRow = null;
  }
  function removeCondition(r: CombatRow, id: string) {
    void op(api.combatUpdateRow(r.id, { remove_conditions: [id] }));
  }
  function saveCustomCondition() {
    if (!t || !customCond.name.trim()) return;
    t.conditions_custom.push({ id: crypto.randomUUID(), name: customCond.name.trim(), color: customCond.color, turns: customCond.turns || null });
    customCond = { name: '', color: '#8a2be2', turns: null };
    commit();
  }

  // ── columns ──
  function addColumn(choice: string) {
    if (!t || !choice) return;
    const populate = choice === '__custom' ? '' : choice;
    const kind = catalog.columns.find((k) => k.populate_with === populate);
    const col: CombatColumn = { id: crypto.randomUUID(), is_editable: true, is_player_visible: 0, populate_with: populate, abbreviation: kind?.abbreviation ?? '' };
    // the service fills each row's cell from its stat block
    t.stats_cols.push(col);
    t.is_stats_add_columns = true;
    commit();
  }
  function moveColumn(i: number, d: number) {
    if (!t || i + d < 0 || i + d >= t.stats_cols.length) return;
    const cols = t.stats_cols;
    [cols[i], cols[i + d]] = [cols[i + d], cols[i]];
    commit();
  }
  function removeColumn(id: string) {
    if (!t) return;
    t.stats_cols = t.stats_cols.filter((c) => c.id !== id);
    for (const r of t.rows) r.cells = r.cells.filter((c) => c.id !== id);
    commit();
  }

  // ── links ──
  let linkOpen = $state(false);
  let linkText = $state('');
  async function copyLink() {
    try {
      await navigator.clipboard.writeText(await api.combatLink());
      app.toast('success', 'Combat link copied', 'Open it on the 5etools DM Screen, or here with “Open link”.');
    } catch (e) {
      app.error('Could not copy the combat link', e);
    }
  }
  async function openLink() {
    try {
      const r = await api.combatImportLink(linkText);
      setTracker(r.tracker);
      linkOpen = false;
      linkText = '';
      if (r.encounter) {
        const miss = r.encounter.missing.length ? ` Not in the bestiary cache: ${r.encounter.missing.join(', ')}.` : '';
        app.toast(r.encounter.missing.length ? 'error' : 'success', `Added ${r.encounter.rows_added} row(s) from the encounter.${miss}`);
      } else app.toast('success', 'Combat loaded');
    } catch (e) {
      app.error('Could not open the link', e);
    }
  }

  async function sync() {
    try {
      syncJobId = await api.bestiarySync();
    } catch (e) {
      app.error('Could not sync the bestiary', e);
    }
  }

  function saveDefaultParty() {
    if (!t) return;
    t.rows_default_party = $state.snapshot(t.rows.filter((r) => !r.source)) as CombatRow[];
    for (const r of t.rows_default_party) r.is_active = false;
    commit();
    app.toast('success', `Default party: ${t.rows_default_party.length} player(s)`);
  }

  const SETTINGS: [keyof Tracker, string][] = [
    ['is_roll_init', 'Roll initiative'],
    ['is_roll_hp', 'Roll hit points'],
    ['is_roll_groups', 'Roll groups of creatures together'],
    ['is_reroll_initiative_each_round', 'Reroll initiative each round'],
    ['is_invert_wound_direction', 'Track hit points as damage'],
  ];
  const IMPORT: [keyof Tracker, string][] = [
    ['import_is_roll_groups', 'Roll groups of creatures together'],
    ['import_is_add_players', 'Add players'],
    ['import_is_append', 'Add to the existing tracker'],
  ];
  function toggle(k: keyof Tracker, v: boolean) {
    if (!t) return;
    (t as unknown as Record<string, unknown>)[k] = v;
    commit();
  }
  const active = $derived(t?.rows.filter((r) => r.is_active) ?? []);
  const sortLabel = (s: CombatSort) => (t?.sort === s ? (t.dir === 'ASC' ? ' ↑' : ' ↓') : '');
</script>

<ViewActions>
  <button class="btn" onclick={() => (linkOpen = true)} disabled={locked} title="Open a 5etools #combat= or #encounter= link">Open link…</button>
  <button class="btn" onclick={copyLink} title="Copy a 5etools DM Screen link that restores this combat">Copy combat link</button>
  <button class="btn primary" onclick={() => op(api.combatNextTurn())} disabled={!t?.rows.length}>Next turn</button>
</ViewActions>

<div class="view">
  <section class="work" aria-label="Combat tracker">
    <div class="view-toolbar">
      <button class="btn" onclick={() => op(api.combatPrevTurn())} disabled={!t?.rows.length} aria-label="Previous turn">◀ Prev</button>
      <button class="btn" onclick={() => op(api.combatNextTurn())} disabled={!t?.rows.length} aria-label="Next turn">Next ▶</button>
      {#if t}
        <label class="round">Round
          <input class="input" type="number" min="1" aria-label="Round" bind:value={t.round} onchange={() => { t!.round = Math.max(1, t!.round || 1); commit(); }} />
        </label>
      {/if}
      <span class="sep"></span>
      <div class="segmented" role="group" aria-label="Sort">
        <button aria-pressed={t?.sort === 'NUMBER'} onclick={() => op(api.combatSort('NUMBER'))} title="Sort by initiative; again to flip">Initiative{sortLabel('NUMBER')}</button>
        <button aria-pressed={t?.sort === 'ALPHA'} onclick={() => op(api.combatSort('ALPHA'))} title="Sort by name; again to flip">Name{sortLabel('ALPHA')}</button>
      </div>
      <span class="sep"></span>
      <button class="btn" onclick={() => op(api.combatAddPlayer())} disabled={locked}>+ Player</button>
      <button class="btn" onclick={openAdd} disabled={locked}>+ Creature…</button>
      <span class="grow"></span>
      {#if t}
        <button class="btn" aria-pressed={t.is_notes_shown} onclick={() => { t!.is_notes_shown = !t!.is_notes_shown; commit(); }}>Encounter notes</button>
        <button class="btn" class:danger={locked} aria-pressed={locked} onclick={() => { t!.is_locked = !t!.is_locked; commit(); }}
          title="While locked, rows can't be added, removed or edited">{locked ? '🔒 Locked' : '🔓 Lock'}</button>
        {#if confirmReset}
          <button class="btn danger" onclick={() => op(api.combatReset()).then(() => (confirmReset = false))}>Reset</button>
          <button class="btn" onclick={() => (confirmReset = false)}>Cancel</button>
        {:else}
          <button class="btn ghost" disabled={locked} onclick={() => (confirmReset = true)} title="Back to the default party at round 1">Reset…</button>
        {/if}
      {/if}
    </div>

    <div class="stage-area stage">
      {#if t?.is_notes_shown}
        <textarea class="input enc-notes" aria-label="Encounter notes" placeholder="Encounter notes" bind:value={t.notes} oninput={commit} disabled={locked}></textarea>
      {/if}
      {#if !t}
        <div class="empty-state"><span>Loading…</span></div>
      {:else if !t.rows.length}
        <div class="empty-state">
          <strong>No combatants yet</strong>
          <span>Add players and creatures, or open a 5etools combat or encounter link.</span>
        </div>
      {:else}
        <table class="tracker" data-testid="combat-table">
          <thead>
            <tr>
              <th class="name">Name</th>
              <th>Conditions</th>
              {#each t.stats_cols as c (c.id)}<th title={catalog.columns.find((k) => k.populate_with === c.populate_with)?.name}>{c.abbreviation || '·'}</th>{/each}
              <th>HP</th>
              <th>Init</th>
              <th><span class="sr">Actions</span></th>
            </tr>
          </thead>
          <tbody>
            {#each t.rows as r (r.id)}
              {@const w = wound(r)}
              <tr class:active={r.is_active} class:defeated={w.text === 'Defeated'} data-testid="combat-row">
                <td class="name">
                  {#if r.source}
                    <span class="cname" title="{r.name} ({r.source})">{label(r)}</span>
                    <span class="src">{r.source}</span>
                  {:else}
                    <input class="input" aria-label="Player name" placeholder="Player" bind:value={r.name} onchange={commit} disabled={locked} />
                  {/if}
                </td>
                <td class="conds">
                  {#each r.conditions as c (c.id)}
                    <button class="chip" style="--c: {c.color}" aria-label="Remove {c.name}" title="Remove {c.name}" onclick={() => removeCondition(r, c.id)} disabled={locked}>
                      {c.name}{c.turns !== null ? ` (${c.turns})` : ''}
                    </button>
                  {/each}
                  <button class="btn ghost icon add-cond" aria-label="Add a condition to {label(r)}" onclick={() => { condRow = r; condTurns = null; }} disabled={locked}>+</button>
                </td>
                {#each t.stats_cols as c (c.id)}
                  {@const cell = cellFor(r, c)}
                  <td class="stat">
                    {#if isCheckbox(c)}
                      <input type="checkbox" aria-label="{c.abbreviation || 'Checkbox'} for {label(r)}" checked={!!cell.value}
                        onchange={(e) => { cell.value = e.currentTarget.checked; commit(); }} disabled={locked} />
                    {:else if c.populate_with === 'legendaryActions'}
                      <span class="pair"><input class="input num" aria-label="Legendary actions left" bind:value={cell.current} onchange={commit} disabled={locked} />/{cell.max ?? '–'}</span>
                    {:else if c.populate_with === 'image'}
                      {#if cell.tokenUrl}<img class="token" src={cell.tokenUrl} alt="" />{/if}
                    {:else}
                      <input class="input" aria-label="{c.abbreviation || 'Value'} for {label(r)}" bind:value={cell.value} onchange={commit} disabled={locked || !c.is_editable} />
                    {/if}
                  </td>
                {/each}
                <td class="hp" style="--w: {w.color}" title={w.text}>
                  <input class="input num" aria-label="Current HP of {label(r)}" value={r.hp_current ?? ''} onchange={(e) => setHp(r, 'hp_current', e.currentTarget.value)} disabled={locked} />
                  <span>/</span>
                  <input class="input num" aria-label="Max HP of {label(r)}" value={r.hp_max ?? ''} onchange={(e) => setHp(r, 'hp_max', e.currentTarget.value)} disabled={locked} />
                </td>
                <td><input class="input num" aria-label="Initiative of {label(r)}" value={r.initiative ?? ''} onchange={(e) => setInit(r, e.currentTarget.value)} disabled={locked} /></td>
                <td class="acts">
                  <button class="btn ghost icon" class:on={!!r.notes} aria-pressed={openNotes.has(r.id)} aria-label="Notes for {label(r)}" title="Notes" onclick={() => toggleNotes(r.id)}>✎</button>
                  <button class="btn ghost icon" aria-pressed={r.is_player_visible} aria-label="Shown to players: {label(r)}" title={r.is_player_visible ? 'Shown to players' : 'Hidden from players'}
                    onclick={() => { r.is_player_visible = !r.is_player_visible; commit(); }} disabled={locked}>{r.is_player_visible ? '👁' : '◌'}</button>
                  {#if r.source}
                    <button class="btn ghost icon" aria-label="Add another {r.name}" title="Add another (Shift: reroll initiative and HP)" disabled={locked}
                      onclick={(e) => op(api.combatDuplicate(r.id, e.shiftKey))}>⧉</button>
                  {/if}
                  <button class="btn ghost icon danger" aria-label="Delete {label(r)}" title="Delete (Shift: every copy)" disabled={locked}
                    onclick={(e) => op(api.combatDelete(r.id, e.shiftKey))}>✕</button>
                </td>
              </tr>
              {#if openNotes.has(r.id)}
                <tr class="notes-row">
                  <td colspan={6 + t.stats_cols.length}>
                    <textarea class="input" aria-label="Notes for {label(r)}" bind:value={r.notes} oninput={commit} disabled={locked}></textarea>
                  </td>
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
    <div class="statusbar" data-testid="combat-status">
      <span>Round {t?.round ?? 1}</span>
      <span>{active.length ? `Turn: ${active.map(label).join(', ')}` : t?.rows.length ? 'Press Next turn to start' : ''}</span>
      <span>{t?.rows.length ?? 0} combatant(s)</span>
    </div>
  </section>

  <aside class="inspector" aria-label="Tracker settings">
    {#if t}
      <section class="section">
        <h3 class="section-title">Settings</h3>
        {#each SETTINGS as [k, l] (k)}
          <label class="check"><input type="checkbox" checked={!!t[k]} onchange={(e) => toggle(k, e.currentTarget.checked)} /> {l}</label>
        {/each}
      </section>

      <section class="section">
        <h3 class="section-title">Import <InfoTip label="About importing" text="How 5etools encounter links are rolled into the tracker." /></h3>
        {#each IMPORT as [k, l] (k)}
          <label class="check"><input type="checkbox" checked={!!t[k]} onchange={(e) => toggle(k, e.currentTarget.checked)} /> {l}</label>
        {/each}
      </section>

      <section class="section">
        <h3 class="section-title">Stat columns</h3>
        {#each t.stats_cols as c, i (c.id)}
          <div class="row nowrap col-row">
            <input class="input abv" aria-label="Column label" bind:value={c.abbreviation} onchange={commit} />
            <span class="muted grow kind">{catalog.columns.find((k) => k.populate_with === c.populate_with)?.name ?? c.populate_with}</span>
            <button class="btn ghost icon" aria-label="Move column up" disabled={i === 0} onclick={() => moveColumn(i, -1)}>↑</button>
            <button class="btn ghost icon" aria-label="Move column down" disabled={i === t.stats_cols.length - 1} onclick={() => moveColumn(i, 1)}>↓</button>
            <button class="btn ghost icon danger" aria-label="Remove column {c.abbreviation}" onclick={() => removeColumn(c.id)}>✕</button>
          </div>
        {/each}
        <select class="input" aria-label="Add a column" value="" onchange={(e) => { addColumn(e.currentTarget.value); e.currentTarget.value = ''; }}>
          <option value="" disabled>+ Add a column…</option>
          {#each catalog.columns as k (k.populate_with)}<option value={k.populate_with || '__custom'}>{k.name}</option>{/each}
        </select>
      </section>

      <section class="section">
        <h3 class="section-title">Default party <InfoTip label="About the default party" text="Players put back on Reset and added when importing an encounter." /></h3>
        <p class="note">{t.rows_default_party.length ? t.rows_default_party.map((r) => r.name || 'Player').join(', ') : 'None yet.'}</p>
        <button class="btn" onclick={saveDefaultParty} disabled={!t.rows.some((r) => !r.source)}>Use the current players</button>
      </section>

      <section class="section">
        <h3 class="section-title">Custom conditions</h3>
        {#each t.conditions_custom as c (c.id)}
          <div class="row nowrap">
            <span class="chip" style="--c: {c.color}">{c.name}{c.turns ? ` (${c.turns})` : ''}</span>
            <span class="grow"></span>
            <button class="btn ghost icon danger" aria-label="Delete condition {c.name}" onclick={() => { t!.conditions_custom = t!.conditions_custom.filter((x) => x.id !== c.id); commit(); }}>✕</button>
          </div>
        {/each}
        <div class="row nowrap">
          <input class="input grow" aria-label="Condition name" placeholder="Name" bind:value={customCond.name} />
          <input type="color" class="swatch" aria-label="Condition colour" bind:value={customCond.color} />
          <input class="input num" type="number" min="1" aria-label="Condition turns" placeholder="∞" bind:value={customCond.turns} />
          <button class="btn" onclick={saveCustomCondition} disabled={!customCond.name.trim()}>Save</button>
        </div>
      </section>

      <section class="section">
        <h3 class="section-title">Bestiary <InfoTip label="About the bestiary" text="Creatures are read from the 5etools mirror set in Settings (e.g. your local copy) and cached, so search works offline. Sync again after updating the mirror." /></h3>
        <p class="note" data-testid="bestiary-status">
          {#if status?.synced}{status.creatures} creatures from {status.files} file(s).{:else}Not synced yet.{/if}
          <br /><span class="muted mono">{status?.base_url}</span>
        </p>
        <button class="btn" onclick={sync} disabled={!!syncJobId}>{syncJobId ? `Syncing… ${Math.round((syncJob?.progress ?? 0) * 100)}%` : 'Sync from mirror'}</button>
        <button class="btn ghost" onclick={() => app.navigate('settings')}>Change mirror…</button>
      </section>
    {/if}
  </aside>
</div>

<Modal title="Add creature" bind:open={addOpen} width={560}>
  <div class="field">
    <label for="combat-search">Search the bestiary</label>
    <input id="combat-search" class="input" placeholder="Goblin, dragon, …" bind:value={query} oninput={search} />
  </div>
  {#if !status?.synced}
    <p class="note warn">The bestiary is not synced. <button class="btn ghost" onclick={sync}>Sync from {status?.base_url}</button></p>
  {/if}
  <ul class="hits" role="listbox" aria-label="Creatures">
    {#each hits as h (h.hash)}
      <li>
        <button role="option" aria-selected={pick?.hash === h.hash} onclick={() => (pick = h)} ondblclick={addCreature}>
          <span class="grow">{h.name}</span>
          <span class="muted">{h.source} · CR {h.cr ?? '—'} · HP {h.hp ?? '—'}</span>
        </button>
      </li>
    {:else}
      <li class="muted">{status?.synced ? 'No matches.' : ''}</li>
    {/each}
  </ul>
  <div class="row nowrap">
    <label class="field small">Count <input class="input num" type="number" min="1" max="100" bind:value={count} /></label>
    <label class="field grow">Custom name <input class="input" placeholder={pick?.name ?? ''} bind:value={customName} /></label>
  </div>
  <label class="check"><input type="checkbox" bind:checked={rollHp} /> Roll hit points (otherwise the average)</label>
  {#snippet footer()}
    <button class="btn primary" disabled={!pick} onclick={addCreature}>Add {count > 1 ? `${count} × ` : ''}{pick?.name ?? 'creature'}</button>
  {/snippet}
</Modal>

<Modal title="Add condition{condRow ? ` to ${label(condRow)}` : ''}" bind:open={condOpen.value} width={480}>
  <label class="field">Turns <span class="muted">(empty: until removed)</span>
    <input class="input num" type="number" min="1" aria-label="Turns" bind:value={condTurns} />
  </label>
  <div class="cond-grid">
    {#each catalog.conditions as [name, color] (name)}
      <button class="chip" style="--c: {color}" onclick={() => addCondition(name, color, condTurns)}>{name}</button>
    {/each}
    {#each t?.conditions_custom ?? [] as c (c.id)}
      <button class="chip" style="--c: {c.color}" onclick={() => addCondition(c.name, c.color, condTurns ?? c.turns)}>{c.name}</button>
    {/each}
  </div>
</Modal>

<Modal title="Open a 5etools link" bind:open={linkOpen} width={560}>
  <div class="field">
    <label for="combat-link">Combat or encounter link</label>
    <textarea id="combat-link" class="input" rows="4" placeholder="http://localhost:8080/dmscreen.html#combat=…" bind:value={linkText}></textarea>
  </div>
  <p class="note">A combat link replaces the tracker. An encounter link (Bestiary → Copy Combat Link) is rolled in with the Import settings.</p>
  {#snippet footer()}
    <button class="btn primary" disabled={!linkText.trim()} onclick={openLink}>Open</button>
  {/snippet}
</Modal>

<style>
  .stage {
    overflow: auto;
    padding: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .round {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-muted);
  }
  .round .input {
    width: 56px;
  }
  .sep {
    width: 1px;
    align-self: stretch;
    background: var(--border);
  }
  .grow {
    flex: 1;
  }
  .enc-notes {
    min-height: 64px;
    resize: vertical;
  }
  .tracker {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  .tracker th {
    text-align: left;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }
  .tracker td {
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  .tracker tr.active td {
    background: var(--accent-soft);
  }
  .tracker tr.active td:first-child {
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .tracker tr.defeated .cname {
    text-decoration: line-through;
    color: var(--text-muted);
  }
  .name {
    min-width: 160px;
  }
  .cname {
    font-weight: 600;
  }
  .src {
    margin-left: 6px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .conds {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
    min-width: 120px;
  }
  .chip {
    border: 1px solid var(--c);
    background: color-mix(in srgb, var(--c) 22%, transparent);
    color: var(--text);
    border-radius: 999px;
    padding: 1px 8px;
    font-size: 11px;
    cursor: pointer;
    white-space: nowrap;
  }
  .stat .input {
    width: 56px;
  }
  .num {
    width: 52px;
    text-align: center;
  }
  .hp {
    white-space: nowrap;
    box-shadow: inset 0 -3px 0 var(--w);
  }
  .pair {
    white-space: nowrap;
  }
  .token {
    width: 28px;
    height: 28px;
    border-radius: 50%;
  }
  .acts {
    white-space: nowrap;
    text-align: right;
  }
  .acts .on {
    color: var(--accent);
  }
  .notes-row textarea {
    width: 100%;
    min-height: 48px;
    resize: vertical;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  .col-row .abv {
    width: 64px;
  }
  .kind {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .nowrap {
    flex-wrap: nowrap;
  }
  .swatch {
    width: 32px;
    height: 28px;
    padding: 0;
    border: 1px solid var(--border);
    background: none;
  }
  .hits {
    list-style: none;
    margin: var(--space-2) 0;
    padding: 0;
    max-height: 260px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .hits li.muted {
    padding: 8px;
  }
  .hits button {
    width: 100%;
    display: flex;
    gap: 8px;
    padding: 6px 8px;
    border: 0;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    text-align: left;
    font-size: 13px;
  }
  .hits button:hover,
  .hits button[aria-selected='true'] {
    background: var(--accent-soft);
  }
  .field {
    font-size: 12px;
  }
  .field.small .input {
    width: 64px;
  }
  .cond-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: var(--space-3);
  }
  .cond-grid .chip {
    font-size: 12px;
    padding: 4px 10px;
  }
</style>
