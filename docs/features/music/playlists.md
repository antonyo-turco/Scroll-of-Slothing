# Music: playlists

## Data
- `music_playlists`: `(id, name, campaign_id, created_at)`.
- `music_playlist_tracks`: `(playlist_id, track_id, position)`, with primary key `(playlist_id, track_id)`. Because of that key, **a track can appear only once per playlist**. The new design should allow duplicates by using an id primary key.

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
- Python had two handlers that both advanced the playlist when a track ended. This should be consolidated into one.
