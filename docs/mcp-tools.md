# MCP tools

Server name: `sleuth`. There are two ways to run it:
- **Standalone over stdio**, for example `claude mcp add sleuth -- sleuth-mcp --data-dir "%APPDATA%\sleuth"`. It uses the same data as the app, and the open app picks up the changes live through `change_log`.
- **Inside the running app**, over streamable HTTP at `http://127.0.0.1:<port>/mcp` with a bearer token. This is off by default; enable it in Settings → Advanced. The in-app server adds UI tools (`ui_*`).

## Conventions
- Tool names are snake_case `domain_action`, and each tool maps to exactly one service function.
- Inputs reference assets by `asset_id` or by an absolute file `path`.
- Outputs are JSON. Tools that write files return the path. Tools that produce images also attach a small preview image, so the model can see the result.
- Long operations return `{job_id}`. Poll with `jobs_status`, stop with `jobs_cancel`.
- Errors are tool errors with a readable message and a machine code (`not_found`, `invalid_input`, `cancelled`, `io`).

## Tool list
Campaign and session tools were removed at the user's request on 2026-10-11.

This list is kept in sync by the test `mcp_tools_documented`, which fails if a registered tool is missing here.

| Tool | Phase | Purpose |
|---|---|---|
| `settings_get` | 1 | All settings |
| `settings_set` | 1 | Update settings; returns errors per key |
| `library_search` | 1 | Search and filter assets |
| `library_get` | 1 | Asset metadata + thumbnail |
| `library_update` | 1 | Name, aliases, size, pinned |
| `library_set_tags` | 1 | Replace an asset's tags |
| `library_delete` | 1 | Delete assets |
| `library_import` | 1 | Import image files as assets |
| `library_export` | 1 | Write original/rendered/pdf to a path |
| `tags_list` | 1 | Tags with counts |
| `tags_rename` | 1 | Rename (auto-merges) |
| `tags_merge` | 1 | Merge src into dst |
| `tags_delete` | 1 | Delete a tag |
| `jobs_status` / `jobs_cancel` | 1 | Background jobs |
| `legacy_import` | 1 | Import from Orb of Many Functions (job; idempotent; source read-only) |
| `token_borders_list` / `token_borders_add` | 2 | Border set with each border's `shape` (`circle`/`square`/`hex`/`octagon`/`decagon`) and `borderless` flag. 17 built-ins: Roll20 gold/grey round, normal and thin; Roll20 gold square, hexagon, octagon and decagon, normal and thin; No border for each shape. Added borders get a detected shape |
| `token_filter_catalog` | 2 | Filters a token can use (9 colour/tone map filters, on the image only) and the 8 map presets built from them |
| `token_compose` | 2 | Make a token from a file or library asset, with optional `filter_preset` or `filters` pipeline (optionally save, add to sheet, write PNG); returns the image |
| `token_sheet_list` / `token_sheet_add` / `token_sheet_update` / `token_sheet_remove` / `token_sheet_clear` / `token_sheet_set_options` | 2 | Token sheet items and options |
| `token_sheet_layout` | 2 | Pages + placements + skipped |
| `token_sheet_export_pdf` | 2 | Export the PDF (job) |
| `cutout_create` | 3 | Lasso (polygon points) + outline from a file or library cut-out; optionally save, add to the object sheet, write PNG; returns the image |
| `object_sheet_list` / `object_sheet_add` / `object_sheet_update` / `object_sheet_remove` / `object_sheet_clear` / `object_sheet_set_options` / `object_sheet_layout` / `object_sheet_export_pdf` | 3 | Object sheet (flat or standee) |
| `cards_templates` / `cards_parse` / `cards_validate` / `cards_export_pdf` | 4 | Cards: format reference + templates, parse, validate (pages, overflow), export (job) |
| `decks_list` / `decks_get` / `decks_save` / `decks_delete` | 4 | Saved decks |
| `map_get` / `map_open` / `map_open_asset` / `map_update` | 5 | Map paginator document: read, open from a file/blob or library asset, merge a partial doc |
| `map_set_grid` / `map_set_style` / `map_set_filters` / `map_filter_catalog` / `map_remove_grid` | 5 | Grid sizing (4 modes + offset), overlay style, filter preset/pipeline (catalog of 14 filters, 18 presets), embedded-grid removal |
| `map_dewarp` / `map_undo_dewarp` | 5 | Perspective de-warp from a reference block, and undo |
| `map_detect_grid` / `map_fit_region` | 5 | Detect the embedded grid (square size + offset); fit the print region to the content (trim uniform borders, snap to grid lines) |
| `map_fewest_pages` | 5 | Switch to the orientation and page-grid shift with the fewest pages (fewer cuts) |
| `map_set_print` / `map_set_region` / `map_toggle_page` / `map_layout` | 5 | Print settings (format Letter, A4 or A3), region and page offset, page include/exclude, tiling + fit check |
| `map_preview` / `map_preview_page` | 5 | Working preview image; one printed page as an image |
| `map_export_pdf` / `map_save_asset` | 5 | Tiled PDF export (job); save to the library with full state (job) |
| `aoe_render` / `aoe_info` | 5 | Render a template (info + image); info only |
| `aoe_presets_list` / `aoe_presets_save` / `aoe_presets_delete` | 5 | Built-in and user presets (overwrite needs `overwrite: true`) |
| `aoe_sheet_add` / `aoe_sheet_list` / `aoe_sheet_remove` / `aoe_sheet_clear` / `aoe_sheet_set_gap` / `aoe_sheet_layout` / `aoe_sheet_export_pdf` | 5 | Multi-template sheet on Letter, A4 or A3 (MaxRects packing, skipped items reported; export is a job) |
| `aoe_export_template_pdf` / `aoe_export_png` | 5 | One template as a tiled true-scale PDF (job) or PNG |
| `ui_navigate` / `ui_open_asset` | 6 | In-app server only |
| `music_tracks_list` / `music_track_add` / `music_track_delete` / `music_track_set_favorite` / `music_tags_suggest` / `music_tags_apply` | 7 | Music library: YouTube links and uploaded audio, tagged, with a `category` (music, ambient, soundbite) and favourites. The list filters by text, tags, `category`, `favorites_only`, `looping` and `min_duration`/`max_duration` (seconds). Tag suggestions come from the titles and are only added on apply. `music_channel_add` without a role uses the track's category |
| `music_state` / `music_channel_add` / `music_channel_update` / `music_channel_remove` / `music_set_all_playing` / `music_stop_all` | 7 | Live mixer channels; `music_stop_all` is the global stop |
| `music_soundbite_add` / `music_soundbite_remove` / `music_soundbite_play` / `music_set_duck` / `music_set_master` | 7 | Randomly timed soundbites, ducking and master volume |
| `music_soundscapes_list` / `music_soundscape_save` / `music_play_soundscape` / `music_soundscape_delete` / `music_soundscape_set_pinned` | 7 | Saved mixes; pinned ones are listed first |
| `music_playlists_list` / `music_playlist_save` / `music_playlist_play` / `music_playlist_control` | 7 | Auto-advancing playlists |
| `music_soundboards_list` / `music_soundboard_press` | 7 | Soundboard buttons (one-shot, restart, toggle) |
| `music_mix_export` / `music_mix_import` / `music_import_legacy` | 7 | Portable `.dmix` files; import of the legacy music data |
| `stt_models_list` / `stt_model_download` / `stt_model_delete` | 8 | Whisper models: catalog with what is installed, hash-checked download (job), delete |
| `stt_cloud_models_list` / `transcribe_estimate` | 8 | Suggested cloud models with price per minute; time per local model and for the cloud (learned from past runs), cloud cost, and the file's duration |
| `stt_cloud_key_status` / `stt_cloud_key_test` | 8 | Whether an OpenRouter key is available (never the key itself), and a live check of it. Keys are set in the app's Settings or `sleuth stt key set`, deliberately not over MCP |
| `transcribe_file` | 8 | Transcribe an audio file, `engine` `local` (Whisper) or `cloud` (OpenRouter) (job; result: engine, model, language, duration, cost, timed segments). Options: model, language, glossary (local), corrections, beam size (local) |
| `combat_get` / `combat_next_turn` / `combat_prev_turn` / `combat_reset` | 9 | The 5etools-compatible initiative tracker: rows in turn order, round, settings; turns tick conditions down and count rounds |
| `combat_add_player` / `combat_add_creature` / `combat_update_row` / `combat_add_condition` / `combat_delete_row` | 9 | Rows: creatures by bestiary hash (count, rolled or average HP), HP or damage, initiative, notes, conditions with turns, per-creature `stats` (5etools JSON fields; null removes one); `similar` deletes every copy |
| `combat_link` / `combat_import_link` | 9 | 5etools `#combat=` links (export and open) and Bestiary `#encounter=` links rolled into the tracker; missing creatures are reported |
| `encounters_list` / `encounter_save` / `encounter_import` / `encounter_save_current` / `encounter_start` / `encounter_link` / `encounter_delete` | 9 | Saved encounters: creature lists (lines may carry 5etools `stats` changes; made here or imported from a 5etools encounter file or `#encounter=` link) and combat snapshots; starting one rolls it into the tracker |
| `encounters_export` / `encounters_import_denc` | 9 | Saved encounters to and from `.denc` files (JSON, `format: sleuth-encounters`) |
| `parties_list` / `party_save` / `party_add_to_tracker` / `party_delete` | 9 | Saved hero parties (name, species, class, level, HP, AC, initiative bonus, passive Perception, speed, spell DC, player, notes); adding them makes player rows, optionally the default party |
| `bestiary_save_creature` / `bestiary_homebrew` / `bestiary_delete_creature` | 9 | Homebrew creatures as 5etools monster JSON, kept in `<data dir>/fivetools/homebrew-bestiary.json` and merged into the bestiary (search, add, encounters) |
| `token_sheet_add_creatures` | 9 | 5etools creature tokens onto the token sheet, sized from each creature |
| `bestiary_legendary_group` | 9 | Lair actions and regional effects of a creature's legendary group (synced as `legendarygroups.json`) |
| `bestiary_status` / `bestiary_sync` / `bestiary_search` / `bestiary_get` | 9 | Local cache of the 5etools mirror's bestiary (setting `fivetools_base_url`): sync (job), search, full stat block |
| `transcript_export` | 8 | Render segments as txt, md, srt or vtt; optionally write to a path |

Music plays in the desktop app. When the standalone `sleuth-mcp` server changes music state, the change is stored, and the running app applies it to its audio engine.

## Resources
- `sleuth://asset/{id}`: metadata JSON and the thumbnail
- `sleuth://deck/{id}`: deck markdown
- `sleuth://settings`

## Prompts
- `tokens_from_folder`: import a folder of portraits as tokens with a chosen border and size, then build a sheet.
