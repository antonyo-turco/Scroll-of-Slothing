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
This list is kept in sync by the test `mcp_tools_documented`, which fails if a registered tool is missing here.

| Tool | Phase | Purpose |
|---|---|---|
| `settings_get` | 1 | All settings |
| `settings_set` | 1 | Update settings; returns errors per key |
| `library_search` | 1 | Search and filter assets |
| `library_get` | 1 | Asset metadata + thumbnail |
| `library_update` | 1 | Name, aliases, size, pinned |
| `library_set_tags` | 1 | Replace an asset's tags |
| `library_set_campaigns` | 1 | Replace an asset's campaign links |
| `library_set_sessions` | 1 | Replace an asset's session links |
| `library_delete` | 1 | Delete assets |
| `library_import` | 1 | Import image files as assets |
| `library_export` | 1 | Write original/rendered/pdf to a path |
| `tags_list` | 1 | Tags with counts |
| `tags_rename` | 1 | Rename (auto-merges) |
| `tags_merge` | 1 | Merge src into dst |
| `tags_delete` | 1 | Delete a tag |
| `campaigns_list` / `campaigns_create` / `campaigns_rename` / `campaigns_delete` | 1 | Campaign CRUD |
| `sessions_list` / `sessions_create` / `sessions_rename` / `sessions_delete` | 1 | Session CRUD |
| `sessions_set_assets` | 1 | Replace a session's assets |
| `jobs_status` / `jobs_cancel` | 1 | Background jobs |
| `token_borders_list` / `token_borders_add` | 2 | Border set |
| `token_compose` | 2 | Make a token (optionally save or add to sheet) |
| `token_sheet_list` / `_add` / `_update` / `_remove` / `_clear` | 2 | Token sheet items |
| `token_sheet_layout` | 2 | Pages + placements + skipped |
| `token_sheet_export_pdf` | 2 | Export the PDF (job) |
| `cutout_create` | 3 | Lasso + outline (optionally save or add to sheet) |
| `object_sheet_list` / `_add` / `_update` / `_remove` / `_clear` / `_layout` / `_export_pdf` | 3 | Object sheet |
| `cards_parse` / `cards_validate` / `cards_preview_page` / `cards_export_pdf` | 4 | Cards |
| `decks_list` / `decks_get` / `decks_save` / `decks_delete` | 4 | Saved decks |
| `map_open` / `map_set_grid` / `map_set_style` / `map_set_filters` / `map_remove_grid` / `map_dewarp` / `map_set_print` / `map_set_region` / `map_toggle_page` / `map_layout` / `map_preview_page` / `map_export_pdf` / `map_save_asset` | 5 | Map paginator session |
| `aoe_render` / `aoe_presets_list` / `aoe_presets_save` / `aoe_presets_delete` / `aoe_sheet_add` / `aoe_sheet_list` / `aoe_sheet_remove` / `aoe_sheet_clear` / `aoe_sheet_export_pdf` / `aoe_export_template_pdf` / `aoe_export_png` | 5 | AoE |
| `ui_navigate` / `ui_open_asset` | 6 | In-app server only |

## Resources
- `sleuth://asset/{id}`: metadata JSON and the thumbnail
- `sleuth://deck/{id}`: deck markdown
- `sleuth://settings`

## Prompts
- `prepare_session_handouts`: given a campaign/session, gather its assets and build the token and object sheets.
- `tokens_from_folder`: import a folder of portraits as tokens with a chosen border and size, then build a sheet.
