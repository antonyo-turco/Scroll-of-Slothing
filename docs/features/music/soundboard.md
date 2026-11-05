# Music: soundboard

## Data
**`music_soundboards`**
- Columns: `id`, `name`, `campaign_id`, `notes`, `created_at`.
- Tags live in `music_soundboard_tags`. There is no UI for them.

**`music_soundboard_buttons`**
- Columns: `id`, `soundboard_id`, `track_id`, `label`, `icon` (emoji text), `color`, `position`, `loop_by_default` (default 0), `play_mode`.
- `play_mode` is one of `oneshot`, `restart`, `toggle`.

**Saving** (`music_set_soundboard_buttons`) deletes every button on the board and inserts them again. This regenerated the button ids every time in Python. This app upserts, so ids stay stable.

## View (Python `ui/dm_music_soundboard_panel.py`)
**Header**
- Campaign filter: All, or one campaign.
- Board select.
- New Board: a dialog asking for a name and a campaign (required).
- Delete Board (asks for confirmation).
- Edit.
- "⏹ Stop Music".

**Play grid**
- 4 columns (`COLS=4`) of 80 px tiles (`BTN_SIZE=80`).
- Each tile shows an optional emoji icon, the label, and the button colour.
- A toggle button that is currently playing gets a green ring (`#00cc66`).

**Play modes**
| Mode | Edit-list symbol | What it does |
|---|---|---|
| `oneshot` | 1× | `play_oneshot` at volume 80. Does not go through the SFX queue and does not duck the music (Python). **Rust:** it ducks the music like a soundbite, without queueing; overlapping presses keep the music ducked until the last one ends. |
| `restart` | ↺→ | Stops, then calls `play_track` on the channel whose id is the button id. |
| `toggle` | ↔ | Starts or stops the channel whose id is the button id. |

Python: volume is fixed at 80, with no per-button volume.

**Edit mode**
- A table with columns Pos, Icon, Label, Track, Mode, plus ✕ / ↑ / ↓ controls.
- A form with:
  - Label;
  - Icon (text);
  - Colour: 6 swatches from `BUTTON_COLORS`;
  - Track: only tracks in the board's campaign;
  - Mode;
  - Loop;
  - "Apply to Selected Row".
- "+ Add Button".
- Save (rejects rows that have no track) and Cancel.

**Hotkeys:** none in Python.

## In this app (`app/src/routes/music/Soundboard.svelte`)
- Header: board select, New Board (name only), Delete Board (with confirmation), Edit. No campaign filter and no Stop Music button.
- The play grid is as in Python: 4 columns of 80 px tiles, the green ring on a playing toggle, plus the playback status indicator on restart and toggle tiles.
- **Per-button volume** (new) and a **Hotkey** field per button (new). Without any hotkeys set on the board, keys 1–9 press the first nine buttons by position; once any button has a hotkey, only the set hotkeys work. Keys are ignored while typing in a field.
- The editor's track picker lists every track (no campaign).
- `oneshot` ducks the music without queueing (see the table above).
- MCP: `music_soundboards_list`, `music_soundboard_press`.
