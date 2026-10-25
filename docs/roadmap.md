# Roadmap

Each phase is developed on `dev`. A phase is done when:
- tests are green;
- the parity checklists in its feature docs are ticked;
- it has been run by hand.

Phases are **not** promoted individually. `master` receives the rewrite only once all phases are complete and the app is at full parity with Orb of Many Functions, and only with the user's explicit approval.

| Phase | Scope | Status |
|---|---|---|
| 0 | Bootstrap: repo, docs/specs, CLAUDE.md, workspace + Tauri/Svelte scaffold, parity fixture tooling, app shell, Settings | done |
| 1 | Store + blobs, Library, tags, campaigns/sessions, legacy importer, MCP v0, CLI | done (pending review) |
| 2 | Token Cutter + Token Sheet | done (pending review) |
| 3 | Object Cutter + Object Sheet | done (pending review) |
| 4 | Cards | done (pending review) |
| 5 | Map Paginator + filters + AoE templates | in progress (sprint) |
| 6 | Perf benchmarks, accessibility, packaging (MSI), in-app MCP HTTP, docs review | done (pending review) |
| 7 | Music (requested by the user; spec in `features/music/`) | done (pending review; alpha testing pending) |
| 8 | Transcribe: speech to text from files and live, local Whisper by default plus an OpenRouter-format cloud engine (requested 2026-10-18; spec in `features/transcribe.md`) | in progress (file transcription done; live and the Transcripts tab to do) |
| 9 | Combat tracker: a port of the 5etools DM Screen Initiative Tracker with 5etools combat/encounter links and a cached bestiary from a 5etools mirror (requested 2026-10-25; spec in `features/combat-tracker.md`) | in progress (v1 core tracker and links) |

## Out of scope
The features on the old repo's `feature/dnd-integration` and `dev-dnd-legacy` branches: LAN multiplayer, character sheets, encounters. Their combat tracker is replaced by Phase 9, a 5etools-compatible tracker requested on 2026-10-25 (not a port of those branches).

## Parity status
On 2026-10-18 the user declared that the rewrite has reached parity with Orb of Many Functions and is feature-complete. Phase 8 (Transcribe) is a new feature beyond parity and does not block the release. On 2026-10-25 the last unticked items (AoE templates, Map Paginator, Map filters) got their tests and the user confirmed the Music alpha test, so every parity checklist is ticked.

## Next features
In order. The parity sprint runs on `sprint/full-parity` and merges back into `dev`.

1. **Alpha testing of the build** (by the user). Fix what turns up, then tick the remaining parity checklist items:
   - Map Paginator: loading, zoom and pan, grid styles, print settings, region and tile editing, export and save.
   - Map filters: user-controlled order.
   - AoE templates: labels and halo, presets, previews, exports.
   - Music: the alpha-test item (everything else is ticked; see `features/music/overview.md`).
   - Install the MSI once and double-click a `.dmix` file (the association only exists in the installed app).
2. **Release.** Once every checklist is ticked, music included, propose a single descriptive `master` merge and wait for the user's approval.

### Removed
- Campaigns and sessions (2026-10-11, at the user's request): UI, filters, links, commands and MCP/CLI tools are gone; the tables stay, unused. See `features/campaigns-sessions.md`.

### Open questions (not blocking)
- Music (`features/music/known-issues.md`, 15): should the playlist channel appear in the mixer and be ducked? It behaves as in Python until the user decides.

### Done in the 2026-10-10 session
- **Music:** service tests for the mixer, soundbites, SFX queue, ducking, playlists, soundboards, `.dmix` files and legacy import (`crates/service/src/music_tests.rs`). They found and fixed an SFX-queue counter underflow on Stop All. There is also an e2e spec (`app/e2e/music.spec.ts`) and a parity checklist.
- **YouTube in WebView2:** checked; embeds play from the packaged app's origin, unmuted and without a user gesture. No fallback needed (`features/music/player-engine.md`).
- **Playback status indicator:** each slot shows loading (dot wave), playing (equalizer bars) or paused.
- **Phase 6:**
  - The performance benchmark and its budgets (`perf.md`); everything is within budget.
  - The accessibility pass: axe on every route in both themes, a skip link, the dialog focus trap, the palette combobox, and contrast fixes (`ux.md`, "Accessibility").
  - The MSI build: `npm run tauri build` produces `target/release/bundle/msi/Scroll of Sleuthing_<version>_x64_en-US.msi`, which registers `.dmix`.
  - In-app MCP HTTP: already implemented and covered by `crates/mcp/tests/http_e2e.rs`.
  - The docs review.

### Build speed (done)
`.cargo/config.toml`:
- links with `rust-lld`;
- builds with line-table-only debug info for our crates and none for dependencies.

This keeps `target/` far smaller (it had grown past 50 GB and filled the disk) and speeds up links. The first build after the change recompiles everything once.
