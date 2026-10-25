// In-memory combat tracker for browser dev and tests. A compact port of
// sleuth_core::combat (sort, turns, rounds, condition countdown, column hooks)
// over a tiny built-in bestiary that appears after "Sync".

import type {
  BestiaryEntry, CombatCatalog, CombatCell, CombatRow, CombatSort, EncounterCreature, EncounterSummary, Hero,
  Party, RowPatch, Settings, Tracker,
} from './types';

type Emit = (e: Record<string, unknown>) => void;
type Handler = (args: Record<string, unknown>) => unknown;

export const COMBAT_CONDITIONS: [string, string][] = [
  ['Blinded', '#525252'], ['Charmed', '#f01789'], ['Concentration', '#009f7a'], ['Deafened', '#ababab'],
  ['Drunk', '#ffcc00'], ['Exhausted', '#947a47'], ['Frightened', '#c9ca18'], ['Grappled', '#8784a0'],
  ['Incapacitated', '#3165a0'], ['Invisible', '#7ad2d6'], ['!!On Fire!!', '#ff6800'], ['Paralyzed', '#c00900'],
  ['Petrified', '#a0a0a0'], ['Poisoned', '#4dc200'], ['Prone', '#5e60a0'], ['Restrained', '#d98000'],
  ['Stunned', '#a23bcb'], ['Unconscious', '#3a40ad'],
];

const COLUMNS: CombatCatalog['columns'] = [
  { populate_with: '', abbreviation: '', name: '(Custom)' },
  { populate_with: 'armorClass', abbreviation: 'AC', name: 'Armor Class' },
  { populate_with: 'passivePerception', abbreviation: 'PP', name: 'Passive Perception' },
  { populate_with: 'speed', abbreviation: 'SPD', name: 'Speed' },
  { populate_with: 'cr', abbreviation: 'CR', name: 'Challenge Rating (CR)' },
  { populate_with: 'legendaryActions', abbreviation: 'LA', name: 'Legendary Actions' },
  { populate_with: 'cbNeutral', abbreviation: '', name: 'Checkbox' },
  { populate_with: 'cbAutoLow', abbreviation: '', name: 'Checkbox; clears at start of turn' },
];

interface Mon {
  name: string; source: string; hp: number; formula: string; ac: number; dex: number; pp: number;
  speed: number; cr: string; la: number | null;
}
const BESTIARY: Mon[] = [
  { name: 'Goblin', source: 'MM', hp: 7, formula: '2d6', ac: 15, dex: 14, pp: 9, speed: 30, cr: '1/4', la: null },
  { name: 'Goblin Boss', source: 'MM', hp: 21, formula: '6d6', ac: 17, dex: 14, pp: 9, speed: 30, cr: '1', la: null },
  { name: 'Ogre', source: 'MM', hp: 59, formula: '7d10+21', ac: 11, dex: 8, pp: 8, speed: 40, cr: '2', la: null },
  { name: 'Adult Red Dragon', source: 'MM', hp: 256, formula: '19d12+133', ac: 19, dex: 10, pp: 23, speed: 80, cr: '17', la: 3 },
];
const hashOf = (m: { name: string; source: string }) => encodeURIComponent(`${m.name}_${m.source}`.toLowerCase());
const mod = (score: number) => Math.floor((score - 10) / 2);
const crNumber = (cr: string) => {
  const [a, b] = cr.split('/').map(Number);
  return b ? a / b : a;
};

export function defaultTracker(): Tracker {
  return {
    sort: 'NUMBER', dir: 'DESC', is_roll_init: true, is_roll_hp: false, is_roll_groups: false,
    is_reroll_initiative_each_round: false, is_invert_wound_direction: false, import_is_roll_groups: true,
    import_is_add_players: true, import_is_append: false, is_stats_add_columns: false,
    player_init_show_exact_player_hp: false, player_init_show_exact_monster_hp: false,
    player_init_hide_new_monster: true, player_init_show_ordinals: false, is_notes_shown: false,
    stats_cols: [], conditions_custom: [], rows: [], rows_default_party: [], defeated: [], round: 1, notes: '',
    is_locked: false, extra: {},
  };
}

const similarKey = (r: CombatRow) =>
  JSON.stringify([r.name, r.custom_name, r.scaled_cr, r.scaled_summon_spell_level, r.scaled_summon_class_level, r.source]);
