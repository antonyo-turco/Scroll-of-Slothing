# UI conventions
- Svelte 5 runes. Global state lives in `app/src/lib/state.svelte.ts`. The navigation model is `app/src/lib/nav.ts`. Its route ids are persisted and must match `sleuth_core::settings::ROUTES` (a vitest test enforces this).
- All backend calls go through `app/src/lib/api.ts`. When adding a command, add it to `src-tauri/src/lib.rs` (`generate_handler!`), `commands.rs`, `api.ts`, and the mock.
- Use the design tokens and primitives in `app/src/app.css` (`.btn`, `.input`, `.segmented`, `.card`). Don't add a component library.
- Layout for every view (`docs/ux.md`, "Layout"):
  - file actions go in the top bar through `<ViewActions>`, with the primary one last;
  - tools go in a `.view-toolbar` band, the content in `.stage-area`, and status text in `.statusbar`;
  - settings go in `aside.inspector`, built from `.section` blocks (`.section-title`, `.field`, `.prop`, `.pair`, `.range`, `.check`, `.note`).
- Don't invent per-view panel widths, header styles or button placements.
- "Save to library" in editors uses `SaveAssetDialog`.
