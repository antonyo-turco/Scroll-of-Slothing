# Music: known issues in the Python implementation

Fix these when the music phase is built.

## Startup and process handling
1. **Blocking startup.** The playlist panel touches the lazy player proxy while it is being constructed. That starts the subprocess during app startup and can block the UI for up to 20 s.
2. **Missing dependency.** `pywebview` is not in requirements.txt. Without it, music silently does nothing.
3. **Unsynchronized writes.** The Tk thread, the SFX worker and the soundbite threads all write to the subprocess's stdin without a lock.
4. **Two "track ended" handlers.** `app._on_channel_ended` is overwritten by the playlist panel's handler.
5. **Stale state after a crash.** Live mixer rows keep `is_playing=1` if the app is killed. The cleanup only runs on Stop All.

## Data model
6. **Campaign delete destroys tracks.** Deleting a campaign deletes every track whose primary `campaign_id` is that campaign, even when the track is also linked to other campaigns.
7. **Soundscapes can't be saved without a campaign.** Saving with campaign "(none)" fails on the NOT NULL constraint.
8. **Playlist campaign can't be changed.** `update_playlist` ignores the campaign field when editing.
9. **No duplicate tracks in playlists.** The primary key `(playlist_id, track_id)` prevents the same track from appearing twice.
10. **Unstable soundboard button ids.** Every save deletes and re-inserts the buttons, so they get new ids.
11. **Uploads break when the project moves.** Uploaded files are stored as absolute `local:` paths.

## Controls and playback
12. **Volume slider lag.** The mixer volume only applies on mouse release, and keyboard changes are ignored.
13. **Duck factor has no UI.** The default factor of 0.35 can't be selected; the radio offers only 0.5 and 0.25. The `hold` parameter is ignored.
14. **No ducking for soundboard one-shots.** They bypass both the queue and ducking.
15. **Playlist channel is invisible to the mixer.** It isn't shown there and isn't ducked.

## Performance and robustness
16. **N+1 queries.** `music_list_tracks` runs 3 extra queries per track, and every DB call opens a new connection.
17. **Name-based lookups.** Combo boxes look items up by name, so duplicate campaign or track names are ambiguous.
18. **Unrestricted file access.** The loopback `/media` endpoint will serve any absolute path. The risk is low (loopback only, random port), but the new design drops that endpoint entirely.

## Status in the Rust version
| # | Status |
|---|---|
| 1–4 | Gone by design: no subprocess, no stdin IPC, one engine event path (`player-engine.md`). |
| 5 | Fixed: the live mix is cleared when the app attaches its engine (`attach_clears_stale_live_state`). |
| 6 | Fixed: deleting a campaign only unlinks shared tracks (`campaign_delete_keeps_shared_tracks`). |
| 7 | Fixed: soundscapes without a campaign save (`soundscapes_save_update_load_delete`). |
| 8 | Fixed: editing a playlist can change its campaign (`playlist_controls`). |
| 9 | Fixed: playlists hold duplicates (`playlist_advances_and_soundboard_toggles`). |
| 10 | Fixed: button ids survive a save (`playlist_advances_and_soundboard_toggles`). |
| 11 | Fixed: uploads live in the blob store (`uploads_live_in_the_blob_store`). |
| 12 | Fixed: volume applies while dragging and from the keyboard (e2e `mixer: add…`). |
| 13 | Decided: the modes are Off / Soft ×0.5 / Strong ×0.25. The unreachable 0.35 default and the ignored `hold` were dropped. |
| 14 | Fixed (the user's decision, 2026-10-10): soundboard one-shots duck the music, without queueing (`soundboard_oneshots_duck_without_queueing`). |
| 15 | Kept as in Python for now: the playlist channel is not in the mixer and is not ducked. Open question for the user (`mixer-soundbites-ducking.md`). |
| 16 | Partly fixed: one connection per call; tags and links are still read per track, which is fine at library sizes. |
| 17 | Fixed: every picker uses ids. |
| 18 | Gone: uploads are served by blob hash only (`sleuth://localhost/media/<hash>`). |
