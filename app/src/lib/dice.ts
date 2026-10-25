// Dice for the combat tracker: attack rolls (with advantage/disadvantage) and
// damage formulas such as `2d6 + 4` (crits double the dice).

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

/** d20 + bonus; `adv` 1 is advantage, -1 disadvantage. */
export function rollAttack(bonus: number, adv: -1 | 0 | 1 = 0): DiceRoll {
  const a = die(20);
  const b = adv ? die(20) : a;
  const natural = adv > 0 ? Math.max(a, b) : adv < 0 ? Math.min(a, b) : a;
  const shown = adv ? `[${a}, ${b}] ${adv > 0 ? 'adv' : 'dis'}` : `[${a}]`;
  return { formula: `1d20${bonus >= 0 ? '+' : ''}${bonus}`, total: natural + bonus, detail: `${shown} ${bonus >= 0 ? '+' : '−'} ${Math.abs(bonus)}`, natural };
}
