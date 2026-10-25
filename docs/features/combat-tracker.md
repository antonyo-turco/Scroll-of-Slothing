# Combat tracker

New feature (not in Orb of Many Functions), requested by the user on 2026-10-25. It ports the
5etools DM Screen **Initiative Tracker** (`5etools-src/js/dmscreen/panels/initiativetracker/`)
so state and links stay compatible with 5etools. Route `combat`, in the rail's **During** group.

## Scope
- **v1:** the core tracker (rows, turns and rounds, HP and wound levels, conditions with
  durations, per-row and encounter notes, stat columns, default party, settings, lock, sort)
  and 5etools combat links (`#combat=` and `#encounter=`).
- **Not planned:** the Player View and the per-row "visible to players" toggle. The
  tracker is for the DM only (user, 2026-10-25); the visibility flag is kept in the data
  only so 5etools links round-trip.
- **Later:** CR and summon scaling (`ScaleCreature`), Send to Foundry, the Creature Viewer and Notes Viewer panels, `[[1d20]]`/`{@tag}` rendering in notes.

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
  with their colours; turns or unlimited.
- **Stat columns:** `populateWith` ids as 5etools: `hpFormula armorClass passivePerception speed
  spellDc initiative cr xp legendaryActions image`, `<abv>Save`, `<abv>Bonus`, `<abv>Score`,
  skills in camelCase, `cbNeutral cbAutoLow cbAutoHigh cbAutoRoundLow cbAutoRoundHigh`, and
  `""` for custom. Cells fill from the stat block when a creature is added. Checkbox-auto columns
  set their value on turn/round start (forwards only); legendary actions refill each round.
- **HP and initiative:** average HP or rolled from the formula; initiative `1d20 + bonus`, or with
  *Passive initiative* (`combat_passive_initiative`) `10 + bonus`, +5 with advantage on initiative
  and −5 with disadvantage (5etools `initiative.advantageMode`), for creatures and heroes alike
  (5etools `getInitiativeBonusNumber`). *Roll groups together* shares one roll per add.
- **Default party** rows are put back on Reset and optionally added on encounter import.
- **Lock** disables every editing control.

## Tabs
The view has three tabs in the top bar, styled like the Music tabs and lists (`.list-row`):
Tracker, Encounters and Parties.

## Tracker
- **Rows.** A coloured left edge tells combatants apart: heroes blue; creatures by side,
  enemies red (default), allies white, neutral black (picked when adding them). Names and max
  HP change only after the row's pencil (edit); creatures get a custom name, players their
  name. The ⓘ after a name shows the creature's type, CR, AC and HP (or the hero's sheet) on
  hover. Clicking a name shows that row in the side panel ("Selected"); Next turn goes back to
  the current turn. Action buttons sit in fixed slots so heroes and creatures line up.
- **HP.** The current HP box takes a number or `+5`/`-7`; ↑/↓ step it by 1 (Shift 5, Ctrl 10).
  Clicking the HP (or the temp box) opens a strip of buttons above it (below for the top row):
  −10 −5 −1 in red, +1 +5 +10 in green (temp: −5 −1 +1 +5 +10 in blue); Esc or a click
  elsewhere closes it. − is damage, eaten by temporary HP first; + heals, capped at max HP. A
  thin bar under the HP shows what is left (green full, yellow over 50%, orange over 25%, red
  below), a blue one under the temp box shows temporary HP (`hp_temp`).
- **Concentration.** When a combatant with the Concentration condition takes damage, a popup
  gives the Constitution save DC (10 or half the damage, at most 30).
