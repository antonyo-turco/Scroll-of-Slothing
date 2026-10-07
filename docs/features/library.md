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
  - campaign: All or one campaign;
  - session: All or one session in that campaign;
  - Pinned only.
- **Views:** a "Pinned + This session" preset reproduces the old Tagged Assets tab. It shows two sections: Pinned (global), and assets in the selected session.

**Grid**
- A virtualized card grid. Each card has:
  - a pin star;
  - the thumbnail (thumb derivative);
  - name, kind badge, tag pills and "aka" aliases;
  - a size select (not shown for maps), saved immediately;
  - the date.
- **Hover preview:** after a 300 ms hover, a popover shows the preview derivative (≤2048 px, scaled to 320 px). Python decoded the full original on the UI thread.
- Card actions, in an overflow menu plus the primary button:
  - **Add to sheet**: tokens go to the Token Sheet, cut-outs to the Object Sheet;
  - **Edit**;
  - **Export**;
  - **Delete** (with confirmation);
  - **Open in editor**: Token Cutter, Object Cutter or Map Paginator, depending on the kind.

## Edit drawer
Fields:
- Name
- Aliases (one per line)
- Tags (chip input that creates tags as you type)
- Campaign checkboxes
- Session checkboxes, labelled "Campaign / Session"

Buttons: "Open in …", Save, Cancel.

## Export
For a single asset:
- Save original
- Save rendered: the token PNG, or the gridded map PNG
- Copy image to clipboard
- Maps only: Save paginated PDF

## Bulk select mode
Toggled with a "Select" button, Ctrl/Shift-click, or Ctrl+A. The bulk bar offers:
- Delete (with confirmation)
- Add tag
- Pin, Unpin
- Add to session, Add to campaign
- Export to folder: writes `<name>_<kind>.png` for each asset
- Add to sheet: tokens go to the Token Sheet and cut-outs to the Object Sheet. Maps are skipped and the number skipped is reported.

## Import
Opened with the "+ Import" button or by dropping files anywhere on the Library view.
- Accepted formats: png, jpg, jpeg, webp, gif, bmp.
- One row per file:
  - a 48 px thumbnail;
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
| `library_search` | `{q?, kinds?, tags?, campaign_id?, session_id?, pinned?, sort?, limit?, offset?}` returns a list of summaries |
| `library_get` | `{id}` returns metadata plus a thumbnail image |
| `library_update` | `{id, name?, aliases?, size?, pinned?}` |
| `library_set_tags` | |
| `library_set_campaigns` | |
| `library_set_sessions` | |
| `library_delete` | `{ids}` |
| `library_import` | `{files:[{path, name?, kind, size?}]}` |
| `library_export` | `{id, what: original\|rendered\|pdf, path}` |

## Intentional changes
- The two Python tabs are merged into one view with filter presets.
- Hover preview uses a derivative instead of decoding the full original.
- Search uses FTS5 instead of `LIKE`.
- The grid is virtualized. Python built cards in chunks of 24 per tick.

## Parity checklist (Python refs)
- [ ] Search across name, aliases and tags; the 6 sort modes (`creature_catalogue_panel.py`)
- [ ] Kind chips and AND tag chips
- [ ] Campaign/session filters, where campaign includes session-linked assets (`creature_db.list_assets_full`)
- [ ] Pin star; Pinned + session view (tagged mode)
- [ ] Size select with immediate save (`update_creature_size`)
- [ ] Edit dialog: name, aliases, tags, campaigns, sessions, open-in-editor
- [ ] Export dialog: original, rendered, clipboard, map PDF (`utils.copy_image_to_clipboard`)
- [ ] All 8 bulk actions
- [ ] Import dialog with kind and size per row (`ui/asset_import_dialog.py`)
- [ ] Re-edit routing by kind (`app.py:539-561`)
