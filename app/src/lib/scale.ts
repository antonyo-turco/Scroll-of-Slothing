// Approximate CR scaling of a 5etools creature with the DMG's "Monster
// Statistics by Challenge Rating" table: AC, to-hit and DCs shift by the
// table's difference, HP and damage scale by the ratio of the table's
// midpoints. Not a port of 5etools ScaleCreature (see combat-tracker.md).

type Mon = Record<string, unknown>;

/** [CR, proficiency, AC, HP min, HP max, attack bonus, DPR min, DPR max, save DC] */
const TABLE: [string, number, number, number, number, number, number, number, number][] = [
  ['0', 2, 13, 1, 6, 3, 0, 1, 13], ['1/8', 2, 13, 7, 35, 3, 2, 3, 13], ['1/4', 2, 13, 36, 49, 3, 4, 5, 13],
  ['1/2', 2, 13, 50, 70, 3, 6, 8, 13], ['1', 2, 13, 71, 85, 3, 9, 14, 13], ['2', 2, 13, 86, 100, 3, 15, 20, 13],
  ['3', 2, 13, 101, 115, 4, 21, 26, 13], ['4', 2, 14, 116, 130, 5, 27, 32, 14], ['5', 3, 15, 131, 145, 6, 33, 38, 15],
  ['6', 3, 15, 146, 160, 6, 39, 44, 15], ['7', 3, 15, 161, 175, 6, 45, 50, 15], ['8', 3, 16, 176, 190, 7, 51, 56, 16],
  ['9', 4, 16, 191, 205, 7, 57, 62, 16], ['10', 4, 17, 206, 220, 7, 63, 68, 16], ['11', 4, 17, 221, 235, 8, 69, 74, 17],
  ['12', 4, 17, 236, 250, 8, 75, 80, 17], ['13', 5, 18, 251, 265, 8, 81, 86, 18], ['14', 5, 18, 266, 280, 8, 87, 92, 18],
  ['15', 5, 18, 281, 295, 8, 93, 98, 18], ['16', 5, 18, 296, 310, 9, 99, 104, 18], ['17', 6, 19, 311, 325, 10, 105, 110, 19],
  ['18', 6, 19, 326, 340, 10, 111, 116, 19], ['19', 6, 19, 341, 355, 10, 117, 122, 19], ['20', 6, 19, 356, 400, 10, 123, 140, 19],
  ['21', 7, 19, 401, 445, 11, 141, 158, 20], ['22', 7, 19, 446, 490, 11, 159, 176, 20], ['23', 7, 19, 491, 535, 11, 177, 194, 20],
  ['24', 7, 19, 536, 580, 12, 195, 212, 21], ['25', 8, 19, 581, 625, 12, 213, 230, 21], ['26', 8, 19, 626, 670, 12, 231, 248, 21],
  ['27', 8, 19, 671, 715, 13, 249, 266, 22], ['28', 8, 19, 716, 760, 13, 267, 284, 22], ['29', 9, 19, 761, 805, 13, 285, 302, 22],
  ['30', 9, 19, 806, 850, 14, 303, 320, 23],
];
export const CRS = TABLE.map((r) => r[0]);

const row = (cr: string) => TABLE.find((r) => r[0] === cr);
export const crOfMon = (m: Mon): string | null => {
  const c = m.cr && typeof m.cr === 'object' ? (m.cr as { cr?: string }).cr : (m.cr as string | undefined);
  return c ?? null;
};

