<script lang="ts">
  // Settings → Combat: the 5etools mirror and bestiary cache, rolling and
  // import options (app settings), and the tracker's stat columns and custom
  // conditions (saved with the tracker, so they travel in 5etools links).
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import { app } from '../../lib/state.svelte';
  import type { BestiaryStatus, CombatCatalog, CombatColumn, Settings, Tracker } from '../../lib/types';

  let status = $state<BestiaryStatus | null>(null);
  let catalog = $state<CombatCatalog>({ conditions: [], columns: [] });
  let t = $state<Tracker | null>(null);
  let syncJobId = $state<string | null>(null);
  const syncJob = $derived(syncJobId ? app.jobs.find((j) => j.id === syncJobId) : undefined);
  let urlError = $state('');

  onMount(async () => {
    try {
      [status, catalog, t] = await Promise.all([api.bestiaryStatus(), api.combatCatalog(), api.combatGet()]);
    } catch (e) {
      app.error('Could not load the combat settings', e);
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

  async function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    const errs = await app.updateSettings({ [key]: value } as Partial<Settings>);
    if (key === 'fivetools_base_url') urlError = errs[key] ?? '';
    if (!errs[key]) app.toast('success', 'Saved');
    if (key === 'fivetools_base_url') status = await api.bestiaryStatus();
  }

  async function sync() {
    try {
      syncJobId = await api.bestiarySync();
    } catch (e) {
      app.error('Could not sync the bestiary', e);
    }
  }

  async function saveTracker() {
    if (!t) return;
    try {
      t = await api.combatSet($state.snapshot(t) as Tracker);
    } catch (e) {
      app.error('Could not save', e);
    }
  }

  // ── stat columns ──
  function addColumn(choice: string) {
    if (!t || !choice) return;
    const populate = choice === '__custom' ? '' : choice;
    const kind = catalog.columns.find((k) => k.populate_with === populate);
    const col: CombatColumn = { id: crypto.randomUUID(), is_editable: true, is_player_visible: 0, populate_with: populate, abbreviation: kind?.abbreviation ?? '' };
    t.stats_cols.push(col);
    t.is_stats_add_columns = true;
    void saveTracker();
  }
  function moveColumn(i: number, d: number) {
    if (!t || i + d < 0 || i + d >= t.stats_cols.length) return;
    const cols = t.stats_cols;
    [cols[i], cols[i + d]] = [cols[i + d], cols[i]];
    void saveTracker();
  }
  function removeColumn(id: string) {
    if (!t) return;
    t.stats_cols = t.stats_cols.filter((c) => c.id !== id);
    for (const r of t.rows) r.cells = r.cells.filter((c) => c.id !== id);
    void saveTracker();
  }



  const ROLLING: [keyof Settings, string][] = [
    ['combat_roll_init', 'Roll initiative for creatures'],
    ['combat_passive_initiative', 'Passive initiative: take 10 + bonus instead of rolling'],
    ['combat_roll_groups', 'Copies of the same creature share one initiative roll'],
    ['combat_roll_hp', 'Roll creature hit points (otherwise the average)'],
    ['combat_reroll_each_round', 'Reroll initiative at the start of each round'],
    ['combat_hp_as_damage', 'Track hit points as damage taken'],
  ];
  const IMPORT: [keyof Settings, string][] = [
    ['combat_import_roll_groups', 'One initiative roll per creature type'],
    ['combat_import_add_players', 'Add the default party'],
    ['combat_import_append', 'Add to the current combat instead of replacing it'],
  ];
</script>

{#snippet toggle(key: keyof Settings, label: string)}
  <label class="toggle">
    <input type="checkbox" checked={app.settings[key] as boolean} onchange={(e) => set(key, e.currentTarget.checked as never)} />
    <span>{label}</span>
  </label>
{/snippet}

<section class="card">
  <h2>Creatures <InfoTip label="About the 5etools mirror" text="The 5etools site creatures are read from, e.g. your local copy on http://localhost:8080. Paths like /data/bestiary/ are read under it. The bestiary is cached, so search works offline; sync again after updating the mirror." /></h2>
  <div class="field">
    <label for="fivetools_base_url">5etools mirror</label>
    <input id="fivetools_base_url" class="input mono" spellcheck="false" value={app.settings.fivetools_base_url}
      aria-invalid={urlError ? 'true' : undefined} onchange={(e) => set('fivetools_base_url', e.currentTarget.value.trim())} />
    {#if urlError}<span class="error-text">{urlError}</span>{/if}
  </div>
  <p class="muted hint" data-testid="bestiary-status">
    {#if status?.synced}{status.creatures} creatures cached from {status.files} file(s).{:else}The bestiary is not synced yet.{/if}
  </p>
  <div>
    <button class="btn" onclick={sync} disabled={!!syncJobId}>{syncJobId ? `Syncing… ${Math.round((syncJob?.progress ?? 0) * 100)}%` : 'Sync from mirror'}</button>
  </div>
</section>

<section class="card">
  <h2>Rolling</h2>
  {#each ROLLING as [k, l] (k)}{@render toggle(k, l)}{/each}
  {@render toggle('combat_turn_popup', "Show whose turn it is (and who's next) on each turn")}
  {@render toggle('combat_reset_confirm', 'Ask before Reset')}
  {@render toggle('combat_reset_keep_players', 'Reset keeps the party (removes only creatures)')}
  <details class="sub">
    <summary>Encounter import</summary>
    {#each IMPORT as [k, l] (k)}{@render toggle(k, l)}{/each}
  </details>
</section>

{#if t}
  <section class="card">
    <h2>Stat columns <InfoTip label="About stat columns" text="Extra columns in the tracker. They fill from each creature's stat block and from your heroes' sheets; checkbox columns can reset on each turn or round." /></h2>
    {#each t.stats_cols as c, i (c.id)}
      <div class="col-row">
        <input class="input abv" aria-label="Column label" bind:value={c.abbreviation} onchange={saveTracker} />
        <span class="muted kind">{catalog.columns.find((k) => k.populate_with === c.populate_with)?.name ?? c.populate_with}</span>
        <button class="btn ghost icon" aria-label="Move column up" disabled={i === 0} onclick={() => moveColumn(i, -1)}>↑</button>
        <button class="btn ghost icon" aria-label="Move column down" disabled={i === t.stats_cols.length - 1} onclick={() => moveColumn(i, 1)}>↓</button>
        <button class="btn ghost icon danger" aria-label="Remove column {c.abbreviation}" onclick={() => removeColumn(c.id)}>✕</button>
      </div>
    {:else}
      <p class="muted hint">No extra columns.</p>
    {/each}
    <select class="input" aria-label="Add a column" value="" onchange={(e) => { addColumn(e.currentTarget.value); e.currentTarget.value = ''; }}>
      <option value="" disabled>+ Add a column…</option>
      {#each catalog.columns as k (k.populate_with)}<option value={k.populate_with || '__custom'}>{k.name}</option>{/each}
    </select>
  </section>

{/if}

<style>
  section {
    padding: var(--space-4);
    display: grid;
    gap: var(--space-3);
  }
  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 650;
  }
  .hint {
    margin: 0;
    font-size: 12px;
  }
  .toggle {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    cursor: pointer;
  }
  .toggle input {
    accent-color: var(--accent);
    width: 16px;
    height: 16px;
  }
  .mono {
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .col-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .abv {
    width: 72px;
  }
  .kind {
    flex: 1;
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  details.sub {
    display: grid;
    gap: var(--space-3);
  }
  details.sub summary {
    cursor: pointer;
    font-size: 12px;
    color: var(--text-muted);
  }
  details.sub[open] {
    display: grid;
    gap: var(--space-3);
  }
</style>
