<script lang="ts">
  // Session planner: everything the next session needs, gathered by reference from
  // the other tools, with notes, a checklist and one-click printing.
  // Spec: docs/features/session-planner.md.
  import { onMount } from 'svelte';
  import { api, blobUrl, isTauri, pickDirectory } from '../../lib/api';
  import Icon from '../../lib/components/Icon.svelte';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import ViewActions from '../../lib/components/ViewActions.svelte';
  import { EDITOR_ROUTE } from '../../lib/library.svelte';
  import { music } from '../../lib/music';
  import { app } from '../../lib/state.svelte';
  import type { PlanEntry, PlannerSummary, PlanSection, SessionView } from '../../lib/types';
  import AddToSession from './AddToSession.svelte';

  const SECTIONS: { id: PlanSection; label: string; empty: string }[] = [
    { id: 'music', label: 'Music', empty: 'Soundscapes, playlists and tracks to have ready.' },
    { id: 'encounters', label: 'Encounters', empty: 'Saved encounters from Combat.' },
    { id: 'tokens', label: 'Tokens and cut-outs', empty: 'They print together on one sheet.' },
    { id: 'maps', label: 'Maps', empty: 'Maps saved from the Map Paginator print their pages.' },
    { id: 'handouts', label: 'Handouts', empty: 'Card decks from Cards.' },
  ];

  let summary = $state<PlannerSummary>({ sessions: [], current: null });
  let selected = $state<string | null>(null);
  let s = $state<SessionView | null>(null);
  let loaded = $state(false);
  let addFor = $state<PlanSection | null>(null);
  let confirmDelete = $state(false);
  let newCheck = $state('');
  let printJob = $state<string | null>(null);

  const bySection = $derived(
    new Map(SECTIONS.map((x) => [x.id, (s?.entries ?? []).filter((e) => e.section === x.id)])),
  );
  const missing = $derived((s?.entries ?? []).filter((e) => e.missing).length);
  const doneCount = $derived(s?.checklist.filter((c) => c.done).length ?? 0);

  async function refresh(keep = true) {
    try {
      summary = await api.plannerList();
      if (!keep || !selected || !summary.sessions.some((x) => x.id === selected)) {
        selected = summary.current ?? summary.sessions.at(-1)?.id ?? null;
      }
      s = selected ? await api.plannerSessionGet(selected) : null;
    } catch (e) {
      app.error('Could not load the planner', e);
    }
  }
  async function apply(p: Promise<SessionView>, what = 'Update failed') {
    try {
      s = await p;
      selected = s.id;
      summary = await api.plannerList();
    } catch (e) {
      app.error(what, e);
    }
  }

  onMount(() => {
    void refresh().then(() => (loaded = true));
  });
  // Changes from elsewhere (pins, MCP, deleted items) show up live.
  let seen = app.dataVersion;
  $effect(() => {
    if (app.dataVersion !== seen) {
      seen = app.dataVersion;
      void refresh();
    }
  });

  async function newSession() {
    await apply(api.plannerSessionCreate(null), 'Could not create the session');
  }
  async function pick(id: string) {
    selected = id;
    confirmDelete = false;
    await refresh();
  }
  async function remove() {
    if (!s) return;
    try {
      summary = await api.plannerSessionDelete(s.id);
      confirmDelete = false;
      selected = null;
      await refresh(false);
    } catch (e) {
      app.error('Could not delete the session', e);
    }
  }

  const patch = (p: Parameters<typeof api.plannerSessionUpdate>[1]) => s && apply(api.plannerSessionUpdate(s.id, p));

  // ── checklist ──
  function addCheck() {
    const text = newCheck.trim();
    if (!s || !text) return;
    newCheck = '';
    void patch({ checklist: [...s.checklist, { id: '', text, done: false }] });
  }
  function toggleCheck(id: string) {
    if (!s) return;
    void patch({ checklist: s.checklist.map((c) => (c.id === id ? { ...c, done: !c.done } : c)) });
  }
  function dropCheck(id: string) {
    if (!s) return;
    void patch({ checklist: s.checklist.filter((c) => c.id !== id) });
  }

  // ── items ──
  function setCopies(e: PlanEntry, raw: string) {
    const n = Math.round(Number(raw));
    if (!s || !e.item_id) return;
    if (!Number.isFinite(n) || n < 1 || n > 99) return app.toast('error', 'Copies must be between 1 and 99.');
    void apply(api.plannerItemUpdate(s.id, e.item_id, n));
  }
  /** Keeps a starred item in this session even after it is unstarred. */
  function keep(e: PlanEntry) {
    if (s) void apply(api.plannerItemsAdd(s.id, [{ kind: e.kind, ref_id: e.ref_id }]));
  }
  async function open(e: PlanEntry) {
    try {
      if (e.kind === 'asset') {
        const route = EDITOR_ROUTE[e.asset_kind ?? ''] ?? 'library';
        if (route !== 'library') app.openRequest = { route, assetId: Number(e.ref_id) };
        app.navigate(route);
      } else if (e.kind === 'encounter') {
        await api.encounterStart(e.ref_id, null);
        app.toast('success', `Started ${e.name}`);
        app.navigate('combat');
      } else if (e.kind === 'soundscape') {
        await music.soundscapeLoad(e.ref_id);
        app.toast('success', `Loaded ${e.name} into the mixer`);
      } else if (e.kind === 'playlist') {
        await music.playlistPlay(e.ref_id);
        app.toast('success', `Playing ${e.name}`);
      } else if (e.kind === 'track') {
        app.navigate('music');
      } else if (e.kind === 'deck') {
        app.navigate('cards');
      }
    } catch (err) {
      app.error(`Could not open ${e.name}`, err);
    }
  }
  const OPEN_LABEL: Record<string, string> = {
    asset: 'Open', encounter: 'Start', soundscape: 'Load', playlist: 'Play', track: 'Music', deck: 'Cards',
  };
  const openTitle = (e: PlanEntry) =>
    e.kind === 'encounter' ? 'Start it in the combat tracker (with the tracker\'s import settings)'
    : e.kind === 'soundscape' ? 'Load it into the music mixer'
    : e.kind === 'playlist' ? 'Play it now'
    : e.kind === 'asset' ? 'Open it in its editor'
    : 'Go to the tool';

  // ── print ──
  async function printAll() {
    if (!s) return;
    const dir = isTauri() ? await pickDirectory(`Print "${s.name}" into a folder`) : 'session-print';
    if (!dir) return;
    try {
      printJob = await api.plannerPrint(s.id, dir);
      app.toast('info', 'Printing… progress is shown bottom-right.');
    } catch (e) {
      app.error('Could not print the session', e);
    }
  }
  // Report what the print skipped (nothing is dropped silently).
  $effect(() => {
    const j = printJob ? app.jobs.find((x) => x.id === printJob) : null;
    if (!j || j.state === 'running') return;
    printJob = null;
    const r = j.result as { files?: string[]; skipped?: string[] } | null;
    if (j.state === 'done' && r) {
      app.toast(r.skipped?.length ? 'info' : 'success', `Printed ${r.files?.length ?? 0} file(s)`, r.skipped?.length ? `Skipped: ${r.skipped.join('; ')}` : undefined);
    }
  });
