# Future features

Ideas agreed with the user (2026-10-25) but not scheduled. Each one gets a spec in
`docs/features/` when it is picked up. The rail groups are Before (prep), During (at the table)
and After.

## Planned ideas

### Session planner (Before)
Picked up 2026-10-25: spec in `features/session-planner.md` (Phase 10).

### Tables: findables and shops (During)
A single During page for what players find and buy, quick to use mid-session.
- **Three tiers of findables:** *trinkets* (flavour, near-worthless), *trophies* (parts and
  spoils from defeated creatures) and *treasure* (coin, gems, art, magic items), rolled from
  5etools data on the mirror (`data/loot.json`, `data/items*.json`, trinket tables).
- **Shops**, modelled on the user's 5etools Store Generator (`store.html`, `js/store/store-ui.js`):
  a priced inventory of mundane goods (listed value) and magic items priced by rarity (2014
  ranges or 2024 flat values; consumables halved; a magic item adds its base item's price),
  with item filters, a rarity ceiling, stock-size ranges, weighting toward lower rarities, a
  price markup and preset shop types.
- Shops can be saved quickly during play and referenced later as prep (and in the session
  planner).

### Spell and item cards (Before)
Picked up 2026-10-26: spec in `features/spell-item-cards.md` (Phase 12). Double-sided printing
was already in the card printer.

### Handouts (Before)
A general Handouts destination; the current **Cards** tool becomes one of its tabs. Other tabs:
letters, notices, wanted posters and similar on templates, printed through the PDF pipeline.

### Timers and trackers (During)
Clocks picked up 2026-10-25: spec in `features/clocks.md` (Phase 11). Still to do:
- **Real time:** countdown and stopwatch timers with notifications.
- **Blades in the Dark style clocks:** segmented clocks (4, 6, 8 … segments) filled by hand.
- **Clocks tied to time:** a clock can advance on its own every N minutes of real time (for
  example one segment every 10 minutes), with a notification on each segment and when it fills.

### Rewards when a fight ends (During)
Picked up 2026-10-26: XP split and individual treasure in `features/combat-tracker.md` (Play
aids, *Rewards*). Hoard and magic-item rolls on the creature's treasure themes are left to the
Tables page above.

### After-session tools (After)
Agreed 2026-11-05 as future features for the After group, which so far holds only Transcribe.
None of them is note-taking; each reads data the app already has.
- **Combat report.** A post-session summary built from the encounters the Combat tracker ran:
  rounds and real time per encounter, damage dealt and taken per PC, who dropped to 0 and how
  often, the longest fight. Meant for balancing the next session. The tracker keeps only the
  current state today, so this needs a light per-encounter record (totals at the end of each
  fight, not the per-round timeline that was declined as a "combat log" below).
- **XP and loot splitter.** Totals XP (or a milestone) from the encounters fought in a session
  and splits treasure among the PCs who were there, with remainder rules (who gets the odd
  coin, items claimed by whom). Prints a "loot receipt" handout through the card printer. It
  extends the per-fight *Rewards* in `features/combat-tracker.md` to a whole session.
- **Clock and world advance.** An end-of-session pass over the Clocks: step faction clocks, the
  in-world calendar and downtime turns forward in one go, then show a "what ticked" summary
  (which clocks moved and the events they reached). Likely an After mode of Clocks rather than
  a new destination.

## Deferred
Possible later, out of bounds for now:
- **Music cues:** switch the Music mixer to a chosen soundscape when combat starts or a boss's
  turn begins.
- **Quick NPC generator:** make a homebrew creature from a template ("CR 2 brute") using the DMG
  statistics by CR and save it through the homebrew bestiary.

## Declined
Considered and decided against:
- Combat log (per-round timeline of damage, conditions and turns).
- Maps and AoE integration with the combat tracker.
- Player-facing view of the tracker (the tracker is DM-only).
- Session recaps generated from transcripts, and a campaign journal.
