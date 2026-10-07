# Data model

## Location
- The data dir defaults to `%APPDATA%\sleuth\`. Every binary accepts `--data-dir`, and tests always use a temp dir.
- Inside the data dir:
  - `sleuth.db`: SQLite, WAL mode, `foreign_keys=ON`
  - `blobs/ab/cd/<blake3-hex>.<ext>`: content-addressed files
  - `legacy/music.json`: music data preserved by the legacy importer, kept until the music phase

## Tables (migration V1)

### blobs
| col | type | notes |
|---|---|---|
| hash | TEXT PK | blake3 hex of the bytes |
| mime | TEXT | `image/png`, `image/webp`, `image/jpeg`, `application/pdf` |
| width, height | INTEGER NULL | images only |
| bytes | INTEGER | file size |
| created | TEXT | ISO-8601 UTC |

Identical bytes are stored once. A blob that no asset references can be removed by `store gc`.

### assets
| col | type | notes |
|---|---|---|
| id | INTEGER PK | |
| kind | TEXT | `token`, `cutout`, `map`. Open-ended for future kinds. |
| name | TEXT NOT NULL | |
| aliases | TEXT | newline-separated |
| size_label | TEXT | one of the size labels (see `features/token-sheet.md`). Not meaningful for maps. |
| pinned | INTEGER 0/1 | global pin |
| original_blob | TEXT FK blobs | the unprocessed source |
| rendered_blob | TEXT FK blobs | the composed token, cut-out, or gridded map |
| pdf_blob | TEXT NULL FK blobs | maps: the paginated PDF |
| thumb_blob, preview_blob | TEXT FK blobs | derivatives: thumb ≤256 px, preview ≤2048 px, lossless WebP |
| params_json | TEXT NULL | non-destructive edit parameters (see below) |
| legacy_id | INTEGER NULL UNIQUE | `creatures.id` from the old DB |
| created, updated | TEXT | |

**params_json by kind**
- **token:** `{border, bg:[r,g,b]\|null, ring_inset_pct, pan:[x,y], zoom}`
- **cutout:** `{mode, points:[[x,y]...], outline_rgb, outline_px}`
- **map:** `{grid:{mode, ...}, style, remove_grid, filters:[...], dewarp_quad, region, page_offset, excluded_pages, print:{...}}`

### tags / asset_tags
- `tags(id INTEGER PK, name TEXT UNIQUE COLLATE NOCASE)`
- `asset_tags(asset_id FK CASCADE, tag_id FK CASCADE, PK(asset_id, tag_id))`

### campaigns / sessions / links
- `campaigns(id TEXT PK uuid, name, created)`
- `sessions(id TEXT PK uuid, campaign_id FK CASCADE, name, notes, created)`
- `asset_campaigns(asset_id, campaign_id)` and `asset_sessions(asset_id, session_id)`. Both cascade on either side.
- **Campaign filter semantics:** an asset matches a campaign if it is linked directly **or** through any session of that campaign.

### aoe_presets
`(name TEXT PK, config_json TEXT, created)`. The built-in presets live in code and are never stored.

### card_decks
`(id INTEGER PK, name, markdown TEXT, settings_json, created, updated)`

### workspaces
`(key TEXT PK, json TEXT, updated)`. Keys:
- `token_sheet`
- `object_sheet`
- `cards.editor`
- `aoe.sheet`
- `maps.session`
- `ui.last_route`

### settings
`(key TEXT PK, json TEXT)`. Typed defaults live in Rust (`service::settings`). For keys and defaults, see `features/settings.md`.

### change_log
`(seq INTEGER PK AUTOINCREMENT, entity TEXT, entity_id TEXT, op TEXT, at TEXT)`. It is trimmed to the last 10k rows.

### assets_fts
An FTS5 virtual table over `name`, `aliases` and `tags`. It is kept in sync by the service layer, not by triggers, because tags are stored in another table.

## Designed but not migrated: music
See `features/music/data-model.md`.

## Migration policy
- Migrations are `crates/store/migrations/V{n}__{name}.sql` files, embedded at compile time and applied in order by `crates/store/src/migrate.rs`, which tracks progress in `PRAGMA user_version`. Each migration runs in one transaction together with its version bump.
- Never edit a migration that has already been applied. Add a new one instead.
