# Music: library and tracks

> The sections below describe the Python app. In this app campaigns and sessions were removed at the user's request on 2026-10-11 (see `overview.md`, "Intentional changes"): no campaign/session columns, filters or pickers, and no campaign is required.

## Track
| Field | Notes |
|---|---|
| id | uuid |
| url | YouTube URL, or `local:<path>` for an uploaded file |
| title | |
| notes | |
| loop_by_default | default true |
| source | `youtube` or `upload` |
| duration | seconds, optional |
| added_at | |
| campaigns | many-to-many, at least one required |
| sessions | many-to-many |
| tags | many-to-many |

## Metadata
**YouTube** (`music_fetch_yt_meta`, `creature_db.py:1055`)
- Title comes from `https://www.youtube.com/oembed?url=…&format=json`.
- Duration is scraped from the watch page: `"lengthSeconds":"N"`, sent with User-Agent Mozilla/5.0 and a 6 s timeout. No API key is needed.

**Uploaded files**
- Duration is read with mutagen (`File(path).info.length`). If that fails, duration stays empty.
- The file is copied to `db/music_uploads/<uuid><ext>` **only when the dialog is saved**.
- Python stored an absolute path, which broke if the project folder moved. The new app stores uploads in the blob store instead.

**Deleting a track** also deletes its uploaded file, but only if that file is inside the uploads folder.

## Library view (Python `ui/dm_music_library_panel.py`)
**Sidebar (180 px)**
- Campaigns: "All", one entry per campaign, and "+ Campaign" (inline form).
- Sessions: "All" plus one entry per session. Shows a hint when no campaign is selected.
- Tags: a checkbox list with a live filter (✕ clears it), a "Clear" button, and "+ Tag" (inline form).
- **Multiple selected tags are combined with AND.**

**Main area**
- Live search on title, re-queried on every keystroke.
- "+ Add Track". This requires at least one campaign to exist.

**Track table**
| Column | Shows |
|---|---|
| ★ | Favourite toggle (new) |
| Title | "⬆" prefix for uploads |
| Tags | |
| Length | M:SS or H:MM:SS |
| Loop | yes / no |
| Added | date (new) |
| Campaign(s) | (removed with campaigns, 2026-10-11) |

Rows are striped. The list is sorted by added_at, newest first. **New:** the Title, Length, Loop and Added headers sort the table; a second click reverses the direction, and tracks of unknown length always sort last.

**Category (new, 2026-10-18).** Every track is `music`, `ambient` or `soundbite` (see `data-model.md` for the backfill).
- It decides where **Add to mix** puts the track: music and ambient tracks become a channel with that role; soundbites join the **soundbite list** (random intervals, 30–90 s, volume 80). `music_channel_add` without a role uses the category too (soundbite → `sfx`).
- The track dialog has a Category switch: **Auto** (new tracks only: the server guesses from the title, loop flag and length), Music, Ambient, Soundbite. Choosing Soundbite turns *Loop by default* off.

**Rows (2026-10-18)** use the same look as the asset Library's list view (`.list-row` in `app.css`): 48 px rows, a 40 px thumbnail, the title (and "uploaded file" for uploads), a category pill, tags, length, loop, added date, the actions and the ★ favourite last. YouTube tracks show the video's thumbnail (`i.ytimg.com/vi/<id>/mqdefault.jpg`, already allowed by the CSP); uploads and unrecognised URLs show the category's icon. It is a `role="table"` grid, so screen readers still read rows and sortable column headers.

**Suggest tags… (2026-10-18).** `sleuth_core::music::suggest_tags` reads tags from the title: places (swamp, cave, dungeon, temple, sewer…), scenes and moods (combat, exploration, peaceful, mysterious, dark…), sounds and creatures (water, insects, rats, undead…) and sources (Darkest Dungeon, Witcher, Zelda, Hollow Knight). Keywords match word starts ("rat " and "rats" are whole words, so "rattling" is not a rat), and a source's name is removed before the other rules run, so "Darkest Dungeon" alone doesn't mean "dark" or "dungeon". The dialog lists the tags each track would gain; existing tags are kept and nothing changes until **Add tags**. Also `sleuth music tags [--apply]` and the `music_tags_suggest` / `music_tags_apply` MCP tools.

**Filters (new, 2026-10-18)**, in a bar under the search:
- **Category**: All, Music, Ambient, Soundbites.
- **★ Favourites**: only favourite tracks.
- **Length**: any, under 1 min, 1–5 min, 5–15 min, over 15 min. Tracks whose length isn't known yet are hidden while a length filter is on.
- **Loop**: loop or once, loops only, plays once only.
- Tags (AND) and the title search as before. "Clear filters" resets all of them; the status bar shows "N tracks of M" while filtering.
- Server side: `TrackQuery` gains `favorites_only`, `looping`, `min_duration` and `max_duration` (seconds, inclusive).

**Actions**
- Double-click a row: **preview** 10 s at volume 80.
- Right-click menu: Preview, Add to Mix, Edit, Delete (with confirmation).
- Buttons:
  - "Add to Mix" creates a channel (volume 80, role `music`, loop = the track default), starts playing immediately, and flashes "Added!" for 1.2 s.
  - "▶ Preview".
  - "⏹ Stop Music" (global stop).
- Status bar: "N track(s)" or "Previewing…".
- Leaving the Library view stops the preview.

## Track dialog (Python `ui/dm_music_track_dialog.py`)
A modal dialog, 380 px wide. Escape cancels.

**Source:** YouTube or Upload file.
- YouTube: a URL field. When the field loses focus, metadata is fetched in the background ("Fetching…"), and the title is filled in only if it is empty.
- Upload: "Choose File…" filtered to the media extensions. The title is filled from the file name, and the duration is probed.

**Other fields**
- Title.
- Campaigns: multi-select chips, at least one required.
- Sessions: multi-select chips across all campaigns, labelled "Campaign / Session".
- Loop by default: on by default.
- Notes.
- Tags: chips.

**Buttons:** Save/Update and Cancel. Editing an uploaded track without picking a new file keeps the existing file.

## Tags (music)
- `name` is UNIQUE. There are also `color` and `category` fields, but the UI only ever sets the name.
- `music_delete_tag` exists but has no UI.
- **Decision for the future phase:** merge music tags into the global `tags` table.
