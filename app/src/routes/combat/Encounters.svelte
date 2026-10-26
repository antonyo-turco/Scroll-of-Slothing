<script lang="ts">
  // Saved encounters: creature lists built here or imported from 5etools (a
  // Bestiary encounter file or #encounter= link), and combat snapshots.
  // Styled like the music library (.list-row in app.css).
  import { onMount } from 'svelte';
  import { api, isTauri, pickFile, pickSavePath } from '../../lib/api';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import Modal from '../../lib/components/Modal.svelte';
  import { app } from '../../lib/state.svelte';
  import { difficulty, RATING_LABEL, xpOf } from '../../lib/difficulty';
  import { CRS, scaleCreature } from '../../lib/scale';
  import type { BestiaryEntry, CombatImport, EncounterCreature, EncounterSummary, Party } from '../../lib/types';

  let { onStarted }: { onStarted: (r: CombatImport) => void } = $props();

  let list = $state<EncounterSummary[]>([]);
  let query = $state('');
  let selected = $state<string | null>(null);

  async function load() {
    try {
      list = await api.encountersList();
    } catch (e) {
      app.error('Could not load encounters', e);
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

  const shown = $derived(
    list.filter((e) => {
      const q = query.trim().toLowerCase();
      return !q || e.name.toLowerCase().includes(q) || e.lines.some((l) => l.name.toLowerCase().includes(q));
    }),
  );
  const COLUMNS = '1fr 110px 90px 90px 110px 150px 32px';
  const summary = (e: EncounterSummary) =>
    e.lines.map((l) => (l.count > 1 ? `${l.count} × ${l.name}` : l.name)).join(', ') || 'No creatures';

  async function start(e: EncounterSummary, append: boolean | null = null) {
    try {
      const r = await api.encounterStart(e.id, append);
      const miss = r.encounter?.missing.length ? ` Not in the bestiary cache: ${r.encounter.missing.join(', ')}.` : '';
      app.toast(miss ? 'error' : 'success', `Started “${e.name}”.${miss}`);
      onStarted(r);
    } catch (err) {
      app.error('Could not start the encounter', err);
    }
  }

  async function copyLink(e: EncounterSummary) {
    try {
      await navigator.clipboard.writeText(await api.encounterLink(e.id));
      app.toast('success', '5etools link copied', 'Open it on the 5etools DM Screen.');
    } catch (err) {
      app.error('Could not copy the link', err);
    }
  }

  async function togglePin(e: EncounterSummary) {
    try {
      await api.encounterSetPinned(e.id, !e.pinned);
      await load();
    } catch (err) {
      app.error('Could not pin', err);
    }
  }

  let confirmDelete = $state<string | null>(null);
  async function remove(e: EncounterSummary) {
    if (confirmDelete !== e.id) return void (confirmDelete = e.id);
    confirmDelete = null;
    try {
      await api.encounterDelete(e.id);
      await load();
    } catch (err) {
      app.error('Could not delete the encounter', err);
    }
  }

  async function saveCurrent() {
    try {
      await api.encounterSaveCurrent('');
      await load();
      app.toast('success', 'Saved the current combat');
    } catch (e) {
      app.error('Could not save the combat', e);
    }
  }

  // ── import from 5etools ──
  let importOpen = $state(false);
  let importText = $state('');
  async function doImport() {
    try {
      const e = await api.encounterImport(importText);
      importOpen = false;
      importText = '';
      await load();
      selected = e.id;
      const unknown = e.lines.filter((l) => !l.known).length;
      app.toast(unknown ? 'error' : 'success', `Imported “${e.name}”`, unknown ? `${unknown} creature(s) are not in the bestiary cache: sync it in the Tracker tab.` : undefined);
    } catch (err) {
      app.error('Could not import', err);
    }
  }
  /** A 5etools encounter (.json) or this app's .denc file. */
  async function importFile() {
    const p = await pickFile('Choose an encounter file', ['denc', 'json'], 'Encounters');
    if (!p) {
      if (!isTauri()) app.toast('info', 'File dialogs are only available in the desktop app.');
      return;
    }
    try {
      if (p.toLowerCase().endsWith('.denc')) {
        const added = await api.encountersImportDenc(p);
        app.toast('success', `Imported ${added.length} encounter(s)`);
        selected = added[0]?.id ?? null;
      } else {
        const e = await api.encounterImportFile(p);
        selected = e.id;
        app.toast('success', `Imported “${e.name}”`);
      }
      importOpen = false;
      await load();
    } catch (err) {
      app.error('Could not import the file', err);
    }
  }

  /** Saves the selected encounter, or all of them, as a .denc file. */
  async function exportDenc() {
    const one = list.find((e) => e.id === selected);
    const name = (one?.name ?? 'encounters').replace(/[\/:*?"<>|]+/g, ' ').trim() || 'encounters';
    const path = await pickSavePath('Export encounters', `${name}.denc`, ['denc']);
    if (!path) {
      if (!isTauri()) app.toast('info', 'File dialogs are only available in the desktop app.');
      return;
    }
    try {
      const n = await api.encountersExport(one ? [one.id] : null, path);
      app.toast('success', `Exported ${n} encounter(s)`, path);
    } catch (err) {
      app.error('Could not export', err);
    }
  }

  // ── editor ──
  let editOpen = $state(false);
  let editId = $state<string | null>(null);
  let editName = $state('');
  let editNotes = $state('');
  /** A line: `cr` as used for XP (scaled or the bestiary's), `baseCr` the bestiary's. */
  let editCreatures = $state<(EncounterCreature & { name: string; source: string; cr: string | null; baseCr: string | null })[]>([]);
  async function scaleLine(c: (typeof editCreatures)[number], cr: string) {
    if (!cr || cr === c.baseCr) {
      c.stats = null;
      c.cr = c.baseCr;
      return;
    }
    try {
      const base = await api.bestiaryGet(c.hash);
      const o = base ? scaleCreature(base, cr) : null;
      if (!o) return;
      c.stats = { ...(c.stats ?? {}), ...o };
      c.cr = cr;
    } catch (e) {
      app.error('Could not scale', e);
    }
  }
  let search = $state('');
  let hits = $state<BestiaryEntry[]>([]);
  let timer: ReturnType<typeof setTimeout> | null = null;

  function openEditor(e: EncounterSummary | null) {
    editId = e?.id ?? null;
    editName = e?.name ?? '';
    editNotes = e?.notes ?? '';
    editCreatures = (e?.lines ?? []).map((l, i) => {
      const stats = e?.creatures[i]?.stats ?? null;
      const scaled = stats && typeof stats.cr === 'string' ? (stats.cr as string) : null;
      return { hash: l.hash, count: l.count, name: l.name, source: l.source, cr: scaled ?? l.cr, baseCr: l.base_cr ?? l.cr, stats };
    });
    search = '';
    editOpen = true;
    runSearch();
    void loadParties();
  }
  function runSearch() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(async () => {
      try {
        hits = await api.bestiarySearch(search, 100);
      } catch (e) {
        app.error('Search failed', e);
      }
    }, 120);
  }
  function addCreature(h: BestiaryEntry) {
    const c = editCreatures.find((x) => x.hash === h.hash && !x.stats);
    if (c) c.count++;
    else editCreatures.push({ hash: h.hash, count: 1, name: h.name, source: h.source, cr: h.cr, baseCr: h.cr, stats: null });
  }
  async function saveEditor() {
    try {
      const e = await api.encounterSave(editId, editName, editNotes, editCreatures.map(({ hash, count, stats }) => ({ hash, count, stats: stats ?? null })));
      editOpen = false;
      await load();
      selected = e.id;
    } catch (err) {
      app.error('Could not save the encounter', err);
    }
  }

  // ── difficulty for every saved party (the default party first) ──
  let parties = $state<Party[]>([]);
  let defaultNames = $state<string[]>([]);
  async function loadParties() {
    try {
      const [ps, tr] = await Promise.all([api.partiesList(), api.combatGet()]);
      defaultNames = tr.rows_default_party.map((r) => r.name.trim().toLowerCase());
      parties = ps;
    } catch {
      /* the calculator just stays empty */
    }
  }
  const isDefault = (p: Party) =>
    defaultNames.length > 0 && p.heroes.length === defaultNames.length && p.heroes.every((h) => defaultNames.includes(h.name.trim().toLowerCase()));
  const orderedParties = $derived([...parties].sort((a, b) => Number(isDefault(b)) - Number(isDefault(a))));
  const use2014 = $derived(app.settings.combat_xp_multiplier_2014);
  const lineXp = (c: { cr: string | null; count: number }) => (xpOf(c.cr) ?? 0) * c.count;
  const totalXp = $derived(editCreatures.reduce((a, c) => a + lineXp(c), 0));
  const totalCount = $derived(editCreatures.reduce((a, c) => a + c.count, 0));
  const unknownXp = $derived(editCreatures.some((c) => xpOf(c.cr) === null));
  const rate = (p: Party) =>
    difficulty(editCreatures.map((c) => ({ xp: xpOf(c.cr), count: c.count })), p.heroes.map((h) => h.level ?? 1), use2014);
</script>

<div class="enc">
  <div class="view-toolbar">
    <input class="input search" type="search" placeholder="Search encounters" aria-label="Search encounters" bind:value={query} />
    <span class="grow"></span>
    <button class="btn" onclick={saveCurrent} title="Save the tracker as it is now (rows, HP, conditions, round)">Save current combat</button>
    <button class="btn" onclick={() => (importOpen = true)}>Import…</button>
    <button class="btn" onclick={exportDenc} disabled={!list.length} title="Save the selected encounter (or all) as a .denc file">
      Export {selected && list.some((e) => e.id === selected) ? 'selected' : 'all'} (.denc)
    </button>
    <button class="btn primary" onclick={() => openEditor(null)}>+ New encounter</button>
  </div>

  <div class="list-wrap">
    {#if shown.length}
      <div class="list" role="table" aria-label="Encounters">
        <div class="list-head" role="row" style:grid-template-columns={COLUMNS}>
          <span role="columnheader">Name</span>
          <span role="columnheader">Kind</span>
          <span role="columnheader">Creatures</span>
          <span role="columnheader">XP</span>
          <span role="columnheader">Changed</span>
          <span role="columnheader"><span class="visually-hidden">Actions</span></span>
          <span role="columnheader" title="Pinned">★</span>
        </div>
        {#each shown as e (e.id)}
          <div
            class="list-row"
            class:selected={selected === e.id}
            role="row"
            tabindex="0"
            style:grid-template-columns={COLUMNS}
            onclick={() => (selected = e.id)}
            ondblclick={() => start(e)}
            onkeydown={(ev) => ev.key === 'Enter' && start(e)}
          >
            <span role="cell" class="title-cell">
              <span class="list-title" title={e.name}>{e.name}</span>
              <span class="list-sub" title={summary(e)}>{summary(e)}{e.notes ? ` · ${e.notes}` : ''}</span>
            </span>
            <span role="cell"><span class="list-pill"><span class="dot {e.kind}"></span>{e.kind === 'combat' ? 'Combat' : 'Encounter'}</span></span>
            <span role="cell" class="num">{e.total}</span>
            <span role="cell" class="num list-muted">{e.xp?.toLocaleString() ?? '—'}</span>
            <span role="cell" class="list-muted num">{e.updated.slice(0, 10)}</span>
            <span role="cell" class="actions">
              <button class="btn ghost" title={e.kind === 'combat' ? 'Load this combat (replaces the tracker)' : 'Roll into the tracker'} aria-label="Start {e.name}"
                onclick={(ev) => (ev.stopPropagation(), void start(e))}>▶</button>
              {#if e.kind === 'encounter'}
                <button class="btn ghost" title="Add to the current combat" aria-label="Add {e.name} to the current combat"
                  onclick={(ev) => (ev.stopPropagation(), void start(e, true))}>+</button>
                <button class="btn ghost" title="Edit" aria-label="Edit {e.name}" onclick={(ev) => (ev.stopPropagation(), openEditor(e))}>✎</button>
              {/if}
              <button class="btn ghost" title="Copy the 5etools link" aria-label="Copy the 5etools link of {e.name}" onclick={(ev) => (ev.stopPropagation(), void copyLink(e))}>🔗</button>
              <button class="btn ghost" class:danger={confirmDelete === e.id} title={confirmDelete === e.id ? 'Click again to delete' : 'Delete'} aria-label="Delete {e.name}"
                onclick={(ev) => (ev.stopPropagation(), void remove(e))}>{confirmDelete === e.id ? 'Delete?' : '✕'}</button>
            </span>
            <span role="cell">
              <button class="list-star" class:on={e.pinned} aria-pressed={e.pinned} aria-label={e.pinned ? `Unpin ${e.name}` : `Pin ${e.name}`}
                onclick={(ev) => (ev.stopPropagation(), void togglePin(e))}>{e.pinned ? '★' : '☆'}</button>
            </span>
          </div>
        {/each}
      </div>
    {:else}
      <div class="empty-state">
        {#if list.length}
          <strong>No encounters match</strong>
          <span>Try another search.</span>
        {:else}
          <strong>No encounters yet</strong>
          <span>Build one from the bestiary, import one from 5etools (Bestiary → encounter → Save to File or Copy Combat Link), or save the current combat.</span>
          <button class="btn primary" onclick={() => openEditor(null)}>+ New encounter</button>
        {/if}
      </div>
    {/if}
  </div>
</div>

<Modal title={editId ? 'Edit encounter' : 'New encounter'} bind:open={editOpen} width={1440}>
  <div class="ed">
    <div class="ed-head">
      <label class="field grow">Name <input class="input big" placeholder="Named from its creatures" bind:value={editName} /></label>
      <label class="field grow">Notes <input class="input big" bind:value={editNotes} /></label>
    </div>
    <div class="ed-body">
      <section class="ed-col diff" aria-label="Difficulty">
        <div class="col-head">
          <h3>Difficulty</h3>
          <label class="check small-check" title="2014 DMG: multiply XP by the number of enemies before comparing with the 2024 budgets">
            <input type="checkbox" checked={use2014} onchange={(e) => app.updateSettings({ combat_xp_multiplier_2014: e.currentTarget.checked })} />
            2014 multiplier
          </label>
        </div>
        {#each orderedParties as p (p.id)}
          {@const d = rate(p)}
          <div class="party-diff rating-{d.rating}" data-testid="party-difficulty">
            <div class="pd-top">
              <span class="pd-name">{p.name}{#if isDefault(p)} <span class="list-pill">default</span>{/if}</span>
              <span class="pd-rating">{RATING_LABEL[d.rating]}</span>
            </div>
            <div class="pd-sub">{p.heroes.length} heroes · levels {p.heroes.map((h) => h.level ?? '?').join(', ')}</div>
            <div class="bar" aria-hidden="true">
              <span class="fill" style:width="{Math.min(100, (d.adjusted / Math.max(1, 2 * d.budget.high)) * 100)}%"></span>
              <span class="tick" style:left="{(d.budget.low / Math.max(1, 2 * d.budget.high)) * 100}%"></span>
              <span class="tick" style:left="{(d.budget.moderate / Math.max(1, 2 * d.budget.high)) * 100}%"></span>
              <span class="tick high" style:left="50%"></span>
            </div>
            <div class="pd-nums">
              <span>{d.adjusted.toLocaleString()} XP{d.multiplier !== 1 ? ` (×${d.multiplier})` : ''}</span>
              <span class="muted">Low {d.budget.low.toLocaleString()} · Mod {d.budget.moderate.toLocaleString()} · High {d.budget.high.toLocaleString()}</span>
            </div>
          </div>
        {:else}
          <p class="muted">Save a party in the Parties tab to see how hard this is for them.</p>
        {/each}
        <p class="muted legend">2024 budgets. Over High is Deadly, over twice High a total party kill.</p>
      </section>

      <section class="ed-col" aria-label="Bestiary">
        <div class="col-head"><h3>Bestiary</h3></div>
        <input id="enc-search" class="input big" type="search" aria-label="Add from the bestiary" placeholder="Search creatures…" bind:value={search} oninput={runSearch} />
        <ul class="hits" aria-label="Bestiary results">
          {#each hits as h (h.hash)}
            <li>
              <button onclick={() => addCreature(h)} aria-label="Add {h.name}">
                <span class="hname">{h.name}</span>
                <span class="hmeta">{h.source}</span>
                <span class="hmeta">CR {h.cr ?? '—'}</span>
                <span class="hmeta xp">{xpOf(h.cr)?.toLocaleString() ?? '—'} XP</span>
              </button>
            </li>
          {:else}
            <li class="muted pad">No matches. Is the bestiary synced?</li>
          {/each}
        </ul>
      </section>

      <section class="ed-col" aria-label="Encounter creatures">
        <div class="col-head"><h3>Creatures ({totalCount})</h3><span class="total">{totalXp.toLocaleString()} XP{unknownXp ? ' +?' : ''}</span></div>
        <ul class="picked">
          {#each editCreatures as c, i (i)}
            <li>
              <span class="pname" title={c.hash}>{c.name} <span class="muted">{c.source}</span></span>
              <label class="pcr">CR
                <select class="input compact" aria-label="CR of {c.name}" value={c.cr ?? ''} onchange={(e) => scaleLine(c, e.currentTarget.value)}>
                  {#if c.baseCr}<option value={c.baseCr}>{c.baseCr}</option>{/if}
                  {#each CRS.filter((x) => x !== c.baseCr) as x (x)}<option value={x}>{x}{c.baseCr ? ' (scaled)' : ''}</option>{/each}
                </select>
              </label>
              <span class="pxp">{xpOf(c.cr) !== null ? `${xpOf(c.cr)!.toLocaleString()} XP each · ${lineXp(c).toLocaleString()}` : 'XP unknown'}</span>
              <span class="stepper">
                <button class="btn ghost icon" aria-label="One fewer {c.name}" onclick={() => (c.count > 1 ? c.count-- : editCreatures.splice(i, 1))}>−</button>
                <span class="num cnt">{c.count}</span>
                <button class="btn ghost icon" aria-label="One more {c.name}" onclick={() => c.count++}>+</button>
              </span>
            </li>
          {:else}
            <li class="muted pad">Pick creatures from the bestiary.</li>
          {/each}
        </ul>
      </section>
    </div>
  </div>
  {#snippet footer()}
    <button class="btn primary big-btn" disabled={!editCreatures.length} onclick={saveEditor}>Save encounter</button>
  {/snippet}
</Modal>

<Modal title="Import encounters" bind:open={importOpen} width={560}>
  <div class="field">
    <div class="label-row">
      <label for="enc-import">5etools encounter link, combat link or encounter JSON</label>
      <InfoTip label="About importing" text="In the 5etools Bestiary, build an encounter, then use Copy Combat Link, or Save to File and pick the file. A DM Screen combat link is saved as a combat snapshot. Choose a file also opens .denc files exported from this app." />
    </div>
    <textarea id="enc-import" class="input" rows="5" placeholder="http://localhost:8080/dmscreen.html#encounter=…" bind:value={importText}></textarea>
  </div>
  {#snippet footer()}
    <button class="btn" onclick={importFile}>Choose a file…</button>
    <button class="btn primary" disabled={!importText.trim()} onclick={doImport}>Import</button>
  {/snippet}
</Modal>

<style>
  .enc {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .search {
    width: 240px;
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
  .list-row {
    cursor: default;
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
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
  }
  .dot.combat {
    background: #cc0000;
  }
  .field {
    font-size: 12px;
  }
  .nowrap {
    flex-wrap: nowrap;
  }
  .ed {
    display: grid;
    gap: var(--space-4);
    font-size: 15px;
  }
  .ed-head {
    display: flex;
    gap: var(--space-4);
  }
  .ed-head .field {
    display: grid;
    gap: 4px;
    font-size: 14px;
  }
  .big {
    font-size: 15px;
    height: 40px;
  }
  .ed-body {
    display: grid;
    grid-template-columns: 1fr 1.2fr 1.3fr;
    gap: var(--space-4);
    height: 64vh;
  }
  .ed-col {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 0;
    min-width: 0;
  }
  .col-head {
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
  }
  .col-head h3 {
    margin: 0;
    font-size: 17px;
    flex: 1;
  }
  .total {
    font-size: 17px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .small-check {
    font-size: 13px;
  }
  .diff {
    overflow: auto;
  }
  .party-diff {
    --r: var(--text-muted);
    display: grid;
    gap: 6px;
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-left: 4px solid var(--r);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  .rating-low { --r: #3fa34d; }
  .rating-moderate { --r: #c9b400; }
  .rating-high { --r: #f08c00; }
  .rating-deadly { --r: #d9381e; }
  .rating-tpk { --r: #9b1fd1; }
  .pd-top {
    display: flex;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .pd-name {
    font-weight: 700;
    font-size: 16px;
  }
  .pd-rating {
    font-weight: 800;
    font-size: 16px;
    color: var(--r);
  }
  .pd-sub,
  .pd-nums {
    font-size: 13px;
  }
  .pd-nums {
    display: grid;
  }
  .bar {
    position: relative;
    height: 10px;
    border-radius: 999px;
    background: var(--surface);
    overflow: hidden;
  }
  .bar .fill {
    position: absolute;
    inset: 0 auto 0 0;
    background: var(--r);
  }
  .bar .tick {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--border);
  }
  .bar .tick.high {
    background: var(--text-muted);
  }
  .legend {
    font-size: 13px;
  }
  .hits,
  .picked {
    list-style: none;
    margin: 0;
    padding: 0;
    flex: 1;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .hits button {
    width: 100%;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto 64px 90px;
    gap: var(--space-3);
    align-items: center;
    padding: 9px 12px;
    border: 0;
    border-bottom: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    cursor: pointer;
    text-align: left;
    font-size: 15px;
  }
  .hits button:hover {
    background: var(--accent-soft);
  }
  .hname {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hmeta {
    font-size: 13px;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .xp {
    text-align: right;
  }
  .picked li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    grid-template-areas: 'name cr step' 'xp xp step';
    gap: 2px var(--space-3);
    align-items: center;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .picked li.pad {
    display: block;
  }
  .pname {
    grid-area: name;
    font-weight: 600;
  }
  .pcr {
    grid-area: cr;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 14px;
  }
  .pxp {
    grid-area: xp;
    font-size: 13px;
    color: var(--text-muted);
  }
  .stepper {
    grid-area: step;
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .cnt {
    min-width: 2.5ch;
    text-align: center;
    font-size: 16px;
    font-weight: 700;
  }
  .big-btn {
    font-size: 15px;
    height: 40px;
    padding: 0 20px;
  }
  .pad {
    padding: 8px;
    font-size: 12px;
  }
</style>
