// Types shared by the Combat view and its parts (notices, rolls, dock, editor).

import type { Difficulty } from '../../lib/difficulty';
import type { CombatRow } from '../../lib/types';

export type Mon = Record<string, unknown>;

/** One line of the rolls window. */
export type RollLine = { id: number; who: string; entry: string; kind: 'hit' | 'damage'; total: number; detail: string; tag: string };

export type ConcAlert = { id: number; who: string; damage: number; dc: number };
export type RechargeNote = { rowId: string; who: string; ability: string; need: number };
export type LegendItem = { who: string; left: number };

/** A turn group in the turn card: copies acting together show once, with a count. */
export type Who = { name: string; count: number; hero: boolean; side: CombatRow['side'] };
export type TurnCard = { id: number; now: Who; next: Who | null; round: number };

/** The fight's difficulty, as drawn by the dock's strip. */
export type Fight = {
  d: Difficulty;
  zones: { cls: string; label: string; to: number; w: number }[];
  levels: number[];
  enemies: number;
  unknown: number;
  pos: number;
};

/** What the creature editor changes on a row (undefined: left as it is). */
export type EditorPatch = {
  overrides: Mon | null;
  custom_name?: string | null;
  hp_max?: number | null;
  hp_current?: number | null;
  initiative?: number | null;
};
