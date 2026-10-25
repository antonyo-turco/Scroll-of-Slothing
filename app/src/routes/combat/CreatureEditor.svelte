<script lang="ts">
  // Edit one creature in the tracker: a Stats form, its 5etools JSON, scaling to
  // a CR, or back to the bestiary. Changes come back as an EditorPatch.
  import Icon from '../../lib/components/Icon.svelte';
  import Modal from '../../lib/components/Modal.svelte';
  import { CRS, crOfMon } from '../../lib/scale';
  import { app } from '../../lib/state.svelte';
  import type { CombatRow } from '../../lib/types';
  import type { EditorPatch, Mon } from './combat-ui';

  let {
    row,
    base,
    title,
    onClose,
    onApply,
    onScale,
    onReset,
  }: {
    /** The creature being edited; null closes the editor. */
    row: CombatRow | null;
    /** Its bestiary entry (without this row's changes). */
    base: Mon | null;
    title: string;
    onClose: () => void;
    onApply: (patch: EditorPatch) => void;
    onScale: (cr: string) => void;
    onReset: () => void;
  } = $props();

  const ABIL = ['str', 'dex', 'con', 'int', 'wis', 'cha'] as const;
  const SPEEDS = ['walk', 'fly', 'swim', 'climb', 'burrow'] as const;
  const LISTS: [string, string][] = [
    ['resist', 'Damage resistances'], ['immune', 'Damage immunities'], ['vulnerable', 'Vulnerabilities'],
    ['conditionImmune', 'Condition immunities'], ['senses', 'Senses'], ['languages', 'Languages'],
  ];
  type StatForm = {
    name: string; ac: string; hp_current: string; hp_max: string; initiative: string; cr: string;
    passive: string; size: string; type: string; speed: Record<string, string>; abil: Record<string, string>;
    save: string; skill: string; lists: Record<string, string>;
  };

  let form = $state<StatForm | null>(null);
  let tab = $state<'stats' | 'json'>('stats');
  let jsonText = $state('');
  let jsonError = $state('');
  let scaleTo = $state('');

  const merged = (): Mon => ({ ...(base ?? {}), ...(row?.overrides ?? {}) });
  const numStr = (v: unknown) => (v === null || v === undefined ? '' : String(v));
  /** 5etools lists (strings or {special}/{resist: [...]} objects) as plain comma text. */
  const listText = (v: unknown): string =>
    Array.isArray(v)
      ? v.map((x) => (typeof x === 'string' ? x : typeof x === 'object' && x ? Object.values(x as object).flat().filter((y) => typeof y === 'string').join(' ') : '')).filter(Boolean).join(', ')
      : '';
  const kvText = (v: unknown) => (v && typeof v === 'object' ? Object.entries(v as Record<string, unknown>).map(([k, x]) => `${k} ${x}`).join(', ') : '');

  // a fresh form whenever another creature is opened
  let formFor = '';
  $effect(() => {
    if (!row) {
      formFor = '';
      return;
    }
    if (formFor === row.id) return;
    formFor = row.id;
    const m = merged();
    const ac = Array.isArray(m.ac) ? (typeof m.ac[0] === 'object' ? (m.ac[0] as { ac?: number }).ac : m.ac[0]) : m.ac;
    const sp = (m.speed ?? {}) as Record<string, unknown>;
    const spd = (k: string) => numStr(typeof sp[k] === 'object' && sp[k] ? (sp[k] as { number?: number }).number : sp[k]);
    const cr = m.cr && typeof m.cr === 'object' ? (m.cr as { cr?: string }).cr : m.cr;
    form = {
      name: row.custom_name ?? '', ac: numStr(ac), hp_current: numStr(row.hp_current), hp_max: numStr(row.hp_max),
      initiative: numStr(row.initiative), cr: numStr(cr), passive: numStr(m.passive),
      size: Array.isArray(m.size) ? (m.size as string[]).join('/') : numStr(m.size),
      type: typeof m.type === 'string' ? m.type : numStr((m.type as { type?: string } | undefined)?.type),
      speed: Object.fromEntries(SPEEDS.map((k) => [k, spd(k)])),
      abil: Object.fromEntries(ABIL.map((a) => [a, numStr(m[a])])),
      save: kvText(m.save), skill: kvText(m.skill),
      lists: Object.fromEntries(LISTS.map(([k]) => [k, listText(m[k])])),
    };
    tab = 'stats';
    jsonText = JSON.stringify(m, null, 2);
    jsonError = '';
    scaleTo = '';
  });
  const isOpen = {
    get value() {
      return row !== null;
    },
    set value(v: boolean) {
      if (!v) onClose();
    },
  };

  /** "dex +4, wis +2" → {dex: "+4", wis: "+2"} (5etools save/skill objects). */
  function kvParse(text: string): Record<string, string> | null {
    const out: Record<string, string> = {};
    for (const part of text.split(',')) {
      const m = part.trim().match(/^(.*?)\s*([+-]\d+)$/);
      if (m && m[1]) out[m[1].trim().toLowerCase()] = m[2];
    }
    return Object.keys(out).length ? out : null;
  }
  const listParse = (text: string) => text.split(',').map((x) => x.trim()).filter(Boolean);

  function save() {
    if (!row || !form) return;
    if (tab === 'json') return saveJson();
    const e = form;
    const n = (v: string) => (v.trim() === '' ? null : Math.round(Number(v)));
    const o: Mon = { ...(row.overrides ?? {}) };
    const ac = n(e.ac);
    if (ac !== null) o.ac = [ac];
    const speed: Record<string, number> = {};
    for (const k of SPEEDS) {
      const v = n(e.speed[k]);
      if (v !== null && v > 0) speed[k] = v;
    }
    if (Object.keys(speed).length) o.speed = speed;
    if (e.cr.trim()) o.cr = e.cr.trim();
    const pp = n(e.passive);
    if (pp !== null) o.passive = pp;
    if (e.size.trim()) o.size = e.size.split('/').map((x) => x.trim().toUpperCase()).filter(Boolean);
    if (e.type.trim()) o.type = e.type.trim();
    for (const a of ABIL) {
      const v = n(e.abil[a]);
      if (v !== null) o[a] = v;
    }
    const sv = kvParse(e.save);
    if (sv) o.save = sv;
    else if (!e.save.trim()) delete o.save;
    const sk = kvParse(e.skill);
    if (sk) o.skill = sk;
    else if (!e.skill.trim()) delete o.skill;
    for (const [k] of LISTS) {
      const v = listParse(e.lists[k]);
      if (v.length) o[k] = v;
      else delete o[k];
    }
    onApply({
      overrides: o, custom_name: e.name.trim() || null,
      hp_max: n(e.hp_max), hp_current: n(e.hp_current), initiative: n(e.initiative),
    });
  }

  /** JSON tab: the edited stat block is stored as the fields that differ from the bestiary. */
  function saveJson() {
    if (!row) return;
    let v: Mon;
    try {
      v = JSON.parse(jsonText);
      if (!v || typeof v !== 'object' || Array.isArray(v)) throw new Error('expected a JSON object');
    } catch (err) {
      jsonError = (err as Error).message;
      return;
    }
    const b = base ?? {};
    const o: Mon = {};
    for (const [k, x] of Object.entries(v)) if (JSON.stringify(b[k]) !== JSON.stringify(x)) o[k] = x;
    const patch: EditorPatch = { overrides: Object.keys(o).length ? o : null };
    if (typeof v.name === 'string' && v.name !== b.name) patch.custom_name = v.name;
    const hp = (v.hp as { average?: number } | undefined)?.average;
    if (typeof hp === 'number' && JSON.stringify(v.hp) !== JSON.stringify(b.hp)) {
      patch.hp_max = hp;
      if (row.hp_current === null || row.hp_current > hp) patch.hp_current = hp;
    }
    onApply(patch);
  }
  function checkJson() {
    try {
      JSON.parse(jsonText);
      jsonError = '';
      app.toast('success', 'Valid JSON');
    } catch (err) {
      jsonError = (err as Error).message;
    }
  }
