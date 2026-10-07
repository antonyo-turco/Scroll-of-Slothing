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
