# DM Music: overview

> **Status:** implemented at the user's request (2026-10), awaiting alpha testing. The design below is what was built.
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
The legacy importer writes every `music_*` row from the old database to `legacy/music.json`. The future phase imports it from there.
