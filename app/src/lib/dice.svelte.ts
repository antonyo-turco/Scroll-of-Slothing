// Floating dice roller state (docs/features/dice-roller.md): which state the widget
// is in, where it sits, and the session-only list of recent rolls.
// Position and the hidden/shown preference persist in the `diceRoller` workspace key.

import { api } from './api';
import type { RollResult } from './types';

export type DiceMode = 'hidden' | 'compact' | 'expanded';

export interface PostedRoll extends RollResult {
  id: number;
}

interface Saved {
  x: number | null;
  y: number | null;
  state: 'hidden' | 'compact';
}

const KEY = 'diceRoller';
/** Recent rolls kept in the list. */
export const HISTORY = 10;

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
      this.rolls = [...this.rolls, { ...r, id: ++this.seq }].slice(-HISTORY);
      return true;
    } catch (e) {
      this.error = (e as { message?: string })?.message ?? String(e);
      return false;
    }
  }

  clear(): void {
    this.rolls = [];
    this.error = null;
  }
}

export const dice = new DiceRoller();
