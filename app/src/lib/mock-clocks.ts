// In-memory clocks for the browser mock (mirror of crates/service/src/clocks.rs).

import * as rules from './clocks';
import type { Clock, ClockNew, ClockPatch, ClockTick } from './types';

type Emit = (e: Record<string, unknown>) => void;
type Handler = (args: Record<string, unknown>) => unknown;

const fail = (code: 'not_found' | 'invalid_input', message: string) => ({ code, message });

export function createClocksMockHandlers(emit: Emit): Record<string, Handler> {
  let clocks: Clock[] = [];
  let seq = 0;
  const now = () => new Date().toISOString();
  const changed = () => emit({ type: 'store_changed', entity: 'clocks', entity_id: 'clocks', op: 'update' });
  const get = (id: unknown) => {
    const c = clocks.find((k) => k.id === id);
    if (!c) throw fail('not_found', `not found: clock ${id}`);
    return c;
  };
  const checkSegments = (n: number) => {
    if (n < rules.MIN_SEGMENTS || n > rules.MAX_SEGMENTS) throw fail('invalid_input', 'a clock has 2–24 segments');
    return n;
  };
  const checkEvery = (s: number) => {
    if (s < rules.MIN_EVERY_SECS || s > rules.MAX_EVERY_SECS) throw fail('invalid_input', 'the timer period is 10 s to 24 h');
    return s;
  };
  const checkColor = (c: string) => {
    const t = c.trim();
    if (!t) return null;
    if (!/^#[0-9a-f]{6}$/i.test(t)) throw fail('invalid_input', `${t} is not a #rrggbb colour`);
    return t.toLowerCase();
  };
  const touch = (c: Clock) => {
    c.updated = now();
    changed();
    return c;
  };

  return {
    clocks_list: () => clocks,
    clocks_create: ({ clock }) => {
      const n = (clock ?? {}) as ClockNew;
      const t = now();
      const c: Clock = {
        id: `clock-${++seq}`,
        name: n.name?.trim() || `Clock ${clocks.length + 1}`,
        segments: checkSegments(n.segments ?? 4),
        filled: 0,
        color: n.color != null ? checkColor(n.color) : null,
        notes: '',
        auto: null,
        events: [],
        created: t,
        updated: t,
      };
      if (n.every_secs != null) {
        rules.setEvery(c, checkEvery(n.every_secs));
        if (n.run) rules.setRunning(c, true, Date.now());
      }
      clocks.push(c);
      changed();
      return c;
    },
    clocks_update: ({ id, patch }) => {
      const c = get(id);
      const p = patch as ClockPatch;
      if (p.name != null) {
        if (!p.name.trim()) throw fail('invalid_input', 'a clock needs a name');
        c.name = p.name.trim();
      }
      if (p.segments != null) rules.setSegments(c, checkSegments(p.segments));
      if (p.filled != null) rules.setFilled(c, p.filled);
      if (p.color != null) c.color = checkColor(p.color);
      if (p.notes != null) c.notes = p.notes;
      if (p.events != null) {
        c.events = p.events.map((e) => {
          const text = e.text.trim();
          if (!text) throw fail('invalid_input', 'an event needs some text');
          if (e.at < 1 || e.at > rules.MAX_SEGMENTS) throw fail('invalid_input', 'an event is at segment 1–24');
          return { id: e.id || `event-${++seq}`, at: e.at, repeat: e.repeat && e.repeat > 0 ? e.repeat : null, text };
        });
      }
      if (p.every_secs === 0) rules.setEvery(c, null);
      else if (p.every_secs != null) rules.setEvery(c, checkEvery(p.every_secs));
      return touch(c);
    },
    clocks_advance: ({ id, by }) => {
      const c = get(id);
      rules.setFilled(c, c.filled + Number(by));
      return touch(c);
    },
    clocks_reset: ({ id }) => {
      const c = get(id);
      rules.reset(c);
      return touch(c);
    },
    clocks_run: ({ id, run }) => {
      const at = Date.now();
      if (id != null) {
        const c = get(id);
        if (!c.auto) throw fail('invalid_input', `${c.name} has no timer: set every_secs first`);
        rules.setRunning(c, Boolean(run), at);
        c.updated = now();
      } else {
        for (const c of clocks) if (rules.isRunning(c) !== run) rules.setRunning(c, Boolean(run), at);
      }
      changed();
      return clocks;
    },
    clocks_delete: ({ id }) => {
      get(id);
      clocks = clocks.filter((k) => k.id !== id);
      changed();
      return clocks;
    },
    clocks_tick: () => {
      const at = Date.now();
      const out: ClockTick[] = [];
      for (const c of clocks) {
        const before = c.filled;
        const a = rules.tick(c, at);
        if (a) {
          c.updated = now();
          const events = rules.eventsReached(c, before, c.filled);
          out.push({ id: c.id, name: c.name, added: a.added, filled: c.filled, segments: c.segments, full: a.full, events });
        }
      }
      if (out.length) changed();
      return out;
    },
  };
}
