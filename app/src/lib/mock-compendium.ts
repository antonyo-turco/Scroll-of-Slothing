// Spell and item cards for the browser mock: a few built-in entries that
// appear after "Download" (mirror of crates/service/src/compendium.rs).

import type { CompendiumCards, CompendiumEntry, CompendiumQuery, CompendiumRef, CompendiumStatus, Settings } from './types';

type Emit = (e: Record<string, unknown>) => void;
type Handler = (args: Record<string, unknown>) => unknown;

type MockEntry = CompendiumEntry & { card: string };

const spell = (name: string, source: string, level: number, detail: string, classes: string[], card: string, reprinted = false): MockEntry => ({
  kind: 'spell', id: `${name.toLowerCase().replace(/ /g, '%20')}_${source.toLowerCase()}`, name, source, detail, level,
  rarity: null, classes, concentration: card.includes('Concentration'), reprinted, card,
});
const item = (name: string, source: string, rarity: string, detail: string, card: string, reprinted = false): MockEntry => ({
  kind: 'item', id: `${name.toLowerCase().replace(/ /g, '%20').replace(/\+/g, '%2b')}_${source.toLowerCase()}`, name, source, detail,
  level: null, rarity, classes: [], concentration: false, reprinted, card,
});

const ENTRIES: MockEntry[] = [
  spell('Fireball', 'XPHB', 3, '3rd-level Evocation', ['Sorcerer', 'Wizard'],
    '# Fireball\n**Type:** Spell\n**Level:** 3rd-level Evocation\n**Casting Time:** 1 action\n**Range:** 150 feet\n**Components:** V, S, M (a ball of bat guano and sulfur)\n**Duration:** Instantaneous\n**Classes:** Sorcerer, Wizard\n\nA bright streak flashes from you to a point you choose within range and then blossoms with a low roar into a fiery explosion. Each creature in a 20-foot-radius Sphere centered on that point makes a Dexterity saving throw, taking 8d6 Fire damage on a failed save or half as much damage on a successful one.\n\nFlammable objects in the area that aren\'t being worn or carried start burning.\n\n**Using a Higher-Level Spell Slot.** The damage increases by 1d6 for each spell slot level above 3.'),
  spell('Fireball', 'PHB', 3, '3rd-level Evocation', ['Sorcerer', 'Wizard'],
    '# Fireball\n**Type:** Spell\n**Level:** 3rd-level Evocation\n**Casting Time:** 1 action\n**Range:** 150 feet\n**Components:** V, S, M (a tiny ball of bat guano and sulfur)\n**Duration:** Instantaneous\n**Classes:** Sorcerer, Wizard\n\nA bright streak flashes from your pointing finger to a point you choose within range and then blossoms with a low roar into an explosion of flame.', true),
  spell('Light', 'XPHB', 0, 'Evocation cantrip', ['Bard', 'Cleric', 'Sorcerer', 'Wizard'],
    '# Light\n**Type:** Spell\n**Level:** Evocation cantrip\n**Casting Time:** 1 action\n**Range:** Touch\n**Components:** V, M (a firefly or phosphorescent moss)\n**Duration:** 1 hour\n**Classes:** Bard, Cleric, Sorcerer, Wizard\n\nYou touch one Large or smaller object that isn\'t being worn or carried by someone else. Until the spell ends, the object sheds Bright Light in a 20-foot radius and Dim Light for an additional 20 feet.'),
  spell('Bless', 'XPHB', 1, '1st-level Enchantment', ['Cleric', 'Paladin'],
    '# Bless\n**Type:** Spell\n**Level:** 1st-level Enchantment\n**Casting Time:** 1 action\n**Range:** 30 feet\n**Components:** V, S, M (a Holy Symbol worth 5+ GP)\n**Duration:** Concentration, up to 1 minute\n**Classes:** Cleric, Paladin\n\nYou bless up to three creatures within range. Whenever a target makes an attack roll or a saving throw before the spell ends, the target adds 1d4 to the attack roll or save.'),
  item('Bag of Holding', 'XDMG', 'uncommon', 'Wondrous item, uncommon',
    '# Bag of Holding\n**Type:** Item #2E7D32\n**Kind:** Wondrous item\n**Rarity:** Uncommon\n**Weight:** 5 lb.\n\nThis bag has an interior space considerably larger than its outside dimensions—roughly 2 feet square and 4 feet deep on the inside. The bag can hold up to 500 pounds, not exceeding a volume of 64 cubic feet.'),
  item('Cloak of Protection', 'XDMG', 'uncommon', 'Wondrous item, uncommon',
    '# Cloak of Protection\n**Type:** Item #2E7D32\n**Kind:** Wondrous item\n**Rarity:** Uncommon\n**Attunement:** Required\n**AC:** +1 bonus\n\nYou gain a +1 bonus to Armor Class and saving throws while you wear this cloak.'),
  item('Flame Tongue', 'XDMG', 'rare', 'Weapon (any melee), rare',
    '# Flame Tongue\n**Type:** Item #1F5FAD\n**Kind:** Weapon (any melee)\n**Rarity:** Rare\n**Attunement:** Required\n\nWhile holding this magic weapon, you can take a Bonus Action and use a command word to cause flames to erupt from it. These flames shed Bright Light in a 40-foot radius and Dim Light for an additional 40 feet. While the weapon is ablaze, it deals an extra 2d6 Fire damage on a hit.'),
  item('Longsword', 'XPHB', 'none', 'Martial weapon',
    '# Longsword\n**Type:** Item\n**Kind:** Martial weapon\n**Damage:** 1d8 slashing (1d10 two-handed)\n**Properties:** Versatile, Mastery: Sap\n**Weight:** 3 lb.\n**Value:** 15 gp'),
];

