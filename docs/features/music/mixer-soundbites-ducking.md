# Music: mixer, soundbites, ducking

## Live mix state
The live mix is persisted in the database.

**`music_channels`**: one row per channel.
| Column | Values |
|---|---|
| id | |
| track_id | |
| volume | default 80 |
| is_looping | |
| is_playing | |
| role | `music`, `ambient` (default) or `sfx` |

The `sfx` role forces looping off when the channel is inserted.

**`music_soundbites`**: one row per soundbite.
| Column | Values |
|---|---|
| id | |
| track_id | |
| volume | default 80 |
| min_interval | default 30 s |
| max_interval | default 90 s |

If min is larger than max, the two values are swapped.

## Mixer view (Python `ui/dm_music_mixer_panel.py`)

### Toolbar
- Title: "Active Mix — <soundscape> (n channels)".
- **"🔉 Duck under SFX"** radio: Off (default), Soft (×0.5), Strong (×0.25). The choice is not persisted.
- ▶ All, ⏸ All, ⏹ Stop All. ⏸ All also stops the soundbite timers.
- "✓ Update “name”": only shown while a soundscape is loaded. Flashes "✓ Updated!" for 1.3 s.
- 💾 Save as New.
- 📹 Toggle Video.
- An error banner that hides itself after 6 s.

### Channel card
- The track title, truncated to 42 characters.
- A role select: music, ambient or sfx. This is metadata only and does not change playback.
- ✕ to remove the channel.
- A volume slider, 0–100. **It applies only on mouse release, and changes from the keyboard are ignored** (a bug to fix; the Rust slider applies live, keyboard included).
- A ▶/⏸ toggle and a ↻ loop toggle.
- Every change is written to the database.

### Soundbites section
- "+ Soundbite" opens a dialog: pick a track from the active campaign, "Every" min/max (1–3600 s, default 30–90), and volume (80).
- Each soundbite row has:
  - ▶ Play, which goes through the SFX queue;
  - ✕ to remove it;
  - min and max spinboxes;
  - a volume slider.

  Any edit restarts all the soundbite timers.

### Save Soundscape dialog
- Fields: Name (required), Campaign (defaults to the active one; "(none)" is allowed), Notes, and tag checkboxes in a 4-column grid.
- **Bug:** soundscapes have `campaign_id NOT NULL`, so saving with "(none)" fails.

## Soundbite scheduler (Python `dm_music_player.py:304-335`)
- Each soundbite loops: wait `uniform(min, max)` seconds, then call `play_sfx`.
  - Clamping: `min ≥ 1` and `max ≥ min`.
- One shared cancel event stops all the soundbite loops at once.

## SFX queue
- Clips play **one at a time**. The queue holds at most 12 clips, and extra requests are dropped.
- The worker waits for `oneshot_ended`. If that never arrives, it gives up after the clip's duration + 1 s, or after 30 s if the duration is unknown.
- Each clip gets an id of the form `os{n}`.

## Ducking
- When a queued clip starts, every channel is set to `base_volume × factor`.
- Volumes are restored only once the queue is empty.
- While ducked, any `set_volume` or `play_track` is sent at the ducked value. Python changed volume instantly; the Rust version should ramp, about 150 ms through a Web Audio gain node.
- **Soundboard one-shots do not go through the queue and do not duck.**
- The player's default duck factor is 0.35, but no UI option selects it.

## Global Stop (`app.py:498-514`)
1. Calls `player.stop_all`, which stops the soundbites, the preview, the ducking, and every player.
2. Clears `music_channels` and `music_soundbites` in the database.
3. Resets the playlist and soundboard UI.
4. Clears the active soundscape.

The new app should also run this clean-up **at startup**.

## Requirements for the Rust version
- Parity with everything above.
- Volume changes apply live while dragging, and from the keyboard.
- Ducking ramps smoothly.
- The playlist channel also appears in the mixer and can be ducked (optional; this is a change in behaviour, so confirm it with the user first).
