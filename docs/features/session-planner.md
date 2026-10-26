# Session planner

One place for everything the next session needs: music, saved encounters, tokens and cut-outs, maps, handouts, notes and a prep checklist. Requested 2026-10-25 (from `docs/future-features.md`). New in this app: Orb of Many Functions had no planner, so there is no parity checklist.

Route `planner`, in the rail right under Library (above the **Before** group), since a session plan spans before, during and after.

## Sessions
- A planner holds any number of sessions. Each has a name (default `Session N`), an optional date (`YYYY-MM-DD` from a date picker), notes (free text), a checklist (to-dos you tick off) and its items.
- One session is **the next session** (`Planner.current`). The first session created becomes it; "Make it the next session" moves it. Deleting the next session makes the newest remaining one next.
- The planner is autosaved in `workspaces` under `planner` (no migration). Every change records a `planner` change so the app and the MCP server stay in sync.

## Items are references
- An item points at something in another tool, by kind and id: a library `asset` (token, cut-out, map), a saved `encounter`, a music `soundscape`, `playlist` or `track`, or a card `deck`. Nothing is copied; renaming or editing the item in its tool shows in the planner.
- Adding the same item twice keeps one reference. References to unknown ids are refused.
- Tokens and cut-outs carry a copy count (1–99, default 1) for printing.
- When the referenced item is deleted in its tool, the line shows as "(deleted)"; "Remove N deleted" (`planner_prune`) drops those lines.
- Sections, in order: Music (soundscapes, playlists, tracks), Encounters, Tokens and cut-outs, Maps, Handouts (card decks). Inside a section, items keep the order they were added.

## Starred items join the next session
- Starring an item in its own tool means "use this next session": pinned library assets, pinned encounters, pinned soundscapes and favourite tracks show up in the next session on their own, marked **★ Starred**, after the section's planned items.
- They are not stored in the session: unstarring removes them again. **Keep** turns one into a planned item that stays.
- Other sessions show only their planned items.

## Using it at the table
Each line has an action: **Open** (an asset in its editor), **Start** (an encounter in the combat tracker, with the tracker's import settings), **Load** (a soundscape into the mixer), **Play** (a playlist), **Music** / **Handouts** (go to the tool).

## Print all
`planner_print` writes everything printable of a session into a folder you pick, as one background job:
- one PDF of the session's tokens and cut-outs, packed together like the Token Sheet (`token-sheet.md`, "Packing with cut-outs"), each at its copy count. With "Also print the Token Sheet's contents" on, the Token Sheet's current tokens and cut-outs are on the same pages. The sheet's page format, cut border and cut-out mode apply;
- each planned map's saved PDF (the pages saved with it from the Map Paginator; a map saved with its paginator state but no PDF is rendered from that state);
- each card deck, with the deck's saved format and duplex settings.

Files are named `<session> - <item>.pdf` (`<session> - Tokens and cut-outs.pdf` for the sheet), never overwriting an existing file (` (2)`, ` (3)` …). **No silent drops:** the job result lists the files and every part it skipped with the reason (a deleted item, a map with no saved pages, a deck that fails to lay out); the app shows the skipped parts in a toast. Items too large for a page fail the export, as on the Token Sheet.

## Code layout
- `crates/core/src/planner.rs`: the model (`Planner`, `SessionPlan`, `PlanItem`, `CheckItem`, `RefKind`, `Section`), add-once and removal rules, the next-session rule, `starred_extras`, safe file names. Unit tests.
- `crates/service/src/planner.rs`: storage, the resolved view (`SessionView` with `PlanEntry` lines), the starred lookup, `planner_print`. It reuses `token_sheet::render_sheet`, `cards::render_deck` and `Service::render_map_pdf`.
- UI: `app/src/routes/planner/Planner.svelte` (sections in the centre; session details, checklist, notes and print options in the inspector) and `AddToSession.svelte` (the picker). Browser mock: `app/src/lib/mock-planner.ts`.

## MCP
`planner_list`, `planner_session_get`, `planner_session_create`, `planner_session_update`, `planner_session_delete`, `planner_set_current`, `planner_items_add`, `planner_item_update`, `planner_item_remove`, `planner_prune`, `planner_print` (see `docs/mcp-tools.md`).

## Not in v1
- Images that are not tokens, cut-outs or maps as handouts (the library has only those three kinds); letters and posters wait for the Handouts destination (`future-features.md`).
- Shops and loot tables, until the Tables page exists.
