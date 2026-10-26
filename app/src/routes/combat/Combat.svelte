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
  import { history } from './combat-history.svelte';
  import StatBlock, { type RollRequest } from './StatBlock.svelte';
  import AddCreatureDialog, { type AddRequest } from './AddCreatureDialog.svelte';
  import CombatDock from './CombatDock.svelte';
  import CombatNotices from './CombatNotices.svelte';
  import CreatureEditor from './CreatureEditor.svelte';
  import GroupBar from './GroupBar.svelte';
  import HpCell from './HpCell.svelte';
  import RollsWindow from './RollsWindow.svelte';
  import type { ConcAlert, EditorPatch, Fight, LegendItem, RechargeNote, RollLine, TurnCard, Who } from './combat-ui';
  import { rollAttack, rollFormula } from '../../lib/dice';
  import { crOfMon, scaleCreature } from '../../lib/scale';
  import Icon from '../../lib/components/Icon.svelte';
  import { difficulty, xpOf } from '../../lib/difficulty';
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
      setTracker(tr, false);
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
  function setTracker(x: Tracker, record = true) {
    for (const r of x.rows) {
      for (const c of x.stats_cols) if (!r.cells.some((cell) => cell.id === c.id)) r.cells.push({ id: c.id, value: null });
    }
    t = x;
    remember(record);
  }

  // ── undo / redo (the last 30 saved states; other windows' changes don't count).
  //    The history lives in combat-history, so it survives switching views. ──
  /** Called after each change lands: the previous state becomes an undo step. */
  function remember(record: boolean) {
    if (!t) return;
    const now = JSON.stringify({ ...$state.snapshot(t), is_locked: false });
    if (now === history.saved) return;
    if (record && history.saved && !restoring) {
      history.undo = [...history.undo.slice(-29), history.saved];
      history.redo = [];
    }
    history.saved = now;
  }
  let restoring = false;
  async function travel(from: 'undo' | 'redo') {
    const stack = from === 'undo' ? history.undo : history.redo;
    const target = stack[stack.length - 1];
    if (!target || !t) return;
    if (from === 'undo') {
      history.undo = history.undo.slice(0, -1);
      history.redo = [...history.redo, history.saved];
    } else {
      history.redo = history.redo.slice(0, -1);
      history.undo = [...history.undo, history.saved];
    }
    restoring = true;
    try {
      const prev = JSON.parse(target) as Tracker;
      prev.is_locked = t.is_locked;
      await op(api.combatSet(prev), 'Could not undo');
    } finally {
      restoring = false;
    }
  }
  function undoKeys(e: KeyboardEvent) {
    if (e.key === 'Escape' && selected.size && !(e.target as HTMLElement)?.closest?.('[role="dialog"]')) return clearSelection();
    if (tab !== 'tracker' || !(e.ctrlKey || e.metaKey)) return;
    if ((e.target as HTMLElement)?.matches?.('input, textarea')) return;
    const k = e.key.toLowerCase();
    if (k === 'z' && !e.shiftKey) return void (e.preventDefault(), travel('undo'));
    if (k === 'y' || (k === 'z' && e.shiftKey)) return void (e.preventDefault(), travel('redo'));
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
  let concAlerts = $state<ConcAlert[]>([]);
  let concSeq = 0;
  /** Damage taken in quick steps (−10, −10, −3) is one hit: it is added up and the
   *  reminder comes once, when the steps stop (a pause, the strip closing, the
   *  window losing focus), with the total and its DC. */
  const pendingDamage = new Map<string, { who: string; total: number }>();
  let damageTimer: ReturnType<typeof setTimeout> | null = null;
  /** A concentrating combatant took damage: remind the DM to roll the save. */
  function tookDamage(r: CombatRow, damage: number) {
    if (!r.conditions.some((c) => c.name.trim().toLowerCase() === 'concentration')) return;
    const p = pendingDamage.get(r.id);
    pendingDamage.set(r.id, { who: label(r), total: (p?.total ?? 0) + damage });
    if (damageTimer) clearTimeout(damageTimer);
    damageTimer = setTimeout(flushDamage, 1500);
  }
  function flushDamage() {
    if (damageTimer) clearTimeout(damageTimer);
    damageTimer = null;
    for (const { who, total } of pendingDamage.values()) {
      const id = ++concSeq;
      concAlerts = [...concAlerts.filter((a) => a.who !== who), { id, who, damage: total, dc: concentrationDc(total) }];
      setTimeout(() => (concAlerts = concAlerts.filter((a) => a.id !== id)), 12000);
    }
    pendingDamage.clear();
  }
  // closing the ± strip or leaving the window ends a burst of damage
  $effect(() => {
    if (!steps && pendingDamage.size) flushDamage();
  });
  $effect(() => {
    const blur = () => pendingDamage.size && flushDamage();
    window.addEventListener('blur', blur);
    return () => window.removeEventListener('blur', blur);
  });

  // ── rolls from the stat block ──
  let rolls = $state<RollLine[]>([]);
  let rollSeq = 0;
  /** Abilities whose last attack was a natural 20: their next damage roll is a crit. */
  const critNext = new Set<string>();
  function doRoll(r: CombatRow, q: RollRequest) {
    const key = `${r.id}|${q.entry}`;
    let line: RollLine;
    if (q.kind === 'hit') {
      const a = rollAttack(Number(q.value) || 0, q.shift ? 1 : q.ctrl ? -1 : 0);
      const tag = a.natural === 20 ? 'CRIT' : a.natural === 1 ? 'MISS' : '';
      if (a.natural === 20) critNext.add(key);
      else critNext.delete(key);
      line = { id: ++rollSeq, who: label(r), entry: q.entry, kind: 'hit', total: a.total, detail: a.detail, tag };
    } else {
      const crit = q.shift || critNext.has(key);
      critNext.delete(key);
      const d = rollFormula(q.value, crit);
      if (!d) return app.toast('error', `Can't roll “${q.value}”`);
      line = { id: ++rollSeq, who: label(r), entry: q.entry, kind: 'damage', total: d.total, detail: d.detail, tag: crit ? 'CRIT' : '' };
    }
    rolls = [line, ...rolls].slice(0, 6);
  }

  // ── lair: a row at initiative 20 that loses ties, showing the lair actions ──
  let lairGroups = $state<Record<string, Record<string, unknown> | null>>({});
  const groupOf = (r: CombatRow) => {
    const g = merged(r)?.legendaryGroup as { name?: string; source?: string } | undefined;
    return g?.name && g.source ? { name: g.name, source: g.source } : null;
  };
  const lairKey = (g: { name: string; source: string }) => `${g.name}|${g.source}`.toLowerCase();
  function fetchGroup(g: { name: string; source: string }) {
    const k = lairKey(g);
    if (k in lairGroups) return;
    lairGroups[k] = null;
    void api.bestiaryLegendaryGroup(g.name, g.source).then((v) => (lairGroups[k] = v)).catch(() => {});
  }
  const lairOf = (r: CombatRow) => {
    const g = r.usage?.__lair as { name: string; source: string } | undefined;
    return g ? lairGroups[lairKey(g)] ?? null : null;
  };
  $effect(() => {
    if (!focus) return;
    const g = focus.side === 'lair' ? (focus.usage?.__lair as { name: string; source: string } | undefined) : groupOf(focus);
    if (g) fetchGroup(g);
  });
  const hasLairRow = (g: { name: string; source: string }) =>
    !!t?.rows.some((r) => r.side === 'lair' && lairKey((r.usage?.__lair ?? { name: '', source: '' }) as { name: string; source: string }) === lairKey(g));
  function addLair(r: CombatRow) {
    const g = groupOf(r);
    if (!t || !g) return;
    t.rows = [...t.rows, {
      id: crypto.randomUUID(), name: `Lair (${g.name})`, display_name: null, custom_name: null, source: null, scaled_cr: null,
      scaled_summon_spell_level: null, scaled_summon_class_level: null, hp_current: null, hp_max: null, hp_temp: null,
      initiative: 20, is_active: false, conditions: [], notes: '', is_player_visible: true, ordinal: 1, cells: [],
      overrides: null, side: 'lair', usage: { __lair: g },
    }];
    commit();
    app.toast('success', `Lair added at initiative 20`);
  }

  /** At the start of a creature's turn: legendary actions refill, spent recharges roll a d6. */
  function startOfTurn(rows: CombatRow[]) {
    const notes: RechargeNote[] = [];
    let refilled = false;
    for (const r of rows) {
      if (!r.source || !r.usage) continue;
      if (r.usage.__la) {
        setUsage(r, '__la', null);
        refilled = true;
      }
      const m = merged(r);
      for (const key of Object.keys(r.usage)) {
        if (key.startsWith('__')) continue;
        const raw = ['trait', 'action', 'bonus', 'reaction', 'legendary']
          .flatMap((k) => (Array.isArray(m?.[k]) ? (m![k] as { name?: string }[]) : []))
          .find((b) => String(b.name ?? '').replace(/\s*\{@recharge[^}]*\}|\s*\((?:Recharge[^)]*)\)/gi, '').trim() === key)?.name;
        const rc = raw?.match(/\{@recharge\s*(\d*)\}/) ?? raw?.match(/\(Recharge (\d)/i);
        if (!rc) continue;
        // a reminder only: the DM rolls and clicks the badge when it recharges
        notes.push({ rowId: r.id, who: label(r), ability: key, need: Number(rc[1]) || 6 });
      }
    }
    if (refilled) commit();
    if (notes.length) showRecharge(notes);
  }
  // ── recharge notice: a reminder to roll for spent recharge abilities at turn start ──
  let rechargeNotice = $state<{ id: number; items: RechargeNote[] } | null>(null);
  let rechargeSeq = 0;
  function showRecharge(items: RechargeNote[]) {
    const id = ++rechargeSeq;
    setTimeout(() => {
      if (rechargeSeq !== id) return;
      rechargeNotice = { id, items };
      setTimeout(() => {
        if (rechargeNotice?.id === id) rechargeNotice = null;
      }, 15000);
    }, legendDelay);
  }

  /** ✓ in the notice: the roll hit, the ability is available again. */
  function markRecharged(it: RechargeNote) {
    const r = t?.rows.find((x) => x.id === it.rowId);
    if (r) {
      setUsage(r, it.ability, null);
      commit();
    }
    if (!rechargeNotice) return;
    const items = rechargeNotice.items.filter((x) => x !== it);
    rechargeNotice = items.length ? { ...rechargeNotice, items } : null;
  }

  // ── legendary actions notice: just above the turn card, on the same timing ──
  let legendNotice = $state<{ id: number; items: LegendItem[] } | null>(null);
  let legendSeq = 0;
  let legendDelay = 0;
  function showLegend(items: { who: string; left: number }[]) {
    const id = ++legendSeq;
    setTimeout(() => {
      if (legendSeq !== id) return;
      legendNotice = { id, items };
      setTimeout(() => {
        if (legendNotice?.id === id) legendNotice = null;
      }, 4000);
    }, legendDelay);
  }
  /** After another combatant's turn: who still has legendary actions; dying heroes roll death saves. */
  function afterTurn(started: CombatRow[]) {
    if (!t) return;
    const startedIds = new Set(started.map((r) => r.id));
    const legend = t.rows
      .filter((r) => r.source && !startedIds.has(r.id) && !isDown(r))
      .map((r) => {
        const m = merged(r);
        const max = Number(m?.legendaryActions ?? (Array.isArray(m?.legendary) && (m!.legendary as unknown[]).length ? 3 : 0)) || 0;
        return { r, left: max - (Number(r.usage?.__la) || 0) };
      })
      .filter((x) => x.left > 0);
    if (legend.length) showLegend(legend.map((x) => ({ who: label(x.r), left: x.left })));
    for (const r of started) if (isDying(r) && deathSaves(r).s < 3) app.toast('info', `${label(r)} is dying: roll a death saving throw`);
  }

  // ── difficulty of the fight: enemies in the tracker vs the heroes in it ──
  $effect(() => {
    for (const r of t?.rows ?? []) if (r.source) fetchMon(r);
  });
  const levelOf = (r: CombatRow) => heroOf(r)?.level ?? null;
  const fight = $derived.by((): Fight | null => {
    if (!t) return null;
    const enemies = t.rows.filter((r) => r.source && !r.side);
    const heroes = t.rows.filter((r) => isHero(r));
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
  let notesOpen = $state(false);
  /** At 0 HP or below (or, tracking damage, at least the max). */
  const isDown = (r: CombatRow) =>
    r.side !== 'lair' &&
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
    // a dying hero who takes damage fails a death save
    if (isDying(r)) setDeathSaves(r, deathSaves(r).s, deathSaves(r).f + 1);
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
    else r.hp_current = Math.min(r.hp_max ?? Infinity, Math.max(0, r.hp_current ?? 0) + n);
    if (!isDown(r) && r.usage?.__ds) clearDeathSaves(r);
    commit();
  }

  // ── usage: spent uses and recharges, legendary actions, death saves ──
  function setUsage(r: CombatRow, key: string, v: unknown) {
    const u = { ...(r.usage ?? {}) };
    if (v === null || v === 0 || v === undefined) delete u[key];
    else u[key] = v;
    r.usage = Object.keys(u).length ? u : null;
  }
  const isHero = (r: CombatRow) => !r.source && r.side !== 'lair';
  const isDying = (r: CombatRow) => isHero(r) && isDown(r);
  const deathSaves = (r: CombatRow) => {
    const d = (r.usage?.__ds ?? {}) as { s?: number; f?: number };
    return { s: d.s ?? 0, f: d.f ?? 0 };
  };
  function setDeathSaves(r: CombatRow, s: number, f: number) {
    setUsage(r, '__ds', { s: Math.max(0, Math.min(3, s)), f: Math.max(0, Math.min(3, f)) });
    commit();
  }
  function clearDeathSaves(r: CombatRow) {
    setUsage(r, '__ds', null);
  }

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

  // ── add creature (the dialog owns its search; adding and scaling happen here) ──
  let addOpen = $state(false);
  async function addCreature(q: AddRequest) {
    const before = new Set(t?.rows.map((r) => r.id) ?? []);
    await op(api.combatAddCreature(q.entry.hash, q.count, q.rollHp, q.customName, q.side), 'Could not add the creature');
    if (t && q.cr && q.mon && q.cr !== crOfMon(q.mon)) {
      for (const r of t.rows.filter((x) => !before.has(x.id))) applyScale(r, q.mon, q.cr);
      commit();
    }
  }
  /** Scale to CR: the stat changes become this row's overrides; HP follows. */
  function applyScale(r: CombatRow, base: Record<string, unknown>, cr: string) {
    const o = scaleCreature(base, cr);
    if (!o) return;
    r.overrides = { ...(r.overrides ?? {}), ...o };
    const [a, b] = cr.split('/').map(Number);
    r.scaled_cr = b ? a / b : a;
    const avg = (o.hp as { average?: number } | undefined)?.average;
    if (avg) {
      r.hp_max = avg;
      r.hp_current = t?.is_invert_wound_direction ? 0 : avg;
    }
    syncRow(r, { ...base, ...o });
  }

  // ── conditions ──
  let condRow = $state<CombatRow | null>(null);
  let condTurns = $state<number | null>(null);
  let condHint = $state<string | null>(null);
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
    // the group bar opens the picker for every selected row
    const ids = condForGroup ? [...selected] : [condRow.id];
    for (const id of ids) await op(api.combatAddCondition(id, name, color, turns && turns > 0 ? turns : null), 'Could not add the condition');
    condRow = null;
    condForGroup = false;
  }

  // ── group actions: Shift+click rows to select them ──
  let selected = $state<Set<string>>(new Set());
  let condForGroup = false;
  let groupAmount = $state<number | null>(null);
  let saveAbility = $state('dex');
  let saveDc = $state<number | null>(null);
  let halfOnSave = $state(true);
  /** Last group save: row id → total and whether it beat the DC. */
  let saveResults = $state<Record<string, { total: number; pass: boolean } | 'self'>>({});
  const selRows = $derived(t?.rows.filter((r) => selected.has(r.id)) ?? []);
  function toggleSelect(r: CombatRow) {
    const s2 = new Set(selected);
    if (s2.has(r.id)) s2.delete(r.id);
    else s2.add(r.id);
    selected = s2;
  }
  function rowClick(r: CombatRow, e: MouseEvent) {
    if (!e.shiftKey) return;
    const el = e.target as HTMLElement;
    if (el.closest('input, textarea, select, .steps, .ds, .chip, .acts')) return;
    e.preventDefault();
    toggleSelect(r);
  }
  function clearSelection() {
    selected = new Set();
    saveResults = {};
  }
  function groupHp(kind: 'damage' | 'heal' | 'temp') {
    const n = Math.round(Number(groupAmount) || 0);
    if (n <= 0) return;
    for (const r of selRows) {
      if (kind === 'heal') heal(r, n);
      else if (kind === 'temp') {
        r.hp_temp = Math.max(r.hp_temp ?? 0, n);
        commit();
      } else {
        const res = saveResults[r.id];
        const half = halfOnSave && res && res !== 'self' && res.pass;
        damage(r, half ? Math.floor(n / 2) : n);
      }
    }
  }
  /** Each selected creature rolls its own save (its save bonus, else the ability modifier). */
  function groupSaves() {
    const out: typeof saveResults = {};
    const lines: RollLine[] = [];
    for (const r of selRows) {
      const m = merged(r);
      if (!m) {
        out[r.id] = 'self';
        continue;
      }
      const save = (m.save as Record<string, string> | undefined)?.[saveAbility];
      const score = Number(m[saveAbility]);
      const bonus = save !== undefined ? Number(String(save).replace(/\s+/g, '')) || 0 : Number.isFinite(score) ? Math.floor((score - 10) / 2) : 0;
      const a = rollAttack(bonus);
      const pass = saveDc ? a.total >= saveDc : false;
      out[r.id] = { total: a.total, pass };
      lines.push({ id: ++rollSeq, who: label(r), entry: `${saveAbility.toUpperCase()} save`, kind: 'hit', total: a.total, detail: a.detail,
        tag: saveDc ? (pass ? 'SAVED' : 'FAILED') : '' });
    }
    saveResults = out;
    rolls = [...lines, ...rolls].slice(0, 8);
  }
  function groupCondition() {
    if (!selRows.length) return;
    condForGroup = true;
    condRow = selRows[0];
    condTurns = null;
  }
  /** Short chip label when a row has more than two conditions: "Prone" → "Pro". */
  const abbr = (name: string) => {
    const letters = name.replace(/[^A-Za-z]/g, '');
    return letters.slice(0, 3) || name.slice(0, 3);
  };
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
    const move = (ev: PointerEvent) => (panelW = Math.min(panelMax, Math.max(300, w0 + (x0 - ev.clientX))));
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
  /** At most 40% of the window (and 1000 px), so the tracker keeps its room. */
  let winW = $state(typeof window === 'undefined' ? 1600 : window.innerWidth);
  const panelMax = $derived(Math.max(300, Math.min(1000, Math.round(winW * 0.4))));
  function resizeKey(e: KeyboardEvent) {
    if (e.key === 'ArrowLeft') panelW = Math.min(panelMax, panelW + 20);
    else if (e.key === 'ArrowRight') panelW = Math.max(300, panelW - 20);
  }

  // ── edit one creature (CreatureEditor owns the form; its patch lands here) ──
  /** The creature's stat block with this row's own changes on top. */
  function merged(r: CombatRow): Record<string, unknown> | null {
    const m = monOf(r);
    return m || r.overrides ? { ...(m ?? {}), ...(r.overrides ?? {}) } : null;
  }
  /** Keeps the tracker row (AC, speed and passive Perception columns) in step with the stats. */
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
  let statEditId = $state<string | null>(null);
  const statRow = $derived(statEditId ? t?.rows.find((x) => x.id === statEditId) ?? null : null);
  function openStatEdit(r: CombatRow) {
    statEditId = r.id;
  }
  function applyEdit(p: EditorPatch) {
    const r = statRow;
    if (!r) return;
    r.overrides = p.overrides;
    if (p.custom_name !== undefined) r.custom_name = p.custom_name;
    if (p.hp_max !== undefined) r.hp_max = p.hp_max;
    if (p.hp_current !== undefined) r.hp_current = p.hp_current;
    if (p.initiative !== undefined) r.initiative = p.initiative;
    syncRow(r, { ...(monOf(r) ?? {}), ...(r.overrides ?? {}) });
    statEditId = null;
    commit();
  }
  function scaleEdited(cr: string) {
    const r = statRow;
    const base = r ? monOf(r) : null;
    if (!r || !base) return;
    applyScale(r, base, cr);
    statEditId = null;
    commit();
    app.toast('success', 'Scaled');
  }
  function resetEdited() {
    const r = statRow;
    if (!r) return;
    r.overrides = null;
    syncRow(r, monOf(r) ?? {});
    statEditId = null;
    commit();
    app.toast('success', 'Back to the bestiary stats');
  }

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
    legendDelay = newRound ? 1700 : 0;
    const started = t?.rows.filter((r) => r.is_active) ?? [];
    startOfTurn(started);
    afterTurn(started);
  }

  // ── turn card: whose turn it is and who comes next ──
  let turnCard = $state<TurnCard | null>(null);
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

