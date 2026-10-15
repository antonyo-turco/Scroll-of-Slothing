-- Music (docs/features/music/data-model.md). Reuses campaigns, sessions, tags, blobs.

CREATE TABLE music_tracks (
    id              TEXT PRIMARY KEY,
    source          TEXT NOT NULL CHECK (source IN ('youtube', 'upload')),
    url             TEXT,
    blob_hash       TEXT REFERENCES blobs(hash),
    title           TEXT NOT NULL,
    notes           TEXT NOT NULL DEFAULT '',
    loop_by_default INTEGER NOT NULL DEFAULT 1,
    duration        REAL,
    added_at        TEXT NOT NULL,
    legacy_id       TEXT
);
CREATE TABLE music_track_campaigns (
    track_id    TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
    campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    PRIMARY KEY (track_id, campaign_id)
);
CREATE TABLE music_track_sessions (
    track_id   TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    PRIMARY KEY (track_id, session_id)
);
CREATE TABLE music_track_tags (
    track_id TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
    tag_id   INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (track_id, tag_id)
);

-- live mix (cleared at app startup)
CREATE TABLE music_channels (
    id         TEXT PRIMARY KEY,
    track_id   TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
    volume     INTEGER NOT NULL DEFAULT 80,
    is_looping INTEGER NOT NULL DEFAULT 1,
    is_playing INTEGER NOT NULL DEFAULT 0,
    role       TEXT NOT NULL DEFAULT 'ambient',
    position   INTEGER NOT NULL
);
CREATE TABLE music_soundbites (
    id           TEXT PRIMARY KEY,
    track_id     TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
    volume       INTEGER NOT NULL DEFAULT 80,
    min_interval REAL NOT NULL DEFAULT 30,
    max_interval REAL NOT NULL DEFAULT 90,
    position     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE music_soundscapes (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    campaign_id TEXT REFERENCES campaigns(id) ON DELETE SET NULL,
    notes       TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL
);
CREATE TABLE music_soundscape_tags (
    soundscape_id TEXT NOT NULL REFERENCES music_soundscapes(id) ON DELETE CASCADE,
    tag_id        INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (soundscape_id, tag_id)
);
CREATE TABLE music_soundscape_channels (
    id            TEXT PRIMARY KEY,
    soundscape_id TEXT NOT NULL REFERENCES music_soundscapes(id) ON DELETE CASCADE,
    track_id      TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
    volume        INTEGER NOT NULL DEFAULT 80,
    is_looping    INTEGER NOT NULL DEFAULT 1,
    role          TEXT NOT NULL DEFAULT 'ambient',
    position      INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE music_soundscape_soundbites (
    id            TEXT PRIMARY KEY,
    soundscape_id TEXT NOT NULL REFERENCES music_soundscapes(id) ON DELETE CASCADE,
    track_id      TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
    volume        INTEGER NOT NULL DEFAULT 80,
    min_interval  REAL NOT NULL DEFAULT 30,
    max_interval  REAL NOT NULL DEFAULT 90,
    position      INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE music_playlists (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    campaign_id TEXT REFERENCES campaigns(id) ON DELETE SET NULL,
    created_at  TEXT NOT NULL
);
CREATE TABLE music_playlist_items (
    id          TEXT PRIMARY KEY,
    playlist_id TEXT NOT NULL REFERENCES music_playlists(id) ON DELETE CASCADE,
    track_id    TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL
);

CREATE TABLE music_soundboards (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    campaign_id TEXT REFERENCES campaigns(id) ON DELETE SET NULL,
    notes       TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL
);
CREATE TABLE music_soundboard_buttons (
    id              TEXT PRIMARY KEY,
    soundboard_id   TEXT NOT NULL REFERENCES music_soundboards(id) ON DELETE CASCADE,
    track_id        TEXT REFERENCES music_tracks(id) ON DELETE SET NULL,
    label           TEXT NOT NULL DEFAULT '',
    icon            TEXT NOT NULL DEFAULT '',
    color           TEXT NOT NULL DEFAULT '',
    position        INTEGER NOT NULL,
    loop_by_default INTEGER NOT NULL DEFAULT 0,
    volume          INTEGER NOT NULL DEFAULT 80,
    play_mode       TEXT NOT NULL DEFAULT 'oneshot' CHECK (play_mode IN ('oneshot', 'restart', 'toggle')),
    hotkey          TEXT
);

CREATE INDEX music_playlist_items_pl ON music_playlist_items(playlist_id, position);
CREATE INDEX music_soundboard_buttons_sb ON music_soundboard_buttons(soundboard_id, position);
