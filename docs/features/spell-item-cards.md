# Spell and item cards

Search the 5etools mirror's spells and items and add them to the card deck as reference cards. The cards are ordinary deck markdown, so they print through the card pipeline (`cards.md`): exact preview, A4 or Letter, and double-sided export where text that overflows the front continues on the back.

Requested 2026-10-25 (`future-features.md`, "Spell and item cards"). This is a new feature beyond parity; there is no Python counterpart.

## Data
- **Source:** the mirror set in `fivetools_base_url` (the same setting the combat tracker's bestiary uses, in Settings → Combat).
- **Sync** (a Job, `compendium_sync`) downloads into the data dir; the old copy stays until the sync succeeds:

  | Mirror path | Local copy | Used for |
  |---|---|---|
  | `data/spells/index.json` and every file it lists | `fivetools/spells/` | spells |
  | `data/spells/sources.json` (optional) | `fivetools/spells/` | the classes that have each spell |
  | `data/items.json` | `fivetools/items/` | magic and mundane items |
  | `data/items-base.json` | `fivetools/items/` | base items (weapons, armor, gear) and the item type and property names |
  | `data/magicvariants.json` (optional) | `fivetools/items/` | generic magic variants ("+1 Weapon", "Flame Tongue") |

- **Ids** are 5etools hashes, lower-cased (`fireball_xphb`, `%2b1%20weapon_xdmg`). Lookups ignore case.
- `_copy` entries are resolved against their base (copies of copies over a few passes; `_mod` is not applied, as in the bestiary).
- A generic magic variant becomes one item: its `inherits` fields over its own. It is not expanded into one item per base weapon or armor.
- `itemGroup` entries are left out.

## Search
`compendium_search {kind, query, level?, class?, rarity?, newest_only?, limit?}`
- Name contains the query, prefix matches first, then by name and source.
- Spells: `level` (0 for cantrips) and `class` (any class in `sources.json`, case-insensitive).
- Items: `rarity` (`none` for mundane items, `common` … `artifact`, `varies`, `unknown`).
- `newest_only` leaves out entries with `reprintedAs` (for example the PHB Fireball, reprinted in XPHB). The dialog turns it on by default.
- `limit` defaults to 100 (at most 2000). The dialog asks for 300 and says when the list is cut.

## Cards (`sleuth_core::compendium`)
Every card is one deck card (`# Title`, `**Key:** value` stats, a blank line, then the body).

**Spell card**
| Stat | From | Example |
|---|---|---|
| Type | | `Spell` (the spell colour) |
| Level | `level`, `school`, `meta.ritual` | `3rd-level Evocation`, `Evocation cantrip`, `1st-level Divination (ritual)` |
| Casting Time | `time` | `1 action`, `1 bonus action`, `1 minute` |
| Range | `range` | `150 feet`, `Touch`, `Self (15-foot cone)` |
| Components | `components` | `V, S, M (a ball of bat guano and sulfur)` |
| Duration | `duration` | `Instantaneous`, `Concentration, up to 1 minute`, `Until dispelled or triggered` |
| Classes | `sources.json` | `Sorcerer, Wizard` |

The body is `entries` then `entriesHigherLevel`.

**Item card**
| Stat | From | Example |
|---|---|---|
| Type | `rarity` | `Item #2E7D32`: the banner takes the rarity colour (below); mundane items keep the item colour |
| Kind | `wondrous`, `staff`, `requires`, `weaponCategory`, `type`, `baseItem` | `Wondrous item`, `Martial weapon`, `Weapon (any)`, `Potion` |
| Rarity | `rarity` (not for mundane items) | `Very Rare` |
| Attunement | `reqAttune` | `Required`, `Required (by a wizard)` |
| Damage | `dmg1`, `dmgType`, `dmg2`, `bonusWeapon` | `1d8 slashing (1d10 two-handed)` |
| AC | `ac`, `bonusAc` | `18`, `11 + Dex modifier`, `+2` (shields), `+1 bonus` |
| Properties | `property`, `range`, `mastery` | `Versatile, Mastery: Sap` |
| Charges | `charges`, `recharge`, `rechargeAmount` | `7 (regains 1d6 + 1 at dawn)` |
| Weight | `weight` | `5 lb.` |
| Value | `value` (copper pieces) | `15 gp`, `1,500 gp` |

Empty stats are left out.

**Rarity colours:** common `#5C5C5C`, uncommon `#2E7D32`, rare `#1F5FAD`, very rare `#6A35A8`, legendary `#B35C00`, artifact `#A8322B`.

**Text**
- 5etools tags show their text: `{@spell fireball|XPHB}` → `fireball`; a third part is the display text (`{@variantrule Sphere [Area of Effect]|XPHB|Sphere}` → `Sphere`); `{@damage 8d6}` → `8d6`; `{@dc 15}` → `DC 15`; `{@hit 5}` → `+5`; `{@chance 25}` → `25 percent`; `{@scaledamage 8d6|3-9|1d6}` → `1d6`. Tags nest and are resolved innermost first. `{@b}` and `{@i}` become card bold and italic.
- A named block starts its first paragraph with a bold `**Name.**`.
- List items start with `•`.
- Tables: the caption and column labels in bold, then one line per row (two cells joined by ` — `, more by ` · `); roll cells show `1` or `2–6`.
- Magic variants fill their `{=bonusWeapon}` placeholders from their fields.
- Text cannot change the card format: underscores are dropped, and a line of only dashes (a card separator) is dropped.

`compendium_cards {items: [{kind, id}]}` returns `{markdown, cards, missing}`: the cards in the order given, separated by `---`. Ids that are not in the synced data are listed in `missing`, never dropped silently.

## UI
- Cards view toolbar: **Spells & items…** opens the dialog.
- If nothing is synced, the dialog explains where the data comes from and offers **Download spells and items** (the sync Job, with its progress on the button).
- Otherwise:
  - Spells | Items switch, a name search and the filters (spell level, class; rarity) and "Newest printings only".
  - A list with a checkbox per entry; picks are kept across searches. Clicking a row shows its card text on the right; double-clicking ticks it.
  - Footer: what is picked, Clear, and **Add N cards**, which appends the cards to the deck (after a `---` when the deck has content) and closes the dialog. Missing ids are shown as an error toast.
- The deck preview then shows the real cards, with the overflow notice and double-sided backs as usual.

## MCP
| Tool | Input | Output / effect |
|---|---|---|
| `compendium_status` | | `{base_url, synced, spells, items}` |
| `compendium_sync` | | job id (result `{spells, items}`) |
| `compendium_search` | `{kind, query?, level?, class?, rarity?, newest_only?, limit?}` | `[{kind, id, name, source, detail, level, rarity, classes, concentration, reprinted}]` |
| `compendium_get` | `{kind, id}` | the full 5etools JSON |
| `compendium_cards` | `{items: [{kind, id}]}` | `{markdown, cards, missing}`; feed the markdown to `cards_validate`, `cards_export_pdf` or `decks_save` |

## Checklist
- [x] Core: tags, entries (named blocks, lists, tables, quotes), spell and item cards, magic variants (unit tests in `crates/core/src/compendium.rs`)
- [x] Service: load with `_copy`, search filters, cards with missing ids, unreachable-mirror sync (tests in `crates/service/src/compendium.rs`)
- [x] Tauri commands, MCP tools, `docs/mcp-tools.md`
- [x] UI dialog and browser mock (`app/src/lib/mock-compendium.ts`)
- [ ] Synced against the real mirror and checked by hand in the app
- [ ] MCP e2e coverage and a Playwright spec