export function createCompendiumMockHandlers(emit: Emit, settings: () => Settings): Record<string, Handler> {
  let synced = false;
  let seq = 0;
  const find = (r: CompendiumRef) => ENTRIES.find((e) => e.kind === r.kind && e.id === r.id.toLowerCase());
  const strip = ({ card: _c, ...e }: MockEntry): CompendiumEntry => e;
  return {
    compendium_status: (): CompendiumStatus => ({
      base_url: settings().fivetools_base_url,
      synced,
      spells: synced ? ENTRIES.filter((e) => e.kind === 'spell').length : 0,
      items: synced ? ENTRIES.filter((e) => e.kind === 'item').length : 0,
    }),
    compendium_search: ({ query }): CompendiumEntry[] => {
      if (!synced) return [];
      const q = query as CompendiumQuery;
      const text = q.query.trim().toLowerCase();
      const hits = ENTRIES.filter((e) =>
        e.kind === q.kind && e.name.toLowerCase().includes(text)
        && !(q.newest_only && e.reprinted)
        && (q.level == null || e.level === q.level)
        && (!q.class || e.classes.some((c) => c.toLowerCase() === q.class!.toLowerCase()))
        && (!q.rarity || e.rarity === q.rarity.toLowerCase()));
      hits.sort((a, b) => Number(!a.name.toLowerCase().startsWith(text)) - Number(!b.name.toLowerCase().startsWith(text)));
      return hits.slice(0, q.limit ?? 100).map(strip);
    },
    compendium_get: ({ item }) => {
      const e = find(item as CompendiumRef);
      if (!e) throw { code: 'not_found', message: `not found: ${(item as CompendiumRef).id}` };
      return { name: e.name, source: e.source };
    },
    compendium_cards: ({ items }): CompendiumCards => {
      const cards: string[] = [];
      const missing: string[] = [];
      for (const r of items as CompendiumRef[]) {
        const e = find(r);
        if (e) cards.push(e.card);
        else missing.push(r.id);
      }
      return { markdown: cards.join('\n\n---\n\n'), cards: cards.length, missing };
    },
    compendium_sync: () => {
      const id = `mock-job-compendium-${++seq}`;
      const base = { id, kind: 'spells and items sync', result: null, error: null };
      emit({ type: 'job', ...base, state: 'running', progress: 0.5, message: 'spells-xphb.json (1/1)' });
      setTimeout(() => {
        synced = true;
        emit({ type: 'job', ...base, state: 'done', progress: 1, message: '', result: { spells: 4, items: 4 } });
        emit({ type: 'store_changed', entity: 'compendium', entity_id: 'cache', op: 'update' });
      }, 60);
      return id;
    },
  };
}