<svelte:window onkeydown={undoKeys} bind:innerWidth={winW} />

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
<div class="view" style:--inspector-w="{panelMin ? 44 : Math.min(panelW, panelMax)}px">
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
      <button class="btn" onclick={() => (addOpen = true)} disabled={locked}>+ Creature…</button>
      <button class="btn ghost icon" onclick={() => travel('undo')} disabled={!history.undo.length} aria-label="Undo" title="Undo (Ctrl+Z)"><Icon name="undo" size={17} /></button>
      <button class="btn ghost icon redo" onclick={() => travel('redo')} disabled={!history.redo.length} aria-label="Redo" title="Redo (Ctrl+Y)"><Icon name="undo" size={17} /></button>
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
        {#if selRows.length}
          <GroupBar count={selRows.length} {locked} bind:amount={groupAmount} bind:ability={saveAbility} bind:dc={saveDc} bind:half={halfOnSave}
            onHp={groupHp} onSaves={groupSaves} onCondition={groupCondition} onClear={clearSelection} />
        {/if}
        <table class="tracker" data-testid="combat-table">
          <thead>
            <tr>
              <th class="init-th">Init</th>
              <th class="name">Name</th>
              <th>Conditions</th>
              {#each t.stats_cols as c (c.id)}<th title={catalog.columns.find((k) => k.populate_with === c.populate_with)?.name}>{c.abbreviation || '·'}</th>{/each}
              <th>HP</th>
              <th><span class="sr">Actions</span></th>
            </tr>
          </thead>
          <tbody>
            {#each t.rows as r (r.id)}
              {@const w = wound(r)}
              <tr onclick={(e) => rowClick(r, e)} onmousedown={(e) => e.shiftKey && !(e.target as HTMLElement).closest('input, textarea') && e.preventDefault()} class:selected={selected.has(r.id)} class:active={r.is_active} class:defeated={w.text === 'Defeated'} class:hero={isHero(r)} class:monster={!!r.source && !r.side} class:ally={r.side === 'ally'} class:neutral={r.side === 'neutral'} class:lair={r.side === 'lair'} data-testid="combat-row">
                <td class="init-td"><input class="init-in" inputmode="numeric" aria-label="Initiative of {label(r)}" value={r.initiative ?? ''}
                  onchange={(e) => setInit(r, e.currentTarget.value)} disabled={locked} /></td>
                <td class="name">
                  {#if editing.has(r.id) && !locked}
                    {#if r.source}
                      <input class="input" aria-label="Name of {label(r)}" placeholder={r.name} value={r.custom_name ?? ''}
                        onchange={(e) => { r.custom_name = e.currentTarget.value.trim() || null; commit(); }} />
                    {:else}
                      <input class="input" aria-label="Player name" placeholder="Player" bind:value={r.name} onchange={commit} />
                    {/if}
                  {:else}
                    <button class="cname" class:inspected={inspectId === r.id} onclick={(e) => { if (!e.shiftKey) inspectId = inspectId === r.id ? null : r.id; }}
                      onkeydown={(e) => { if ((e.key === 'Enter' || e.key === ' ') && e.shiftKey) { e.preventDefault(); toggleSelect(r); } }}
                      aria-pressed={selected.has(r.id)} title="Show in the side panel · Shift+click or Shift+Enter: select for group actions">{label(r)}</button>
                    {#if r.source}<span class="src">{r.source}</span>{/if}
                    <button type="button" class="info" aria-label="Details of {label(r)}" onmouseenter={() => fetchMon(r)} onfocus={() => fetchMon(r)}>
                      i
                      <span class="info-pop">{#each infoLines(r) as l, i (i)}<span class:first={i === 0}>{l}</span>{:else}<span>No details</span>{/each}</span>
                    </button>
                    {#if saveResults[r.id]}
                      {@const sr = saveResults[r.id]}
                      <span class="save-badge" class:pass={sr !== 'self' && sr.pass} class:fail={sr !== 'self' && !sr.pass && !!saveDc}>
                        {sr === 'self' ? 'rolls own save' : `${sr.total}${saveDc ? (sr.pass ? ' ✓' : ' ✗') : ''}`}
                      </span>
                    {/if}
                  {/if}
                </td>
                <td class="conds-td"><div class="conds">
                  {#each r.conditions as c (c.id)}
                    <span class="chip-wrap">
                      <button class="chip" style="--c: {c.color}" aria-label="Remove {c.name}" onclick={() => removeCondition(r, c.id)} disabled={locked}>
                        {r.conditions.length > 2 ? abbr(c.name) : c.name} ({c.turns !== null ? `${c.elapsed ?? 0}/${(c.elapsed ?? 0) + c.turns}` : c.elapsed ?? 0})
                      </button>
                      <span class="tip" role="tooltip">
                        <b>{c.name}</b> · {c.elapsed ?? 0} turn(s) so far{c.turns !== null ? `, ${c.turns} left` : ''}
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
                  <HpCell {r} name={label(r)} {locked} editing={editing.has(r.id)} invert={!!t.is_invert_wound_direction} first={t.rows[0]?.id === r.id}
                    open={steps?.id === r.id ? steps.kind : null} wound={w.text} down={isDown(r)} deathSaves={isDying(r) ? deathSaves(r) : null}
                    onOpen={(k) => openSteps(r, k)} onSetHp={(f, raw) => setHp(r, f, raw)} onArrowHp={(e) => arrowHp(r, e)}
                    onSetTemp={(raw) => setTemp(r, raw)} onArrowTemp={(e) => arrowTemp(r, e)} onStepHp={(d) => stepHp(r, d)} onStepTemp={(d) => stepTemp(r, d)}
                    onDeathSaves={(s2, f) => setDeathSaves(r, s2, f)} onDefeat={() => defeat(r)} />
                </td>
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
        <CombatDock defeated={t?.defeated ?? []} {fight} {locked} {label} onRestore={restore} onRemove={removeDefeated} />
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
        aria-valuemin={300} aria-valuemax={panelMax} tabindex="0" onpointerdown={startResize} onkeydown={resizeKey}></div>
      <div class="panel-body">
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
        {#if focus.source && groupOf(focus) && lairGroups[lairKey(groupOf(focus)!)]?.lairActions && !hasLairRow(groupOf(focus)!)}
          <button class="btn lair-btn" onclick={() => addLair(focus!)} disabled={locked}>+ Add lair (initiative 20)</button>
        {/if}
        <StatBlock mon={focus.side === 'lair' ? null : merged(focus)} hero={heroOf(focus)} name={label(focus)} sub={focus.display_name ?? undefined}
          lair={focus.side === 'lair' ? lairOf(focus) : null} usage={focus.usage}
          onRoll={(q) => doRoll(focus!, q)} onUse={(key, n) => { setUsage(focus!, key, Math.max(0, n)); commit(); }} />
        {#if active.length > 1}<p class="note">Also acting: {active.slice(1).map(label).join(', ')}</p>{/if}
      {:else}
        <p class="note">The stat block of whoever's turn it is shows here. Press <b>Next turn</b> to start.</p>
      {/if}
      {#if rolls.length}
        <RollsWindow {rolls} onClear={() => (rolls = [])} />
      {/if}
      </div>
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

<CombatNotices {concAlerts} recharge={rechargeNotice?.items ?? null} legend={legendNotice?.items ?? null} {turnCard} {splash}
  onDismissConc={(id) => (concAlerts = concAlerts.filter((x) => x.id !== id))} onRecharged={markRecharged} />

<AddCreatureDialog bind:open={addOpen} {status} onAdd={addCreature} />

<CreatureEditor row={statRow} base={statRow ? monOf(statRow) : null} title={statRow ? label(statRow) : 'creature'}
  onClose={() => (statEditId = null)} onApply={applyEdit} onScale={scaleEdited} onReset={resetEdited} />

<Modal title="Add condition{condRow ? ` to ${label(condRow)}` : ''}" bind:open={condOpen.value} width={480}>
  <label class="field">Turns <span class="muted">(empty: until removed)</span>
    <input class="input num" type="number" min="1" aria-label="Turns" bind:value={condTurns} />
  </label>
  <div class="cond-grid" role="group" aria-label="Conditions" onmouseleave={() => (condHint = null)}>
    {#each catalog.conditions as [name, color] (name)}
      <button class="chip" style="--c: {color}" onclick={() => addCondition(name, color, condTurns)}
        onmouseenter={() => (condHint = name)} onfocus={() => (condHint = name)}>{name}</button>
    {/each}
  </div>
  <!-- the tip of the chip under the pointer, in a box that never moves the chips -->
  <p class="cond-hint" aria-live="polite">
    {#if condHint}<b>{condHint}</b>{conditionTip(condHint) ? ` — ${conditionTip(condHint)}` : ''}{:else}<span class="muted">Point at a condition to see what it does.</span>{/if}
  </p>
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
  /* side colour: heroes blue, enemies red, allies white, neutral grey, lair purple */
  .tracker tr {
    --side: var(--text);
  }
  .tracker tr.hero {
    --side: #4aa3ff;
  }
  .tracker tr.monster {
    --side: #e5533d;
  }
  .tracker tr.ally {
    --side: #f2f2f2;
  }
  /* white is invisible on a light background */
  :global(html[data-theme='light']) .tracker tr.ally {
    --side: #64748b;
  }
  @media (prefers-color-scheme: light) {
    :global(html:not([data-theme])) .tracker tr.ally {
      --side: #64748b;
    }
  }
  .tracker tr.neutral {
    --side: #9a9a9a;
  }
  .tracker tr.lair {
    --side: #a78bfa;
  }
  .init-th {
    width: 64px;
    text-align: center;
  }
  .init-in {
    width: 56px;
    height: 40px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 8px;
    background: transparent;
    color: var(--side);
    font: inherit;
    font-size: 24px;
    font-weight: 800;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .init-in:hover:not(:disabled) {
    border-color: var(--border);
  }
  .init-in:focus {
    outline: none;
    border-color: var(--accent);
    background: var(--surface-2);
  }
  /* left edge: heroes blue; creatures by side: enemies red, allies white, neutral black */
  .tracker tr.hero td:first-child {
    box-shadow: inset 4px 0 0 #4aa3ff;
  }
  .tracker tr.monster td:first-child {
    box-shadow: inset 4px 0 0 #d9381e;
  }
  .tracker tr.ally td:first-child {
    box-shadow: inset 4px 0 0 var(--side);
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
    padding: 0;
    overflow: hidden;
  }
  .panel-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
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
  .slot {
    display: inline-block;
    width: 32px;
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
  .tracker tbody tr {
    user-select: none;
  }
  .tracker tr.selected td {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .tracker tr.selected td:last-child {
    box-shadow: inset -3px 0 0 var(--accent);
  }
  .save-badge {
    margin-left: 8px;
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--surface-2);
    font-size: 13px;
    font-weight: 700;
  }
  .save-badge.pass {
    color: #2fb344;
  }
  .save-badge.fail {
    color: #e5533d;
  }
  .redo :global(svg) {
    transform: scaleX(-1);
  }
  .tracker tr.lair td:first-child {
    box-shadow: inset 4px 0 0 #8b5cf6;
  }
  .lair-btn {
    margin-bottom: var(--space-3);
    border-color: #8b5cf6;
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
  .conds-td {
    min-width: 140px;
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
  .field {
    font-size: 12px;
  }
  .cond-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: var(--space-3);
  }
  .cond-hint {
    min-height: 4.5em;
    margin: var(--space-3) 0 0;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    font-size: 14px;
    line-height: 1.45;
  }
  .cond-grid .chip {
    font-size: 12px;
    padding: 4px 10px;
  }
</style>
