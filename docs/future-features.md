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
When every enemy in the combat tracker is down, offer the end-of-fight rewards: split the
defeated creatures' XP among the party and roll on each creature's treasure tables, if it has
any.

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
