import { describe, expect, it } from 'vitest';
import { scaleCreature } from './scale';

describe('CR scaling', () => {
  const goblin = {
    name: 'Goblin', cr: '1/4', ac: [{ ac: 15, from: ['leather armor'] }], hp: { average: 7, formula: '2d6' },
    save: { dex: '+4' },
    action: [{ name: 'Scimitar', entries: ['{@atk mw} {@hit 4} to hit. {@h}5 ({@damage 1d6 + 2}) slashing damage. DC 12 to resist.'] }],
  };

  it('raises AC, HP, to-hit, damage and DCs to the target CR', () => {
    const s = scaleCreature(goblin, '5')!;
    expect(s.cr).toBe('5');
    expect((s.ac as { ac: number }[])[0].ac).toBe(17); // 13 → 15
    const hp = s.hp as { average: number; formula: string };
    expect(hp.average).toBeGreaterThan(100);
    expect(hp.formula).toMatch(/^\d+d6$/);
    const line = (s.action as { entries: string[] }[])[0].entries[0];
    expect(line).toContain('{@hit 7}'); // 3 → 6 attack bonus
    expect(line).toContain('DC 14'); // 13 → 15 DC
    expect(line).not.toContain('{@damage 1d6 + 2}');
    expect((s.save as Record<string, string>).dex).toBe('+5'); // proficiency 2 → 3
  });

  it('returns nothing for the same CR or an unknown one', () => {
    expect(scaleCreature(goblin, '1/4')).toBeNull();
    expect(scaleCreature({ cr: 'Unknown' }, '3')).toBeNull();
  });
});