/** Scales a creature to `target` CR; returns the changed 5etools fields (overrides). */
export function scaleCreature(mon: Mon, target: string): Mon | null {
  const from = row(crOfMon(mon) ?? '');
  const to = row(target);
  if (!from || !to || from[0] === to[0]) return null;
  const dProf = to[1] - from[1];
  const dAc = to[2] - from[2];
  const hpRatio = (to[3] + to[4]) / Math.max(1, from[3] + from[4]);
  const dHit = to[5] - from[5];
  const dprRatio = (to[6] + to[7] || 1) / Math.max(1, from[6] + from[7]);
  const dDc = to[8] - from[8];
  const out: Mon = { cr: target };

  // AC
  if (Array.isArray(mon.ac)) {
    out.ac = (mon.ac as unknown[]).map((a) =>
      typeof a === 'number' ? a + dAc : a && typeof a === 'object' && typeof (a as { ac?: number }).ac === 'number' ? { ...(a as object), ac: (a as { ac: number }).ac + dAc } : a,
    );
  }
  // HP: same die size, more or fewer dice
  const hp = mon.hp as { average?: number; formula?: string } | undefined;
  if (hp?.average) {
    const avg = Math.max(1, Math.round(hp.average * hpRatio));
    const m = hp.formula?.match(/^(\d+)d(\d+)\s*([+-]\s*\d+)?$/);
    if (m) {
      const n = Number(m[1]);
      const size = Number(m[2]);
      const perDie = m[3] ? Number(m[3].replace(/\s+/g, '')) / n : 0;
      const dice = Math.max(1, Math.round(avg / ((size + 1) / 2 + perDie)));
      const bonus = Math.round(perDie * dice);
      out.hp = { average: Math.max(1, Math.floor(dice * ((size + 1) / 2)) + bonus), formula: `${dice}d${size}${bonus ? (bonus > 0 ? ` + ${bonus}` : ` - ${-bonus}`) : ''}` };
    } else {
      out.hp = { ...hp, average: avg };
    }
  }
  // saves and skills follow proficiency
  const shiftBonus = (o: unknown) =>
    o && typeof o === 'object'
      ? Object.fromEntries(Object.entries(o as Record<string, unknown>).map(([k, v]) => {
          const n = Number(String(v).replace(/\s+/g, ''));
          return [k, Number.isFinite(n) ? `${n + dProf >= 0 ? '+' : ''}${n + dProf}` : v];
        }))
      : o;
  if (mon.save) out.save = shiftBonus(mon.save);
  if (mon.skill) out.skill = shiftBonus(mon.skill);

  // text: to-hit, DCs and damage
  const scaleDamage = (f: string) => {
    const m = f.replace(/\s+/g, '').match(/^(\d*)d(\d+)([+-]\d+)?$/);
    if (!m) return f;
    const n = Math.max(1, Math.round((Number(m[1]) || 1) * dprRatio));
    const k = m[3] ? Math.round(Number(m[3]) * Math.min(dprRatio, 2)) : 0;
    return `${n}d${m[2]}${k ? (k > 0 ? ` + ${k}` : ` - ${-k}`) : ''}`;
  };
  const avgOf = (f: string) => {
    const m = f.replace(/\s+/g, '').match(/^(\d*)d(\d+)([+-]\d+)?$/);
    return m ? Math.floor((Number(m[1]) || 1) * ((Number(m[2]) + 1) / 2)) + (Number(m[3]) || 0) : null;
  };
  const text = (s: string) =>
    s
      .replace(/(\d+) \(\{@damage ([^}]+)\}\)/g, (_, _avg, f: string) => {
        const nf = scaleDamage(f);
        return `${avgOf(nf) ?? _avg} ({@damage ${nf}})`;
      })
      .replace(/\{@damage ([^}]+)\}/g, (_, f: string) => `{@damage ${scaleDamage(f)}}`)
      .replace(/\{@hit (-?\d+)\}/g, (_, n: string) => `{@hit ${Number(n) + dHit}}`)
      .replace(/\{@dc (\d+)\}/g, (_, n: string) => `{@dc ${Number(n) + dDc}}`)
      .replace(/\bDC (\d+)/g, (_, n: string) => `DC ${Number(n) + dDc}`);
  const deep = (v: unknown): unknown =>
    typeof v === 'string' ? text(v) : Array.isArray(v) ? v.map(deep) : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, deep(x)])) : v;
  for (const k of ['trait', 'action', 'bonus', 'reaction', 'legendary', 'mythic', 'spellcasting']) {
    if (Array.isArray(mon[k])) out[k] = deep(mon[k]);
  }
  return out;
}
