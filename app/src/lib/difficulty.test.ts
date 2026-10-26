import { describe, expect, it } from 'vitest';
import { difficulty, multiplier2014, xpOf } from './difficulty';

describe('encounter difficulty', () => {
  const party = [3, 3, 3, 3]; // 2024 budgets: low 600, moderate 900, high 1600

  it('rates against the 2024 budgets; over High is Deadly, over 2×High a TPK', () => {
    const r = (xp: number) => difficulty([{ xp, count: 1 }], party, false).rating;
    expect(r(400)).toBe('low');
    expect(r(600)).toBe('low');
    expect(r(900)).toBe('moderate');
    expect(r(1600)).toBe('high');
    expect(r(1601)).toBe('deadly');
    expect(r(3201)).toBe('tpk');
    expect(r(100)).toBe('trivial');
  });

  it('applies the 2014 multiplier only when asked', () => {
    const goblins = [{ xp: xpOf('1/4'), count: 6 }];
    expect(difficulty(goblins, party, false)).toMatchObject({ base: 300, adjusted: 300, multiplier: 1 });
    expect(difficulty(goblins, party, true)).toMatchObject({ base: 300, adjusted: 600, multiplier: 2 });
  });

  it('shifts the 2014 multiplier for small and large parties', () => {
    expect(multiplier2014(1, 4)).toBe(1);
    expect(multiplier2014(2, 4)).toBe(1.5);
    expect(multiplier2014(15, 4)).toBe(4);
    expect(multiplier2014(1, 2)).toBe(1.5);
    expect(multiplier2014(1, 6)).toBe(0.5);
    expect(multiplier2014(15, 1)).toBe(5);
  });
});
