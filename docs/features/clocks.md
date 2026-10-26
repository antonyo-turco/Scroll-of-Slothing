# Clocks

Blades in the Dark style clocks: a circle cut into segments that fill as trouble grows. Requested 2026-10-25 (from `docs/future-features.md`, "Timers and trackers"). This is new in this app, so there is no parity checklist.

Route `clocks`, in the rail's **During** group, after Combat. A view toggle at the end of the toolbar (icons, as in the Library) switches between two views and is remembered (`workspaces` key `clocks.view`):
- **Board**: every clock as a card, filled and timed in place. Double-click a card, press Enter on it, or use its **Details** button to switch to Detail scrolled to that clock.
- **Detail**: a scrolling list. Each row holds the clock (its face fills the card, with the fill and timer controls under it) beside its events, both the same height. Three rows fill the visible height (at least 260 px each); more scroll.

Both views share the selected clock and the inspector (name, segments, colour, timer, notes, reset, delete).

## Clocks
- A clock has a name (default `Clock N`), 2–24 segments (quick picks 4, 6, 8, 12; default 4), a fill, a colour (the accent colour or a swatch, `#rrggbb`) and notes.
- Dividers get thinner as the segment count grows (1.6 at 4, about 0.9 at 12, 0.65 at 24). Past 8 segments they stop short of the centre so they don't merge into a blob.
- Fill by hand: click a segment to fill up to it, or click the last filled one to empty it. The − / + buttons and the arrow or +/− keys on a focused card do the same. Changing the segment count keeps the fill, clamped to the new count.
- The clocks are autosaved in `workspaces` under `clocks` (no migration). Every change records a `clocks` change.

## Events
- An event is something that happens when a segment fills: a segment number (1-based) and a text, written in the Detail view. The explanation sits under the ⓘ next to the Events title. With **Repeat every N segments** it fires again every N segments after its first one (at 3, every 2: 3, 5, 7 …).
- A dot outside the rim marks each segment that has an event, styled like the legendary action pips in Combat: a hollow ring until its segment fills, then solid in the clock's colour. The next one to come has a ring in the clock's colour. Hovering or focusing a dot shows a tooltip with the segment, whether it is reached, next or ahead, its events, and any repeat.
- The list shows when each event fires, tags the next one (**next**) and those already passed (**done**), and flags an event whose segment is past the last one. Events are kept when the segment count shrinks; they fire again if it grows back.
- Filling a segment with an event announces it with a toast (`<clock> · segment N: <text>`, shown for 15 s) and a chime. This happens whether you fill it by hand, on the Board or in Detail, or the timer fills it. Filling several segments at once announces each one's events in order. Emptying never triggers events.
- `clocks_update` replaces the whole list (`events`); events without an id get one. An event needs some text and a segment from 1 to 24; a repeat of 0 means no repeat.

## Clocks tied to time
- "Fill a segment on its own" gives a clock a timer: one segment every N seconds, minutes or hours (10 s to 24 h; default 10 minutes).
- Start, pause, Start all and Pause all. A paused timer keeps the time it already counted toward the next segment. Changing the period keeps the running state and the counted time.
- The next segment counts from the moment the last one was due, not from when it was seen, so the timer keeps its pace. A late tick (the app was asleep or closed) adds every segment owed, up to full.
- A full clock stops its timer and drops its partial segment. A full clock doesn't start. Reset empties the clock and pauses the timer.
- While the app is open, on any page, `ClockWatcher` waits for the next due segment, calls `clocks_tick`, and gives a toast and a chime for each segment and a triple chime when a clock fills. Only `clocks_tick` moves timers, and its result lists, per clock, the events of the segments that filled. Notifications go to whichever caller ticks (the app, or an MCP client calling `clocks_tick`).

## Code layout
- `crates/core/src/clocks.rs`: `Clock`, `ClockEvent`, `AutoAdvance`, the fill, timer and tick rules and `events_reached` (times are Unix ms passed in). Unit tests.
- `crates/service/src/clocks.rs`: storage, validation, `clocks_*`. Service tests use `clocks_tick_at` / `clocks_run_at` with fixed times.
- UI: `app/src/routes/clocks/Clocks.svelte` (both views and the toggle), `ClockEvents.svelte` (a clock's events editor), `ClockSettings.svelte` (the shared inspector) and `ClockFace.svelte` (the SVG). The dot tooltips use the shared `use:tooltip` action (`app/src/lib/tooltip.svelte.ts`, card in `components/TooltipHost.svelte`, mounted in `App.svelte`), which any element can use. `app/src/lib/clocks.ts` mirrors the core rules, `app/src/lib/clocks.svelte.ts` holds the shared state, the selected clock, the actions both views use (they announce events reached by hand fills) and the app-wide timer, and `app/src/lib/mock-clocks.ts` is the browser mock.

## MCP
`clocks_list`, `clocks_create`, `clocks_update`, `clocks_advance`, `clocks_reset`, `clocks_run`, `clocks_delete`, `clocks_tick` (see `docs/mcp-tools.md`).

## Not in v1
- Countdown and stopwatch timers (the other half of "Timers and trackers").
- OS notifications outside the app window (only toasts and a chime for now).
- Reordering clocks, and clocks in the session planner.
