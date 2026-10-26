// Clock rules for the UI and the browser mock (mirror of crates/core/src/clocks.rs).
// Times are Unix milliseconds.

import type { Clock, ClockEvent, FiredEvent } from './types';

export const SEGMENT_CHOICES = [4, 6, 8, 12];
export const MIN_SEGMENTS = 2;
export const MAX_SEGMENTS = 24;
export const MIN_EVERY_SECS = 10;
export const MAX_EVERY_SECS = 24 * 3600;

/** Swatches offered for a clock (null is the accent colour). */
export const CLOCK_COLORS: (string | null)[] = [null, '#c0392b', '#d68910', '#1e8449', '#2471a3', '#7d3c98', '#5d6d7e'];

export const isRunning = (c: Clock): boolean => c.auto?.since_ms != null;
export const isFull = (c: Clock): boolean => c.filled >= c.segments;

/** Time counted toward the next segment. */
export function elapsedMs(c: Clock, now: number): number {
  const a = c.auto;
  if (!a) return 0;
  return a.since_ms != null ? Math.max(0, now - a.since_ms) : a.carry_ms;
}

/** When the timer adds its next segment (null when it is not running). */
export function nextDueMs(c: Clock): number | null {
  const a = c.auto;
  return a?.since_ms != null ? a.since_ms + a.every_secs * 1000 : null;
}

/** Share of the next segment counted so far, 0–1. */
export function progress(c: Clock, now: number): number {
  if (!c.auto || isFull(c)) return 0;
  return Math.min(1, elapsedMs(c, now) / (c.auto.every_secs * 1000));
}

/** `m:ss`, or `h:mm:ss` from an hour. */
export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, '0');
  return h ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`;
}

/** A period in seconds as words: "10 min", "90 s", "1 h 30 min". */
export function formatEvery(secs: number): string {
  if (secs < 60) return `${secs} s`;
  if (secs % 60) return `${Math.floor(secs / 60)} min ${secs % 60} s`;
  const m = secs / 60;
  if (m < 60) return `${m} min`;
  return m % 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m / 60} h`;
}

/** The timer line under a clock: "next in 4:32", "paused · 4:32 left", "full · every 10 min". */
export function timerText(c: Clock, now: number): string {
  const a = c.auto;
  if (!a) return '';
  if (isFull(c)) return `full · every ${formatEvery(a.every_secs)}`;
  const left = a.every_secs * 1000 - elapsedMs(c, now);
  return isRunning(c) ? `next in ${formatDuration(left)}` : `paused · ${formatDuration(left)} left`;
}

/** Whether filling segment `segment` (1-based) triggers `e`. */
export function firesAt(e: ClockEvent, segment: number): boolean {
  if (segment === e.at) return true;
  return !!e.repeat && e.repeat > 0 && segment > e.at && (segment - e.at) % e.repeat === 0;
}

/** Events triggered by filling from `from` to `to` segments, in segment order. */
export function eventsReached(c: Clock, from: number, to: number): FiredEvent[] {
  const out: FiredEvent[] = [];
  for (let seg = from + 1; seg <= Math.min(to, c.segments); seg++) {
    for (const e of c.events) if (firesAt(e, seg)) out.push({ segment: seg, text: e.text });
  }
  return out;
}

/** The segments of this clock at which `e` fires. */
export function firingSegments(e: ClockEvent, segments: number): number[] {
  const out: number[] = [];
  for (let s = 1; s <= segments; s++) if (firesAt(e, s)) out.push(s);
  return out;
}

function stopFull(c: Clock): void {
  if (c.auto) {
    c.auto.since_ms = null;
    c.auto.carry_ms = 0;
  }
}

export function setFilled(c: Clock, filled: number): void {
  c.filled = Math.max(0, Math.min(c.segments, filled));
  if (isFull(c)) stopFull(c);
}

export function setSegments(c: Clock, n: number): void {
  c.segments = Math.max(MIN_SEGMENTS, Math.min(MAX_SEGMENTS, n));
  setFilled(c, c.filled);
}

export function setEvery(c: Clock, secs: number | null): void {
  if (secs == null) c.auto = null;
  else if (c.auto) c.auto.every_secs = secs;
  else c.auto = { every_secs: secs, since_ms: null, carry_ms: 0 };
}

export function setRunning(c: Clock, run: boolean, now: number): boolean {
  const a = c.auto;
  if (!a) return false;
  if (run && a.since_ms == null && !isFull(c)) a.since_ms = now - a.carry_ms;
  else if (!run && a.since_ms != null) {
    a.carry_ms = elapsedMs(c, now);
    a.since_ms = null;
  }
  return a.since_ms != null;
}

export function reset(c: Clock): void {
  c.filled = 0;
  if (c.auto) {
    c.auto.since_ms = null;
    c.auto.carry_ms = 0;
  }
}

/** Adds every segment the timer owes at `now`; null when nothing was due. */
export function tick(c: Clock, now: number): { added: number; full: boolean } | null {
  const a = c.auto;
  if (a?.since_ms == null) return null;
  const period = a.every_secs * 1000;
  const due = Math.floor((now - a.since_ms) / period);
  if (due <= 0) return null;
  const added = Math.min(due, c.segments - c.filled);
  a.since_ms += added * period;
  c.filled += added;
  const full = isFull(c);
  if (full) stopFull(c);
  return { added, full };
}
