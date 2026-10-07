# CLAUDE.md: notes for agents working on Kastanomata's Scroll of Sleuthing

## What this is
A Rust rewrite of the Python/Tkinter app **Orb of Many Functions**, a DM print-and-prep toolkit. Stack: Tauri 2 + Svelte 5 (TS) UI, SQLite with a content-addressed blob store, and an MCP server so LLMs can drive it.

- Reference implementation (read-only, never modify): `C:\Users\anton\Documents\projects\orb-of-many-functions` (branch `dev`).
- Full plan: `docs/architecture.md`. Feature specs and parity checklists: `docs/features/`.

## Workspace map
```
crates/core     pure logic, no IO/async: packing, card parser+layout, map grid/tiling/dewarp/FFT, filters, AoE, compositing
crates/pdf      PDF writers (krilla); one bundled font, so preview text metrics == PDF
crates/store    SQLite (rusqlite, FTS5, own migration runner) + blob store + change_log + workspaces
crates/service  THE API: one async fn per user action; long ops are Jobs (progress/cancel)
crates/mcp      MCP server (rmcp): `sleuth-mcp` stdio binary + in-app HTTP
crates/cli      `sleuth` CLI (--json), incl. `import-legacy`
crates/legacy   importer for the old creatures.db / app_settings.json / aoe_presets.json
app/src-tauri   thin Tauri commands -> service; `sleuth://` protocol serves images (never base64 over IPC)
app/src         Svelte 5 + Vite UI (app/ is the npm project); CSS design tokens; lazy routes
fixtures/parity golden JSON generated from the Python app
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
Run Rust commands from **PowerShell**, not Git Bash. Git Bash's `/usr/bin/link` shadows MSVC `link.exe`, so linking fails there.
- `pwsh scripts/check.ps1`: the full gate (fmt, clippy -D warnings, cargo test, svelte-check, vitest, Playwright). Use `-Quick` to skip the Tauri crate and e2e.
- `cargo test`: crates only (default-members). `cargo test --workspace` also builds the Tauri app crate, which is slow.
- `cd app; npm run tauri dev`: run the desktop app (Vite on :5173 plus Tauri).
- `cd app; npm run dev`: run the UI in a plain browser against the in-memory mock backend (`src/lib/mock.ts`).
- `cd app; npm test`: vitest (jsdom, `pool: 'threads'`; the forks pool times out on this machine).
- `cd app; npx playwright test`: smoke tests using the installed **Edge** (`channel: 'msedge'`), so no browser download is needed.
- CLI: `cargo run -p sleuth-cli -- --help`, e.g. `sleuth import-legacy <creatures.db> [--settings app_settings.json] [--aoe aoe_presets.json]` or `sleuth --json library search goblin`.
- MCP: `cargo build -p sleuth-mcp --release`, then `claude mcp add sleuth -- <repo>/target/release/sleuth-mcp.exe --data-dir <dir>`. The e2e test (`crates/mcp/tests/mcp_e2e.rs`) drives it with a real MCP client. `tools_are_documented` fails if a tool is missing from `docs/mcp-tools.md`.
- Data dir: `$SLEUTH_DATA_DIR`, otherwise `%APPDATA%\sleuth`. Tests always use temp dirs.

## Parity workflow
- Each `docs/features/<feature>.md` ends with a parity checklist with refs into the Python source. Tick items as they are implemented and tested.
- Golden fixtures live in `fixtures/parity/*.json`. They are generated by `tools/parity/generate.py`, run with the reference repo's venv:
  `& C:\Users\anton\Documents\projects\orb-of-many-functions\.venv\Scripts\python.exe -I -B tools/parity/generate.py C:\Users\anton\Documents\projects\orb-of-many-functions`
  Regenerate only deliberately, and explain any diff in the commit. Rust tests read them via `CARGO_MANIFEST_DIR/../../fixtures/parity`.
- Intentional behaviour changes from the Python app are listed in each feature doc under "Intentional changes".
- **Porting pitfalls (Python → Rust):**
  - Python `round()` rounds half to even. Use `f64::round_ties_even`, not `round`.
  - Python `%` and `//` on floats floor toward −∞. Use `rem_euclid` / `div_euclid` (or `(a / b).floor()`).
  - Python `sorted` is stable, and so is Rust `sort_by`. Keep the same key and direction.
  - ReportLab A4 is `21/2.54*72 × 29.7/2.54*72` pt (`sleuth_core::units::PageFormat`), not 595×842.

## UI conventions
- Svelte 5 runes. Global state lives in `app/src/lib/state.svelte.ts`. The navigation model is `app/src/lib/nav.ts`. Its route ids are persisted and must match `sleuth_core::settings::ROUTES` (a vitest test enforces this).
- All backend calls go through `app/src/lib/api.ts`. When adding a command, add it to `src-tauri/src/lib.rs` (`generate_handler!`), `commands.rs`, `api.ts`, and the mock.
- Use the design tokens and primitives in `app/src/app.css` (`.btn`, `.input`, `.segmented`, `.card`). Don't add a component library.
- Layout for tool views: canvas in the centre, inspector on the right, primary action in the top bar (see `docs/ux.md`).

## Phases
See `docs/roadmap.md` for status.
0. Bootstrap + docs
1. Store / library / campaigns / legacy import / MCP v0
2. Tokens
3. Cut-outs
4. Cards
5. Maps + AoE
6. Polish / release

Future, not scheduled: Music.
