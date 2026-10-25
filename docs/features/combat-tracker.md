# Combat tracker

New feature (not in Orb of Many Functions), requested by the user on 2026-10-25. It ports the
5etools DM Screen **Initiative Tracker** (`5etools-src/js/dmscreen/panels/initiativetracker/`)
so state and links stay compatible with 5etools. Route `combat`, in the rail's **During** group.

## Scope
- **v1:** the core tracker (rows, turns and rounds, HP and wound levels, conditions with
  durations, per-row and encounter notes, stat columns, default party, settings, lock, sort)
  and 5etools combat links (`#combat=` and `#encounter=`).
- **Later:** CR and summon scaling (`ScaleCreature`), the networked Player View, Send to
  Foundry, the Creature Viewer and Notes Viewer panels, `[[1d20]]`/`{@tag}` rendering in notes.

## Creature data: the 5etools mirror
- Setting `fivetools_base_url` (default `http://localhost:8080`): the root of a 5etools
  site or mirror. Every path is absolute under it (`<base>/data/bestiary/index.json`,
  `<base>/data/bestiary/<file>`, token images `<base>/img/bestiary/tokens/<SRC>/<Name>.webp`).
- **Sync** (a Job with progress and cancel) downloads `index.json` and every
  `bestiary-*.json` into `<data dir>/fivetools/bestiary/`, then builds a search index. Search
  and adds work offline from the cache; Sync again to refresh.
- `_copy` creatures are resolved against their base (own fields win; `_mod` is ignored, see
  Intentional changes).
- Creature hashes follow 5etools `UrlUtil.URL_TO_HASH_BUILDER`: `encodeURIComponent(name_source)`
  lower-cased, e.g. `goblin_mm`.

## Behaviour (as 5etools)
- **Rows:** name (with a custom display name), source (empty for players), HP current/max,
  initiative, active flag, conditions, notes, player-visible flag, ordinal, stat cells.
  Creatures with the same name, custom name and source are *similar*; a new one gets ordinal
  `similar + 1` and shows as `Goblin (2)` when there is more than one.
- **Sort:** by initiative (default, descending) or by name; clicking the active sort flips
  the direction. Ties: initiative → name → ordinal → id (name sort: name → ordinal → initiative → id).
- **Turns:** exactly one group is active. *Next* ticks down the active rows' timed conditions
  (removing those that reach 0), moves to the next row in sort order (wrapping), activates every
  similar row with the same initiative, runs turn-start column hooks, and when the new active
  row is the first one, increments the round and runs round-start hooks. *Previous* moves
  backwards with no countdown and no round change. With *Reroll initiative each round*, the
  round start rerolls every creature and re-sorts.
- **Wound levels** from `round(clamp(hp/max·100, 0, 100))`: 100 Healthy, >50 Hurt, >0 Bloodied,
  0 Defeated, no HP Unknown. *Track HP as damage* inverts it (`(max − hp)/max`).
- **Conditions:** the 5etools list (Blinded … Unconscious, Concentration, Drunk, !!On Fire!!)
  with their colours, plus saved custom conditions; turns or unlimited.
- **Stat columns:** `populateWith` ids as 5etools: `hpFormula armorClass passivePerception speed
  spellDc initiative cr xp legendaryActions image`, `<abv>Save`, `<abv>Bonus`, `<abv>Score`,
  skills in camelCase, `cbNeutral cbAutoLow cbAutoHigh cbAutoRoundLow cbAutoRoundHigh`, and
  `""` for custom. Cells fill from the stat block when a creature is added. Checkbox-auto columns
  set their value on turn/round start (forwards only); legendary actions refill each round.
- **HP and initiative:** average HP or rolled from the formula; initiative `1d20 + bonus`
  (5etools `getInitiativeBonusNumber`). *Roll groups together* shares one roll per add.
- **Default party** rows are put back on Reset and optionally added on encounter import.
- **Lock** disables every editing control.

## Links and state
- State is stored in the 5etools serial form (`s d ri m rg rri wId g p a k piHp piHm piV piO
  nov c cndc r rdp n not`; rows `id n n_d n_m h g i a s c nt v o k`), so the same JSON round-trips
  through 5etools. Unknown keys are kept.
- **Copy combat link:** `<base>/dmscreen.html#combat=<base64url(deflate-raw(json))>`.
  **Open link** accepts that URL, its hash, or the bare payload, for `combat=` and
  `encounter=`. An encounter is the Bestiary's exported sublist (`items[].h`, `items[].c`,
  plus the encounter builder's `activePartyId`/`statePartyComps`), rolled into rows using the
  import settings (append or replace, add players, roll groups).
- Autosaved in the workspace key `combat.tracker`.

## Intentional changes
- Creature scaling (`customHashId`, scaled CR, summon levels) is not applied yet: scaled
  creatures import at their listed stats, and their scale fields are kept on the row.
- `_copy` `_mod` operations are not applied; the copy takes the base's fields plus its own.
- Notes are plain text (no inline rolls or tag rendering yet).

## Checklist
- [ ] Core: serial round-trip, sort, turns and rounds, condition countdown, column hooks, wound levels, ordinals
- [ ] Core: creature stats (init bonus, HP, AC, PP, speed, spell DC, CR, XP, LA, saves, skills), dice
- [ ] Core: combat and encounter links (encode/decode, encounter → rows)
- [ ] Service: mirror sync Job, offline search, add creature/player, import/export link, autosave
- [ ] Tauri commands and MCP tools (documented, e2e)
- [ ] UI: tracker under During, add creature dialog, conditions, columns, settings, mock backend, axe in both themes