</script>

<ViewActions>
  <button class="btn" onclick={newSession}>New session</button>
  <button class="btn primary" disabled={!s} onclick={printAll} title="Print the session's tokens, cut-outs, maps and card decks into a folder">Print all…</button>
</ViewActions>

{#if loaded && !summary.sessions.length}
  <div class="empty-state big-empty">
    <strong>Plan your next session</strong>
    <span>Gather the music, encounters, tokens, maps and handouts you need, keep notes and a checklist, and print it all in one go.</span>
    <button class="btn primary" onclick={newSession}>New session</button>
  </div>
{:else}
  <div class="view">
    <section class="work" aria-label="Session">
      <div class="view-toolbar">
        <label class="session-pick">
          <span class="tool-label">Session</span>
          <select class="input" aria-label="Session" value={selected} onchange={(e) => pick(e.currentTarget.value)}>
            {#each summary.sessions as x (x.id)}
              <option value={x.id}>{x.name}{x.date ? ` · ${x.date}` : ''}{x.id === summary.current ? ' (next)' : ''}</option>
            {/each}
          </select>
        </label>
        {#if s}
          <button class="btn" class:next={s.current} aria-pressed={s.current} disabled={s.current} onclick={() => s && apply(api.plannerSetCurrent(s.id))}
            title="Starred items (pinned assets, encounters and soundscapes, favourite tracks) join the next session on their own">
            {s.current ? 'Next session' : 'Make it the next session'}
          </button>
          <InfoTip label="About the next session" text="One session is the next one. Everything you star in its own tool (a pinned asset, encounter or soundscape, a favourite track) shows up in it on its own, marked Starred. Keep it to make it part of the session for good." />
        {/if}
        <div class="spacer"></div>
        {#if missing}
          <button class="btn ghost" onclick={() => s && api.plannerPrune(s.id).then(() => refresh())}>Remove {missing} deleted</button>
        {/if}
      </div>

      <div class="stage-area sections">
        {#if s}
          {#each SECTIONS as sec (sec.id)}
            {@const list = bySection.get(sec.id) ?? []}
            <section class="plan-section" aria-label={sec.label}>
              <header>
                <h2>{sec.label} <span class="count">{list.length}</span></h2>
                <button class="btn" onclick={() => (addFor = sec.id)}>+ Add</button>
              </header>
              {#if !list.length}
                <p class="hint">{sec.empty}</p>
              {:else}
                <ul>
                  {#each list as e (`${e.kind}:${e.ref_id}`)}
                    <li class="entry" class:missing={e.missing} data-testid="plan-entry">
                      <span class="thumb">
                        {#if e.thumb}<img src={blobUrl(e.thumb)} alt="" loading="lazy" />{:else}
                          <Icon name={e.section === 'music' ? 'music' : e.section === 'encounters' ? 'swords' : e.section === 'maps' ? 'map' : 'cards'} size={22} />
                        {/if}
                      </span>
                      <span class="what">
                        <span class="name">{e.name}</span>
                        <span class="detail">{e.detail}{#if e.starred}<span class="star" title="Starred in its own tool">★ Starred</span>{/if}</span>
                      </span>
                      {#if e.section === 'tokens' && e.item_id}
                        <label class="copies" title="Copies to print">×
                          <input class="input" type="number" min="1" max="99" aria-label="Copies of {e.name}" value={e.copies}
                            onchange={(ev) => setCopies(e, ev.currentTarget.value)} />
                        </label>
                      {/if}
                      {#if !e.missing}
                        <button class="btn" onclick={() => open(e)} title={openTitle(e)}>{OPEN_LABEL[e.kind]}</button>
                      {/if}
                      {#if e.item_id}
                        <button class="btn ghost icon" aria-label="Remove {e.name} from the session" title="Remove from the session (it stays in its tool)"
                          onclick={() => s && e.item_id && apply(api.plannerItemRemove(s.id, e.item_id))}><Icon name="x" /></button>
                      {:else}
                        <button class="btn ghost" onclick={() => keep(e)} title="Keep it in this session even when it is unstarred">Keep</button>
                      {/if}
                    </li>
                  {/each}
                </ul>
              {/if}
            </section>
          {/each}
        {/if}
      </div>

      <div class="statusbar">
        {#if s}
          <span>{s.entries.length} item{s.entries.length === 1 ? '' : 's'} · checklist {doneCount}/{s.checklist.length}</span>
        {/if}
      </div>
    </section>

    <aside class="inspector" aria-label="Session details">
      {#if s}
        {#key s.id}
          <section class="section">
            <h3 class="section-title">Session</h3>
            <label class="field"><span class="label">Name</span>
              <input class="input" value={s.name} onchange={(e) => patch({ name: e.currentTarget.value })} />
            </label>
            <label class="field"><span class="label">Date</span>
              <input class="input" type="date" value={s.date ?? ''} onchange={(e) => patch({ date: e.currentTarget.value })} />
            </label>
          </section>
          <section class="section">
            <h3 class="section-title">Checklist <span class="count">{doneCount}/{s.checklist.length}</span></h3>
            <ul class="checklist">
              {#each s.checklist as c (c.id)}
                <li>
                  <label class="check"><input type="checkbox" checked={c.done} onchange={() => toggleCheck(c.id)} /> <span class:done={c.done}>{c.text}</span></label>
                  <button class="btn ghost icon" aria-label="Remove {c.text}" onclick={() => dropCheck(c.id)}><Icon name="x" /></button>
                </li>
              {/each}
            </ul>
            <form class="add-check" onsubmit={(e) => (e.preventDefault(), addCheck())}>
              <input class="input" placeholder="Add a to-do…" aria-label="New checklist item" bind:value={newCheck} />
              <button class="btn" disabled={!newCheck.trim()}>Add</button>
            </form>
          </section>
          <section class="section">
            <h3 class="section-title">Notes</h3>
            <textarea class="input notes" aria-label="Session notes" placeholder="Plot points, NPC names, what happened last time…"
              value={s.notes} onchange={(e) => patch({ notes: e.currentTarget.value })}></textarea>
          </section>
          <section class="section">
            <h3 class="section-title">Printing</h3>
            <label class="check">
              <input type="checkbox" checked={s.include_token_sheet} onchange={(e) => patch({ include_token_sheet: e.currentTarget.checked })} />
              Also print the Token Sheet's contents
            </label>
            <p class="note">Print all writes one PDF of the session's tokens and cut-outs, each map's saved pages and each card deck into a folder you pick.</p>
          </section>
          <div class="inspector-footer">
            {#if confirmDelete}
              <span class="note">Delete "{s.name}"? Its items stay in their tools.</span>
              <div class="pair">
                <button class="btn" onclick={() => (confirmDelete = false)}>Cancel</button>
                <button class="btn danger" onclick={remove}>Delete</button>
              </div>
            {:else}
              <button class="btn ghost danger" onclick={() => (confirmDelete = true)}>Delete session…</button>
            {/if}
          </div>
        {/key}
      {/if}
    </aside>
  </div>
{/if}

{#if s}
  <AddToSession bind:section={addFor} session={s} onAdd={(refs) => (s ? apply(api.plannerItemsAdd(s.id, refs), 'Could not add to the session') : undefined)} />
{/if}

<style>
  .big-empty {
    height: 100%;
    gap: var(--space-3);
    font-size: 16px;
  }
  .session-pick {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .session-pick select {
    min-width: 260px;
    font-size: 15px;
  }
  .tool-label {
    color: var(--text-muted);
  }
  .next {
    border-color: var(--accent);
    color: var(--accent);
  }
  .next:disabled {
    opacity: 1;
  }
  .sections {
    overflow: auto;
    padding: var(--space-4);
    display: grid;
    align-content: start;
    gap: var(--space-4);
  }
  .plan-section {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    padding: var(--space-3) var(--space-4);
  }
  .plan-section header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  .count {
    margin-left: 6px;
    font-size: 14px;
    font-weight: 500;
    color: var(--text-muted);
  }
  .hint {
    margin: var(--space-2) 0 0;
    font-size: 15px;
    color: var(--text-muted);
  }
  ul {
    list-style: none;
    margin: var(--space-2) 0 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
  }
  .entry:hover {
    background: var(--surface-2);
  }
  .entry.missing {
    opacity: 0.6;
  }
  .thumb {
    width: 52px;
    height: 52px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    color: var(--text-muted);
    overflow: hidden;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .what {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 2px;
  }
  .name {
    font-size: 16px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .detail {
    font-size: 14px;
    color: var(--text-muted);
  }
  .star {
    margin-left: var(--space-2);
    color: #d6a21a;
    font-weight: 600;
  }
  .copies {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-muted);
  }
  .copies .input {
    width: 64px;
  }
  .checklist {
    margin: 0;
  }
  .checklist li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .checklist .check {
    font-size: 15px;
  }
  .done {
    text-decoration: line-through;
    color: var(--text-muted);
  }
  .add-check {
    display: flex;
    gap: var(--space-2);
  }
  .add-check .input {
    flex: 1;
  }
  .notes {
    min-height: 180px;
    resize: vertical;
    font-size: 15px;
    line-height: 1.5;
    padding: var(--space-2);
  }
</style>
