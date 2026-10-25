// D&D classes and subclasses for the hero editor (2024 Player's Handbook, plus
// the Artificer). Anything else goes in as a custom value.

export const CLASSES: Record<string, string[]> = {
  Artificer: ['Alchemist', 'Armorer', 'Artillerist', 'Battle Smith'],
  Barbarian: ['Path of the Berserker', 'Path of the Wild Heart', 'Path of the World Tree', 'Path of the Zealot'],
  Bard: ['College of Dance', 'College of Glamour', 'College of Lore', 'College of Valor'],
  Cleric: ['Life Domain', 'Light Domain', 'Trickery Domain', 'War Domain'],
  Druid: ['Circle of the Land', 'Circle of the Moon', 'Circle of the Sea', 'Circle of the Stars'],
  Fighter: ['Battle Master', 'Champion', 'Eldritch Knight', 'Psi Warrior'],
  Monk: ['Warrior of Mercy', 'Warrior of Shadow', 'Warrior of the Elements', 'Warrior of the Open Hand'],
  Paladin: ['Oath of Devotion', 'Oath of Glory', 'Oath of the Ancients', 'Oath of Vengeance'],
  Ranger: ['Beast Master', 'Fey Wanderer', 'Gloom Stalker', 'Hunter'],
  Rogue: ['Arcane Trickster', 'Assassin', 'Soulknife', 'Thief'],
  Sorcerer: ['Aberrant Sorcery', 'Clockwork Sorcery', 'Draconic Sorcery', 'Wild Magic Sorcery'],
  Warlock: ['Archfey Patron', 'Celestial Patron', 'Fiend Patron', 'Great Old One Patron'],
  Wizard: ['Abjurer', 'Diviner', 'Evoker', 'Illusionist'],
};

export const CLASS_NAMES = Object.keys(CLASSES);
