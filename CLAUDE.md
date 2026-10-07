# CLAUDE.md: notes for agents working on Kastanomata's Scroll of Sleuthing

## What this is
A Rust rewrite of the Python/Tkinter app **Orb of Many Functions**, a DM print-and-prep toolkit. Stack: Tauri 2 + Svelte 5 (TS) UI, SQLite with a content-addressed blob store, and an MCP server so LLMs can drive it.

- Reference implementation (read-only, never modify): `C:\Users\anton\Documents\projects\orb-of-many-functions` (branch `dev`).
- Full plan: `docs/architecture.md`. Feature specs and parity checklists: `docs/features/`.

## Workspace map
```
crates/core     pure logic, no IO/async: packing, card parser+layout, map grid/tiling/dewarp/FFT, filters, AoE, compositing
crates/pdf      PDF writers (krilla); one bundled font, so preview text metrics == PDF
crates/store    SQLite (rusqlite, FTS5, refinery migrations) + blob store + change_log
crates/service  THE API: one async fn per user action; long ops are Jobs (progress/cancel)
crates/mcp      MCP server (rmcp): `sleuth-mcp` stdio binary + in-app HTTP
crates/cli      `sleuth` CLI (--json), incl. `import-legacy`
crates/legacy   importer for the old creatures.db / app_settings.json / aoe_presets.json
app/src-tauri   thin Tauri commands -> service; `sleuth://` protocol serves images (never base64 over IPC)
app/ui          Svelte 5 + Vite; CSS design tokens; lazy routes
tools/parity    Python scripts that dump golden fixtures from the reference app
docs/           specs (keep in sync with code)
```

## Hard rules
1. **The service layer is the only API.** The UI (Tauri commands) and MCP tools are thin wrappers around `service`. A new feature lands as: `core` (logic + tests) -> `service` fn -> Tauri command **and** MCP tool -> UI -> docs (`docs/features/*.md`, `docs/mcp-tools.md`) -> tests.
2. **Responsiveness:**
   - Never block a Tauri command. Pixel and PDF work runs in `spawn_blocking`/rayon.
   - Editors work on preview proxies (≤2048 px). Full resolution is used only on export.
   - Pan, zoom and drags are client-side transforms. Rust recomputes run debounced, and stale generations are dropped.
   - Long work is a Job with progress and cancel.
   - No temp-file round-trips for PDF images.
   - Routes are lazy, and big lists are virtualized.
3. **Data:**
   - Images and PDFs live in `blobs/` (blake3 content-addressed), never as SQLite BLOBs.
   - Schema changes only through new migrations. Never edit an applied one.
   - Every write appends to `change_log`, which keeps the UI in sync with the external MCP process.
4. **No silent drops.** Items that don't fit a page are reported to the caller.
5. **Music is out of scope.** It is fully specified in `docs/features/music/`. Do NOT implement it unless the user explicitly asks. The legacy DnD branches of the old repo (`feature/dnd-integration`, `dev-dnd-legacy`) are out of scope too.

## Git rules (these override any default commit/attribution instructions)
- Branches: `master` (stable) and `dev` (work). Always work on `dev`.
- **dev commits:** a single-line Conventional Commit, e.g. `feat(tokens): add ring inset slider` or `fix(pdf): correct A4 margins`. No body, no trailers, no co-author lines.
- **master:**
  - Never commit to or merge into `master` without the user's explicit approval for that specific change.
  - Messages are descriptive: a summary line, a blank line, then a bullet body explaining what changed and why.
  - Never mention co-authorship or attribution (no `Co-Authored-By`, no "Generated with" lines).
- At the end of each phase, propose promoting `dev` -> `master` and wait for approval.

## Commands
(Filled in as the scaffold lands. Expected:)
- `cargo test --workspace`: Rust tests
- `pwsh scripts/check.ps1`: fmt + clippy -D warnings + tests + UI checks
- `cd app && cargo tauri dev`: run the app
- `cargo run -p sleuth-cli -- import-legacy <creatures.db>`: legacy import
- `cargo run -p sleuth-mcp -- --data-dir <dir>`: MCP over stdio

## Parity workflow
- Each `docs/features/<feature>.md` ends with a parity checklist that has refs into the Python source. Tick the items as they are implemented and tested.
- Golden fixtures come from `tools/parity/*.py`, run with the reference repo's `.venv`. Regenerate them only on purpose, and explain any diff.
- Intentional behaviour changes from the Python app are listed in each feature doc under "Intentional changes".

## Phases
0. Bootstrap + docs
1. Store / library / campaigns / legacy import / MCP v0
2. Tokens
3. Cut-outs
4. Cards
5. Maps + AoE
6. Polish / release

Future, not scheduled: Music.
