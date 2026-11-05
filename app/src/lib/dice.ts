// Dice for the combat tracker: attack rolls (with advantage/disadvantage) and
// damage formulas such as `2d6 + 4` (crits double the dice).
// Also the notation roller behind the floating dice widget's mock backend.

import type { DieGroup, RollResult } from './types';

export interface DiceRoll {
  formula: string;
  total: number;
  /** The individual dice, e.g. "[4, 6] + 4". */
  detail: string;
  /** The kept d20 of an attack roll. */
  natural?: number;
}

const die = (n: number) => 1 + Math.floor(Math.random() * n);

/** Rolls a formula of `NdM` and number terms. A crit doubles the dice. Null on other syntax. */
export function rollFormula(formula: string, crit = false): DiceRoll | null {
  const f = formula.replace(/\s+/g, '');
  const terms = f.match(/[+-]?[^+-]+/g);
  if (!terms) return null;
  let total = 0;
  const parts: string[] = [];
  for (const raw of terms) {
    const sign = raw.startsWith('-') ? -1 : 1;
    const t = raw.replace(/^[+-]/, '');
    const m = t.match(/^(\d*)d(\d+)$/i);
    if (m) {
      const count = (Number(m[1]) || 1) * (crit ? 2 : 1);
      const faces = Number(m[2]);
      if (!faces || count > 200) return null;
      const dice = Array.from({ length: count }, () => die(faces));
      total += sign * dice.reduce((a, b) => a + b, 0);
      parts.push(`${sign < 0 ? '− ' : parts.length ? '+ ' : ''}[${dice.join(', ')}]`);
    } else if (/^\d+$/.test(t)) {
      total += sign * Number(t);
      parts.push(`${sign < 0 ? '−' : '+'} ${t}`);
    } else {
      return null;
    }
  }
  return { formula, total, detail: parts.join(' ') };
}

// ── notation roller: mirror of crates/core/src/dice.rs (used by the browser mock) ──

export const MAX_DICE = 200;
const MAX_SIDES = 10_000;

type Term =
  | { kind: 'dice'; sign: number; count: number; sides: number }
  | { kind: 'keep'; sign: number; high: boolean }
  | { kind: 'const'; sign: number; value: number };

const joinSigned = (parts: [number, string][]) =>
  parts.map(([s, b], i) => (i === 0 ? (s < 0 ? `−${b}` : b) : ` ${s < 0 ? '−' : '+'} ${b}`)).join('');

/** Parses notation into terms, or returns the error message. */
export function parseNotation(input: string): { text: string; terms: Term[] } | string {
  const clean = input.toLowerCase().replace(/−/g, '-').replace(/d%/g, 'd100').replace(/\s+/g, '');
  const chunks = clean.match(/[+-]?[^+-]*/g)?.filter((c, i, a) => c !== '' || i < a.length - 1) ?? [];
  const bodies = chunks.map((c) => c.replace(/^[+-]/, ''));
  if (bodies.every((b) => b === '')) return 'nothing to roll';
  const terms: Term[] = [];
  let dice = 0;
  for (const chunk of chunks) {
    const sign = chunk.startsWith('-') ? -1 : 1;
    const body = chunk.replace(/^[+-]/, '');
    const m = body.match(/^(\d*)d(\d+)$/);
    let term: Term;
    if (body === 'adv' || body === 'dis') term = { kind: 'keep', sign, high: body === 'adv' };
    else if (m && (m[1] === '' || Number(m[1]) > 0) && Number(m[2]) > 0 && Number(m[2]) <= MAX_SIDES)
      term = { kind: 'dice', sign, count: m[1] ? Number(m[1]) : 1, sides: Number(m[2]) };
    else if (/^\d+$/.test(body)) term = { kind: 'const', sign, value: Number(body) };
    else return `'${body}' isn't a die roll or a number`;
    dice += term.kind === 'dice' ? term.count : term.kind === 'keep' ? 2 : 0;
    if (dice > MAX_DICE) return `at most ${MAX_DICE} dice in one roll`;
    terms.push(term);
  }
  const text = joinSigned(terms.map((t): [number, string] =>
    t.kind === 'dice' ? [t.sign, `${t.count === 1 ? '' : t.count}d${t.sides}`]
      : t.kind === 'keep' ? [t.sign, t.high ? 'adv' : 'dis'] : [t.sign, String(t.value)]));
  return { text, terms };
}

/** Rolls notation like the service's `dice_roll`, or returns the error message. */
export function rollNotation(input: string): RollResult | string {
  const parsed = parseNotation(input);
  if (typeof parsed === 'string') return parsed;
  const groups: DieGroup[] = [];
  const parts: [number, string][] = [];
  let total = 0;
  let anyDice = false;
  for (const t of parsed.terms) {
    if (t.kind === 'const') {
      total += t.sign * t.value;
      parts.push([t.sign, String(t.value)]);
      continue;
    }
    anyDice = true;
    if (t.kind === 'dice') {
      const faces = Array.from({ length: t.count }, () => die(t.sides));
      total += t.sign * faces.reduce((a, b) => a + b, 0);
      parts.push([t.sign, `[${faces.join(', ')}]`]);
      groups.push({ sides: t.sides, faces, sign: t.sign });
    } else {
      const faces = [die(20), die(20)];
      const kept = t.high ? Math.max(...faces) : Math.min(...faces);
      total += t.sign * kept;
      parts.push([t.sign, `[${faces.join(', ')}] ${t.high ? 'adv' : 'dis'}`]);
      groups.push({ sides: 20, faces, kept, sign: t.sign });
    }
  }
  const detail = joinSigned(parts) + (anyDice ? ` = ${total}` : '');
  return { expression: parsed.text, total, detail, groups };
}

/** d20 + bonus; `adv` 1 is advantage, -1 disadvantage. */
export function rollAttack(bonus: number, adv: -1 | 0 | 1 = 0): DiceRoll {
  const a = die(20);
  const b = adv ? die(20) : a;
  const natural = adv > 0 ? Math.max(a, b) : adv < 0 ? Math.min(a, b) : a;
  const shown = adv ? `[${a}, ${b}] ${adv > 0 ? 'adv' : 'dis'}` : `[${a}]`;
  return { formula: `1d20${bonus >= 0 ? '+' : ''}${bonus}`, total: natural + bonus, detail: `${shown} ${bonus >= 0 ? '+' : '−'} ${Math.abs(bonus)}`, natural };
}
