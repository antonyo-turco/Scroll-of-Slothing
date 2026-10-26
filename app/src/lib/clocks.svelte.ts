// Shared clocks state for the Board and Detail views, their actions, and the
// timer that advances clocks while the app is open (any route): when the next
// segment is due it calls clocks_tick, then notifies with a toast and a chime,
// naming the events of the segments that filled. Spec: docs/features/clocks.md.

import { api } from './api';
import { eventsReached, isRunning, nextDueMs } from './clocks';
import { app } from './state.svelte';
import type { Clock, ClockPatch, ClockTick, FiredEvent } from './types';

// setTimeout overflows past ~24.8 days; re-plan at least hourly anyway.
const MAX_WAIT = 3600_000;

let audio: AudioContext | null = null;

/** A short bell: one tone per segment, a rising triple when a clock fills. */
function chime(full: boolean): void {
  try {
    audio ??= new AudioContext();
    const ctx = audio;
    const notes = full ? [660, 880, 1320] : [880];
    notes.forEach((f, i) => {
      const t = ctx.currentTime + i * 0.18;
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = 'sine';
      osc.frequency.value = f;
      gain.gain.setValueAtTime(0.0001, t);
      gain.gain.exponentialRampToValueAtTime(0.25, t + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, t + 0.9);
      osc.connect(gain).connect(ctx.destination);
      osc.start(t);
      osc.stop(t + 1);
    });
  } catch {
    // no audio device: the toast is enough
  }
}

class ClockState {
  list = $state<Clock[]>([]);
  loaded = $state(false);
  /** The clock shown in the inspector and the Detail view (shared, so switching views keeps it). */
  selected = $state<string | null>(null);
  #timer: ReturnType<typeof setTimeout> | null = null;

  async refresh(): Promise<void> {
    try {
      this.list = await api.clocksList();
      this.loaded = true;
      if (!this.list.some((c) => c.id === this.selected)) this.selected = this.list[0]?.id ?? null;
    } catch (e) {
      app.error('Could not load the clocks', e);
    }
    this.schedule();
  }

  /** Puts a clock returned by a command into the list. */
  put(c: Clock): void {
    const i = this.list.findIndex((k) => k.id === c.id);
    if (i >= 0) this.list[i] = c;
    else this.list.push(c);
    this.schedule();
  }

  setAll(list: Clock[]): void {
    this.list = list;
    this.schedule();
  }

  schedule(): void {
    if (this.#timer) clearTimeout(this.#timer);
    this.#timer = null;
    const due = this.list.map(nextDueMs).filter((t): t is number => t != null);
    if (!due.length) return;
    const wait = Math.min(MAX_WAIT, Math.max(0, Math.min(...due) - Date.now()) + 50);
    this.#timer = setTimeout(() => void this.tick(), wait);
  }

  async tick(): Promise<void> {
    this.#timer = null;
    try {
      const moved = await api.clocksTick();
      if (moved.length) this.notify(moved);
    } catch (e) {
      app.error('Could not advance the clocks', e);
    }
    await this.refresh();
  }

  notify(moved: ClockTick[]): void {
    for (const t of moved) {
      const segs = t.added === 1 ? 'a segment' : `${t.added} segments`;
      app.toast(t.full ? 'success' : 'info', t.full ? `${t.name} is full (${t.filled}/${t.segments})` : `${t.name} advanced ${segs}: ${t.filled}/${t.segments}`);
      announce(t.name, t.events);
    }
    chime(moved.some((t) => t.full));
  }

  // ── actions shared by the views ──

  async #run<T>(p: Promise<T>, what: string): Promise<T | undefined> {
    try {
      return await p;
    } catch (e) {
      app.error(what, e);
    }
  }

  /** Applies a changed clock; filling by hand into a segment with events announces them. */
  async #apply(before: Clock, p: Promise<Clock>, what = 'Could not change the clock'): Promise<void> {
    const c = await this.#run(p, what);
    if (!c) return;
    this.put(c);
    const fired = eventsReached(c, before.filled, c.filled);
    if (fired.length) {
      announce(c.name, fired);
      chime(false);
    }
  }

  async create(): Promise<void> {
    const c = await this.#run(api.clocksCreate({}), 'Could not make a clock');
    if (c) {
      this.put(c);
      this.selected = c.id;
    }
  }
  patch(c: Clock, p: ClockPatch): Promise<void> {
    return this.#apply(c, api.clocksUpdate(c.id, p));
  }
  advance(c: Clock, by: number): Promise<void> {
    return this.#apply(c, api.clocksAdvance(c.id, by));
  }
  reset(c: Clock): Promise<void> {
    return this.#apply(c, api.clocksReset(c.id));
  }
  async toggleRun(c: Clock): Promise<void> {
    const all = await this.#run(api.clocksRun(c.id, !isRunning(c)), 'Could not start the timer');
    if (all) this.setAll(all);
  }
  async runAll(on: boolean): Promise<void> {
    const all = await this.#run(api.clocksRun(null, on), on ? 'Could not start the timers' : 'Could not pause the timers');
    if (all) this.setAll(all);
  }
  async remove(c: Clock): Promise<void> {
    const all = await this.#run(api.clocksDelete(c.id), 'Could not delete the clock');
    if (all) {
      this.setAll(all);
      if (this.selected === c.id) this.selected = all[0]?.id ?? null;
    }
  }

  dispose(): void {
    if (this.#timer) clearTimeout(this.#timer);
    this.#timer = null;
  }
}

// Events stay up longer than the usual toast: they are what the DM must act on.
const EVENT_TOAST_MS = 15_000;

function announce(name: string, fired: FiredEvent[]): void {
  for (const f of fired) app.toast('info', `${name} · segment ${f.segment}: ${f.text}`, undefined, EVENT_TOAST_MS);
}

export const clocks = new ClockState();
