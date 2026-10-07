# Campaigns and sessions

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
- `sessions_create {campaign_id, name, notes?}`, `sessions_rename`, `sessions_delete`
- `sessions_set_assets {session_id, asset_ids}`

## Parity checklist
- [ ] Campaign CRUD (`ui/dm_music_campaigns_panel.py`, `music_*_campaign`)
- [ ] Session CRUD with notes (`create_session`, `rename_session`, `delete_session`)
- [ ] Session ↔ asset links (`add_assets_to_session`, `remove_asset_from_session`, `set_session_assets`)
- [ ] Campaign ↔ asset links (`add_assets_to_campaign`, `set_asset_campaigns`)
- [ ] Active campaign context
