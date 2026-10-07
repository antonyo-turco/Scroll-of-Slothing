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
| 2 | Token Cutter + Token Sheet | planned |
| 3 | Object Cutter + Object Sheet | planned |
| 4 | Cards | planned |
| 5 | Map Paginator + filters + AoE templates | planned |
| 6 | Perf benchmarks, accessibility, packaging (MSI), in-app MCP HTTP, docs review | planned |
| — | Music (spec in `features/music/`) | not scheduled; only on the user's request |

## Out of scope
The features on the old repo's `feature/dnd-integration` and `dev-dnd-legacy` branches: LAN multiplayer, combat tracker, character sheets, encounters.
