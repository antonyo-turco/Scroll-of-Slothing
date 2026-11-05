# Legacy import (from Orb of Many Functions)

```
sloth import-legacy <path\to\creatures.db> [--settings <app_settings.json>] [--aoe <aoe_presets.json>] [--data-dir <dir>] [--json]
```

Three entry points run the same service call (`legacy_import`):
- **Settings → Data → "Import from Orb of Many Functions…"** (desktop app only): pick `db/creatures.db`; `app_settings.json` and `aoe_presets.json` are taken from the old project root next to it, if present.
- The CLI command above.
- The MCP tool `legacy_import`.

- The source database is opened **read-only** (`SQLITE_OPEN_READ_ONLY`). Never point this command at the only copy of your data; the reference repo should stay untouched.
- The import runs as a job with progress. It reports counts when finished.
- **The import is idempotent.** Assets are matched on `assets.legacy_id`, campaigns, sessions and tags on their legacy id or name. Running it again skips anything already imported.

> Campaigns, sessions and their links are still imported, into tables the app no longer shows (removed at the user's request on 2026-10-11). Nothing is lost if they ever come back.

## Mapping
| Old | New |
|---|---|
| `creatures` (id, name, original, token, size, kind, aliases, pdf, pinned, created) | `assets` + blobs. `original` becomes `original_blob`, `token` becomes `rendered_blob`, `pdf` becomes `pdf_blob`. Thumbnail and preview derivatives are regenerated. `legacy_id` = old id. |
| size labels `"Medium (1 inch)"` etc. | `Medium` etc. |
| `asset_thumbs` | ignored (regenerated) |
| `asset_tags`, `asset_tag_links` | `tags`, `asset_tags` |
| `music_campaigns` | `campaigns`, keeping the uuid |
| `sessions` | `sessions`, keeping the uuid |
| `session_assets`, `asset_campaigns` | `asset_sessions`, `asset_campaigns` |
| all `music_*` tables | `legacy/music.json` (raw rows, one array per table), then imported by "Import legacy music" (`music_import_legacy`) |
| `app_meta` | ignored |
| `app_settings.json` | `settings`. `start_tab` labels are mapped to routes, e.g. "Token Sheet" becomes `tokens.sheet`. |
| `aoe_presets.json` | `aoe_presets` |

## Performance
The old database is about 579 MB. BLOBs are read one row at a time with `rusqlite` incremental blob I/O, hashed with blake3, and written to `blobs/`. Derivatives are generated in parallel with rayon. Memory use stays bounded by one row at a time per worker.

## Report
Fields of `LegacyImportReport` (`crates/service/src/legacy.rs`):
```json
{"assets": {"token": 56, "cutout": 1, "map": 19}, "skipped_existing": 0, "failed": [],
 "campaigns": 1, "sessions": 0, "tag_links": 0, "asset_campaign_links": 0, "asset_session_links": 0,
 "music_rows": {"music_tracks": 26}, "music_json": "<data dir>/legacy/music.json",
 "settings_imported": true, "aoe_presets": 0}
```
`failed` lists assets that could not be converted, each with its `legacy_id`, `name` and an `error` message.
