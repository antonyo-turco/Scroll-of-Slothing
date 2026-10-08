# Music: player engine

## Python architecture (for reference)

### Parent process
- `MusicPlayer()` (`ui/dm_music_player.py:361`) returns `_SubprocessPlayer` when `import webview` works. Otherwise it returns `_NullPlayer`, where every call does nothing.
- pywebview is **missing from requirements.txt**, so music silently does nothing on a fresh install.

`start()`:
1. Launches `python dm_music_server.py` (stdin/stdout pipes, text mode, line-buffered).
2. Registers an `atexit` stop handler.
3. Starts two daemon threads, `_reader` and `_sfx_worker`.
4. **Blocks the Tk thread for up to 20 s**, waiting for the `ready` event.

`stop()`:
1. Calls `terminate`, then waits 3 s.
2. Runs `taskkill /F /T /PID`, which also kills the `msedgewebview2` child processes.

Lazy start:
- The player is supposed to start on the first visit to a music tab, through a `_LazyPlayer` proxy.
- **Bug:** the playlist panel's constructor touches the proxy, so the player actually starts during app startup.

### Child process (`ui/dm_music_server.py`)
- Runs `webview.create_window("DM Music", url=http://127.0.0.1:<random port>/, 320x240, js_api=_Api())`.
- On Windows the window is **shown 4000 px off-screen** with `WS_EX_TOOLWINDOW` set, because a hidden webview stops playing audio. This is done with `FindWindowW` and has a 15 s timeout.
- `toggle_video` moves the window to (200,200) at 640×400 with a taskbar button, or hides it off-screen again.
- The HTTP server exists only because YouTube rejects embedding from a `null` origin (error 153).

### IPC commands (JSON lines, parent → child)
| action | fields |
|---|---|
| `play_track` | channel_id, url, volume (80), loop (true), show_video (never used) |
| `pause_channel` / `resume_channel` / `stop_channel` | channel_id |
| `set_volume` | channel_id, volume 0–100 (YouTube `setVolume(v)`, audio `v/100`) |
| `set_loop` | channel_id, loop |
| `stop_all` | – (destroys all players) |
| `play_oneshot` | url, volume, osid? |
| `play_preview` | url, volume, seconds (default 10). One slot only; it stops itself via a JS timer. |
| `stop_preview` | – |
| `toggle_video` | – |

### IPC events (child → parent)
- `ready`
- `channel_ended {channel_id}` (only sent when the channel is not looping)
- `channel_error {channel_id, code}`
- `oneshot_ended {osid}` (also sent on error, so the queue never stalls)
- `oneshot_duration {osid, seconds}`

### HTTP (loopback, random port)
- `GET /` serves the player page.
- `GET /media?path=<abs path>` streams a file:
  - supports `Range: bytes=a-b`, answering 206, or 416 for a bad range;
  - sends 64 KB chunks;
  - MIME types: mp3, m4a, aac, ogg/oga/opus, wav, flac, weba, webm, mp4/m4v, mov, mkv.
- `local:<path>` URLs are rewritten to `/media?path=`.

### JavaScript engine
- `players[id] = {p, loop, kind: 'yt'|'file'}`.
- YouTube player parameters: `autoplay=1, controls=0, disablekb=1, fs=0, iv_load_policy=3, modestbranding=1, rel=0, origin`.
- YouTube looping is manual: `seekTo(0)` when the video ends. File looping uses `audio.loop`.
- Video ID parsing handles `?v=`, `youtu.be/`, `/embed/`, plus a regex fallback.
- Commands that arrive before the YouTube API is ready are queued.
- No crossfade, no fades, no master volume, no seeking, no EQ.

### YouTube error codes shown to the user (`app.py:282`)
| Code | Meaning |
|---|---|
| 2 | invalid parameter / bad URL |
| 5 | HTML5 player error |
| 100 | video not found or private |
| 101, 150 | the owner does not allow embedding |

Errors appear in a banner that hides itself after 6 s.

## Requirements for the Rust version
- Feature parity with every IPC command and event listed above, as service functions.
- Per-channel volume, loop, pause and resume; a single preview slot; one-shots; a video toggle (show the YouTube player in a small floating panel).
- Local files are served from the blob store with Range support.
- No subprocess, and nothing blocks at startup.