- **Conditions.** Chips count the turns spent under the condition, "Prone (2)", or for a timed
  one turns spent of its total, "Blinded (1/3)"; the count rises at the end of the creature's
  turn (`elapsed`, stored as `el` in the condition, which 5etools ignores). With three or more
  conditions the chips shorten to three letters. Hovering one shows a short rules reminder
  (2024, paraphrased in `app/src/lib/conditions.ts`); clicking removes it. The picker shows
  the reminder in a box under the chips. Custom conditions are not offered (removed at the
  user's request on 2026-10-25); any saved ones stay in the data for 5etools round-trips.
- **Defeated.** A red skull before the HP appears at 0 HP or below (or, tracking damage, at the
  max); clicking it moves the row to a *Defeated* accordion at the bottom (`defeated`, out of
  the turn order; Restore puts it back). If it was that row's turn, the turn moves on first.
- **Difficulty.** Under the Defeated list, one strip shows the fight's rating, a bar with green
  (Low), yellow (Moderate), orange (High), red (Deadly, to twice High) and black (TPK) zones
  and a marker, and the XP. Only enemies still in the turn order count; heroes' levels come
  from the saved parties (matched by name), else the default party. The calculation is
  `app/src/lib/difficulty.ts`: 2024 XP budgets per character, an optional 2014 multiplier for
  the number of enemies (`combat_xp_multiplier_2014`), over High is Deadly, over twice High a
  total party kill. Both strips sit flush on the status bar.
- **Side panel.** The stat block (full 5etools entry, with this creature's changes) of the
  selected row or whoever's turn it is, with its notes on top. It can be dragged wider
  (300–1000 px) or minimized to a strip; both are remembered on this computer. *Edit* opens
  the creature editor: a **Stats** form (name, size, type, CR, AC, HP, max HP, initiative,
  passive Perception, speeds, abilities, saves, skills, resistances, immunities,
  vulnerabilities, condition immunities, senses, languages) and a **5etools JSON** tab with
  the whole stat block to check and edit. Only fields that differ from the bestiary are kept
  (`overrides`); the row's HP, AC, speed and passive Perception follow. *Reset to bestiary*
  drops them.
- **Notes.** Encounter notes open in a popup from the toolbar; creature notes are on top of the
  side panel.
- **Turns.** *Next* at the start of a round plays a full-screen “Round N” animation, then a
  card shows whose turn it is and who is next (copies acting together as “Goblin ×3”), unless
  `combat_turn_popup` is off. Both use a plain fade with reduced motion.
- **Add creature.** A wide picker: search, CR band and source filters, token thumbnails, a stat
  block preview, count, custom name, roll HP and the side.
- **Reset** asks in a popup (unless `combat_reset_confirm` is off) with *Keep the party*
  (`combat_reset_keep_players`: remove only creatures) and *Don't ask again*.

## Play aids
- **Rolls from the stat block.** `{@hit}`, `{@damage}`, `{@dice}` and `{@d20}` tags in the side
  panel are buttons (`app/src/lib/dice.ts`). To hit: Shift advantage, Ctrl disadvantage; a
  natural 20 makes that ability's next damage roll a crit (dice doubled), as does Shift on a
  damage button. The panel keeps a log of the last rolls (crits and natural 1s marked).
- **Limited uses.** Abilities named "(N/Day)" get N pips; "{@recharge N}" abilities get a
  Recharge badge. At the start of the creature's turn a notice reminds you to roll a d6 for each
  spent recharge (the app does not roll); click the badge when it recharges. Legendary actions (`legendaryActions`, else 3) get pips that refill
  on the creature's turn; after other combatants' turns a toast lists who still has some.
  All of this lives in the row's `usage` (`_us`).
- **Lair.** A creature with a `legendaryGroup` that has lair actions offers *Add lair*: a row at
  initiative 20 that loses ties (`side: lair`, purple edge) whose panel shows the lair actions
  and regional effects (`legendarygroups.json`, synced with the bestiary;
  `bestiary_legendary_group`). Sync again to get it on an existing cache.
- **Death saves.** A hero at 0 HP shows three success and three failure pips instead of the
  skull; damage while dying adds a failure, healing clears them, three successes read
  "Stable", three failures bring back the skull. Their turn reminds you to roll.
- **Undo / redo.** Ctrl+Z / Ctrl+Y (or the toolbar arrows) step through the last 30 saved
  states of the tracker; changes from elsewhere (MCP) are not undo steps.
- **CR scaling.** *Scale to CR* in the creature editor, a CR choice in *Add creature*, and a CR
  per line in the encounter editor scale a creature with the DMG's monster statistics by CR
  (`app/src/lib/scale.ts`): AC, to-hit, save DCs, saves and skills shift by the table's
  difference; HP and damage scale by the table's midpoint ratios (dice counts and printed
  averages adjusted). The result is stored as the creature's stat changes (and `n_scr`), so it
  can be edited or reset. It approximates, rather than ports, 5etools ScaleCreature.
- **Group actions.** Shift+click rows to select them; a bar offers an amount with Damage, Heal
  and Temp HP, a group saving throw (ability and DC; each creature rolls its own save bonus or
  ability modifier, heroes roll their own), *Half on save* for damage, and a condition for
  everyone. Esc clears the selection.

## Encounters
Saved encounters (workspace `combat.encounters`), pinned first. An encounter is either a
creature list (`kind: encounter`) or a snapshot of a whole tracker (`kind: combat`, from *Save
current combat* or a `#combat=` link; it keeps each creature's changes).
- **Editor.** Three columns in large type: a difficulty card per saved party (default party
  first, rating, bar, budgets, the 2014 multiplier switch), the bestiary with CR and XP, and the
  encounter's creatures with CR, XP each and per line, and the total XP.
- **Lines with changes.** A creature line can carry `stats` (5etools fields, e.g. `{"ac": [17]}`)
  applied to those copies when the encounter starts; lines with different changes stay apart.
- **Import.** A 5etools Bestiary encounter (*Save to File* JSON or *Copy Combat Link*), a DM
  Screen combat link, or a `.denc` file.
- **`.denc` files.** *Export* writes the selected encounter (or all) as JSON
  (`{"format": "sleuth-encounters", "version": 1, "encounters": [...]}`); importing adds copies
  with new ids.
- ▶ rolls a creature list into the tracker with the import settings (or loads a snapshot,
  replacing the tracker), + adds it to the current combat, 🔗 copies its 5etools link.

## Parties
Saved hero parties (workspace `combat.parties`). A hero has name, species, class and subclass
(dropdowns of the 2024 Player's Handbook classes plus the Artificer, `app/src/lib/dnd-classes.ts`,
each with *Custom…*), level, hit points, AC, initiative bonus, passive Perception, speed, spell
save DC, player and notes. Adding a party makes player rows with full HP, initiative rolled from
the bonus (when rolling initiative is on), species · class · subclass · level as the display line,
and the AC/PP/speed/spell DC/initiative columns filled. ⚑ also makes it the default party.

## Homebrew creatures
`bestiary_save_creature` (MCP and Tauri) stores a 5etools monster object (at least `name` and
`source`) in `<data dir>/fivetools/homebrew-bestiary.json`, outside the synced copy. It is merged
into the bestiary, so search, add, encounters and tokens treat it like any creature; same name
and source replace it.

## Settings
Settings → Combat (a vertical tab) holds the 5etools mirror and bestiary sync, the rolling
options (`combat_roll_init`, `combat_roll_groups` (on: copies of a creature share one roll),
`combat_roll_hp`, `combat_reroll_each_round`, `combat_hp_as_damage`), the encounter import
options in an accordion (`combat_import_*`), and the tracker's stat columns. The rolling and import options are app settings applied to the tracker on every
load; they are still written into links so 5etools reads them.

## Links and state
- State is stored in the 5etools serial form (`s d ri m rg rri wId g p a k piHp piHm piV piO
  nov c cndc r rdp n not`; rows `id n n_d n_m h g i a s c nt v o k`), so the same JSON round-trips
  through 5etools. Unknown keys are kept. This app's extensions, which 5etools ignores: rows
  `_ht` (temporary HP), `_so` (stat changes), `_sd` (side); the tracker `_dead` (defeated rows);
  the autosave alone keeps `_sleuthLocked` (the lock, which 5etools treats as temporary).
- **Copy combat link:** `<base>/dmscreen.html#combat=<base64url(deflate-raw(json))>`.
  **Open link** accepts that URL, its hash, or the bare payload, for `combat=` and
  `encounter=`. An encounter is the Bestiary's exported sublist (`items[].h`, `items[].c`,
  plus the encounter builder's `activePartyId`/`statePartyComps`), rolled into rows using the
  import settings (append or replace, add players, roll groups).
- Autosaved in the workspace key `combat.tracker`.

## Intentional changes
- 5etools scaling links (`customHashId`, summon levels) are not applied: such creatures import
  at their listed stats. CR scaling here uses the DMG table instead (see Play aids).
- `_copy` `_mod` operations are not applied; the copy takes the base's fields plus its own.
- Notes are plain text (no inline rolls or tag rendering yet).

## Checklist
- [x] Core: serial round-trip, sort, turns and rounds, condition countdown, column hooks, wound levels, ordinals
- [x] Core: creature stats (init bonus, HP, AC, PP, speed, spell DC, CR, XP, LA, saves, skills), dice
- [x] Core: combat and encounter links (encode/decode, encounter → rows)
- [x] Service: mirror sync Job, offline search, add creature/player, import/export link, autosave
- [ ] Tauri commands and MCP tools (documented; not yet covered by `mcp_e2e.rs`)
- [x] Saved encounters (create, import 5etools files and links, snapshots, start, link) and hero parties: service tests, e2e
- [x] UI: tracker under During, add creature dialog, conditions, columns, settings, mock backend, axe in both themes
- [ ] Tracker rework (HP strips, temp HP, defeated, difficulty strip, side panel editor, sides, turn card), homebrew, `.denc`, creature tokens: tests to write (service tests exist for homebrew, temp HP and `.denc` but have not been run)
