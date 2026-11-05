# Music: schema

The tables are created by `crates/store/migrations/V2__music.sql` (read it for the exact columns and defaults) and extended by V3 and V4 below. They reuse the global `campaigns`, `sessions`, `tags` and `blobs` tables. All ids are uuid `TEXT`.

| Table | Holds |
|---|---|
| `music_tracks` | `source` (`youtube` or `upload`), `url` or `blob_hash`, title, notes, `loop_by_default`, `duration`, `added_at`, `legacy_id` |
| `music_track_tags` | track ↔ global tag |
| `music_track_campaigns`, `music_track_sessions` | links kept from the legacy import; unused since campaigns were removed |
| `music_channels`, `music_soundbites` | the live mix (cleared when the engine attaches at startup); soundbites have `min_interval`/`max_interval` (default 30/90 s) |
| `music_soundscapes` (+ `_tags`, `_channels`, `_soundbites`) | saved mixes; `campaign_id` is nullable and unused |
| `music_playlists`, `music_playlist_items` | playlists; items have their own id, so a track can appear more than once |
| `music_soundboards`, `music_soundboard_buttons` | boards and buttons with stable ids, `play_mode` (`oneshot`, `restart`, `toggle`), `volume`, `hotkey`, label, icon, colour |

## Added later (`V3__music_favorites.sql`, 2026-10-18)
- `music_tracks.favorite INTEGER NOT NULL DEFAULT 0`: the ★ favourite flag (`music_track_set_favorite`).
- `music_soundscapes.pinned INTEGER NOT NULL DEFAULT 0`: pinned soundscapes are listed first (`music_soundscape_set_pinned`).
- Both are kept when a track is edited or a soundscape's mix is overwritten, because the saves use `UPDATE` and never touch these columns. They are not part of `.dmix` files.

## Track category (`V4__music_categories.sql`, 2026-10-18)
- `music_tracks.category TEXT NOT NULL DEFAULT 'music' CHECK (category IN ('music','ambient','soundbite'))`.
- Backfill of existing tracks, strongest evidence first:
  1. **soundbite**: used as a soundbite in a soundscape or the live mix, a title starting with or containing "SFX", or a one-shot of 120 s or less;
  2. **ambient**: used more often as an `ambient` channel than as a `music` channel; or never used in a mix and an "ambience/ambient/atmosphere/soundscape/sounds/dripping" title without "music" or "theme";
  3. everything else **music**.
- New and imported tracks with no category get `sloth_core::music::guess_category` (title keywords, loop flag, length); the store fills it in, so the importers and `.dmix` files without one are covered. `.dmix` exports now carry `category`.

## Differences from the Python schema
- The track's single `campaign_id NOT NULL ON DELETE CASCADE` column is removed. Campaigns are linked only through the many-to-many table, so **deleting a campaign never deletes tracks**.
- Uploads are stored as blobs instead of absolute `local:` paths.
- Music tags are merged into the global `tags` table.
- Playlists can contain the same track more than once.
- Soundboard buttons keep stable ids and gain `volume` and `hotkey`.
- Soundscapes, playlists and soundboards can have no campaign.

## Legacy source
The legacy importer copies these into `legacy/music.json`, with one array per table:
- `music_campaigns`
- `music_tracks`
- `music_tags`
- `music_track_tags`
- `music_track_campaigns`
- `music_track_sessions`
- `music_channels`
- `music_soundbites`
- `music_soundscapes` and `music_soundscape_*`
- `music_playlists`
- `music_playlist_tracks`
- `music_soundboards`
- `music_soundboard_tags`
- `music_soundboard_buttons`
