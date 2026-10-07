# Music: library and tracks (spec, not implemented)

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
| Title | "⬆" prefix for uploads |
| Tags | |
| Length | M:SS or H:MM:SS |
| Loop | yes / no |
| Campaign(s) | |

Rows are striped. The list is sorted by added_at, newest first.

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
