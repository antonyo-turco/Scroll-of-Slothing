# Library

The asset library holds tokens, cut-outs and maps. It **merges** the two Python tabs "Assets" (`ui/creature_catalogue_panel.py`, library mode) and "Tagged Assets" (the same panel in tagged mode) into one view, with filter presets.

## Data
See the `assets` table in `../data-model.md`. Kinds: `token`, `cutout`, `map`.

**Size labels:** Tiny, Small, Medium, Large, Huge, Gargantuan. Point sizes are listed in `token-sheet.md`.

## Browsing
- **Search** is debounced (150 ms) and matches name, aliases and tag names through FTS5 prefix queries. Python used a `LIKE` search over the same fields.
- **Sort:** Newest (default), Oldest, Name A–Z, Name Z–A, Kind, Size.
- **Filters:**
  - kind chips: All, Token, Cut-out, Map;
  - tag chips, where all selected tags must match (AND), with "clear";
  - Pinned only.
- **Views:** the "★ Pinned" toggle replaces the old Tagged Assets tab (Pinned + This session); sessions were removed at the user's request on 2026-10-11.

**View modes** (new, 2026-10-18): a segmented switch next to the sort picker, remembered in the workspace (`library.view`).
- **Cards**: the grid described below (176 × 258 px).
- **List**: one full-width row per asset (48 px) with a 40 px thumbnail, name and aliases, kind, size select, tags, updated date and the pin star, under a column header. The rows share their look with the Music library (`.list-head`, `.list-row`, `.list-thumb`, `.list-pill`, `.list-star` in `app.css`). The whole row is the click target, so selection (click, Ctrl/Shift+click) works as in the grid.
- **Pills**: compact tiles (112 × 142 px) with a smaller image and the name only; the pin star shows on hover.
- All three are the same `AssetCard` component (`variant`), so the hover preview, pinning and selection behave identically; `VirtualGrid` has a `fill` mode for the single-column list.

**Cards**
- A virtualized grid. Each card has a pin star, the thumbnail, name, kind badge, tag pills, "aka" aliases, a size select (not for maps, saved immediately) and the date.
- **Hover preview:** after a 350 ms hover, a popover shows the preview derivative (≤2048 px, scaled to 320 px). Python decoded the full original on the UI thread.
- A plain click opens the asset in the inspector. Cards have no menu of their own: every per-asset action is in the inspector.

## Inspector
Always shown on the right (empty when nothing is selected), in sections:
- **Header:** the preview, **Open in …** (Token Cutter, Object Cutter or Map Paginator, by kind) and, for tokens and cut-outs, **Add to token/object sheet**.
- **Details:** Name, Print size (not for maps), Aliases (one per line) and Tags (a chip input that creates tags as you type). Name and aliases save on blur (Enter commits the name); size and tags save on change. There is no Save/Cancel.
- **Export:** Save image… (the rendered token, cut-out or gridded map), Save original…, Save PDF… (only when the asset has a paginated PDF), Copy image.
- **Footer:** Delete asset… with an inline confirmation.

## Selection and bulk actions
- Ctrl+click toggles, Shift+click selects a range, Ctrl+A (or "Select all") selects everything, Esc clears. Ctrl+F focuses the search.
- Esc in a text field leaves the field (committing the edit); the next Esc deselects.
- With a selection, the bulk bar offers: Add tag, Pin, Unpin, **Add to sheets** (tokens to the Token Sheet, cut-outs to the Object Sheet; maps are skipped and the count reported), **Export…** to a folder (`<name>_<kind>.png` per asset, with ` (2)`, ` (3)`… on clashes) and Delete… with confirmation.

## Import
Opened with the "+ Import" button in the top bar or by dropping files anywhere on the Library view. "Manage tags…" sits next to it.
- Accepted formats: png, jpg, jpeg, webp, gif, bmp.
- One row per file (no thumbnail: the webview cannot read arbitrary local paths; thumbnails appear in the grid once imported):
  - the name, defaulting to the file stem;
  - the kind (Token, Cut-out or Map);
  - the size (disabled for maps).
- Each file is stored unprocessed: the same image is used as both original and rendered.
- Failures are listed in a summary at the end.

## Re-edit flow
"Open in editor" loads the original and the `params_json`, so the user carries on where they left off.

On save, the editor asks:
- **Update existing**: Python's "Yes".
- **Save as new**: Python's "No".
- **Cancel**.

Assets imported from the old app have no params. They open with default framing, as in Python.

## MCP
| Tool | Purpose |
|---|---|
| `library_search` | `{q?, kinds?, tags?, pinned_only?, sort?, limit?, offset?}` returns a list of summaries. `sort`: `newest` (default), `oldest`, `name_asc`, `name_desc`, `kind`, `size`; `limit` defaults to 50, max 500 |
| `library_get` | `{id}` returns metadata plus a thumbnail image |
| `library_update` | `{id, name?, aliases?, size?, pinned?}` |
| `library_set_tags` | |
| `library_delete` | `{ids}` |
| `library_import` | `{files:[{path, name?, kind, size?}]}` |
| `library_export` | `{id, what: original\|rendered\|pdf, path}` |

## Intentional changes
- UI consistency pass (2026-10-11, branch `ui/consistency`): the details inspector is always shown (empty when nothing is selected) and grouped into Open/Add, Details, Export and a footer with Delete. Import and Manage tags moved to the top bar.
- Campaigns and sessions were removed at the user's request on 2026-10-11: no filters, inspector links or bulk links. Existing links stay in the database, unused.
- The two Python tabs are merged into one view with filter presets.
- Hover preview uses a derivative instead of decoding the full original.
- Search uses FTS5 instead of `LIKE`.
- The grid is virtualized. Python built cards in chunks of 24 per tick.

## Parity checklist (Python refs)
- [x] Search across name, aliases and tags; the 6 sort modes (`creature_catalogue_panel.py`) (store tests, `library.test.ts`)
- [x] Kind chips and AND tag chips
- [x] ~~Campaign/session filters~~ (`creature_db.list_assets_full`): removed at the user's request on 2026-10-11
- [x] Pin star; the "★ Pinned" toggle (the session half of the tagged view was removed at the user's request on 2026-10-11)
- [x] Size select with immediate save (`update_creature_size`), on the card and in the inspector
- [x] Edit dialog: name, aliases, tags (inspector panel); campaigns and sessions removed at the user's request on 2026-10-11
- [x] Open-in-editor: tokens open in the Token Cutter with their saved framing, cut-outs in the Object Cutter with their saved trace, and maps in the Map Paginator with their saved state (e2e `open in editor routes each kind…`, vitest `ui_navigate`)
- [x] Export: original, rendered, clipboard, map PDF (`utils.copy_image_to_clipboard`)
- [x] Bulk actions: delete, add tag, pin, unpin, export to folder (add to session/campaign removed at the user's request on 2026-10-11)
- [x] Bulk "add to sheet": tokens go to the token sheet and cut-outs to the object sheet; maps are skipped and counted (as in Python).
- [x] Import dialog with kind and size per row (`ui/asset_import_dialog.py`), plus drop-to-import
- [x] Re-edit routing by kind (`app.py:539-561`), with editor state restore (same tests)