</script>

<Modal title="Edit {title}" bind:open={isOpen.value} width={980}>
  {#if form && row}
    <div class="segmented ed-tabs" role="tablist" aria-label="Edit as">
      <button role="tab" aria-selected={tab === 'stats'} onclick={() => (tab = 'stats')}><Icon name="pencil" size={15} /> Stats</button>
      <button role="tab" aria-selected={tab === 'json'} onclick={() => { tab = 'json'; jsonText = JSON.stringify(merged(), null, 2); }}><Icon name="braces" size={15} /> 5etools JSON</button>
    </div>
    <p class="note">Changes apply to this {row.name} only and update its row in the tracker.{row.overrides ? ' It already has changes from the bestiary.' : ''}</p>
    {#if tab === 'stats'}
      <div class="scale-row">
        <span>Scale to CR</span>
        <select class="input compact" aria-label="Scale to CR" bind:value={scaleTo}>
          <option value="">—</option>
          {#each CRS as c (c)}<option value={c}>{c}</option>{/each}
        </select>
        <button class="btn" disabled={!scaleTo || !base} onclick={() => onScale(scaleTo)}
          title="Adjusts AC, HP, to-hit, damage, save DCs, saves and skills with the DMG's monster statistics by CR (from the bestiary stats)">Scale</button>
        <span class="muted">from CR {base ? crOfMon(base) : '—'} in the bestiary</span>
      </div>
      <div class="stat-form">
        <fieldset>
          <legend>Basics</legend>
          <label class="field wide">Name <input class="input" placeholder={row.name} bind:value={form.name} /></label>
          <label class="field">Size <input class="input" placeholder="M" bind:value={form.size} /></label>
          <label class="field">Type <input class="input" placeholder="humanoid" bind:value={form.type} /></label>
          <label class="field">CR <input class="input" bind:value={form.cr} /></label>
        </fieldset>
        <fieldset>
          <legend>Combat</legend>
          <label class="field">AC <input class="input" inputmode="numeric" bind:value={form.ac} /></label>
          <label class="field">HP <input class="input" inputmode="numeric" bind:value={form.hp_current} /></label>
          <label class="field">Max HP <input class="input" inputmode="numeric" bind:value={form.hp_max} /></label>
          <label class="field">Initiative <input class="input" inputmode="numeric" bind:value={form.initiative} /></label>
          <label class="field">Passive Perception <input class="input" inputmode="numeric" bind:value={form.passive} /></label>
        </fieldset>
        <fieldset>
          <legend>Speed (ft.)</legend>
          {#each SPEEDS as k (k)}
            <label class="field">{k[0].toUpperCase() + k.slice(1)} <input class="input" inputmode="numeric" bind:value={form.speed[k]} /></label>
          {/each}
        </fieldset>
        <fieldset>
          <legend>Abilities</legend>
          {#each ABIL as a (a)}
            <label class="field">{a.toUpperCase()} <input class="input" inputmode="numeric" bind:value={form.abil[a]} /></label>
          {/each}
        </fieldset>
        <fieldset class="wide-set">
          <legend>Proficiencies and defences</legend>
          <label class="field wide">Saving throws <input class="input" placeholder="dex +4, wis +2" bind:value={form.save} /></label>
          <label class="field wide">Skills <input class="input" placeholder="perception +5, stealth +6" bind:value={form.skill} /></label>
          {#each LISTS as [k, l] (k)}
            <label class="field wide">{l} <input class="input" placeholder="comma separated" bind:value={form.lists[k]} /></label>
          {/each}
        </fieldset>
      </div>
    {:else}
      <p class="muted json-hint">The full stat block in 5etools format (the bestiary entry with this creature's changes). Edit it and save; only the fields that differ from the bestiary are stored.</p>
      <textarea class="input json" spellcheck="false" aria-label="5etools JSON" bind:value={jsonText} oninput={() => (jsonError = '')}></textarea>
      {#if jsonError}<p class="error-text" role="alert">{jsonError}</p>{/if}
    {/if}
  {/if}
  {#snippet footer()}
    {#if row?.overrides}<button class="btn ghost" onclick={onReset}><Icon name="undo" size={15} /> Reset to bestiary</button>{/if}
    <span class="grow"></span>
    {#if tab === 'json'}<button class="btn" onclick={checkJson}>Check JSON</button>{/if}
    <button class="btn primary" onclick={save}>Save</button>
  {/snippet}
</Modal>

<style>
  .ed-tabs {
    margin-bottom: var(--space-3);
  }
  .ed-tabs button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .scale-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin-bottom: var(--space-3);
    font-size: 15px;
  }
  .stat-form {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-3);
    max-height: 62vh;
    overflow: auto;
    font-size: 15px;
  }
  .stat-form fieldset {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--space-2) var(--space-3);
    margin: 0;
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .stat-form .wide-set {
    grid-column: 1 / -1;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .stat-form legend {
    padding: 0 6px;
    font-size: 13px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }
  .stat-form .field {
    display: grid;
    gap: 4px;
    font-size: 13px;
  }
  .stat-form .input {
    font-size: 15px;
  }
  .stat-form .wide {
    grid-column: 1 / -1;
  }
  .wide-set .wide {
    grid-column: auto;
  }
  .json {
    width: 100%;
    height: 56vh;
    font-family: var(--font-mono);
    font-size: 13px;
    line-height: 1.5;
    white-space: pre;
    resize: vertical;
  }
  .json-hint {
    margin: 0 0 var(--space-2);
    font-size: 13px;
  }
  .grow {
    flex: 1;
  }
</style>
