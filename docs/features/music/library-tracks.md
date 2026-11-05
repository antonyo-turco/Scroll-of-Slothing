# Music: library and tracks

Campaigns and sessions were removed at the user's request on 2026-10-11 (see `overview.md`, "Intentional changes"): tracks have no campaign or session fields, filters or pickers.

## Track
| Field | Notes |
|---|---|
| id | uuid |
| source | `youtube` (with `url`) or `upload` (with `blob_hash`: the file is stored in the blob store) |
| title, notes | |
| loop_by_default | default true |
| duration | seconds, optional |
| category | `music`, `ambient` or `soundbite` |
| favorite | ★ |
| tags | global tags, many-to-many |
| added_at | |

## Metadata
- **YouTube** (`music_fetch_youtube_meta`): the title from `https://www.youtube.com/oembed?url=…&format=json`, the duration scraped from the watch page (`"lengthSeconds":"N"`). Best effort, through the system `curl`, with a 6 s timeout; no API key.
- **Uploaded files** (`music_probe_file`): the title comes from the file name. The duration is not read from the file; the engine reports it the first time the track plays, and it is saved then.
- Deleting a track does not delete its blob. There is no blob garbage collection yet, so the file stays in `blobs/`.

## Library view (`app/src/routes/music/Library.svelte`)
**Search and filters**
- Live search on the title, and tag chips under it (several combine with AND).
- A filter bar: **Category** (All, Music, Ambient, Soundbites), **★ Favourites**, **Length** (any, under 1 min, 1–5, 5–15, over 15 min; unknown lengths are hidden while it is on), **Loop** (either, loops only, plays once only). "Clear filters" resets everything; the status bar shows "N tracks of M" while filtering.
- Server side, `TrackQuery` has `favorites_only`, `looping`, `min_duration` and `max_duration` (seconds, inclusive).

**Track table**
- Rows share the asset Library's list look (`.list-row` in `app.css`): 48 px rows, a 40 px thumbnail (the YouTube thumbnail from `i.ytimg.com`, or the category icon), the title ("uploaded file" under uploads), a category pill, tags, length (M:SS or H:MM:SS), loop, added date, row actions (▶ preview, ✎ edit, ✕ delete with confirmation) and the ★ favourite.
- Sorted newest first. The Title, Length, Loop and Added headers sort; a second click reverses; unknown lengths always sort last. It is a `role="table"` grid for screen readers.

**Category (new, 2026-10-18).** Every track is `music`, `ambient` or `soundbite` (see `data-model.md` for the backfill).
- It decides where **Add to mix** puts the track: music and ambient tracks become a channel with that role; soundbites join the **soundbite list** (random intervals, 30–90 s, volume 80). `music_channel_add` without a role uses the category too (soundbite → `sfx`).
- The track dialog has a Category switch: **Auto** (new tracks only: the server guesses from the title, loop flag and length), Music, Ambient, Soundbite. Choosing Soundbite turns *Loop by default* off.

**Rows (2026-10-18)** use the same look as the asset Library's list view (`.list-row` in `app.css`): 48 px rows, a 40 px thumbnail, the title (and "uploaded file" for uploads), a category pill, tags, length, loop, added date, the actions and the ★ favourite last. YouTube tracks show the video's thumbnail (`i.ytimg.com/vi/<id>/mqdefault.jpg`, already allowed by the CSP); uploads and unrecognised URLs show the category's icon. It is a `role="table"` grid, so screen readers still read rows and sortable column headers.

**Suggest tags… (2026-10-18).** `sloth_core::music::suggest_tags` reads tags from the title: places (swamp, cave, dungeon, temple, sewer…), scenes and moods (combat, exploration, peaceful, mysterious, dark…), sounds and creatures (water, insects, rats, undead…) and sources (Darkest Dungeon, Witcher, Zelda, Hollow Knight). Keywords match word starts ("rat " and "rats" are whole words, so "rattling" is not a rat), and a source's name is removed before the other rules run, so "Darkest Dungeon" alone doesn't mean "dark" or "dungeon". The dialog lists the tags each track would gain; existing tags are kept and nothing changes until **Add tags**. Also `sloth music tags [--apply]` and the `music_tags_suggest` / `music_tags_apply` MCP tools.

**Filters (new, 2026-10-18)**, in a bar under the search:
- **Category**: All, Music, Ambient, Soundbites.
- **★ Favourites**: only favourite tracks.
- **Length**: any, under 1 min, 1–5 min, 5–15 min, over 15 min. Tracks whose length isn't known yet are hidden while a length filter is on.
- **Loop**: loop or once, loops only, plays once only.
- Tags (AND) and the title search as before. "Clear filters" resets all of them; the status bar shows "N tracks of M" while filtering.
- Server side: `TrackQuery` gains `favorites_only`, `looping`, `min_duration` and `max_duration` (seconds, inclusive).

**Actions**
- Double-click a row or its ▶: **preview** 10 s at volume 80 in a single preview slot.
- Toolbar: **Add to Mix** (by category, see above; starts playing and flashes "Added!" for 1.2 s), **▶ Preview** and, while previewing, **Stop preview**.
- Status bar: "N track(s)", or "Loading preview…" / "Previewing…".
- Leaving the Library view stops the preview.

## Track dialog (`TrackDialog.svelte`)
- **Source:** YouTube or Upload file.
  - YouTube: a URL field. On blur, metadata is fetched ("Fetching…"); the title is filled in only if empty, and the duration if found.
  - Upload: "Choose file" filtered to the media extensions; the title is filled from the file name.
- **Category:** Auto (new tracks only), Music, Ambient, Soundbite. Choosing Soundbite turns *Loop by default* off.
- Title, Loop by default (on), Notes, Tags (chips).
- Editing an uploaded track without picking a new file keeps the existing file.

## Python differences (reference: `ui/dm_music_library_panel.py`, `ui/dm_music_track_dialog.py`)
- Python had a 180 px sidebar with campaign, session and tag lists; it is replaced by tag chips and the filter bar.
- Python required at least one campaign per track, and stored uploads as absolute `local:<path>` URLs under `db/music_uploads/`, which broke when the folder moved.
- Python read upload durations with mutagen; here the engine reports them.
- Python's right-click menu (Preview, Add to Mix, Edit, Delete) became row buttons. Its "⏹ Stop Music" global stop is in the Mixer.
- Music tags had their own table (with unused `color` and `category` columns); they are merged into the global `tags` table.
