# DM Music: overview

> **Status:** implemented at the user's request (2026-10). Service, engine and UI are tested (checklist below); awaiting the user's alpha test. The design below is what was built.
>
> **Where it lives**
> - Store: `crates/store/src/music.rs`, migration `V2__music.sql`.
> - Service: `crates/service/src/music.rs`, which owns the live state, the soundbite timers and the SFX queue.
> - Engine: `app/src/lib/music-engine.ts`, mounted by `MusicEngine.svelte`. The UI is in `app/src/routes/music/`.
> - Tauri: the `music_*` commands. `sleuth://localhost/media/<hash>` streams uploads with Range support.
> - `.dmix` files are associated with the app. Opening one imports it, loads the mix and plays it, even when the app is already running (single instance).
> - MCP: the `music_*` tools, listed in `docs/mcp-tools.md`.

The Python app has a DM music system for live sessions:
- a **library** of YouTube links and uploaded audio files, tagged and assigned to campaigns and sessions;
- a live **mixer** of looping channels, plus randomly timed **soundbites** that can duck the music;
- saved mixes called **soundscapes**;
- **playlists** that auto-advance;
- **soundboards** of one-shot and toggle buttons.

## Files in this folder
| File | Covers |
|---|---|
| `player-engine.md` | How audio is played; Python's process/IPC architecture and what replaces it |
| `library-tracks.md` | Tracks, tags, the add/edit dialog, metadata fetching, uploads |
| `mixer-soundbites-ducking.md` | Live channels, soundbite scheduler, SFX queue, ducking |
| `soundscapes.md` | Saving and loading mixes |
| `playlists.md` | Playlist editing and playback |
| `soundboard.md` | Boards, buttons, play modes |
| `data-model.md` | The designed SQLite schema for the new app |
| `known-issues.md` | Python bugs and pain points to fix when this is built |

## Python source (reference repo, branch `dev`)
- `ui/dm_music_*.py`: the panels, the player and the server
- `db/creature_db.py:647-1546`: tables and CRUD
- `app.py:398-514`: wiring, global stop, YouTube error mapping

## Rust/Tauri design
**Playback engine**
- Run it **inside the main Tauri webview**, as a hidden, always-mounted `<MusicEngine>` Svelte component.
- It holds the YouTube IFrame players and `<audio>` elements.
- Use a Web Audio `GainNode` per channel, so ducking and fades can ramp smoothly.

**What this removes**
- the Python subprocess
- the stdin/stdout JSON IPC
- the loopback HTTP server
- the off-screen-window hack

YouTube also needs a real page origin to embed videos. The Tauri app origin provides that, and it is the reason the Python app ran an HTTP server.

**Local files**
- Serve them with a `sleuth://media/<blob>` protocol that supports Range requests.
- Store uploads in the blob store, not at absolute paths.

**Control and state**
- A `service::music` module owns the state: channels, soundbites, queue, ducking.
- It sends commands to the engine through Tauri events, and receives `ended`, `error` and `duration` events back.
- The soundbite timers run as tokio tasks.
- MCP tools such as `music_play_soundscape`, `music_stop_all` and `music_soundboard_press` make it drivable by an LLM.

**Startup:** the live mixer state is cleared on launch, fixing the stale `is_playing` rows after a crash.

## Data preservation
The legacy importer writes every `music_*` row from the old database to `legacy/music.json`. "Import legacy music" (`music_import_legacy`) imports it from there; it is idempotent.

## Intentional changes
- No subprocess, IPC or loopback HTTP server: the engine runs in the main webview (see `player-engine.md`).
- Volume changes apply while dragging and from the keyboard, and ramp over 150 ms.
- Uploads are stored in the blob store; `.dmix` files carry them embedded.
- Every slot shows whether it is loading, playing or paused (`player-engine.md`, "Playback status"). Python showed only what it had asked for.
- The known Python bugs are fixed as listed in `known-issues.md`, "Status in the Rust version".

## Parity checklist (Python `ui/dm_music_*.py`, `db/creature_db.py:647-1546`, `app.py:398-514`)
Service tests are in `crates/service/src/music_tests.rs`, engine tests in `app/src/lib/music-engine.test.ts`, UI smoke tests in `app/e2e/music.spec.ts`.
- [x] Tracks: YouTube and upload sources, validation, campaigns/sessions/tags, edit keeps the upload (`dm_music_track_dialog.py`; `track_validation`, `uploads_live_in_the_blob_store`)
- [x] Library search, tag and campaign filters; 10 s preview in a single slot (`dm_music_library_panel.py`; `preview_slot_and_video_toggle`, e2e `library filters…`)
- [x] Live channels: volume 0–100, loop, play/pause, role, sfx forces loop off (`dm_music_mixer_panel.py`; `channel_rules_volume_role_and_loop`)
- [x] ▶ All / ⏸ All (also stops soundbite timers) / ⏹ Stop All keeps duck mode and master (`app.py:498-514`; `play_all_pause_all_and_stop_all`)
- [x] Soundbite scheduler: uniform random interval, clamped 1–3600 s, swapped when min > max (`dm_music_player.py:304-335`; `soundbite_timers_fire_and_stop_with_pause_all`)
- [x] SFX queue: one clip at a time, at most 12 waiting, cleared by a global stop (`sfx_queue_is_capped_and_cleared_by_stop_all`)
- [x] Ducking: Off / Soft ×0.5 / Strong ×0.25 while a soundbite plays, restored when the queue empties (`soundbite_ducks_the_music_until_it_ends`, `duck_off_leaves_the_volume_alone`)
- [x] Soundscapes: save (campaign optional), update in place, load replaces the mix and starts soundbites, delete (`dm_music_soundscape_panel.py`; `soundscapes_save_update_load_delete`)
- [x] Playlists: duplicates, play from index, prev/next/pause/resume/stop, auto-advance, finish at the end (`dm_music_playlist_panel.py`; `playlist_controls`, `playlist_advances_and_soundboard_toggles`, e2e `playlists…`)
- [x] Soundboards: oneshot / restart / toggle, stable button ids (`dm_music_soundboard_panel.py`; `soundboard_modes`, `playlist_advances_and_soundboard_toggles`)
- [x] Engine events: ended, error codes (2, 5, 100, 101/150), discovered duration, webview reload (`app.py:282`; `engine_events_update_the_mix`, vitest)
- [x] Video toggle; YouTube embeds play inside the WebView2 window (verified 2026-10-10, `player-engine.md`)
- [x] The standalone MCP server drives the app's engine through queued requests (`standalone_process_queues_for_the_app`)
- [x] `.dmix` export/import with embedded uploads, master and ducking; rejects other files and newer versions (`dmix_embeds_uploads_and_carries_settings`)
- [x] Legacy import from `legacy/music.json`: tracks, tags, campaigns, soundscapes, playlists, soundboards; missing files reported (`legacy_music_import_is_idempotent`)
- [ ] Alpha test by the user in the desktop app

