<script lang="ts">
  // Combat (initiative) tracker: a port of the 5etools DM Screen Initiative Tracker.
  // Spec: docs/features/combat-tracker.md. Creatures come from the cached bestiary
  // of the 5etools mirror set in Settings (fivetools_base_url).
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import type {
    BestiaryEntry, BestiaryStatus, CombatCatalog, CombatCell, CombatColumn, CombatImport, CombatRow, CombatSort, Party, Tracker,
  } from '../../lib/types';
  import Modal from '../../lib/components/Modal.svelte';
  import ViewActions from '../../lib/components/ViewActions.svelte';
  import Encounters from './Encounters.svelte';
  import StatBlock from './StatBlock.svelte';
  import Icon from '../../lib/components/Icon.svelte';
  import { difficulty, RATING_LABEL, xpOf } from '../../lib/difficulty';
  import { concentrationDc, conditionTip } from '../../lib/conditions';
  import Parties from './Parties.svelte';

  type Tab = 'tracker' | 'encounters' | 'parties';
  const TABS: [Tab, string][] = [
    ['tracker', 'Tracker'],
    ['encounters', 'Encounters'],
    ['parties', 'Parties'],
  ];
  let tab = $state<Tab>('tracker');

  let t = $state<Tracker | null>(null);
  let catalog = $state<CombatCatalog>({ conditions: [], columns: [] });
  let status = $state<BestiaryStatus | null>(null);
  let syncJobId = $state<string | null>(null);
  const syncJob = $derived(syncJobId ? app.jobs.find((j) => j.id === syncJobId) : undefined);
  /** Rows open for editing (pen): name, HP max and notes. */
  let editing = $state<Set<string>>(new Set());
  // ── reset: a popup with "keep the party" and "don't ask again" (both also in Settings → Combat) ──
  let resetOpen = $state(false);
  let resetKeep = $state(false);
  let resetNoAsk = $state(false);
  function askReset() {
    if (!app.settings.combat_reset_confirm) return void op(api.combatReset(), 'Could not reset');
    resetKeep = app.settings.combat_reset_keep_players;
    resetNoAsk = false;
    resetOpen = true;
  }
  async function doReset() {
    resetOpen = false;
    const patch: Record<string, boolean> = {};
    if (resetKeep !== app.settings.combat_reset_keep_players) patch.combat_reset_keep_players = resetKeep;
    if (resetNoAsk) patch.combat_reset_confirm = false;
    if (Object.keys(patch).length) await app.updateSettings(patch);
    await op(api.combatReset(resetKeep), 'Could not reset');
  }

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
  let parties = $state<Party[]>([]);
  onMount(() => {
    void load();
    void api.partiesList().then((p) => (parties = p)).catch(() => {});
  });

  // refetch after changes from elsewhere (MCP, a finished sync)
  let seenVersion = app.dataVersion;
  $effect(() => {
    if (app.dataVersion !== seenVersion) {
      seenVersion = app.dataVersion;
      if (!saveTimer) void load();
      void api.partiesList().then((p) => (parties = p)).catch(() => {});
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
    if (!r.source) return r.custom_name || r.name || 'Player';
    const n = r.custom_name || r.display_name || r.name || 'Creature';
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
    const before = r.hp_current;
    if (!s) r[field] = null;
    else if (/^[+-]\d+$/.test(s)) r[field] = (r[field] ?? 0) + Number(s);
    else if (/^\d+$/.test(s)) r[field] = Number(s);
    if (field === 'hp_current' && before !== null && r.hp_current !== null) {
      const hurt = t?.is_invert_wound_direction ? r.hp_current - before : before - r.hp_current;
      if (hurt > 0) tookDamage(r, hurt);
    }
    commit();
  }

  // ── concentration reminder ──
  let concAlerts = $state<{ id: number; who: string; damage: number; dc: number }[]>([]);
  let concSeq = 0;
  /** A concentrating combatant took damage: remind the DM to roll the save. */
  function tookDamage(r: CombatRow, damage: number) {
    if (!r.conditions.some((c) => c.name.trim().toLowerCase() === 'concentration')) return;
    const id = ++concSeq;
    concAlerts = [...concAlerts.filter((a) => a.who !== label(r)), { id, who: label(r), damage, dc: concentrationDc(damage) }];
    setTimeout(() => (concAlerts = concAlerts.filter((a) => a.id !== id)), 12000);
  }

  // ── difficulty of the fight: enemies in the tracker vs the heroes in it ──
  $effect(() => {
    for (const r of t?.rows ?? []) if (r.source) fetchMon(r);
  });
  const levelOf = (r: CombatRow) => heroOf(r)?.level ?? null;
  const fight = $derived.by(() => {
    if (!t) return null;
    const enemies = t.rows.filter((r) => r.source && !r.side);
    const heroes = t.rows.filter((r) => !r.source);
    let levels = heroes.map(levelOf).filter((l): l is number => !!l);
    if (!levels.length) {
      // no hero levels in the tracker: fall back on the default party
      const names = t.rows_default_party.map((r) => r.name.trim().toLowerCase());
      const p = parties.find((p) => p.heroes.length && p.heroes.every((h) => names.includes(h.name.trim().toLowerCase())));
      levels = (p?.heroes ?? []).map((h) => h.level ?? 1);
    }
    if (!enemies.length || !levels.length) return null;
    const crOf = (r: CombatRow) => {
      const m = merged(r);
      const cr = m?.cr && typeof m.cr === 'object' ? (m.cr as { cr?: string }).cr : (m?.cr as string | undefined);
      return cr ?? null;
    };
    const lines = enemies.map((r) => ({ xp: xpOf(crOf(r)), count: 1 }));
    const d = difficulty(lines, levels, app.settings.combat_xp_multiplier_2014);
    const max = 2.5 * Math.max(1, d.budget.high);
    const stops = [
      { cls: 'low', label: 'Low', to: d.budget.low },
      { cls: 'moderate', label: 'Moderate', to: d.budget.moderate },
      { cls: 'high', label: 'High', to: d.budget.high },
      { cls: 'deadly', label: 'Deadly', to: 2 * d.budget.high },
      { cls: 'tpk', label: 'Total party kill', to: max },
    ];
    let prev = 0;
    const zones = stops.map((z) => {
      const w = ((z.to - prev) / max) * 100;
      prev = z.to;
      return { ...z, w };
    });
    return {
      d, zones, levels, enemies: enemies.length,
      unknown: lines.filter((l) => l.xp === null).length,
      pos: Math.min(100, (d.adjusted / max) * 100),
    };
  });

  // ── defeated creatures ──
  let defeatedOpen = $state(false);
  let notesOpen = $state(false);
  /** At 0 HP or below (or, tracking damage, at least the max). */
  const isDown = (r: CombatRow) =>
    r.hp_current !== null && r.hp_max !== null && (t?.is_invert_wound_direction ? r.hp_current >= r.hp_max : r.hp_current <= 0);
  /** Sets a row aside; if it was its turn, the turn moves on first. */
  async function defeat(r: CombatRow) {
    if (!t) return;
    if (r.is_active && t.rows.length > 1) await turn(api.combatNextTurn());
    if (!t) return;
    const row = t.rows.find((x) => x.id === r.id);
    if (!row) return;
    t.rows = t.rows.filter((x) => x.id !== row.id);
    t.defeated = [...t.defeated, { ...row, is_active: false }];
    commit();
  }
  function restore(r: CombatRow) {
    if (!t) return;
    t.defeated = t.defeated.filter((x) => x.id !== r.id);
    t.rows = [...t.rows, r];
    commit();
  }
  function removeDefeated(r: CombatRow) {
    if (!t) return;
    t.defeated = t.defeated.filter((x) => x.id !== r.id);
    commit();
  }

  // ── HP steps: clicking the HP (or temp) opens its strip of ± buttons ──
  const HP_STEPS = [-10, -5, -1, 1, 5, 10];
  const TMP_STEPS = [-5, -1, 1, 5, 10];
  let steps = $state<{ id: string; kind: 'hp' | 'tmp' } | null>(null);
  const openSteps = (r: CombatRow, kind: 'hp' | 'tmp') => {
    if (!locked) steps = { id: r.id, kind };
  };
  const stepsOpen = (r: CombatRow, kind: 'hp' | 'tmp') => steps?.id === r.id && steps.kind === kind;
  $effect(() => {
    if (!steps) return;
    const down = (e: PointerEvent) => {
      if (!(e.target as HTMLElement).closest?.('.hp-part, .tmp-part')) steps = null;
    };
    const key = (e: KeyboardEvent) => e.key === 'Escape' && (steps = null);
    window.addEventListener('pointerdown', down, true);
    window.addEventListener('keydown', key);
    return () => {
      window.removeEventListener('pointerdown', down, true);
      window.removeEventListener('keydown', key);
    };
  });
  /** A − button is damage (temp HP first, concentration reminder), + is healing (capped at max). */
  function stepHp(r: CombatRow, d: number) {
    const hurt = t?.is_invert_wound_direction ? d > 0 : d < 0;
    if (hurt) damage(r, Math.abs(d));
    else heal(r, Math.abs(d));
  }
  function stepTemp(r: CombatRow, d: number) {
    r.hp_temp = Math.max(0, (r.hp_temp ?? 0) + d) || null;
    commit();
  }

  /** ↑/↓ step a focused number by 1, Shift by 5, Ctrl by 10; 0 for other keys. */
  function arrowStep(e: KeyboardEvent): number {
    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return 0;
    e.preventDefault();
    const n = e.ctrlKey || e.metaKey ? 10 : e.shiftKey ? 5 : 1;
    return e.key === 'ArrowUp' ? n : -n;
  }
  function arrowHp(r: CombatRow, e: KeyboardEvent) {
    const d = arrowStep(e);
    if (!d) return;
    const cur = r.hp_current ?? (t?.is_invert_wound_direction ? 0 : r.hp_max ?? 0);
    setHp(r, 'hp_current', String(Math.max(t?.is_invert_wound_direction ? 0 : -999, cur + d)));
  }
  function arrowTemp(r: CombatRow, e: KeyboardEvent) {
    const d = arrowStep(e);
    if (d) setTemp(r, String(Math.max(0, (r.hp_temp ?? 0) + d)));
  }

  /** Damage: temporary HP soak it first; a concentrating target gets a reminder. */
  function damage(r: CombatRow, n: number) {
    if (n <= 0) return;
    tookDamage(r, n);
    let rest = n;
    if (r.hp_temp) {
      const soak = Math.min(r.hp_temp, rest);
      r.hp_temp = r.hp_temp - soak || null;
      rest -= soak;
    }
    if (t?.is_invert_wound_direction) r.hp_current = (r.hp_current ?? 0) + rest;
    else r.hp_current = (r.hp_current ?? r.hp_max ?? 0) - rest;
    commit();
  }
  /** Healing never goes above the max HP. */
  function heal(r: CombatRow, n: number) {
    if (n <= 0) return;
    if (t?.is_invert_wound_direction) r.hp_current = Math.max(0, (r.hp_current ?? 0) - n);
    else r.hp_current = Math.min(r.hp_max ?? Infinity, (r.hp_current ?? 0) + n);
    commit();
  }

  /** Share of HP left, 0–100 (as damage: of HP not yet lost). */
  function hpPct(r: CombatRow): number {
    if (r.hp_current === null || !r.hp_max) return 0;
    const cur = t?.is_invert_wound_direction ? r.hp_max - r.hp_current : r.hp_current;
    return Math.min(100, Math.max(0, (cur / r.hp_max) * 100));
  }
  /** Green when full, yellow above 50%, orange above 25%, red below. */
  const hpColor = (pct: number) => (pct >= 100 ? '#2fb344' : pct > 50 ? '#d6c21a' : pct > 25 ? '#f08c00' : '#d9381e');

  function setTemp(r: CombatRow, raw: string) {
    const v = Math.round(Number(raw));
    r.hp_temp = raw.trim() && v > 0 ? v : null;
    commit();
  }

  function setInit(r: CombatRow, raw: string) {
    r.initiative = raw.trim() === '' ? null : Math.round(Number(raw)) || 0;
    commit();
  }

  const cellFor = (r: CombatRow, c: CombatColumn): CombatCell => r.cells.find((x) => x.id === c.id) ?? { id: c.id, value: null };
  const isCheckbox = (c: CombatColumn) => c.populate_with.startsWith('cb');

  function toggleEdit(id: string) {
    const s = new Set(editing);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    editing = s;
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
    rollHp = !!app.settings.combat_roll_hp;
    void search();
  }
  function search() {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(async () => {
      try {
        hits = await api.bestiarySearch(query, 200);
        if (pick && !hits.some((h) => h.hash === pick!.hash)) pick = null;
      } catch (e) {
        app.error('Search failed', e);
      }
    }, 120);
  }
  type Side = 'enemy' | 'ally' | 'neutral';
  const SIDES: [Side, string][] = [['enemy', 'Enemies'], ['ally', 'Allies'], ['neutral', 'Neutral']];
  let side = $state<Side>('enemy');
  const CR_BANDS: [string, string][] = [['low', 'CR 0–1'], ['mid', 'CR 2–5'], ['high', 'CR 6–10'], ['epic', 'CR 11+']];
  let crFilter = $state('');
  let srcFilter = $state('');
  const crNum = (cr: string | null) => {
    if (!cr) return NaN;
    const [a, b] = cr.split('/').map(Number);
    return b ? a / b : a;
  };
  const sources = $derived([...new Set(hits.map((h) => h.source))].sort());
  const shownHits = $derived(
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
  let pickMon = $state<Record<string, unknown> | null>(null);
  function choose(h: BestiaryEntry) {
    pick = h;
    pickMon = null;
    void api.bestiaryGet(h.hash).then((m) => {
      if (pick?.hash === h.hash) pickMon = m;
    });
  }
  async function addCreature() {
    if (!pick) return;
    await op(api.combatAddCreature(pick.hash, Math.max(1, Math.min(100, count || 1)), rollHp, customName.trim() || null, side), 'Could not add the creature');
    addOpen = false;
    [pick, count, customName] = [null, 1, ''];
  }

  // ── conditions ──
  let condRow = $state<CombatRow | null>(null);
  let condTurns = $state<number | null>(null);
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

  // ── stat blocks (side panel and the ⓘ popovers) ──
  const hashOf = (r: CombatRow) => encodeURIComponent(`${r.name}_${r.source}`.toLowerCase());
  let mons = $state<Record<string, Record<string, unknown> | null>>({});
  function fetchMon(r: CombatRow) {
    if (!r.source) return;
    const h = hashOf(r);
    if (h in mons) return;
    mons[h] = null;
    void api.bestiaryGet(h).then((m) => (mons[h] = m)).catch(() => {});
  }
  const monOf = (r: CombatRow) => (r.source ? mons[hashOf(r)] ?? null : null);
  const heroOf = (r: CombatRow) =>
    r.source ? null : parties.flatMap((p) => p.heroes).find((h) => h.name.trim().toLowerCase() === r.name.trim().toLowerCase()) ?? null;
  /** The panel shows a clicked row, else whoever's turn it is. */
  let inspectId = $state<string | null>(null);
  const focus = $derived(t?.rows.find((r) => r.id === inspectId) ?? t?.rows.find((r) => r.is_active) ?? null);
  const inspecting = $derived(!!inspectId && !!t?.rows.some((r) => r.id === inspectId));
  $effect(() => {
    if (focus) fetchMon(focus);
  });
  function infoLines(r: CombatRow): string[] {
    const m = monOf(r);
    if (m) {
      const cr = m.cr && typeof m.cr === 'object' ? (m.cr as { cr?: string }).cr : m.cr;
      const ac = Array.isArray(m.ac) ? (typeof m.ac[0] === 'object' ? (m.ac[0] as { ac?: number }).ac : m.ac[0]) : '';
      const hp = m.hp as { average?: number; formula?: string } | undefined;
      const type = typeof m.type === 'string' ? m.type : (m.type as { type?: string } | undefined)?.type ?? '';
      return [`${r.name} (${r.source})`, [type, cr != null ? `CR ${cr}` : ''].filter(Boolean).join(' · '), `AC ${ac} · HP ${hp?.average ?? '?'}${hp?.formula ? ` (${hp.formula})` : ''}`];
    }
    if (r.source) return [`${r.name} (${r.source})`, 'Not in the bestiary cache'];
    const h = heroOf(r);
    const lines = [r.display_name ?? ''];
    if (h) {
      lines.push([h.ac != null ? `AC ${h.ac}` : '', h.passive_perception != null ? `PP ${h.passive_perception}` : '', h.speed != null ? `${h.speed} ft.` : '', h.spell_dc != null ? `DC ${h.spell_dc}` : ''].filter(Boolean).join(' · '));
      if (h.player) lines.push(`Player: ${h.player}`);
    }
    return lines.filter(Boolean);
  }

  // ── side panel width (dragged; remembered on this computer) ──
  const PANEL_KEY = 'combat.panelWidth';
  let panelW = $state(460);
  try {
    const w = Number(localStorage.getItem(PANEL_KEY));
    if (w >= 300 && w <= 1000) panelW = w;
  } catch {
    /* storage unavailable: keep the default */
  }
  function startResize(e: PointerEvent) {
    const x0 = e.clientX;
    const w0 = panelW;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => (panelW = Math.min(1000, Math.max(300, w0 + (x0 - ev.clientX))));
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      try {
        localStorage.setItem(PANEL_KEY, String(Math.round(panelW)));
      } catch {
        /* ignore */
      }
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }
  const MIN_KEY = 'combat.panelMin';
  let panelMin = $state(false);
  try {
    panelMin = localStorage.getItem(MIN_KEY) === '1';
  } catch {
    /* storage unavailable */
  }
  function setPanelMin(v: boolean) {
    panelMin = v;
    try {
      localStorage.setItem(MIN_KEY, v ? '1' : '0');
    } catch {
      /* ignore */
    }
  }
  function resizeKey(e: KeyboardEvent) {
    if (e.key === 'ArrowLeft') panelW = Math.min(1000, panelW + 20);
    else if (e.key === 'ArrowRight') panelW = Math.max(300, panelW - 20);
  }

  // ── edit one creature: a stats form, or its 5etools JSON ──
  const ABIL = ['str', 'dex', 'con', 'int', 'wis', 'cha'] as const;
  const SPEEDS = ['walk', 'fly', 'swim', 'climb', 'burrow'] as const;
  const LISTS: [string, string][] = [
    ['resist', 'Damage resistances'], ['immune', 'Damage immunities'], ['vulnerable', 'Vulnerabilities'],
    ['conditionImmune', 'Condition immunities'], ['senses', 'Senses'], ['languages', 'Languages'],
  ];
  type StatForm = {
    id: string; name: string; ac: string; hp_current: string; hp_max: string; initiative: string; cr: string;
    passive: string; size: string; type: string; speed: Record<string, string>; abil: Record<string, string>;
    save: string; skill: string; lists: Record<string, string>;
  };
  let statEdit = $state<StatForm | null>(null);
  let statTab = $state<'stats' | 'json'>('stats');
  let jsonText = $state('');
  let jsonError = $state('');
  const statOpen = {
    get value() {
      return statEdit !== null;
    },
    set value(v: boolean) {
      if (!v) statEdit = null;
    },
  };
  /** The creature's stat block with this row's own changes on top. */
  function merged(r: CombatRow): Record<string, unknown> | null {
    const m = monOf(r);
    return m || r.overrides ? { ...(m ?? {}), ...(r.overrides ?? {}) } : null;
  }
  const numStr = (v: unknown) => (v === null || v === undefined ? '' : String(v));
  /** 5etools lists (strings or {special}/{resist: [...]} objects) as plain comma text. */
  const listText = (v: unknown): string =>
    Array.isArray(v)
      ? v.map((x) => (typeof x === 'string' ? x : typeof x === 'object' && x ? Object.values(x as object).flat().filter((y) => typeof y === 'string').join(' ') : '')).filter(Boolean).join(', ')
      : '';
  const kvText = (v: unknown) => (v && typeof v === 'object' ? Object.entries(v as Record<string, unknown>).map(([k, x]) => `${k} ${x}`).join(', ') : '');
  function openStatEdit(r: CombatRow) {
    const m = merged(r) ?? {};
    const ac = Array.isArray(m.ac) ? (typeof m.ac[0] === 'object' ? (m.ac[0] as { ac?: number }).ac : m.ac[0]) : m.ac;
    const sp = (m.speed ?? {}) as Record<string, unknown>;
    const spd = (k: string) => numStr(typeof sp[k] === 'object' && sp[k] ? (sp[k] as { number?: number }).number : sp[k]);
    const cr = m.cr && typeof m.cr === 'object' ? (m.cr as { cr?: string }).cr : m.cr;
    statEdit = {
      id: r.id, name: r.custom_name ?? '', ac: numStr(ac), hp_current: numStr(r.hp_current), hp_max: numStr(r.hp_max),
      initiative: numStr(r.initiative), cr: numStr(cr), passive: numStr(m.passive),
      size: Array.isArray(m.size) ? (m.size as string[]).join('/') : numStr(m.size),
      type: typeof m.type === 'string' ? m.type : numStr((m.type as { type?: string } | undefined)?.type),
      speed: Object.fromEntries(SPEEDS.map((k) => [k, spd(k)])),
      abil: Object.fromEntries(ABIL.map((a) => [a, numStr(m[a])])),
      save: kvText(m.save), skill: kvText(m.skill),
      lists: Object.fromEntries(LISTS.map(([k]) => [k, listText(m[k])])),
    };
    statTab = 'stats';
    jsonText = JSON.stringify(m, null, 2);
    jsonError = '';
  }
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

  /** Keeps the tracker row (name, HP, AC and speed columns) in step with the stats. */
  function syncRow(r: CombatRow, stats: Record<string, unknown>) {
    if (!t) return;
    const ac = Array.isArray(stats.ac) ? (typeof stats.ac[0] === 'object' ? (stats.ac[0] as { ac?: number }).ac : stats.ac[0]) : null;
    const walk = (stats.speed as Record<string, unknown> | undefined)?.walk;
    for (const c of t.stats_cols) {
      const cell = r.cells.find((x) => x.id === c.id);
      if (!cell) continue;
      if (c.populate_with === 'armorClass' && typeof ac === 'number') cell.value = ac;
      if (c.populate_with === 'speed' && typeof walk === 'number') cell.value = walk;
      if (c.populate_with === 'passivePerception' && typeof stats.passive === 'number') cell.value = stats.passive;
    }
  }

  function saveStatEdit() {
    const e = statEdit;
    const r = t?.rows.find((x) => x.id === e?.id);
    if (!e || !r || !t) return;
    if (statTab === 'json') return saveJson(r);
    const n = (v: string) => (v.trim() === '' ? null : Math.round(Number(v)));
    const o: Record<string, unknown> = { ...(r.overrides ?? {}) };
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
    const save = kvParse(e.save);
    if (save) o.save = save;
    else if (!e.save.trim()) delete o.save;
    const skill = kvParse(e.skill);
    if (skill) o.skill = skill;
    else if (!e.skill.trim()) delete o.skill;
    for (const [k] of LISTS) {
      const v = listParse(e.lists[k]);
      if (v.length) o[k] = v;
      else delete o[k];
    }
    r.overrides = o;
    r.custom_name = e.name.trim() || null;
    r.hp_max = n(e.hp_max);
    r.hp_current = n(e.hp_current);
    r.initiative = n(e.initiative);
    syncRow(r, { ...(monOf(r) ?? {}), ...o });
    statEdit = null;
    commit();
  }

  /** JSON tab: the edited stat block is stored as the fields that differ from the bestiary. */
  function saveJson(r: CombatRow) {
    let v: Record<string, unknown>;
    try {
      v = JSON.parse(jsonText);
      if (!v || typeof v !== 'object' || Array.isArray(v)) throw new Error('expected a JSON object');
    } catch (err) {
      jsonError = (err as Error).message;
      return;
    }
    const base = monOf(r) ?? {};
    const o: Record<string, unknown> = {};
    for (const [k, x] of Object.entries(v)) if (JSON.stringify(base[k]) !== JSON.stringify(x)) o[k] = x;
    r.overrides = Object.keys(o).length ? o : null;
    if (typeof v.name === 'string' && v.name !== base.name) r.custom_name = v.name;
    const hp = (v.hp as { average?: number } | undefined)?.average;
    if (typeof hp === 'number' && JSON.stringify(v.hp) !== JSON.stringify(base.hp)) {
      r.hp_max = hp;
      if (r.hp_current === null || r.hp_current > hp) r.hp_current = hp;
    }
    syncRow(r, v);
    statEdit = null;
    commit();
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
  function resetToBestiary() {
    const r = t?.rows.find((x) => x.id === statEdit?.id);
    if (!r) return;
    r.overrides = null;
    syncRow(r, monOf(r) ?? {});
    statEdit = null;
    commit();
    app.toast('success', 'Back to the bestiary stats');
  }
  const statRow = $derived(statEdit ? t?.rows.find((x) => x.id === statEdit!.id) ?? null : null);

  // ── new-round splash ──
  let splash = $state<number | null>(null);
  let splashTimer: ReturnType<typeof setTimeout> | null = null;
  async function turn(p: Promise<Tracker>) {
    inspectId = null;
    const before = t?.round ?? 1;
    await op(p);
    const newRound = !!t && t.round > before;
    if (t && newRound) {
      splash = t.round;
      if (splashTimer) clearTimeout(splashTimer);
      splashTimer = setTimeout(() => (splash = null), 1700);
    }
    showTurnCard(newRound ? 1700 : 0);
  }

  // ── turn card: whose turn it is and who comes next ──
  type Who = { name: string; count: number; hero: boolean; side: CombatRow['side'] };
  let turnCard = $state<{ id: number; now: Who; next: Who | null; round: number } | null>(null);
  let turnSeq = 0;
  /** A turn group: copies acting together show once, with a count. */
  function who(rows: CombatRow[]): Who {
    const r = rows[0];
    const n = r.custom_name || r.display_name || r.name || (r.source ? 'Creature' : 'Player');
    return { name: r.source ? n : r.name || 'Player', count: rows.length, hero: !r.source, side: r.side };
  }
  function showTurnCard(delay: number) {
    if (!t || !app.settings.combat_turn_popup) return;
    const rows = t.rows;
    const act = rows.filter((r) => r.is_active);
    if (!act.length) return;
    const last = rows.findLastIndex((r) => r.is_active);
    let next: Who | null = null;
    if (rows.length > act.length) {
      const first = rows[(last + 1) % rows.length];
      const group = rows.filter((r) => !r.is_active && r.source && r.source === first.source && r.name === first.name
        && r.custom_name === first.custom_name && (t!.sort !== 'NUMBER' || r.initiative === first.initiative));
      next = who(group.length ? group : [first]);
    }
    const id = ++turnSeq;
    const card = { id, now: who(act), next, round: t.round };
    setTimeout(() => {
      if (turnSeq !== id) return;
      turnCard = card;
      setTimeout(() => {
        if (turnCard?.id === id) turnCard = null;
      }, 3200);
    }, delay);
  }

  function started(r: CombatImport) {
    setTracker(r.tracker);
    tab = 'tracker';
  }
  const active = $derived(t?.rows.filter((r) => r.is_active) ?? []);
  const sortLabel = (s: CombatSort) => (t?.sort === s ? (t.dir === 'ASC' ? ' ↑' : ' ↓') : '');
</script>

<ViewActions slot="nav">
  <div class="segmented" role="tablist" aria-label="Combat views">
    {#each TABS as [id, l] (id)}
      <button role="tab" aria-selected={tab === id} onclick={() => (tab = id)}>{l}</button>
    {/each}
  </div>
</ViewActions>
{#if tab === 'tracker'}
<ViewActions>
  <button class="btn" onclick={() => (linkOpen = true)} disabled={locked} title="Open a 5etools #combat= or #encounter= link">Open link…</button>
  <button class="btn" onclick={copyLink} title="Copy a 5etools DM Screen link that restores this combat">Copy combat link</button>
  <button class="btn primary" onclick={() => turn(api.combatNextTurn())} disabled={!t?.rows.length}>Next turn</button>
</ViewActions>
{/if}

{#if tab === 'encounters'}
  <Encounters onStarted={started} />
{:else if tab === 'parties'}
  <Parties onAdded={(x) => { setTracker(x); tab = 'tracker'; }} />
{:else}
<div class="view" style:--inspector-w="{panelMin ? 44 : panelW}px">
  <section class="work" aria-label="Combat tracker">
    <div class="view-toolbar">
      <button class="btn" onclick={() => op(api.combatPrevTurn())} disabled={!t?.rows.length} aria-label="Previous turn">◀ Prev</button>
      <button class="btn" onclick={() => turn(api.combatNextTurn())} disabled={!t?.rows.length} aria-label="Next turn">Next ▶</button>
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
      {#if t}
        <button class="btn" class:danger={locked} aria-pressed={locked} onclick={() => { t!.is_locked = !t!.is_locked; commit(); }}
          title="While locked, rows can't be added, removed or edited">{locked ? '🔒 Locked' : '🔓 Lock'}</button>
      {/if}
      <span class="grow"></span>
      {#if t}
        <button class="btn" class:has-notes={!!t.notes.trim()} onclick={() => (notesOpen = true)} title="Notes for the whole encounter">Encounter notes{t.notes.trim() ? ' •' : ''}</button>
        <button class="btn ghost" disabled={locked} onclick={askReset} title="Start over at round 1">Reset{app.settings.combat_reset_confirm ? '…' : ''}</button>
      {/if}
    </div>

    <div class="stage-area stage">
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
              <tr class:active={r.is_active} class:defeated={w.text === 'Defeated'} class:hero={!r.source} class:monster={!!r.source && !r.side} class:ally={r.side === 'ally'} class:neutral={r.side === 'neutral'} data-testid="combat-row">
                <td class="name">
                  {#if editing.has(r.id) && !locked}
                    {#if r.source}
                      <input class="input" aria-label="Name of {label(r)}" placeholder={r.name} value={r.custom_name ?? ''}
                        onchange={(e) => { r.custom_name = e.currentTarget.value.trim() || null; commit(); }} />
                    {:else}
                      <input class="input" aria-label="Player name" placeholder="Player" bind:value={r.name} onchange={commit} />
                    {/if}
                  {:else}
                    <button class="cname" class:inspected={inspectId === r.id} onclick={() => (inspectId = inspectId === r.id ? null : r.id)}
                      title="Show in the side panel">{label(r)}</button>
                    {#if r.source}<span class="src">{r.source}</span>{/if}
                    <button type="button" class="info" aria-label="Details of {label(r)}" onmouseenter={() => fetchMon(r)} onfocus={() => fetchMon(r)}>
                      i
                      <span class="info-pop">{#each infoLines(r) as l, i (i)}<span class:first={i === 0}>{l}</span>{:else}<span>No details</span>{/each}</span>
                    </button>
                  {/if}
                </td>
                <td class="conds-td"><div class="conds">
                  {#each r.conditions as c (c.id)}
                    <span class="chip-wrap">
                      <button class="chip" style="--c: {c.color}" aria-label="Remove {c.name}" onclick={() => removeCondition(r, c.id)} disabled={locked}>
                        {c.name}{c.turns !== null ? ` (${c.turns})` : ''}
                      </button>
                      <span class="tip" role="tooltip">
                        <b>{c.name}</b>{c.turns !== null ? ` · ${c.turns} turn(s) left` : ''}
                        {#if conditionTip(c.name)}<span>{conditionTip(c.name)}</span>{/if}
                        <small>Click to remove</small>
                      </span>
                    </span>
                  {/each}
                  <button class="btn ghost icon add-cond" aria-label="Add a condition to {label(r)}" onclick={() => { condRow = r; condTurns = null; }} disabled={locked}>+</button>
                </div></td>
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
                <td class="hp">
                  <div class="hp-cell">
                    {#if isDown(r)}
                      <button class="skull" aria-label="Set {label(r)} aside as defeated" title="Defeated: move to the list below the tracker"
                        onclick={() => defeat(r)} disabled={locked}><Icon name="skull" size={18} /></button>
                    {:else}
                      <span class="skull-slot" aria-hidden="true"></span>
                    {/if}
                    <div class="hp-part" title={w.text} role="presentation" onclick={() => openSteps(r, 'hp')}>
                      <div class="hp-inputs">
                        <input class="input num" aria-label="Current HP of {label(r)}" value={r.hp_current ?? ''} onchange={(e) => setHp(r, 'hp_current', e.currentTarget.value)}
                          onkeydown={(e) => arrowHp(r, e)} title="↑/↓ ±1 · Shift ±5 · Ctrl ±10" disabled={locked} />
                        <span class="slash">/</span>
                        <input class="input num" aria-label="Max HP of {label(r)}" value={r.hp_max ?? ''} onchange={(e) => setHp(r, 'hp_max', e.currentTarget.value)}
                          disabled={locked || !editing.has(r.id)} title={editing.has(r.id) ? 'Max HP' : 'Click ✎ to change the max HP'} />
                      </div>
                      <span class="hpbar" aria-hidden="true"><span style:width="{hpPct(r)}%" style:background={hpColor(hpPct(r))}></span></span>
                      {#if stepsOpen(r, 'hp')}
                        <div class="pop" class:below={t.rows[0]?.id === r.id}>
                          <span class="steps" role="group" aria-label="Change the HP of {label(r)}">
                            {#each HP_STEPS as d (d)}
                              <button class="btn ghost step" class:minus={d < 0} aria-label="{d > 0 ? '+' : ''}{d} HP to {label(r)}" onclick={() => stepHp(r, d)}>{d > 0 ? '+' : ''}{d}</button>
                            {/each}
                          </span>
                        </div>
                      {/if}
                    </div>
                    <div class="tmp-part" role="presentation" onclick={() => openSteps(r, 'tmp')}>
                      <input class="input num temp" class:has={!!r.hp_temp} aria-label="Temporary HP of {label(r)}" placeholder="+tmp" title="Temporary hit points (lost first)"
                        value={r.hp_temp ?? ''} onchange={(e) => setTemp(r, e.currentTarget.value)} onkeydown={(e) => arrowTemp(r, e)} disabled={locked} />
                      <span class="hpbar tmpbar" aria-hidden="true"><span style:width="{r.hp_temp && r.hp_max ? Math.min(100, (r.hp_temp / r.hp_max) * 100) : r.hp_temp ? 100 : 0}%"></span></span>
                      {#if stepsOpen(r, 'tmp')}
                        <div class="pop" class:below={t.rows[0]?.id === r.id}>
                          <span class="steps tmp-steps" role="group" aria-label="Change the temporary HP of {label(r)}">
                            {#each TMP_STEPS as d (d)}
                              <button class="btn ghost step" aria-label="{d > 0 ? '+' : ''}{d} temporary HP to {label(r)}" onclick={() => stepTemp(r, d)}>{d > 0 ? '+' : ''}{d}</button>
                            {/each}
                          </span>
                        </div>
                      {/if}
                    </div>
                  </div>
                </td>
                <td><input class="input num" aria-label="Initiative of {label(r)}" value={r.initiative ?? ''} onchange={(e) => setInit(r, e.currentTarget.value)} disabled={locked} /></td>
                <td class="acts">
                  <button class="btn ghost icon" class:on={!!r.notes} aria-pressed={editing.has(r.id)} aria-label="Edit {label(r)}" title="Edit name and max HP" onclick={() => toggleEdit(r.id)} disabled={locked}><Icon name="pencil" size={17} /></button>
                  {#if r.source}
                    <button class="btn ghost icon" aria-label="Add another {r.name}" title="Add another (Shift: reroll initiative and HP)" disabled={locked}
                      onclick={(e) => op(api.combatDuplicate(r.id, e.shiftKey))}><Icon name="copy" size={17} /></button>
                  {:else}
                    <span class="slot" aria-hidden="true"></span>
                  {/if}
                  <button class="btn ghost icon danger" aria-label="Delete {label(r)}" title="Delete (Shift: every copy)" disabled={locked}
                    onclick={(e) => op(api.combatDelete(r.id, e.shiftKey))}>✕</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
      {#if (t && t.defeated.length) || fight}
      <div class="dock">
      {#if t && t.defeated.length}
        <details class="defeated" bind:open={defeatedOpen}>
          <summary><Icon name="skull" size={18} /> Defeated ({t.defeated.length})</summary>
          <ul>
            {#each t.defeated as r (r.id)}
              <li>
                <span class="d-name">{label(r)}</span>
                {#if r.source}<span class="src">{r.source}</span>{/if}
                <span class="muted d-hp">HP {r.hp_current ?? '—'} / {r.hp_max ?? '—'}</span>
                <span class="grow"></span>
                <button class="btn ghost" onclick={() => restore(r)} disabled={locked} title="Back into the turn order"><Icon name="undo" size={16} /> Restore</button>
                <button class="btn ghost icon danger" aria-label="Remove {label(r)} for good" onclick={() => removeDefeated(r)} disabled={locked}>✕</button>
              </li>
            {/each}
          </ul>
        </details>
      {/if}
      {#if fight}
        <div class="diffbar" data-testid="combat-difficulty">
          <span class="db-title">Difficulty</span>
          <span class="db-rating rating-{fight.d.rating}">{RATING_LABEL[fight.d.rating]}</span>
          <div class="db-track" role="img"
            aria-label="{RATING_LABEL[fight.d.rating]}: {fight.d.adjusted} XP against Low {fight.d.budget.low}, Moderate {fight.d.budget.moderate}, High {fight.d.budget.high}"
            title="Low ≤ {fight.d.budget.low.toLocaleString()} · Moderate ≤ {fight.d.budget.moderate.toLocaleString()} · High ≤ {fight.d.budget.high.toLocaleString()} · Deadly ≤ {(2 * fight.d.budget.high).toLocaleString()} · TPK above">
            {#each fight.zones as z (z.cls)}<span class="db-zone {z.cls}" style:width="{z.w}%"></span>{/each}
            <span class="db-marker" style:left="{fight.pos}%"></span>
          </div>
          <span class="db-xp" title="{fight.enemies} enemies vs {fight.levels.length} heroes (levels {fight.levels.join(', ')}){fight.unknown ? `; ${fight.unknown} without CR` : ''}">
            {fight.d.adjusted.toLocaleString()} XP{fight.d.multiplier !== 1 ? ` ×${fight.d.multiplier}` : ''}
          </span>
        </div>
      {/if}
      </div>
      {/if}
    </div>
    <div class="statusbar" data-testid="combat-status">
      <span>Round {t?.round ?? 1}</span>
      <span>{active.length ? `Turn: ${active.map(label).join(', ')}` : t?.rows.length ? 'Press Next turn to start' : ''}</span>
      <span>{t?.rows.length ?? 0} combatant(s)</span>
    </div>
  </section>

  <aside class="inspector panel" class:mini={panelMin} aria-label="Current turn">
    {#if panelMin}
      <button class="panel-expand" onclick={() => setPanelMin(false)} aria-label="Show the stat block panel" title="Show the stat block">
        <span aria-hidden="true">‹</span>
        <span class="panel-expand-label">{focus ? label(focus) : 'Stat block'}</span>
      </button>
    {:else}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div class="resizer" role="separator" aria-orientation="vertical" aria-label="Resize the stat block panel" aria-valuenow={Math.round(panelW)}
        aria-valuemin={300} aria-valuemax={1000} tabindex="0" onpointerdown={startResize} onkeydown={resizeKey}></div>
      <div class="panel-head">
        <span class="panel-title">{inspecting ? 'Selected' : 'Current turn'}</span>
        {#if inspecting}<button class="btn ghost" onclick={() => (inspectId = null)} title="Back to whoever's turn it is">Current turn</button>{/if}
        {#if focus?.source}
          <button class="btn edit-btn" onclick={() => openStatEdit(focus!)} disabled={locked} title="Edit this creature's stats or its 5etools JSON (only this one)"><Icon name="pencil" size={16} /> Edit</button>
        {/if}
        <button class="btn ghost icon" onclick={() => setPanelMin(true)} aria-label="Minimize the stat block panel" title="Minimize">›</button>
      </div>
      {#if focus}
        <label class="row-notes">
          <span>Notes</span>
          <textarea class="input" aria-label="Notes for {label(focus)}" placeholder="Notes for {label(focus)}…" bind:value={focus.notes} oninput={commit} disabled={locked}></textarea>
        </label>
        <StatBlock mon={merged(focus)} hero={heroOf(focus)} name={label(focus)} sub={focus.display_name ?? undefined} />
        {#if active.length > 1}<p class="note">Also acting: {active.slice(1).map(label).join(', ')}</p>{/if}
      {:else}
        <p class="note">The stat block of whoever's turn it is shows here. Press <b>Next turn</b> to start.</p>
      {/if}
    {/if}
  </aside>
</div>

{/if}

<Modal title="Encounter notes" bind:open={notesOpen} width={720}>
  {#if t}
    <textarea class="input notes-pop" aria-label="Encounter notes" placeholder="Read-aloud text, lair actions, loot, reminders…" bind:value={t.notes} oninput={commit} disabled={locked}></textarea>
  {/if}
</Modal>

<Modal title="Reset the combat?" bind:open={resetOpen} width={480}>
  <p class="reset-text">
    {resetKeep ? 'Creatures are removed and your players stay; the round goes back to 1.' : 'Every row is replaced by the default party; the round goes back to 1.'}
  </p>
  <label class="check reset-check"><input type="checkbox" bind:checked={resetKeep} /> Keep the party (remove only creatures)</label>
  <label class="check reset-check"><input type="checkbox" bind:checked={resetNoAsk} /> Don't ask again</label>
  <p class="muted reset-hint">Both can be changed later in Settings → Combat.</p>
  {#snippet footer()}
    <button class="btn" onclick={() => (resetOpen = false)}>Cancel</button>
    <button class="btn danger" onclick={doReset}>Reset</button>
  {/snippet}
</Modal>

{#if concAlerts.length}
  <div class="conc-stack" role="alert" aria-live="assertive">
    {#each concAlerts as a (a.id)}
      <div class="conc" data-testid="concentration-alert">
        <span class="conc-icon" aria-hidden="true">🎯</span>
        <span class="conc-text"><b>{a.who}</b> is concentrating and took {a.damage} damage.<br />Constitution save, <b>DC {a.dc}</b>.</span>
        <button class="btn ghost icon" aria-label="Dismiss" onclick={() => (concAlerts = concAlerts.filter((x) => x.id !== a.id))}>✕</button>
      </div>
    {/each}
  </div>
{/if}

{#if turnCard}
  {@const c = turnCard}
  <div class="turn-card" role="status" aria-live="polite" data-testid="turn-card">
    <div class="tc-now" class:hero={c.now.hero} class:ally={c.now.side === 'ally'} class:neutral={c.now.side === 'neutral'}>
      <span class="tc-label">Round {c.round} · Now</span>
      <span class="tc-name">{c.now.name}{c.now.count > 1 ? ` ×${c.now.count}` : ''}</span>
    </div>
    {#if c.next}
      <div class="tc-next" class:hero={c.next.hero} class:ally={c.next.side === 'ally'} class:neutral={c.next.side === 'neutral'}>
        <span class="tc-label">Next</span>
        <span class="tc-name small">{c.next.name}{c.next.count > 1 ? ` ×${c.next.count}` : ''}</span>
      </div>
    {/if}
  </div>
{/if}

{#if splash !== null}
  <div class="splash" aria-live="polite" data-testid="round-splash"><span class="splash-label">Round</span><span class="splash-num">{splash}</span></div>
{/if}

<Modal title="Add creature" bind:open={addOpen} width={1000}>
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
        {#each shownHits as h (h.hash)}
          <button class="list-row pick-row" role="option" aria-selected={pick?.hash === h.hash} class:selected={pick?.hash === h.hash}
            onclick={() => choose(h)} ondblclick={addCreature}>
            <span class="list-thumb">{#if h.token_url}<img src={h.token_url} alt="" loading="lazy" />{:else}<span aria-hidden="true">🐉</span>{/if}</span>
            <span class="title-cell"><span class="list-title">{h.name}</span><span class="list-sub">{h.source}</span></span>
            <span class="list-pill">CR {h.cr ?? '—'}</span>
            <span class="list-muted num">HP {h.hp ?? '—'}</span>
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
      <label class="check"><input type="checkbox" bind:checked={rollHp} /> Roll HP</label>
      <span class="grow"></span>
      <button class="btn primary" disabled={!pick} onclick={addCreature}>Add {count > 1 ? `${count} × ` : ''}{pick?.name ?? 'creature'}</button>
    </div>
  {/snippet}
</Modal>

<Modal title="Edit {statRow ? label(statRow) : 'creature'}" bind:open={statOpen.value} width={980}>
  {#if statEdit && statRow}
    <div class="segmented ed-tabs" role="tablist" aria-label="Edit as">
      <button role="tab" aria-selected={statTab === 'stats'} onclick={() => (statTab = 'stats')}><Icon name="pencil" size={15} /> Stats</button>
      <button role="tab" aria-selected={statTab === 'json'} onclick={() => { statTab = 'json'; jsonText = JSON.stringify(merged(statRow!) ?? {}, null, 2); }}><Icon name="braces" size={15} /> 5etools JSON</button>
    </div>
    <p class="note">Changes apply to this {statRow.name} only and update its row in the tracker.{statRow.overrides ? ' It already has changes from the bestiary.' : ''}</p>
    {#if statTab === 'stats'}
      <div class="stat-form">
        <fieldset>
          <legend>Basics</legend>
          <label class="field wide">Name <input class="input" placeholder={statRow.name} bind:value={statEdit.name} /></label>
          <label class="field">Size <input class="input" placeholder="M" bind:value={statEdit.size} /></label>
          <label class="field">Type <input class="input" placeholder="humanoid" bind:value={statEdit.type} /></label>
          <label class="field">CR <input class="input" bind:value={statEdit.cr} /></label>
        </fieldset>
        <fieldset>
          <legend>Combat</legend>
          <label class="field">AC <input class="input" inputmode="numeric" bind:value={statEdit.ac} /></label>
          <label class="field">HP <input class="input" inputmode="numeric" bind:value={statEdit.hp_current} /></label>
          <label class="field">Max HP <input class="input" inputmode="numeric" bind:value={statEdit.hp_max} /></label>
          <label class="field">Initiative <input class="input" inputmode="numeric" bind:value={statEdit.initiative} /></label>
          <label class="field">Passive Perception <input class="input" inputmode="numeric" bind:value={statEdit.passive} /></label>
        </fieldset>
        <fieldset>
          <legend>Speed (ft.)</legend>
          {#each SPEEDS as k (k)}
            <label class="field">{k[0].toUpperCase() + k.slice(1)} <input class="input" inputmode="numeric" bind:value={statEdit.speed[k]} /></label>
          {/each}
        </fieldset>
        <fieldset>
          <legend>Abilities</legend>
          {#each ABIL as a (a)}
            <label class="field">{a.toUpperCase()} <input class="input" inputmode="numeric" bind:value={statEdit.abil[a]} /></label>
          {/each}
        </fieldset>
        <fieldset class="wide-set">
          <legend>Proficiencies and defences</legend>
          <label class="field wide">Saving throws <input class="input" placeholder="dex +4, wis +2" bind:value={statEdit.save} /></label>
          <label class="field wide">Skills <input class="input" placeholder="perception +5, stealth +6" bind:value={statEdit.skill} /></label>
          {#each LISTS as [k, l] (k)}
            <label class="field wide">{l} <input class="input" placeholder="comma separated" bind:value={statEdit.lists[k]} /></label>
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
    {#if statRow?.overrides}<button class="btn ghost" onclick={resetToBestiary}><Icon name="undo" size={15} /> Reset to bestiary</button>{/if}
    <span class="grow"></span>
    {#if statTab === 'json'}<button class="btn" onclick={checkJson}>Check JSON</button>{/if}
    <button class="btn primary" onclick={saveStatEdit}>Save</button>
  {/snippet}
</Modal>

<Modal title="Add condition{condRow ? ` to ${label(condRow)}` : ''}" bind:open={condOpen.value} width={480}>
  <label class="field">Turns <span class="muted">(empty: until removed)</span>
    <input class="input num" type="number" min="1" aria-label="Turns" bind:value={condTurns} />
  </label>
  <div class="cond-grid">
    {#each catalog.conditions as [name, color] (name)}
      <span class="chip-wrap">
        <button class="chip" style="--c: {color}" onclick={() => addCondition(name, color, condTurns)}>{name}</button>
        {#if conditionTip(name)}<span class="tip" role="tooltip"><b>{name}</b><span>{conditionTip(name)}</span></span>{/if}
      </span>
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
    scrollbar-gutter: stable; /* opening the Defeated list must not narrow the page */
    padding: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .round {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 14px;
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
  .notes-pop {
    width: 100%;
    min-height: 50vh;
    font-size: 15px;
    line-height: 1.5;
    resize: vertical;
  }
  .has-notes {
    border-color: var(--accent);
  }
  .cname {
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
    text-align: left;
  }
  .cname:hover,
  .cname.inspected {
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .row-notes {
    display: grid;
    gap: 4px;
    margin-bottom: var(--space-3);
    font-size: 13px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .row-notes textarea {
    min-height: 64px;
    font-size: 15px;
    font-weight: 400;
    color: var(--text);
    resize: vertical;
  }
  .diffbar {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 44px;
    padding: 0 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  .db-title {
    font-weight: 600;
    font-size: 15px;
  }
  .db-rating {
    min-width: 7ch;
    font-weight: 800;
    font-size: 15px;
  }
  .db-rating.rating-trivial,
  .db-rating.rating-low { color: #2fb344; }
  .db-rating.rating-moderate { color: #d6c21a; }
  .db-rating.rating-high { color: #f08c00; }
  .db-rating.rating-deadly { color: #d9381e; }
  .db-rating.rating-tpk { color: var(--text); }
  .db-track {
    position: relative;
    flex: 1;
    display: flex;
    height: 8px;
    border-radius: 999px;
    overflow: hidden;
    cursor: help;
  }
  .db-zone.low { background: #2fb344; }
  .db-zone.moderate { background: #d6c21a; }
  .db-zone.high { background: #f08c00; }
  .db-zone.deadly { background: #d9381e; }
  .db-zone.tpk { background: #111; }
  .db-marker {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 3px;
    margin-left: -1px;
    background: #fff;
    box-shadow: 0 0 0 1px rgb(0 0 0 / 0.6);
  }
  .db-xp {
    min-width: 9ch;
    text-align: right;
    font-size: 14px;
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
  }
  .enc-notes {
    min-height: 64px;
    resize: vertical;
  }
  .tracker {
    width: 100%;
    border-collapse: collapse;
    font-size: 15px;
  }
  .tracker th {
    text-align: left;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-muted);
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }
  .tracker td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  /* left edge: heroes blue; creatures by side: enemies red, allies white, neutral black */
  .tracker tr.hero td:first-child {
    box-shadow: inset 4px 0 0 #4aa3ff;
  }
  .tracker tr.monster td:first-child {
    box-shadow: inset 4px 0 0 #d9381e;
  }
  .tracker tr.ally td:first-child {
    box-shadow: inset 4px 0 0 #f2f2f2;
  }
  .tracker tr.neutral td:first-child {
    box-shadow: inset 4px 0 0 #000, inset 5px 0 0 #6b6b6b;
  }
  .tracker tr.active td {
    background: var(--accent-soft);
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
    font-size: 13px;
    color: var(--text-muted);
  }
  .panel {
    position: relative;
    padding: var(--space-4) var(--space-4) var(--space-4) calc(var(--space-4) + 4px);
  }
  .resizer {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 6px;
    cursor: col-resize;
    z-index: 3;
  }
  .resizer:hover,
  .resizer:focus-visible {
    background: color-mix(in srgb, var(--accent) 40%, transparent);
    outline: none;
  }
  .panel-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-3);
    padding-bottom: var(--space-2);
    border-bottom: 1px solid var(--border);
  }
  .panel-title {
    flex: 1;
    font-size: 13px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }
  .panel.mini {
    padding: 0;
  }
  .panel-expand {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 12px 0;
    border: 0;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    cursor: pointer;
  }
  .panel-expand:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .panel-expand > span:first-child {
    font-size: 20px;
  }
  .panel-expand-label {
    writing-mode: vertical-rl;
    font-size: 14px;
    font-weight: 600;
    white-space: nowrap;
  }
  /* .num sets a narrower width later in the sheet, so this needs the extra class */
  .input.num.temp {
    width: 60px;
    padding: 0 4px;
    font-size: 13px;
    text-align: center;
    color: #4aa3ff;
    border-color: color-mix(in srgb, #4aa3ff 55%, var(--border));
  }
  .input.num.temp::placeholder {
    color: color-mix(in srgb, #4aa3ff 65%, transparent);
  }
  .input.num.temp.has {
    border-color: #4aa3ff;
    font-weight: 700;
  }
  .slot {
    display: inline-block;
    width: 32px;
  }
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
  .pad {
    padding: var(--space-3);
  }
  .title-cell {
    display: grid;
  }
  .ed-tabs {
    margin-bottom: var(--space-3);
  }
  .ed-tabs button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
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
  .edit-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .info {
    position: relative;
    display: inline-grid;
    place-items: center;
    width: 16px;
    height: 16px;
    margin-left: 6px;
    border: 1px solid var(--border);
    border-radius: 50%;
    padding: 0;
    background: none;
    font-size: 11px;
    font-style: italic;
    font-weight: 700;
    color: var(--text-muted);
    cursor: help;
    vertical-align: middle;
  }
  .info .info-pop {
    display: none;
    text-align: left;
    position: absolute;
    left: 22px;
    top: -6px;
    z-index: 5;
    min-width: 260px;
    max-width: 380px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    box-shadow: var(--shadow);
    font-size: 14px;
    font-style: normal;
    font-weight: 400;
    color: var(--text);
    white-space: normal;
  }
  .info .info-pop span {
    display: block;
  }
  .info .info-pop .first {
    font-weight: 600;
  }
  .info:hover .info-pop,
  .info:focus .info-pop {
    display: block;
  }
  .reset-text {
    margin: 0 0 var(--space-3);
    font-size: 15px;
  }
  .reset-check {
    font-size: 15px;
    margin-bottom: var(--space-2);
  }
  .reset-hint {
    margin: var(--space-2) 0 0;
    font-size: 13px;
  }
  .chip-wrap {
    position: relative;
    display: inline-flex;
  }
  .chip-wrap .tip {
    display: none;
    position: absolute;
    left: 0;
    top: calc(100% + 6px);
    z-index: 6;
    width: 300px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    box-shadow: var(--shadow);
    font-size: 14px;
    line-height: 1.45;
    color: var(--text);
    white-space: normal;
  }
  .chip-wrap .tip span,
  .chip-wrap .tip small {
    display: block;
    margin-top: 4px;
  }
  .chip-wrap .tip small {
    color: var(--text-muted);
    font-size: 12px;
  }
  .chip-wrap:hover .tip,
  .chip-wrap:focus-within .tip {
    display: block;
  }
  .conc-stack {
    position: fixed;
    top: 72px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 90;
    display: grid;
    gap: var(--space-2);
  }
  .conc {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 420px;
    padding: 14px 16px;
    border: 2px solid #009f7a;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
    font-size: 16px;
    animation: conc-in 0.25s ease-out;
  }
  .conc-icon {
    font-size: 26px;
  }
  .conc-text {
    flex: 1;
  }
  @keyframes conc-in {
    from { transform: translateY(-12px); opacity: 0; }
    to { transform: none; opacity: 1; }
  }
  .turn-card {
    position: fixed;
    top: 64px;
    left: 50%;
    z-index: 85;
    display: flex;
    align-items: stretch;
    min-width: 440px;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--surface);
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.4);
    overflow: hidden;
    pointer-events: none;
    animation: tc-life 3.2s ease forwards;
  }
  .tc-now,
  .tc-next {
    --edge: #d9381e;
    display: grid;
    gap: 2px;
    padding: 14px 22px 14px 20px;
    box-shadow: inset 5px 0 0 var(--edge);
  }
  .tc-now {
    flex: 1.4;
  }
  .tc-next {
    flex: 1;
    border-left: 1px solid var(--border);
    background: var(--surface-2);
  }
  .tc-now.hero,
  .tc-next.hero {
    --edge: #4aa3ff;
  }
  .tc-now.ally,
  .tc-next.ally {
    --edge: #f2f2f2;
  }
  .tc-now.neutral,
  .tc-next.neutral {
    --edge: #6b6b6b;
  }
  .tc-label {
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .tc-name {
    font-size: 26px;
    font-weight: 800;
    line-height: 1.15;
  }
  .tc-name.small {
    font-size: 19px;
    font-weight: 700;
  }
  @keyframes tc-life {
    0% { opacity: 0; transform: translate(-50%, -10px); }
    8% { opacity: 1; transform: translate(-50%, 0); }
    85% { opacity: 1; transform: translate(-50%, 0); }
    100% { opacity: 0; transform: translate(-50%, -6px); }
  }
  @media (prefers-reduced-motion: reduce) {
    .turn-card {
      animation: none;
      transform: translateX(-50%);
    }
  }
  .splash {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    place-content: center;
    justify-items: center;
    pointer-events: none;
    background: radial-gradient(circle, color-mix(in srgb, var(--accent) 35%, transparent), rgb(0 0 0 / 0.55));
    animation: splash-bg 1.7s ease forwards;
  }
  .splash-label {
    font-size: 28px;
    letter-spacing: 0.4em;
    text-transform: uppercase;
    color: #fff;
    animation: splash-in 1.7s cubic-bezier(0.2, 0.8, 0.2, 1) forwards;
  }
  .splash-num {
    font-size: 160px;
    font-weight: 800;
    line-height: 1;
    color: #fff;
    text-shadow: 0 8px 40px rgb(0 0 0 / 0.5);
    animation: splash-in 1.7s cubic-bezier(0.2, 0.8, 0.2, 1) forwards;
  }
  @keyframes splash-bg {
    0% { opacity: 0; }
    15% { opacity: 1; }
    75% { opacity: 1; }
    100% { opacity: 0; }
  }
  @keyframes splash-in {
    0% { transform: scale(0.6); opacity: 0; }
    20% { transform: scale(1.05); opacity: 1; }
    30% { transform: scale(1); }
    75% { transform: scale(1); opacity: 1; }
    100% { transform: scale(1.15); opacity: 0; }
  }
  @media (prefers-reduced-motion: reduce) {
    .splash-label,
    .splash-num {
      animation: splash-bg 1.7s linear forwards;
    }
  }
  .conds-td {
    min-width: 140px;
  }
  .skull,
  .skull-slot {
    width: 30px;
    flex: none;
  }
  .skull {
    display: grid;
    place-items: center;
    height: 32px;
    border: 1px solid color-mix(in srgb, #d9381e 60%, var(--border));
    border-radius: 8px;
    background: color-mix(in srgb, #d9381e 18%, transparent);
    color: #e5533d;
    cursor: pointer;
  }
  .skull:hover {
    background: color-mix(in srgb, #d9381e 32%, transparent);
  }
  /* Defeated + Difficulty sit flush on the status bar, as one block */
  .dock {
    position: sticky;
    bottom: calc(-1 * var(--space-3));
    z-index: 3;
    display: grid;
    margin: auto calc(-1 * var(--space-3)) calc(-1 * var(--space-3));
  }
  .dock > .defeated,
  .dock > .diffbar {
    border-radius: 0;
    border-width: 1px 0 0;
    box-shadow: none;
  }
  .defeated {
    width: 100%;
    box-shadow: 0 -6px 18px rgb(0 0 0 / 0.25);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  .defeated summary {
    display: flex;
    align-items: center;
    gap: 8px;
    color: #e5533d;
    height: 44px;
    padding: 0 14px;
    cursor: pointer;
    font-size: 15px;
    font-weight: 600;
  }
  .defeated ul {
    max-height: 30vh;
    overflow: auto;
    list-style: none;
    margin: 0;
    padding: 0 8px 8px;
  }
  .defeated li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-top: 1px solid var(--border);
    font-size: 15px;
  }
  .d-name {
    font-weight: 600;
    text-decoration: line-through;
    color: var(--text-muted);
  }
  .d-hp {
    font-size: 13px;
  }
  .conds {
    display: flex;
    min-height: 32px;
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
    padding: 2px 10px;
    font-size: 13px;
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
  }
  .hp-cell {
    display: flex;
    gap: 6px;
    align-items: flex-start;
  }
  .hp-part,
  .tmp-part {
    position: relative;
    display: grid;
    gap: 3px;
  }
  .hp-inputs {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .slash {
    text-align: center;
  }
  .hpbar {
    height: 4px;
    border-radius: 999px;
    background: var(--surface-2);
    overflow: hidden;
  }
  .hpbar > span {
    display: block;
    height: 100%;
    border-radius: inherit;
  }
  .tmpbar > span {
    background: #4aa3ff;
  }
  .hp-cell {
    align-items: center;
  }
  .hp-part,
  .tmp-part {
    cursor: pointer;
  }
  /* the strip opens above what was clicked, centred on it */
  .pop {
    position: absolute;
    left: 50%;
    bottom: 100%;
    transform: translateX(-50%);
    z-index: 6;
    padding-bottom: 6px;
  }
  /* the first row has nothing above it: open below */
  .pop.below {
    bottom: auto;
    top: 100%;
    padding-bottom: 0;
    padding-top: 6px;
  }
  .pop .steps {
    display: flex;
    gap: 2px;
    padding: 5px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    box-shadow: 0 8px 28px rgb(0 0 0 / 0.35);
    animation: pop-in 0.1s ease-out;
  }
  .pop .tmp-steps {
    border-color: color-mix(in srgb, #4aa3ff 55%, var(--border));
  }
  @keyframes pop-in {
    from { opacity: 0; transform: translateY(4px); }
    to { opacity: 1; transform: none; }
  }
  .step {
    min-width: 40px;
    height: 32px;
    padding: 0 6px;
    border-radius: 7px;
    font-size: 15px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: #2fb344;
  }
  .step:hover {
    background: color-mix(in srgb, currentColor 14%, transparent);
  }
  .step.minus {
    color: #e5533d;
  }
  .tmp-steps .step {
    color: #4aa3ff;
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
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  .nowrap {
    flex-wrap: nowrap;
  }
  .field {
    font-size: 12px;
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
