# Music: designed schema for the new app (NOT migrated yet)

These tables reuse the global `campaigns`, `sessions`, `tags` and `blobs` tables.

```sql
CREATE TABLE music_tracks (
  id TEXT PRIMARY KEY,                 -- uuid
  source TEXT NOT NULL CHECK (source IN ('youtube','upload')),
  url TEXT,                            -- youtube url (NULL for uploads)
  blob_hash TEXT REFERENCES blobs(hash),-- upload (NULL for youtube)
  title TEXT NOT NULL,
  notes TEXT NOT NULL DEFAULT '',
  loop_by_default INTEGER NOT NULL DEFAULT 1,
  duration REAL,
  added_at TEXT NOT NULL
);
CREATE TABLE music_track_campaigns (track_id TEXT REFERENCES music_tracks(id) ON DELETE CASCADE,
  campaign_id TEXT REFERENCES campaigns(id) ON DELETE CASCADE, PRIMARY KEY (track_id, campaign_id));
CREATE TABLE music_track_sessions (track_id TEXT REFERENCES music_tracks(id) ON DELETE CASCADE,
  session_id TEXT REFERENCES sessions(id) ON DELETE CASCADE, PRIMARY KEY (track_id, session_id));
CREATE TABLE music_track_tags (track_id TEXT REFERENCES music_tracks(id) ON DELETE CASCADE,
  tag_id INTEGER REFERENCES tags(id) ON DELETE CASCADE, PRIMARY KEY (track_id, tag_id));

-- live mix (cleared at startup)
CREATE TABLE music_channels (id TEXT PRIMARY KEY, track_id TEXT REFERENCES music_tracks(id) ON DELETE CASCADE,
  volume INTEGER NOT NULL DEFAULT 80, is_looping INTEGER NOT NULL DEFAULT 1,
  is_playing INTEGER NOT NULL DEFAULT 0, role TEXT NOT NULL DEFAULT 'ambient', position INTEGER NOT NULL);
CREATE TABLE music_soundbites (id TEXT PRIMARY KEY, track_id TEXT REFERENCES music_tracks(id) ON DELETE CASCADE,
  volume INTEGER NOT NULL DEFAULT 80, min_interval REAL NOT NULL DEFAULT 30, max_interval REAL NOT NULL DEFAULT 90);

CREATE TABLE music_soundscapes (id TEXT PRIMARY KEY, name TEXT NOT NULL,
  campaign_id TEXT REFERENCES campaigns(id) ON DELETE SET NULL,   -- nullable (fixes "(none)" bug)
  notes TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL);
CREATE TABLE music_soundscape_tags (soundscape_id TEXT REFERENCES music_soundscapes(id) ON DELETE CASCADE,
  tag_id INTEGER REFERENCES tags(id) ON DELETE CASCADE, PRIMARY KEY (soundscape_id, tag_id));
CREATE TABLE music_soundscape_channels (id TEXT PRIMARY KEY, soundscape_id TEXT REFERENCES music_soundscapes(id) ON DELETE CASCADE,
  track_id TEXT REFERENCES music_tracks(id) ON DELETE CASCADE, volume INTEGER, is_looping INTEGER, role TEXT, position INTEGER);
CREATE TABLE music_soundscape_soundbites (id TEXT PRIMARY KEY, soundscape_id TEXT REFERENCES music_soundscapes(id) ON DELETE CASCADE,
  track_id TEXT REFERENCES music_tracks(id) ON DELETE CASCADE, volume INTEGER, min_interval REAL, max_interval REAL);

CREATE TABLE music_playlists (id TEXT PRIMARY KEY, name TEXT NOT NULL,
  campaign_id TEXT REFERENCES campaigns(id) ON DELETE SET NULL, created_at TEXT NOT NULL);
CREATE TABLE music_playlist_items (id TEXT PRIMARY KEY,           -- allows duplicates
  playlist_id TEXT REFERENCES music_playlists(id) ON DELETE CASCADE,
  track_id TEXT REFERENCES music_tracks(id) ON DELETE CASCADE, position INTEGER NOT NULL);

CREATE TABLE music_soundboards (id TEXT PRIMARY KEY, name TEXT NOT NULL,
  campaign_id TEXT REFERENCES campaigns(id) ON DELETE SET NULL, notes TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL);
CREATE TABLE music_soundboard_buttons (id TEXT PRIMARY KEY,       -- stable ids (upsert, not delete+insert)
  soundboard_id TEXT REFERENCES music_soundboards(id) ON DELETE CASCADE,
  track_id TEXT REFERENCES music_tracks(id) ON DELETE SET NULL,
  label TEXT, icon TEXT, color TEXT, position INTEGER NOT NULL,
  loop_by_default INTEGER NOT NULL DEFAULT 0, volume INTEGER NOT NULL DEFAULT 80,
  play_mode TEXT NOT NULL CHECK (play_mode IN ('oneshot','restart','toggle')),
  hotkey TEXT);
```

## Differences from the Python schema
- The track's single `campaign_id NOT NULL ON DELETE CASCADE` column is removed. Campaigns are linked only through the many-to-many table, so **deleting a campaign never deletes tracks**.
- Uploads are stored as blobs instead of absolute `local:` paths.
- Music tags are merged into the global `tags` table.
- Playlists can contain the same track more than once.
- Soundboard buttons keep stable ids and gain `volume` and `hotkey`.
- Soundscapes, playlists and soundboards can have no campaign.

## Legacy source
The legacy importer copies these into `legacy/music.json`, with one array per table:
- `music_campaigns`
- `music_tracks`
- `music_tags`
- `music_track_tags`
- `music_track_campaigns`
- `music_track_sessions`
- `music_channels`
- `music_soundbites`
- `music_soundscapes` and `music_soundscape_*`
- `music_playlists`
- `music_playlist_tracks`
- `music_soundboards`
- `music_soundboard_tags`
- `music_soundboard_buttons`
