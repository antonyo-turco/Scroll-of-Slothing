# Architecture

## Goals
1. **Parity**: every capability of Orb of Many Functions survives (see `features/`). Music is specified but deferred.
2. **Responsiveness**: the UI thread never waits on pixels or PDFs.
3. **Clarity**: one minimal, consistent layout for every tool.
4. **LLM-drivable**: every user action is also an MCP tool.
5. **Extensible**: open-ended asset kinds, JSON params, and migrations.

## Layering
```
            ┌─────────── app/src (Svelte 5) ────────────┐
            │ routes · inspector · canvas · jobs tray    │
            └──────────────┬─────────────────────────────┘
                 invoke()  │  events (job:*, store:changed)   sleuth:// protocol (images)
            ┌──────────────▼─────────────┐        ┌──────────────────────┐
            │ app/src-tauri (commands)   │        │ crates/mcp (rmcp)    │◄── Claude / any MCP client
            └──────────────┬─────────────┘        └──────────┬───────────┘
                           └──────────────┬──────────────────┘
                                  ┌───────▼────────┐
                                  │ crates/service │  async API, Jobs, events
                                  └───┬───────┬────┘
                         ┌────────────▼┐   ┌──▼──────────┐
                         │ crates/core │   │ crates/store│  SQLite + blobs + change_log
                         │ crates/pdf  │   └─────────────┘
                         └─────────────┘
```

- **core** is deterministic and pure: no filesystem, no clock, no async. Everything in it is unit- and property-testable.
- **pdf** turns core layouts plus pixel buffers into PDF bytes. It bundles one font (Liberation Sans, which has Helvetica metrics) for both measuring and drawing text.
- **store** owns persistence:
  - SQLite in WAL mode with `foreign_keys=ON`;
  - a minimal forward-only migration runner (`PRAGMA user_version` + embedded `migrations/V{n}__*.sql`);
  - a blob store at `blobs/ab/cd/<blake3>.<ext>`;
  - derivative generation (a thumbnail ≤256 px and a preview ≤2048 px, both lossless WebP);
  - a `change_log`.
- **service** is the only public API. Each user action is an `async fn`. CPU work runs in `spawn_blocking`.
  - Long operations (exports, full-resolution FFT, legacy import) are **Jobs**: an id, progress 0..1, a message, a cancel token, and a result.
  - Service emits `Event`s (`JobProgress`, `JobDone`, `StoreChanged{entity,id}`).
- **mcp** registers one tool per service call. It runs either standalone (`sleuth-mcp`, stdio) or inside the app (streamable HTTP on localhost with a bearer token, off by default).
- **cli** gives scriptable access to service (`--json`), plus `import-legacy`.
- **app/src-tauri** contains only thin commands and the `sleuth://` URI scheme:
  - `sleuth://blob/<hash>[?w=<px>]` serves a stored or derived image;
  - `sleuth://preview/<key>` serves an in-memory render (for example the current token compose).

## Multi-process consistency
The standalone MCP process and the app can both open the same data dir. SQLite WAL handles concurrent access.
1. Every service write inserts a `change_log` row.
2. The app polls `max(change_log.seq)` every 500 ms (`Service::spawn_change_watcher`). New rows, whether written by this process or another one, are broadcast as `StoreChanged` events and forwarded to the UI on the `sleuth://event` Tauri channel. The UI refetches only the affected queries.

## Responsiveness rules
1. Never block a command. Pixel and PDF work goes to `spawn_blocking`/rayon.
2. Editors use preview proxies. Full resolution is used only on export.
3. Pan, zoom and handle drags are client-side transforms (CSS/canvas). Rust recomputes are debounced (≈80-120 ms), and every request carries a generation number so stale results are dropped.
4. Dilation uses a distance transform. FFT grid removal runs on the proxy for preview and at full resolution only in the export job.
5. PDFs embed encoded images directly, with no temp files.
6. UI routes are lazy-loaded. The library grid is virtualized.

## State persistence
Editor and sheet state lives in the `workspaces` table (key → JSON), autosaved with a debounce. Restarting the app restores:
- the token sheet;
- the object sheet;
- the card editor;
- the AoE sheet;
- the last map paginator session.

## Adding a feature (checklist)
1. Write the algorithm in `core`, with unit and property tests.
2. Add the service fn, plus a Job if it runs long.
3. Add the Tauri command and the MCP tool (same name, snake_case), then update `docs/mcp-tools.md`.
4. Build the UI view and inspector sections.
5. Update `docs/features/<x>.md`, including the parity checklist and any intentional changes.
6. Add an integration test through service or MCP.

## Key dependencies
| Concern | Crate / lib |
|---|---|
| images | `image`, `fast_image_resize`, `imageproc` |
| FFT | `rustfft` |
| PDF | `krilla` (+ `lopdf` in tests) |
| font metrics | `ttf-parser` |
| DB | `rusqlite` (bundled, FTS5) + own migration runner |
| hashing | `blake3` |
| MCP | `rmcp` |
| clipboard | `arboard` |
| async | `tokio` |
| tests | `proptest`, `insta`, `tempfile` |
| UI | Svelte 5, Vite, TypeScript, CodeMirror 6 (cards), vitest, Playwright |
