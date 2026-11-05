import { describe, expect, it } from 'vitest';
import { parseNotation, rollNotation } from './dice';

// Mirror of crates/core/src/dice.rs: same canonical text and the same errors.
describe('dice notation', () => {
  const text = (s: string) => {
    const p = parseNotation(s);
    if (typeof p === 'string') throw new Error(p);
    return p.text;
  };

  it('canonicalises like the core', () => {
    expect(text('2d6+3')).toBe('2d6 + 3');
    expect(text('1D20')).toBe('d20');
    expect(text('d%')).toBe('d100');
    expect(text('1d20-1')).toBe('d20 − 1');
    expect(text('-1+d4')).toBe('−1 + d4');
    expect(text('ADV+5')).toBe('adv + 5');
    expect(text(text('2d8+1d6-4'))).toBe('2d8 + d6 − 4');
  });

  it('rejects what the core rejects', () => {
    expect(parseNotation('')).toBe('nothing to roll');
    expect(parseNotation('+')).toBe('nothing to roll');
    for (const bad of ['abc', 'd', '2d0', '0d6', '2d', 'd-4', '2.5d6', '2d6+', 'd20++3', 'advx']) {
      expect(typeof parseNotation(bad), bad).toBe('string');
    }
    expect(parseNotation('300d6')).toBe('at most 200 dice in one roll');
    expect(typeof parseNotation('199d1+1d1')).toBe('object');
  });

  it('rolls totals within bounds with a readable detail', () => {
    for (let i = 0; i < 200; i++) {
      const r = rollNotation('2d6-1d4+3');
      if (typeof r === 'string') throw new Error(r);
      const [a, b] = r.groups;
      expect(r.total).toBe(a.faces[0] + a.faces[1] - b.faces[0] + 3);
      expect(r.detail).toBe(`[${a.faces.join(', ')}] − [${b.faces[0]}] + 3 = ${r.total}`);
      const adv = rollNotation('adv');
      if (typeof adv === 'string') throw new Error(adv);
      expect(adv.groups[0].kept).toBe(Math.max(...adv.groups[0].faces));
    }
    expect(rollNotation('5')).toMatchObject({ total: 5, detail: '5', groups: [] });
  });
});
