// Encounter difficulty: 2024 DMG XP budgets per character, with an optional
// 2014 DMG multiplier for the number of enemies. Above the High budget counts
// as Deadly, above twice High as a likely total party kill.

export const CR_XP: Record<string, number> = {
  '0': 10, '1/8': 25, '1/4': 50, '1/2': 100, '1': 200, '2': 450, '3': 700, '4': 1100, '5': 1800, '6': 2300,
  '7': 2900, '8': 3900, '9': 5000, '10': 5900, '11': 7200, '12': 8400, '13': 10000, '14': 11500, '15': 13000,
  '16': 15000, '17': 18000, '18': 20000, '19': 22000, '20': 25000, '21': 33000, '22': 41000, '23': 50000,
  '24': 62000, '25': 75000, '26': 90000, '27': 105000, '28': 120000, '29': 135000, '30': 155000,
};

/** 2024 DMG, XP budget per character: [Low, Moderate, High] by level 1–20. */
export const BUDGET_2024: [number, number, number][] = [
  [50, 75, 100], [100, 150, 200], [150, 225, 400], [250, 375, 500], [500, 750, 1100],
  [600, 1000, 1400], [750, 1300, 1700], [1000, 1700, 2100], [1300, 2000, 2600], [1600, 2300, 3100],
  [1900, 2900, 4100], [2200, 3700, 4700], [2600, 4200, 5400], [2900, 4900, 6200], [3300, 5400, 7800],
  [3800, 6100, 9800], [4500, 7200, 11700], [5000, 8700, 14200], [5500, 10700, 17200], [6400, 13200, 22000],
];

export const xpOf = (cr: string | null | undefined): number | null => (cr != null && cr in CR_XP ? CR_XP[cr] : null);

/** 2014 DMG multipliers; parties under 3 use the next higher step, 6+ the next lower. */
const STEPS = [0.5, 1, 1.5, 2, 2.5, 3, 4, 5];
export function multiplier2014(monsters: number, partySize: number): number {
  if (monsters <= 0) return 1;
  let i = monsters === 1 ? 1 : monsters === 2 ? 2 : monsters <= 6 ? 3 : monsters <= 10 ? 4 : monsters <= 14 ? 5 : 6;
  if (partySize > 0 && partySize < 3) i += 1;
  else if (partySize >= 6) i -= 1;
  return STEPS[Math.max(0, Math.min(STEPS.length - 1, i))];
}

export type Rating = 'trivial' | 'low' | 'moderate' | 'high' | 'deadly' | 'tpk';
export const RATING_LABEL: Record<Rating, string> = {
  trivial: 'Trivial', low: 'Low', moderate: 'Moderate', high: 'High', deadly: 'Deadly', tpk: 'Total party kill',
};

export interface Difficulty {
  /** Sum of the creatures' XP. */
  base: number;
  /** XP compared with the budgets (base × multiplier). */
  adjusted: number;
  multiplier: number;
  budget: { low: number; moderate: number; high: number };
  rating: Rating;
}

/** Budgets are "up to": at most Low is Low, …, over High is Deadly, over 2×High a TPK. */
export function difficulty(
  creatures: { xp: number | null; count: number }[],
  levels: number[],
  use2014Multiplier: boolean,
): Difficulty {
  const base = creatures.reduce((a, c) => a + (c.xp ?? 0) * c.count, 0);
  const n = creatures.reduce((a, c) => a + (c.xp != null ? c.count : 0), 0);
  const multiplier = use2014Multiplier ? multiplier2014(n, levels.length) : 1;
  const adjusted = Math.round(base * multiplier);
  const budget = { low: 0, moderate: 0, high: 0 };
  for (const l of levels) {
    const b = BUDGET_2024[Math.max(1, Math.min(20, Math.round(l))) - 1];
    budget.low += b[0];
    budget.moderate += b[1];
    budget.high += b[2];
  }
  let rating: Rating;
  if (adjusted === 0) rating = 'trivial';
  else if (adjusted > 2 * budget.high) rating = 'tpk';
  else if (adjusted > budget.high) rating = 'deadly';
  else if (adjusted > budget.moderate) rating = 'high';
  else if (adjusted > budget.low) rating = 'moderate';
  else if (adjusted >= budget.low / 2) rating = 'low';
  else rating = 'trivial';
  return { base, adjusted, multiplier, budget, rating };
}
