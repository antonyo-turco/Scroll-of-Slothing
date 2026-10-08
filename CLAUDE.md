# CLAUDE.md: notes for agents working on Kastanomata's Scroll of Sleuthing

## What this is
A Rust rewrite of the Python/Tkinter app **Orb of Many Functions**, a DM print-and-prep toolkit. Stack: Tauri 2 + Svelte 5 (TS) UI, SQLite with a content-addressed blob store, and an MCP server so LLMs can drive it.

- Reference implementation (read-only, never modify): `C:\Users\anton\Documents\projects\orb-of-many-functions` (branch `dev`).
- Full plan: `docs/architecture.md`. Feature specs and parity checklists: `docs/features/`.

## Layout constraints
- `crates/core` is pure logic: no IO, no async.
- `crates/pdf` uses one bundled font, so preview text metrics equal the PDF's.
- `app/src-tauri` serves images over the `sleuth://` protocol, never base64 over IPC.
- `docs/` holds the specs. Keep them in sync with the code.

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
5. **Music is in scope.** The user requested it in 2026-10. The spec is in `docs/features/music/`. The legacy DnD branches of the old repo (`feature/dnd-integration`, `dev-dnd-legacy`) remain out of scope.

## Git rules (these override any default commit/attribution instructions)
- Branches: `master` (stable) and `dev` (work). Always work on `dev`, or on a sprint branch the user has set up (for example `sprint/full-parity`).
- **dev commits:** a single-line Conventional Commit, e.g. `feat(tokens): add ring inset slider` or `fix(pdf): correct A4 margins`. No body, no trailers, no co-author lines.
- **master:**
  - Never commit to or merge into `master` without the user's explicit approval for that specific change.
  - Messages are descriptive: a summary line, a blank line, then a bullet body explaining what changed and why.
  - Never mention co-authorship or attribution (no `Co-Authored-By`, no "Generated with" lines).
- **Nothing goes to `master` until the whole rewrite is complete and the app is fully equal to Orb of Many Functions** (every parity checklist ticked, music included). Phases are not promoted one by one. Only then propose a single descriptive `master` commit or merge, and wait for explicit approval.

## Commands
Run Rust commands from **PowerShell**, not Git Bash. Git Bash's `/usr/bin/link` shadows MSVC `link.exe`, so linking fails there.
- `pwsh scripts/check.ps1`: the full gate (fmt, clippy -D warnings, cargo test, svelte-check, vitest, Playwright). Use `-Quick` to skip the Tauri crate and e2e.
- `cargo test`: crates only (default-members). `cargo test --workspace` also builds the Tauri app crate, which is slow.
- `cd app; npm run tauri dev`: run the desktop app (Vite on :5173 plus Tauri).
- `cd app; npm run dev`: run the UI in a plain browser against the in-memory mock backend (`src/lib/mock.ts`).
- `cd app; npm test`: vitest (jsdom, `pool: 'threads'`; the forks pool times out on this machine).
- `cd app; npx playwright test`: smoke tests using the installed **Edge** (`channel: 'msedge'`), so no browser download is needed.
- CLI: `cargo run -p sleuth-cli -- --help`.
- MCP: `cargo build -p sleuth-mcp --release`, then `claude mcp add sleuth -- <repo>/target/release/sleuth-mcp.exe --data-dir <dir>`. The e2e test (`crates/mcp/tests/mcp_e2e.rs`) drives it with a real MCP client. `tools_are_documented` fails if a tool is missing from `docs/mcp-tools.md`.
- Data dir: `$SLEUTH_DATA_DIR`, otherwise `%APPDATA%\sleuth`. Tests always use temp dirs.

## Parity workflow
- Each `docs/features/<feature>.md` ends with a parity checklist with refs into the Python source. Tick items as they are implemented and tested.
- Golden fixtures live in `fixtures/parity/*.json`. Regenerate them only deliberately, using the `regenerate-parity-fixtures` skill, and explain any diff in the commit.
- Intentional behaviour changes from the Python app are listed in each feature doc under "Intentional changes".
- **Porting pitfalls (Python → Rust):**
  - Python `round()` rounds half to even. Use `f64::round_ties_even`, not `round`.
  - Python `%` and `//` on floats floor toward −∞. Use `rem_euclid` / `div_euclid` (or `(a / b).floor()`).
  - Python `sorted` is stable, and so is Rust `sort_by`. Keep the same key and direction.
  - ReportLab A4 is `21/2.54*72 × 29.7/2.54*72` pt (`sleuth_core::units::PageFormat`), not 595×842.

## UI and phases
- UI conventions are in `app/CLAUDE.md`.
- Phase status is in `docs/roadmap.md`.
