# Campaigns and sessions

> **Removed from the app (2026-10-11).** The user found them unhelpful because most material is reused across sessions; tags and the ★ Pinned star organise the library instead. The screen, the top-bar picker, all campaign/session filters and links, the Tauri commands and the MCP/CLI tools are gone. The tables (`campaigns`, `sessions`, `asset_campaigns`, `asset_sessions`, music `campaign_id` columns) are kept, unused, so nothing was deleted and the legacy importer still copies them. The spec below is kept for reference.

In Python, campaigns lived in `music_campaigns`, but assets used them too. In the new app they are generic: `campaigns` and `sessions` tables, linked to assets many-to-many. When music is built later, it will reference the same campaigns.

## Behaviour
**Campaigns**
- Create, rename and delete.
- Deleting a campaign cascades to its sessions and to the links. **Assets are never deleted.** In Python, deleting a campaign cascaded into music tracks.

**Sessions**
- Belong to a campaign.
- Create, rename and delete. Deleting a session asks for confirmation and states that "Assets are not deleted".

**Active campaign**
- Python had an "active campaign" (Set Active / Deselect, with a banner) that pre-filtered the music views.
- Here it becomes a global **campaign context** in the top bar. It pre-selects the campaign filter in the Library and will scope the music views later.
- It is stored in the `workspaces` table under the key `ui.campaign`.

**Linking assets**
- A session's assets can be added, removed, or replaced as a whole set.
- Assets can also be linked directly to a campaign.

## UI: the Campaigns view
- A list of campaigns. Each one expands to show its sessions.
- Each session shows its notes and asset count, and links to "Show in library".
- Inline create and rename. Delete asks for confirmation.

## MCP
- `campaigns_list`
- `campaigns_create {name}`, `campaigns_rename`, `campaigns_delete`
- `sessions_list {campaign_id?}`
- `sessions_create {campaign_id, name, notes?}`, `sessions_update {id, name?, notes?}`, `sessions_delete`
- `sessions_set_assets {session_id, asset_ids}`

## Parity checklist
- [x] Campaign CRUD (`ui/dm_music_campaigns_panel.py`, `music_*_campaign`)
- [x] Session CRUD with notes (`create_session`, `rename_session`, `delete_session`)
- [x] Session ↔ asset links (`add_assets_to_session`, `remove_asset_from_session`, `set_session_assets`)
- [x] Campaign ↔ asset links (`add_assets_to_campaign`, `set_asset_campaigns`)
- [x] Active campaign context (top bar picker, persisted in `ui.campaign`, default Library filter)
