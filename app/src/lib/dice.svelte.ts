// Floating dice roller state (docs/features/dice-roller.md): which state the widget
// is in, where it sits, and the session-only list of recent rolls.
// Position and the hidden/shown preference persist in the `diceRoller` workspace key.

import { api } from './api';
import { app } from './state.svelte';
import type { RollResult } from './types';

export type DiceMode = 'hidden' | 'compact' | 'expanded';

export interface PostedRoll extends RollResult {
  id: number;
  /** Where a roll came from outside the widget, e.g. `Claw to hit`. */
  label?: string;
  /** Who rolled it, e.g. the creature. */
  who?: string;
  /** CRIT, MISS, SAVED, FAILED. */
  tag?: string;
}

interface Saved {
  x: number | null;
  y: number | null;
  state: 'hidden' | 'compact';
}

const KEY = 'diceRoller';
/** Most rolls ever kept; the list shows the last `dice_history` (a setting, 1-50). */
export const MAX_HISTORY = 50;

class DiceRoller {
  mode = $state<DiceMode>('compact');
  /** Top-left in px; null = default corner (bottom-right). */
  x = $state<number | null>(null);
  y = $state<number | null>(null);
  rolls = $state<PostedRoll[]>([]);
  error = $state<string | null>(null);
  /** Bumped to ask the expression line to take focus. */
  focusTick = $state(0);
  private seq = 0;
  private loaded = false;

  readonly latest = $derived(this.rolls.at(-1) ?? null);
  /** The rolls the list shows, oldest first. */
  readonly shown = $derived(this.rolls.slice(-Math.max(1, app.settings.dice_history || 10)));

  async load(): Promise<void> {
    try {
      const s = await api.workspaceGet<Saved>(KEY);
      if (s) {
        this.x = s.x;
        this.y = s.y;
        this.mode = s.state === 'hidden' ? 'hidden' : 'compact';
      }
    } catch {
      // first run, or the mock without the key: keep the defaults
    }
    this.loaded = true;
  }

  private save(): void {
    if (!this.loaded) return;
    const state = this.mode === 'hidden' ? 'hidden' : 'compact';
    void api.workspaceSet(KEY, { x: this.x, y: this.y, state } satisfies Saved).catch(() => {});
  }

  /** Rail button / `D` key / palette: hide when shown, open (expanded) when hidden. */
  toggle(): void {
    if (this.mode === 'hidden') this.expand();
    else this.hide();
  }

  expand(): void {
    const wasHidden = this.mode === 'hidden';
    this.mode = 'expanded';
    this.focusTick++;
    if (wasHidden) this.save();
  }

  collapse(): void {
    if (this.mode === 'expanded') this.mode = 'compact';
    this.error = null;
  }

  hide(): void {
    this.mode = 'hidden';
    this.error = null;
    this.save();
  }

  moveTo(x: number, y: number): void {
    this.x = Math.round(x);
    this.y = Math.round(y);
  }

  endMove(): void {
    this.save();
  }

  /** Rolls notation through the service; an invalid one sets `error` and posts nothing. */
  async roll(notation: string): Promise<boolean> {
    try {
      const r = await api.diceRoll(notation);
      this.error = null;
      this.push(r);
      return true;
    } catch (e) {
      this.error = (e as { message?: string })?.message ?? String(e);
      return false;
    }
  }

  /** Posts a roll made elsewhere (stat blocks in Combat) into the list. */
  post(r: RollResult & { label?: string; who?: string; tag?: string }): void {
    this.push(r);
  }

  private push(r: Omit<PostedRoll, 'id'>): void {
    this.rolls = [...this.rolls, { ...r, id: ++this.seq }].slice(-MAX_HISTORY);
  }

  clear(): void {
    this.rolls = [];
    this.error = null;
  }
}

export const dice = new DiceRoller();
