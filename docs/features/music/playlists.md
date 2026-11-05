# Music: playlists

## Data
- `music_playlists`: `(id, name, campaign_id, created_at)`.
- `music_playlist_tracks`: `(playlist_id, track_id, position)`, with primary key `(playlist_id, track_id)`. Because of that key, **a track can appear only once per playlist** in Python. This app's `music_playlist_items` has its own id primary key, so duplicates are allowed.

## View (Python `ui/dm_music_playlist_panel.py`)
**Now-playing bar**
- Shows "<playlist name> — Track i/n" and the track title.
- Buttons: ⏮ Prev, ⏸/▶, ⏭ Next, ⏹ Stop.

**Header**
- "⏹ Stop Music".
- Campaign filter.
- "+ New Playlist".

**Playlist card**
- Expand toggle (▸/▾) showing a numbered track list, with "◄" marking the current track.
- ▶ Play, Edit, ✕ Delete.

**Create/Edit dialog** (700×500, resizable)
- Name.
- Campaign. Changing it during an edit has no effect, because `update_playlist` ignores it (bug).
- Two multi-select lists, "All Tracks" and "Playlist Tracks (ordered)".
- Buttons: Add →, ← Remove, ↑ Up, ↓ Down.

## Playback
- Everything plays on one fixed channel, id `"playlist"`, at **volume 80 with looping off**.
- When a track ends (`channel_ended`), the next track starts automatically. The playlist stops after the last track.
- Not available: volume control, shuffle, repeat, crossfade.
- The playlist channel does not appear in the mixer and is not ducked.
- Python had two handlers that both advanced the playlist when a track ended. This app has one, in the service.

## In this app (`app/src/routes/music/Playlists.svelte`)
- The view, dialog and playback are as above, without the campaign filter and field, and without a Stop Music button (the global stop is in the Mixer).
- The "Now playing" card has ⏮ Prev, ⏸/▶, ⏭ Next and ⏹ Stop, plus the playback status indicator.
- Delete asks for confirmation.
- Playback is unchanged from Python: one `playlist` slot at volume 80, no loop, not in the mixer and not ducked (open question, `known-issues.md` item 15).
- MCP: `music_playlists_list`, `music_playlist_save`, `music_playlist_play`, `music_playlist_control`.
