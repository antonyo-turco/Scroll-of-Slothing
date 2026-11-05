# Tags

Tags are free-form, case-insensitive labels on assets: `tags.name UNIQUE COLLATE NOCASE`.

## Behaviour
- `set_asset_tags(asset, names)` replaces the asset's whole tag set. Tags that don't exist yet are created. Matching ignores case.
- **Rename:** if the new name already exists (case-insensitive), the tag is **merged** into that tag instead.
- **Merge X into Y:** every link to X is moved to Y, duplicate links are dropped, then X is deleted. The UI does not ask for confirmation.
- **Delete:** removes the tag and all of its links (the UI asks first).
- **Counts:** each tag shows "used by N".

## UI: the tag manager (a dialog in Library)
Opened with "Manage tags…" in the Library top bar.
- A row at the top: "Merge [X] into [Y]" and a Merge button.
- One row per tag: the name as an editable field (renames on blur; a toast says when the rename merged into an existing tag), the "used by N" count, and Delete, which asks inline (Delete / Keep).

## MCP
- `tags_list` (with counts)
- `tags_rename {id, name}` (returns `merged_into` when a merge happened)
- `tags_merge {src, dst}`
- `tags_delete {id}`

## Parity checklist
- [x] `set_asset_tags`, `rename_asset_tag` with auto-merge, `merge_asset_tags`, `delete_asset_tag`, `asset_tag_counts` (`db/creature_db.py`)
- [x] Tag manager UI (`ui/asset_tag_manager.py`)
