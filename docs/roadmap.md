# Roadmap

Status as of 2026-11-05. The per-feature detail lives in each doc's checklist under `features/`; this page only summarises it.

Work happens on `dev` (or a sprint branch). `master` receives the rewrite once, as a single descriptive merge, when the whole app is at parity with Orb of Many Functions, and only with the user's explicit approval.

## Phases
| Phase | Scope | Status |
|---|---|---|
| 0 | Bootstrap: repo, specs, workspace, Tauri/Svelte scaffold, parity fixture tooling, app shell, Settings | done |
| 1 | Store and blobs, Library, tags, legacy importer, MCP, CLI | done |
| 2 | Token Cutter, Token Sheet | done |
| 3 | Object Cutter, Object Sheet | done |
| 4 | Cards | done |
| 5 | Map Paginator, map filters, AoE templates | done |
| 6 | Perf benchmark, accessibility pass, MSI packaging, in-app MCP over HTTP | done |
| 7 | Music (`features/music/`) | done; alpha test confirmed by the user on 2026-10-25 |
| 8 | Transcribe (`features/transcribe.md`), beyond parity | partial: see below |
| 9 | Combat tracker (`features/combat-tracker.md`), beyond parity | partial: see below |
| 10 | Session planner (`features/session-planner.md`), beyond parity | v1 done, untested |
| 11 | Clocks (`features/clocks.md`), beyond parity | v1 done, tried against the browser mock only |
| 12 | Spell and item cards (`features/spell-item-cards.md`), beyond parity | v1 done, not tried in the app |

"Done" means the feature doc's checklist is fully ticked: implemented, tested, run by hand.

## Parity
Every parity checklist (phases 0-7) is ticked. The user declared parity on 2026-10-18; the last map and AoE items got their tests on 2026-10-25.

Still open before the `master` merge:
- Install the MSI once and double-click a `.dmix` file (the file association only exists in the installed app).
- User alpha testing of the full build; fix what turns up.

Phases 8 to 12 go beyond parity and do not block the merge.

## Phase 8: Transcribe
Built:
- File transcription as a Job, with the local Whisper engine and the OpenRouter-format cloud engine (key in the OS keyring, cost shown, confirmation on first use).
- Export to TXT, MD, SRT and VTT.
- Only the File tab exists in the UI (`routes/transcribe/TranscribeFile.svelte`).

Not built:
- Storing transcripts (no migration yet), so no Transcripts tab and no saved cost.
- Live transcription (local and cloud).
- Unticked checklist items need tests: local file transcription, model download/delete, corrections, MCP e2e coverage, the UI checklist.

## Phase 9: Combat tracker
Built: the 5etools-compatible tracker, the cached 5etools bestiary, combat and encounter links, saved encounters, hero parties, homebrew creatures, `.denc` files, stat block rolls, the rolls window, undo.

Not done:
- The MCP tools are not covered by `mcp_e2e.rs`.
- The tracker rework (HP strips, temp HP, defeated list, difficulty, side panel editor, homebrew, `.denc`, creature tokens) has no UI tests; the service tests for homebrew, temp HP and `.denc` exist but are not confirmed as run.

### Possible future features (not to build until asked)
Saved at the user's request on 2026-10-25:
- **Music cues:** switch the Music mixer to a chosen soundscape when combat starts or a "boss" row's turn begins.
- **Quick NPC generator:** make a homebrew creature from a template ("CR 2 brute") with the 2024 DMG statistics by CR.

### Decided against
- Combat log (a per-round timeline).
- Maps integration (encounters from a map, AoE templates sized from stat blocks).
- Player view and per-row "visible to players": the tracker is for the DM only.
- Custom conditions (removed 2026-10-25).

## Removed
- Campaigns and sessions (2026-10-11, at the user's request): UI, filters, commands and MCP/CLI tools are gone; the tables remain, unused. See `features/campaigns-sessions.md`.

## Out of scope
The old repo's `feature/dnd-integration` and `dev-dnd-legacy` branches (LAN multiplayer, character sheets, their combat tracker). Phase 9 is a new 5etools-compatible tracker, not a port of them.

## Beyond parity
New features are not ports: they are scored against this app's own specs in `features/*.md` and get no Python parity checklist. The first is the floating dice roller (`features/dice-roller.md`). Unscheduled, deferred and declined ideas are listed in `future-features.md`.

## Open questions
- Music (`features/music/known-issues.md`, item 15): should the playlist channel appear in the mixer and be ducked? It behaves as in Python until the user decides.
