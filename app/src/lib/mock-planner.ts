// In-memory session planner for the browser mock (mirror of crates/service/src/planner.rs).
// References resolve through the other mock handlers, so the planner sees the
// mock library, encounters, music and decks.

import type { Asset, DeckSummary, EncounterSummary, PlanEntry, PlanRef, PlanRefKind, PlanSection, SessionPatch, SessionView } from './types';
import type { Playlist, Soundscape, Track } from './music';

type Emit = (e: Record<string, unknown>) => void;
type Handler = (args: Record<string, unknown>) => unknown;
type Call = (cmd: string, args?: Record<string, unknown>) => unknown;
type Plan = Omit<SessionView, 'current' | 'entries'>;

const fail = (code: 'not_found' | 'invalid_input', message: string) => ({ code, message });

const SECTION_ORDER: PlanSection[] = ['music', 'encounters', 'tokens', 'maps', 'handouts'];

export function createPlannerMockHandlers(emit: Emit, call: Call): Record<string, Handler> {
  let sessions: Plan[] = [];
  let current: string | null = null;
  let seq = 0;
  const id = (p: string) => `${p}-${++seq}`;
  const now = () => new Date().toISOString();
  const changed = () => emit({ type: 'store_changed', entity: 'planner', entity_id: 'planner', op: 'update' });
  const get = (sid: unknown) => {
    const p = sessions.find((s) => s.id === sid);
    if (!p) throw fail('not_found', `not found: session ${sid}`);
    return p;
  };

  type Hit = { name: string; detail: string; thumb: string | null; asset_kind: string | null; starred: boolean };
  const resolve = (kind: PlanRefKind, ref: string): Hit | null => {
    try {
      switch (kind) {
        case 'asset': {
          const a = call('library_get', { id: Number(ref) }) as Asset;
          return { name: a.name, detail: a.kind, thumb: a.thumb_blob ?? a.rendered_blob, asset_kind: a.kind, starred: a.pinned };
        }
        case 'encounter': {
          const e = (call('encounters_list') as EncounterSummary[]).find((x) => x.id === ref);
          return e ? { name: e.name, detail: `${e.total} creatures`, thumb: null, asset_kind: null, starred: e.pinned } : null;
        }
        case 'soundscape': {
          const x = (call('music_soundscapes_list') as Soundscape[]).find((s) => s.id === ref);
          return x ? { name: x.name, detail: `soundscape, ${x.channels.length} channels`, thumb: null, asset_kind: null, starred: x.pinned } : null;
        }
        case 'playlist': {
          const x = (call('music_playlists_list') as Playlist[]).find((s) => s.id === ref);
          return x ? { name: x.name, detail: `playlist, ${x.items.length} tracks`, thumb: null, asset_kind: null, starred: false } : null;
        }
        case 'track': {
          const x = (call('music_tracks_list', { query: {} }) as Track[]).find((s) => s.id === ref);
          return x ? { name: x.title, detail: `${x.category} track`, thumb: null, asset_kind: null, starred: x.favorite } : null;
        }
        case 'deck': {
          const x = (call('decks_list') as DeckSummary[]).find((s) => String(s.id) === ref);
          return x ? { name: x.name, detail: 'card deck', thumb: null, asset_kind: null, starred: false } : null;
        }
      }
    } catch {
      return null;
    }
  };
  const sectionOf = (kind: PlanRefKind, assetKind: string | null): PlanSection => {
    if (kind === 'soundscape' || kind === 'playlist' || kind === 'track') return 'music';
    if (kind === 'encounter') return 'encounters';
    if (kind === 'deck') return 'handouts';
    return assetKind === 'token' || assetKind === 'cutout' ? 'tokens' : assetKind === 'map' ? 'maps' : 'handouts';
  };
  const starred = (): [PlanRefKind, string][] => {
    const out: [PlanRefKind, string][] = [];
    const page = call('library_search', { query: { pinned_only: true, limit: 500 } }) as { items: Asset[] };
    page.items.forEach((a) => out.push(['asset', String(a.id)]));
    (call('encounters_list') as EncounterSummary[]).filter((e) => e.pinned).forEach((e) => out.push(['encounter', e.id]));
    (call('music_soundscapes_list') as Soundscape[]).filter((s) => s.pinned).forEach((s) => out.push(['soundscape', s.id]));
    (call('music_tracks_list', { query: { favorites_only: true } }) as Track[]).filter((t) => t.favorite).forEach((t) => out.push(['track', t.id]));
    return out;
  };
  const view = (p: Plan): SessionView => {
    const entry = (item_id: string | null, kind: PlanRefKind, ref: string, copies: number): PlanEntry => {
      const r = resolve(kind, ref);
      return r
        ? { item_id, kind, ref_id: ref, section: sectionOf(kind, r.asset_kind), copies, missing: false, ...r }
        : { item_id, kind, ref_id: ref, section: sectionOf(kind, null), name: '(deleted)', detail: 'no longer in its tool', thumb: null, asset_kind: null, copies, starred: false, missing: true };
    };
    const entries = p.items.map((i) => entry(i.id, i.kind, i.ref_id, i.copies));
    if (current === p.id) {
      for (const [k, r] of starred()) {
        if (!entries.some((e) => e.kind === k && e.ref_id === r)) entries.push(entry(null, k, r, 1));
      }
    }
    entries.sort((a, b) => SECTION_ORDER.indexOf(a.section) - SECTION_ORDER.indexOf(b.section));
    return { ...p, current: current === p.id, entries };
  };
  const summary = () => ({
    sessions: sessions.map((p) => ({
      id: p.id, name: p.name, date: p.date, items: p.items.length,
      done: p.checklist.filter((c) => c.done).length, total: p.checklist.length, updated: p.updated,
    })),
    current,
  });
  return {
    planner_list: () => summary(),
    planner_session_get: ({ id: sid }) => view(get(sid)),
    planner_session_create: ({ name, current: cur }) => {
      const p: Plan = {
        id: id('session'), name: (name as string | null)?.trim() || `Session ${sessions.length + 1}`, date: null, notes: '',
        checklist: [], items: [], include_token_sheet: false, created: now(), updated: now(),
      };
      sessions.push(p);
      if (!current || cur) current = p.id;
      changed();
      return view(p);
    },
    planner_session_update: ({ id: sid, patch }) => {
      const p = get(sid);
      const x = patch as SessionPatch;
      if (x.name != null) {
        if (!x.name.trim()) throw fail('invalid_input', 'a session needs a name');
        p.name = x.name.trim();
      }
      if (x.date != null) p.date = x.date.trim() || null;
      if (x.notes != null) p.notes = x.notes;
      if (x.checklist != null) p.checklist = x.checklist.map((c) => ({ ...c, id: c.id || id('check') }));
      if (x.include_token_sheet != null) p.include_token_sheet = x.include_token_sheet;
      p.updated = now();
      changed();
      return view(p);
    },
    planner_session_delete: ({ id: sid }) => {
      get(sid);
      sessions = sessions.filter((s) => s.id !== sid);
      if (current === sid) current = sessions.at(-1)?.id ?? null;
      changed();
      return summary();
    },
    planner_set_current: ({ id: sid }) => {
      const p = get(sid);
      current = p.id;
      changed();
      return view(p);
    },
    planner_items_add: ({ session, items }) => {
      const p = get(session);
      const refs = items as PlanRef[];
      const missing = refs.filter((r) => !resolve(r.kind, r.ref_id));
      if (missing.length) throw fail('not_found', `not found: ${missing.map((m) => `${m.kind} ${m.ref_id}`).join(', ')}`);
      for (const r of refs) {
        if (!p.items.some((i) => i.kind === r.kind && i.ref_id === r.ref_id)) {
          p.items.push({ id: id('item'), kind: r.kind, ref_id: r.ref_id, copies: Math.min(99, Math.max(1, r.copies ?? 1)) });
        }
      }
      p.updated = now();
      changed();
      return view(p);
    },
    planner_item_update: ({ session, item, copies }) => {
      const p = get(session);
      const it = p.items.find((i) => i.id === item);
      if (!it) throw fail('not_found', `not found: planned item ${item}`);
      const n = copies as number;
      if (!(n >= 1 && n <= 99)) throw fail('invalid_input', 'copies must be 1–99');
      it.copies = n;
      changed();
      return view(p);
    },
    planner_item_remove: ({ session, item }) => {
      const p = get(session);
      if (!p.items.some((i) => i.id === item)) throw fail('not_found', `not found: planned item ${item}`);
      p.items = p.items.filter((i) => i.id !== item);
      changed();
      return view(p);
    },
    planner_prune: ({ session }) => {
      const p = get(session);
      const before = p.items.length;
      p.items = p.items.filter((i) => resolve(i.kind, i.ref_id));
      changed();
      return before - p.items.length;
    },
    planner_print: ({ session, dir }) => {
      const v = view(get(session));
      const files: string[] = [];
      if (v.entries.some((e) => e.section === 'tokens') || v.include_token_sheet) files.push(`${dir}/${v.name} - Tokens and cut-outs.pdf`);
      v.entries.filter((e) => e.asset_kind === 'map' || e.kind === 'deck').forEach((e) => files.push(`${dir}/${v.name} - ${e.name}.pdf`));
      if (!files.length) throw fail('invalid_input', 'nothing to print: add tokens, cut-outs, maps or card decks');
      const job = { id: id('mock-job'), kind: 'session print', state: 'done' as const, progress: 1, message: '', result: { dir, files, skipped: [] }, error: null };
      setTimeout(() => emit({ type: 'job', ...job }), 10);
      return job.id;
    },
  };
}
