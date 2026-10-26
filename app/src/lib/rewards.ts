// End-of-fight rewards: the defeated enemies' XP split among the heroes, and
// each creature's individual treasure (2024 Dungeon Master's Guide, coins by CR).

import { rollFormula } from './dice';

export type Coin = 'cp' | 'sp' | 'gp' | 'pp';

/** Individual treasure by challenge rating: dice × multiplier coins. */
export const INDIVIDUAL_TREASURE: { upTo: number; dice: string; times: number; coin: Coin }[] = [
  { upTo: 4, dice: '3d6', times: 1, coin: 'gp' },
  { upTo: 10, dice: '2d8', times: 10, coin: 'gp' },
  { upTo: 16, dice: '2d10', times: 10, coin: 'pp' },
  { upTo: Infinity, dice: '2d8', times: 100, coin: 'pp' },
];

/** "1/4" → 0.25, "5" → 5; null when it isn't a CR. */
export function crNumber(cr: string | null | undefined): number | null {
  if (cr == null || cr === '') return null;
  const m = String(cr).trim().match(/^(\d+)(?:\/(\d+))?$/);
  if (!m) return null;
  return m[2] ? Number(m[1]) / Number(m[2]) : Number(m[1]);
}

export function treasureRow(cr: string | null | undefined) {
  const n = crNumber(cr);
  return n === null ? null : INDIVIDUAL_TREASURE.find((r) => n <= r.upTo)!;
}

export type CoinRoll = { amount: number; coin: Coin; detail: string };

/** Rolls a creature's individual treasure; null without a CR. */
export function rollIndividualTreasure(cr: string | null | undefined): CoinRoll | null {
  const row = treasureRow(cr);
  if (!row) return null;
  const r = rollFormula(row.dice)!;
  return { amount: r.total * row.times, coin: row.coin, detail: `${row.dice}${row.times > 1 ? ` × ${row.times}` : ''}: ${r.detail}` };
}

/** XP per hero (rounded down) and what is left over. */
export function splitXp(total: number, heroes: number): { each: number; rest: number } {
  if (heroes < 1) return { each: 0, rest: total };
  const each = Math.floor(total / heroes);
  return { each, rest: total - each * heroes };
}

/** Coin totals, largest coin first: "12 pp, 340 gp". */
export function coinTotal(rolls: (CoinRoll | null)[]): string {
  const sum: Record<Coin, number> = { pp: 0, gp: 0, sp: 0, cp: 0 };
  for (const r of rolls) if (r) sum[r.coin] += r.amount;
  return (Object.entries(sum) as [Coin, number][])
    .filter(([, n]) => n)
    .map(([c, n]) => `${n.toLocaleString()} ${c}`)
    .join(', ');
}

/** The 2024 treasure themes of a 5etools creature (`treasure: ["arcana", …]`), title-cased. */
export function treasureThemes(mon: Record<string, unknown> | null): string[] {
  const t = mon?.treasure;
  if (!Array.isArray(t)) return [];
  return t
    .map((x) => (typeof x === 'string' ? x : ''))
    .filter((x) => x && x !== 'individual' && x !== 'any')
    .map((x) => x[0].toUpperCase() + x.slice(1));
}