const label = (r: CombatRow) => (r.custom_name || r.name).toLowerCase();

/** sleuth_core::combat::Tracker::sorted */
export function sortRows(t: Tracker): void {
  const m = t.dir === 'DESC' ? -1 : 1;
  const cmp = (a: number | string, b: number | string) => (a < b ? -1 : a > b ? 1 : 0);
  const name = (a: CombatRow, b: CombatRow) => m * cmp(label(a), label(b));
  const init = (a: CombatRow, b: CombatRow) => m * cmp(a.initiative ?? 0, b.initiative ?? 0);
  const ord = (a: CombatRow, b: CombatRow) => cmp(a.ordinal, b.ordinal);
  const id = (a: CombatRow, b: CombatRow) => cmp(a.id, b.id);
  t.rows.sort(t.sort === 'ALPHA'
    ? (a, b) => name(a, b) || ord(a, b) || init(a, b) || id(a, b)
    : (a, b) => init(a, b) || name(a, b) || ord(a, b) || id(a, b));
}

export function createCombatMockHandlers(emit: Emit, settings: () => Settings): Record<string, Handler> {
  let t = defaultTracker();
  let synced = false;
  let seq = 0;
  const newId = () => `mock-row-${++seq}`;
  const d20 = () => 1 + Math.floor(Math.random() * 20);
  const changed = () => emit({ type: 'store_changed', entity: 'combat', entity_id: 'tracker', op: 'update' });
  /** The rolling and import options come from the app settings (as the service). */
  const syncSettings = () => {
    const st = settings();
    Object.assign(t, {
      is_roll_init: st.combat_roll_init, is_roll_hp: st.combat_roll_hp, is_roll_groups: st.combat_roll_groups,
      is_reroll_initiative_each_round: st.combat_reroll_each_round, is_invert_wound_direction: st.combat_hp_as_damage,
      import_is_roll_groups: st.combat_import_roll_groups, import_is_add_players: st.combat_import_add_players,
      import_is_append: st.combat_import_append,
    });
  };
  const save = () => {
    syncSettings();
    sortRows(t);
    changed();
    return t;
  };
  const edit = () => {
    if (t.is_locked) throw { code: 'invalid_input', message: 'invalid input: the tracker is locked' };
  };
  const row = (id: unknown) => {
    const r = t.rows.find((r) => r.id === id);
    if (!r) throw { code: 'not_found', message: `not found: row ${id}` };
    return r;
  };
  const kind = (cellId: string) => t.stats_cols.find((c) => c.id === cellId)?.populate_with;
  const cellsFor = (mon: Mon | null): CombatCell[] =>
    t.stats_cols.map((c) => {
      const v = (value: CombatCell['value']): CombatCell => ({ id: c.id, value, isEditable: c.is_editable });
      switch (c.populate_with) {
        case 'armorClass': return v(mon?.ac ?? null);
        case 'passivePerception': return v(mon?.pp ?? null);
        case 'speed': return v(mon?.speed ?? null);
        case 'cr': return v(mon ? crNumber(mon.cr) : null);
        case 'legendaryActions': return { id: c.id, current: mon?.la ?? null, max: mon?.la ?? null, isEditable: c.is_editable };
        case 'cbNeutral': case 'cbAutoLow': return v(false);
        default: return v('');
      }
    });
  const blank = (over: Partial<CombatRow>): CombatRow => ({
    id: newId(), name: '', display_name: null, custom_name: null, source: null, scaled_cr: null,
    scaled_summon_spell_level: null, scaled_summon_class_level: null, hp_current: null, hp_max: null,
    hp_temp: null, initiative: null, is_active: false, conditions: [], notes: '', is_player_visible: true, ordinal: 1,
    cells: cellsFor(null), overrides: null, side: null, usage: null, ...over,
  });

  /** Turn/round-start column hooks (forwards only). */
  const hooks = (r: CombatRow, round: boolean) => {
    for (const c of r.cells) {
      const k = kind(c.id);
      if (!round && k === 'cbAutoLow') c.value = false;
      if (round && k === 'legendaryActions') c.current = c.max ?? null;
    }
  };
  const setActive = (ix: number, forwards: boolean, skipRound: boolean) => {
    const key = similarKey(t.rows[ix]);
    const init = t.rows[ix].initiative;
    t.rows.forEach((r, j) => {
      if (similarKey(r) !== key || (t.sort === 'NUMBER' && r.initiative !== init)) return;
      r.is_active = true;
      if (!skipRound && j === 0) {
        if (forwards) t.round++;
        if (forwards) t.rows.forEach((x) => hooks(x, true));
      }
      if (forwards) hooks(r, false);
    });
  };
  const shift = (forwards: boolean) => {
    if (!t.rows.length) return;
    sortRows(t);
    if (!t.rows.some((r) => r.is_active)) return setActive(0, false, false);
    const order = forwards ? t.rows : [...t.rows].reverse();
    if (forwards) {
      for (const r of order.filter((r) => r.is_active)) {
        r.conditions = r.conditions.filter((c) => c.turns === null || --c.turns > 0);
      }
    }
    let last = -1;
    order.forEach((r, i) => r.is_active && (last = i));
    order.forEach((r) => (r.is_active = false));
    const next = order[last + 1 < order.length ? last + 1 : 0];
    setActive(t.rows.indexOf(next), forwards, !forwards);
  };

  // ── saved encounters and parties ──
  let encounters: EncounterSummary[] = [];
  let parties: Party[] = [];
  const now = () => new Date().toISOString().slice(0, 19) + 'Z';
  const XP: Record<string, number> = { '1/4': 50, '1': 200, '2': 450, '17': 18000 };
  const summarize = (e: EncounterSummary, creatures: EncounterCreature[]): EncounterSummary => {
    const lines = creatures.map((c) => {
      const m = BESTIARY.find((x) => hashOf(x) === c.hash);
      return { hash: c.hash, name: m?.name ?? c.hash.split('_')[0], source: m?.source ?? '', count: c.count, cr: m?.cr ?? null, base_cr: m?.cr ?? null, known: !!m };
    });
    const known = lines.every((l) => l.known) && lines.length > 0;
    return {
      ...e, creatures, lines, total: lines.reduce((a, l) => a + l.count, 0),
      xp: known ? lines.reduce((a, l) => a + (XP[l.cr ?? ''] ?? 0) * l.count, 0) : null,
    };
  };
  const autoName = (e: EncounterSummary) => e.lines.map((l) => (l.count > 1 ? `${l.count} × ${l.name}` : l.name)).join(', ') || 'Empty encounter';
  const enc = (id: unknown) => {
    const e = encounters.find((x) => x.id === id);
    if (!e) throw { code: 'not_found', message: `not found: encounter ${id}` };
    return e;
  };
  const changedList = (what: string) => emit({ type: 'store_changed', entity: 'combat', entity_id: what, op: 'update' });
  const newEncounter = (over: Partial<EncounterSummary>): EncounterSummary => ({
    id: newId(), name: '', notes: '', kind: 'encounter', creatures: [], combat: null, pinned: false,
    created: now(), updated: now(), lines: [], total: 0, xp: null, ...over,
  });
  const addEncounterRows = (e: EncounterSummary, append: boolean) => {
    if (!append) [t.rows, t.round] = [[], 1];
    for (const c of e.creatures) {
      const mon = BESTIARY.find((m) => hashOf(m) === c.hash);
      if (!mon) continue;
      const init = d20() + mod(mon.dex);
      for (let i = 0; i < c.count; i++) {
        const r = blank({ name: mon.name, source: mon.source, hp_current: mon.hp, hp_max: mon.hp, initiative: init, cells: cellsFor(mon) });
        r.ordinal = t.rows.filter((x) => similarKey(x) === similarKey(r)).length + 1;
        t.rows.push(r);
      }
    }
  };
  const heroRow = (h: Hero, roll: boolean) => {
    const sub = [h.species, h.class, h.subclass ?? '', h.level ? `level ${h.level}` : ''].filter((x) => x.trim()).join(' · ');
    const r = blank({
      name: h.name, hp_max: h.hp_max, hp_current: h.hp_max, display_name: sub || null, notes: h.notes,
      initiative: roll ? d20() + (h.initiative_bonus ?? 0) : null,
    });
    for (const c of r.cells) {
      const k = kind(c.id);
      if (k === 'armorClass') c.value = h.ac;
      if (k === 'passivePerception') c.value = h.passive_perception;
      if (k === 'speed') c.value = h.speed;
    }
    return r;
  };

  return {
    encounters_list: () => [...encounters].sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.updated.localeCompare(a.updated)),
    encounter_save: ({ id, name, notes, creatures }) => {
      const merged: EncounterCreature[] = [];
      for (const c of creatures as EncounterCreature[]) {
        const h = c.hash.toLowerCase();
        const m = merged.find((x) => x.hash === h);
        if (m) m.count += c.count;
        else if (c.count > 0) merged.push({ hash: h, count: c.count });
      }
      let e = id ? enc(id) : newEncounter({});
      e = summarize({ ...e, notes: notes as string, updated: now() }, merged);
      e.name = String(name).trim() || autoName(e);
      encounters = [...encounters.filter((x) => x.id !== e.id), e];
      changedList('encounters');
      return e;
    },
    encounter_set_pinned: ({ id, pinned }) => {
      enc(id).pinned = !!pinned;
      changedList('encounters');
    },
    encounter_delete: ({ id }) => {
      enc(id);
      encounters = encounters.filter((e) => e.id !== id);
      changedList('encounters');
    },
    encounter_save_current: ({ name }) => {
      if (!t.rows.length) throw { code: 'invalid_input', message: 'invalid input: the tracker is empty' };
      const creatures: EncounterCreature[] = [];
      for (const r of t.rows.filter((r) => r.source)) {
        const h = hashOf({ name: r.name, source: r.source! });
        const c = creatures.find((x) => x.hash === h);
        if (c) c.count++;
        else creatures.push({ hash: h, count: 1 });
      }
      const e = summarize(newEncounter({ kind: 'combat', combat: structuredClone(t), name: String(name).trim() || `Combat, round ${t.round}` }), creatures);
      encounters.push(e);
      changedList('encounters');
      return e;
    },
    encounter_import: ({ text, name }) => {
      const s = String(text).trim();
      let creatures: EncounterCreature[];
      let given = (name as string | null) ?? '';
      if (s.startsWith('{')) {
        const v = JSON.parse(s) as { name?: string; items?: { h: string; c?: number }[] };
        if (!v.items?.length) throw { code: 'invalid_input', message: 'invalid input: the encounter has no creatures' };
        creatures = v.items.map((i) => ({ hash: i.h.toLowerCase(), count: i.c ?? 1 }));
        given ||= v.name ?? '';
      } else if (s.includes('encounter=')) {
        creatures = [{ hash: 'goblin_mm', count: 3 }, { hash: 'kraken_mm', count: 1 }];
      } else {
        throw { code: 'invalid_input', message: 'invalid input: not a 5etools combat or encounter link' };
      }
      const e = summarize(newEncounter({}), creatures);
      e.name = given.trim() || autoName(e);
      encounters.push(e);
      changedList('encounters');
      return e;
    },
    encounters_export: ({ ids }) => (ids ? (ids as string[]).length : encounters.length),
    encounters_import_denc: () => {
      throw { code: 'invalid_input', message: 'invalid input: files are only read in the desktop app' };
    },
    encounter_import_file: () => {
      throw { code: 'invalid_input', message: 'invalid input: files are only read in the desktop app' };
    },
    encounter_start: ({ id, append }) => {
      edit();
      const e = enc(id);
      if (e.kind === 'combat' && e.combat) {
        t = structuredClone(e.combat as Tracker);
        return { kind: 'combat', tracker: save(), encounter: null };
      }
      const before = t.rows.length;
      const add = typeof append === 'boolean' ? append : t.import_is_append;
      addEncounterRows(e, add);
      if (e.notes && !t.notes) t.notes = e.notes;
      save();
      const missing = e.lines.filter((l) => !l.known).map((l) => l.hash);
      return { kind: 'encounter', tracker: t, encounter: { rows_added: t.rows.length - (add ? before : 0), missing, unscaled: [] } };
    },
    encounter_link: ({ id }) => `${settings().fivetools_base_url}/dmscreen.html#encounter=mock${btoa(enc(id).name)}`,
    parties_list: () => [...parties].sort((a, b) => a.name.localeCompare(b.name)),
    party_save: ({ id, name, notes, heroes }) => {
      if (!String(name).trim()) throw { code: 'invalid_input', message: 'invalid input: the party needs a name' };
      const p: Party = {
        id: (id as string | null) ?? newId(), name: String(name).trim(), notes: notes as string, updated: now(),
        heroes: (heroes as Hero[]).map((h) => ({ ...h, id: h.id || newId() })),
      };
      parties = [...parties.filter((x) => x.id !== p.id), p];
      changedList('parties');
      return p;
    },
    party_delete: ({ id }) => {
      parties = parties.filter((p) => p.id !== id);
      changedList('parties');
    },
    party_add_to_tracker: ({ id, heroIds, asDefault }) => {
      edit();
      const p = parties.find((x) => x.id === id);
      if (!p) throw { code: 'not_found', message: `not found: party ${id}` };
      const heroes = p.heroes.filter((h) => !heroIds || (heroIds as string[]).includes(h.id));
      t.rows.push(...heroes.map((h) => heroRow(h, t.is_roll_init)));
      if (asDefault) t.rows_default_party = heroes.map((h) => heroRow(h, false));
      return save();
    },
    combat_get: () => (syncSettings(), t),
    combat_set: ({ tracker }) => {
      t = tracker as Tracker;
      // cells of a new column fill from the stat block (as the service)
      for (const r of t.rows) {
        const mon = BESTIARY.find((m) => m.name === r.name && m.source === r.source) ?? null;
        r.cells.push(...cellsFor(mon).filter((c) => !r.cells.some((x) => x.id === c.id)));
      }
      return save();
    },
    combat_next_turn: () => (shift(true), save()),
    combat_prev_turn: () => (shift(false), save()),
    combat_sort: ({ sort }) => {
      const s = sort as CombatSort;
      if (t.sort === s) t.dir = t.dir === 'ASC' ? 'DESC' : 'ASC';
      else [t.sort, t.dir] = [s, s === 'ALPHA' ? 'ASC' : 'DESC'];
      return save();
    },
    combat_add_player: ({ name }) => {
      edit();
      t.rows.push(blank({ name: (name as string | null) ?? '' }));
      return save();
    },
    combat_add_creature: ({ hash, count, rollHp, customName, side }) => {
      edit();
      syncSettings();
      const mon = synced ? BESTIARY.find((m) => hashOf(m) === hash) : undefined;
      if (!mon) throw { code: 'not_found', message: `not found: creature "${hash}" (sync the bestiary first?)` };
      const n = Number(count) || 1;
      const groupInit = d20() + mod(mon.dex);
      for (let i = 0; i < n; i++) {
        const r = blank({
          name: mon.name, source: mon.source, custom_name: (customName as string | null) || null,
          hp_current: t.is_invert_wound_direction ? 0 : mon.hp, hp_max: mon.hp,
          initiative: t.is_roll_init ? (t.is_roll_groups || i === 0 ? groupInit : d20() + mod(mon.dex)) : null,
          is_player_visible: !t.player_init_hide_new_monster, cells: cellsFor(mon),
          side: side === 'ally' || side === 'neutral' ? side : null,
        });
        r.ordinal = t.rows.filter((x) => similarKey(x) === similarKey(r)).length + 1;
        t.rows.push(r);
      }
      t.is_roll_hp = !!rollHp;
      return save();
    },
    combat_duplicate: ({ id, reroll }) => {
      edit();
      const src = row(id);
      const copy: CombatRow = { ...structuredClone(src), id: newId(), is_active: false, conditions: [] };
      if (reroll) copy.initiative = d20();
      copy.ordinal = t.rows.filter((x) => similarKey(x) === similarKey(copy)).length + 1;
      t.rows.push(copy);
      return save();
    },
    combat_delete: ({ id, similar }) => {
      edit();
      const key = similarKey(row(id));
      t.rows = t.rows.filter((r) => (similar ? similarKey(r) !== key : r.id !== id));
      return save();
    },
    combat_update_row: ({ id, patch }) => {
      edit();
      const r = row(id);
      const p = patch as RowPatch;
      if (p.name !== undefined) r.name = p.name;
      if (p.custom_name !== undefined) r.custom_name = p.custom_name.trim() ? p.custom_name : null;
      if (p.hp_max !== undefined) r.hp_max = p.hp_max;
      if (p.hp_temp !== undefined) r.hp_temp = p.hp_temp > 0 ? p.hp_temp : null;
      if (p.hp_current !== undefined) r.hp_current = p.hp_current;
      if (p.damage !== undefined) r.hp_current = (r.hp_current ?? r.hp_max ?? 0) - p.damage;
      if (p.initiative !== undefined) r.initiative = p.initiative;
      if (p.notes !== undefined) r.notes = p.notes;
      if (p.remove_conditions) r.conditions = r.conditions.filter((c) => !p.remove_conditions!.includes(c.id));
      return save();
    },
    combat_add_condition: ({ rowId, name, color, turns }) => {
      edit();
      const n = String(name).trim();
      const builtin = COMBAT_CONDITIONS.find(([c]) => c.toLowerCase() === n.toLowerCase())?.[1];
      const custom = t.conditions_custom.find((c) => c.name.toLowerCase() === n.toLowerCase())?.color;
      row(rowId).conditions.push({
        id: newId(), name: n, color: (color as string | null) ?? builtin ?? custom ?? '#888888',
        turns: typeof turns === 'number' && turns > 0 ? turns : null,
      });
      return save();
    },
    combat_reset: ({ keepPlayers }) => {
      edit();
      const keep = typeof keepPlayers === 'boolean' ? keepPlayers : settings().combat_reset_keep_players;
      t.rows = keep
        ? t.rows.filter((r) => !r.source).map((r) => ({ ...r, is_active: false }))
        : t.rows_default_party.map((r) => ({ ...structuredClone(r), id: newId(), is_active: false }));
      [t.round, t.sort, t.dir] = [1, 'NUMBER', 'DESC'];
      return save();
    },
    combat_link: () => `${settings().fivetools_base_url}/dmscreen.html#combat=mock${btoa(JSON.stringify({ n: t.round, r: t.rows.length }))}`,
    combat_import_link: ({ text }) => {
      edit();
      const s = String(text);
      if (!s.includes('combat=') && !s.includes('encounter=')) {
        throw { code: 'invalid_input', message: 'invalid input: not a 5etools combat or encounter link' };
      }
      if (s.includes('encounter=')) {
        const before = t.rows.length;
        const goblin = BESTIARY[0];
        for (let i = 0; i < 2; i++) t.rows.push(blank({ name: goblin.name, source: 'MM', hp_current: 7, hp_max: 7, initiative: d20() + 2, ordinal: i + 1 }));
        save();
        return { kind: 'encounter', tracker: t, encounter: { rows_added: t.rows.length - before, missing: ['kraken_mm'], unscaled: [] } };
      }
      return { kind: 'combat', tracker: save(), encounter: null };
    },
    combat_catalog: (): CombatCatalog => ({ conditions: COMBAT_CONDITIONS, columns: COLUMNS }),
    bestiary_status: () => ({
      base_url: settings().fivetools_base_url, synced, files: synced ? 1 : 0, creatures: synced ? BESTIARY.length : 0,
    }),
    bestiary_search: ({ query, limit }): BestiaryEntry[] => {
      if (!synced) return [];
      const q = String(query).trim().toLowerCase();
      const hits = BESTIARY.filter((m) => m.name.toLowerCase().includes(q))
        .sort((a, b) => Number(!a.name.toLowerCase().startsWith(q)) - Number(!b.name.toLowerCase().startsWith(q)));
      return hits.slice(0, (limit as number | null) ?? 50).map((m) => ({
        hash: hashOf(m), name: m.name, source: m.source, cr: m.cr, hp: m.hp, ac: m.ac,
        token_url: null, size: m.name.includes('Dragon') ? 'Huge' : m.name === 'Ogre' ? 'Large' : 'Small',
      }));
    },
    image_source_from_creature: () => {
      throw { code: 'io', message: 'io error: creature images are downloaded in the desktop app' };
    },
    token_sheet_add_creatures: () => {
      throw { code: 'io', message: 'io error: creature tokens are downloaded in the desktop app' };
    },
    bestiary_legendary_group: () => null,
    bestiary_get: ({ hash }) => BESTIARY.find((m) => hashOf(m) === hash) ?? null,
    bestiary_sync: () => {
      const id = `mock-job-bestiary-${++seq}`;
      const base = { id, kind: 'bestiary sync', result: null, error: null };
      emit({ type: 'job', ...base, state: 'running', progress: 0.5, message: 'bestiary-mm.json (1/1)' });
      setTimeout(() => {
        synced = true;
        emit({ type: 'job', ...base, state: 'done', progress: 1, message: '', result: { files: 1, creatures: BESTIARY.length } });
        emit({ type: 'store_changed', entity: 'bestiary', entity_id: 'cache', op: 'update' });
      }, 60);
      return id;
    },
  };
}
