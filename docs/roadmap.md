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
| 6 | Perf benchmarks, accessibility, packaging (MSI), in-app MCP HTTP, docs review | planned |
| 7 | Music (requested by the user; spec in `features/music/`) | implemented, alpha testing pending |

## Out of scope
The features on the old repo's `feature/dnd-integration` and `dev-dnd-legacy` branches: LAN multiplayer, combat tracker, character sheets, encounters.

## Next features
In order. The parity sprint runs on `sprint/full-parity` and merges back into `dev`.

1. **Alpha testing of the sprint build** (by the user). Fix what turns up, then tick the remaining parity checklist items:
   - Map Paginator: loading, zoom and pan, grid styles, print settings, region and tile editing, export and save.
   - Map filters: user-controlled order.
   - AoE templates: labels and halo, presets, previews, exports.
   - Library: open in editor.
   - Settings: cut-line stroke.
   - Music: everything (it is new and has no parity checklist yet).
2. **Music checklist and tests.** Add a parity checklist to `features/music/overview.md`, plus service tests for the mixer, soundbites, ducking, playlists and `.dmix` files.
3. **Phase 6.**
   - Perf benchmarks for map preview, filters and PDF export.
   - An accessibility pass: keyboard paths, focus and labels.
   - The MSI installer. It is also what makes `.dmix` double-click work; `tauri dev` cannot register file associations.
   - A final docs review.
4. **Known risk to check early:** YouTube may refuse to embed in the WebView2 window. If it does, music needs a fallback for YouTube tracks.
5. **Release.** Once every checklist is ticked, music included, propose a single descriptive `master` merge and wait for the user's approval.

### Build speed (done)
`.cargo/config.toml`:
- links with `rust-lld`;
- builds with line-table-only debug info for our crates and none for dependencies.

This keeps `target/` far smaller (it had grown past 50 GB and filled the disk) and speeds up links. The first build after the change recompiles everything once.
