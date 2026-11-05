# Music: soundscapes

A soundscape is a saved mix: a set of channels plus a set of soundbites.

## Data
- `music_soundscapes`: `(id, name, campaign_id, notes, created_at, pinned)`. In Python `campaign_id` was `NOT NULL` with cascade; here it is nullable and unused.
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
**Header** (Python)
- "⏹ Stop Music".
- Campaign filter: All, or a single campaign.
- ⟳ Refresh.

**Soundscape card**
- ★ Pin toggle (new, 2026-10-18). Pinned soundscapes are highlighted and listed first, above a divider; the rest follow by name.
- Name and channel count.
- "▶ Load".
- "▼ Preview" expands a summary:
  - for each channel: role, title, volume, and ↻ if it loops;
  - for each soundbite: 🔔 and its interval;
  - the notes.
- ✕ Delete.
- Coloured tag pills.

## In this app (`app/src/routes/music/Soundscapes.svelte`)
- Header: ⟳ Refresh. There is no campaign filter (campaigns were removed) and no Stop Music button here; the global stop is in the Mixer.
- Each card has the ★ pin, name and channel count, ▶ Load, the expandable summary, **Export .dmix…** (new) and ✕ Delete (with confirmation). The loaded soundscape's card is highlighted.
- Save and Update are in the Mixer ("💾 Save as New", "✓ Update “name”").
- MCP: `music_soundscapes_list`, `music_soundscape_save`, `music_play_soundscape`, `music_soundscape_delete`, `music_soundscape_set_pinned`.
