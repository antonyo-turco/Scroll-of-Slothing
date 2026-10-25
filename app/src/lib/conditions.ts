// Short reminders of what each condition does (2024 rules, paraphrased), for
// the hover tips on condition chips. Keys are lower-case condition names.

export const CONDITION_TIPS: Record<string, string> = {
  blinded: "Can't see; fails sight checks. Its attacks have Disadvantage, attacks against it have Advantage.",
  charmed: "Can't attack or target the charmer with harmful effects; the charmer has Advantage on social checks against it.",
  concentration: 'Keeping a spell going. Taking damage calls for a CON save (DC 10 or half the damage, up to 30); Incapacitated or dead ends it.',
  deafened: "Can't hear; fails hearing checks.",
  drunk: 'House condition: tipsy. Usually Disadvantage on DEX and INT checks, and a lot of confidence.',
  exhausted: 'Exhaustion levels: −2 per level to d20 tests, speed −5 ft per level; level 6 is death. A Long Rest removes one level.',
  exhaustion: 'Exhaustion levels: −2 per level to d20 tests, speed −5 ft per level; level 6 is death. A Long Rest removes one level.',
  frightened: "Disadvantage on checks and attacks while the source is in sight; can't willingly move closer to it.",
  grappled: "Speed 0; Disadvantage on attacks against anyone but the grappler. The grappler can move it (extra movement cost).",
  incapacitated: "No actions, Bonus Actions or Reactions; concentration ends; can't speak; Disadvantage on Initiative.",
  invisible: "Can't be seen without magic; Advantage on Initiative and attacks, attacks against it have Disadvantage.",
  '!!on fire!!': 'House condition: burning. Takes fire damage at the start of each turn until someone spends an action to put it out.',
  paralyzed: 'Incapacitated, speed 0, fails STR and DEX saves. Attacks against it have Advantage; hits within 5 ft are critical.',
  petrified: 'Turned to stone: Incapacitated, speed 0, fails STR/DEX saves, resistance to all damage, immune to poison.',
  poisoned: 'Disadvantage on attack rolls and ability checks.',
  prone: 'Can only crawl or stand up (half speed). Its attacks have Disadvantage; melee attacks against it within 5 ft have Advantage, ranged ones Disadvantage.',
  restrained: 'Speed 0; its attacks and DEX saves have Disadvantage; attacks against it have Advantage.',
  stunned: 'Incapacitated, fails STR and DEX saves, attacks against it have Advantage.',
  unconscious: 'Incapacitated and Prone, drops what it holds, fails STR/DEX saves; attacks have Advantage and hits within 5 ft are critical.',
};

export const conditionTip = (name: string): string | undefined => CONDITION_TIPS[name.trim().toLowerCase()];

/** Concentration save DC after taking `damage` (2024: 10 or half, max 30). */
export const concentrationDc = (damage: number): number => Math.min(30, Math.max(10, Math.floor(damage / 2)));
