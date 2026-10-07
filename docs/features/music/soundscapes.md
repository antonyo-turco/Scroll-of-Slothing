# Music: soundscapes (spec, not implemented)

A soundscape is a saved mix: a set of channels plus a set of soundbites.

## Data
- `music_soundscapes`: `(id, name, campaign_id NOT NULL cascade, notes, created_at)`.
- Linked tables:
  - `music_soundscape_tags`;
  - `music_soundscape_channels`: `(track_id, volume, is_looping, role)`;
  - `music_soundscape_soundbites`: `(track_id, volume, min, max)`.

## Operations
- **Save as new**: copies the current live mix.
- **Update**: `music_update_soundscape` overwrites the saved soundscape in place with the current live mix.
- **Load**: `music_load_soundscape`. Steps:
  1. Run `stop_all`.
  2. Wipe the live channels and soundbites.
  3. Re-insert the soundscape's channels and soundbites with `is_playing=1`.
  4. Play every channel.
  5. Start the soundbites.
  6. Switch to the Mixer.

  Only one soundscape is active at a time.
- **Delete** (with confirmation).

## View (Python `ui/dm_music_soundscape_panel.py`)
**Header**
- "⏹ Stop Music".
- Campaign filter: All, or a single campaign.
- ⟳ Refresh.

**Soundscape card**
- Name and channel count.
- "▶ Load".
- "▼ Preview" expands a summary:
  - for each channel: role, title, volume, and ↻ if it loops;
  - for each soundbite: 🔔 and its interval;
  - the notes.
- ✕ Delete.
- Coloured tag pills.
