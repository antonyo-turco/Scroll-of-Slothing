-- Exact schema of Orb of Many Functions db/creatures.db (dumped read-only). Test fixture only.
CREATE TABLE creatures (
                id       INTEGER PRIMARY KEY AUTOINCREMENT,
                name     TEXT NOT NULL,
                original BLOB NOT NULL,
                token    BLOB NOT NULL,
                created  TEXT DEFAULT (datetime('now'))
            , size TEXT NOT NULL DEFAULT 'Medium (1 inch)', kind TEXT NOT NULL DEFAULT 'token', aliases TEXT NOT NULL DEFAULT '', pdf BLOB, pinned INTEGER NOT NULL DEFAULT 0, thumb BLOB);
CREATE TABLE music_campaigns (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
CREATE TABLE music_tracks (
            id              TEXT PRIMARY KEY,
            url             TEXT NOT NULL,
            title           TEXT NOT NULL,
            notes           TEXT,
            loop_by_default INTEGER NOT NULL DEFAULT 1,
            campaign_id     TEXT NOT NULL REFERENCES music_campaigns(id) ON DELETE CASCADE,
            added_at        TEXT NOT NULL
        , session_id TEXT, source TEXT NOT NULL DEFAULT 'youtube', duration REAL);
CREATE TABLE music_tags (
            id       TEXT PRIMARY KEY,
            name     TEXT NOT NULL UNIQUE,
            color    TEXT,
            category TEXT
        );
CREATE TABLE music_track_tags (
            track_id TEXT REFERENCES music_tracks(id) ON DELETE CASCADE,
            tag_id   TEXT REFERENCES music_tags(id) ON DELETE CASCADE,
            PRIMARY KEY (track_id, tag_id)
        );
CREATE TABLE music_channels (
            id         TEXT PRIMARY KEY,
            track_id   TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
            volume     INTEGER NOT NULL DEFAULT 80,
            is_looping INTEGER NOT NULL DEFAULT 1,
            is_playing INTEGER NOT NULL DEFAULT 0,
            role       TEXT NOT NULL DEFAULT 'ambient'
        );
CREATE TABLE music_soundscapes (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            campaign_id TEXT NOT NULL REFERENCES music_campaigns(id) ON DELETE CASCADE,
            notes       TEXT,
            created_at  TEXT NOT NULL
        );
CREATE TABLE music_soundscape_tags (
            soundscape_id TEXT REFERENCES music_soundscapes(id) ON DELETE CASCADE,
            tag_id        TEXT REFERENCES music_tags(id) ON DELETE CASCADE,
            PRIMARY KEY (soundscape_id, tag_id)
        );
CREATE TABLE music_soundscape_channels (
            id            TEXT PRIMARY KEY,
            soundscape_id TEXT NOT NULL REFERENCES music_soundscapes(id) ON DELETE CASCADE,
            track_id      TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
            volume        INTEGER NOT NULL DEFAULT 80,
            is_looping    INTEGER NOT NULL DEFAULT 1,
            role          TEXT NOT NULL DEFAULT 'ambient'
        );
CREATE TABLE music_playlists (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            campaign_id TEXT NOT NULL REFERENCES music_campaigns(id) ON DELETE CASCADE,
            created_at  TEXT NOT NULL
        );
CREATE TABLE music_playlist_tracks (
            playlist_id TEXT REFERENCES music_playlists(id) ON DELETE CASCADE,
            track_id    TEXT REFERENCES music_tracks(id) ON DELETE CASCADE,
            position    INTEGER NOT NULL,
            PRIMARY KEY (playlist_id, track_id)
        );
CREATE TABLE music_soundboards (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            campaign_id TEXT NOT NULL REFERENCES music_campaigns(id) ON DELETE CASCADE,
            notes       TEXT,
            created_at  TEXT NOT NULL
        );
CREATE TABLE music_soundboard_tags (
            soundboard_id TEXT REFERENCES music_soundboards(id) ON DELETE CASCADE,
            tag_id        TEXT REFERENCES music_tags(id) ON DELETE CASCADE,
            PRIMARY KEY (soundboard_id, tag_id)
        );
CREATE TABLE music_soundboard_buttons (
            id              TEXT PRIMARY KEY,
            soundboard_id   TEXT NOT NULL REFERENCES music_soundboards(id) ON DELETE CASCADE,
            track_id        TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
            label           TEXT NOT NULL,
            icon            TEXT,
            color           TEXT,
            position        INTEGER NOT NULL,
            loop_by_default INTEGER NOT NULL DEFAULT 0,
            play_mode       TEXT NOT NULL DEFAULT 'oneshot'
        );
CREATE TABLE music_soundbites (
            id           TEXT PRIMARY KEY,
            track_id     TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
            volume       INTEGER NOT NULL DEFAULT 80,
            min_interval INTEGER NOT NULL DEFAULT 30,
            max_interval INTEGER NOT NULL DEFAULT 90
        );
CREATE TABLE music_soundscape_soundbites (
            id            TEXT PRIMARY KEY,
            soundscape_id TEXT NOT NULL REFERENCES music_soundscapes(id) ON DELETE CASCADE,
            track_id      TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
            volume        INTEGER NOT NULL DEFAULT 80,
            min_interval  INTEGER NOT NULL DEFAULT 30,
            max_interval  INTEGER NOT NULL DEFAULT 90
        );
CREATE TABLE asset_tags (
                id   INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE COLLATE NOCASE
            );
CREATE TABLE asset_tag_links (
                asset_id INTEGER NOT NULL REFERENCES creatures(id) ON DELETE CASCADE,
                tag_id   INTEGER NOT NULL REFERENCES asset_tags(id) ON DELETE CASCADE,
                PRIMARY KEY (asset_id, tag_id)
            );
CREATE TABLE sessions (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                campaign_id TEXT NOT NULL REFERENCES music_campaigns(id) ON DELETE CASCADE,
                notes       TEXT,
                created_at  TEXT NOT NULL
            );
CREATE TABLE session_assets (
                session_id TEXT    NOT NULL REFERENCES sessions(id)  ON DELETE CASCADE,
                asset_id   INTEGER NOT NULL REFERENCES creatures(id) ON DELETE CASCADE,
                PRIMARY KEY (session_id, asset_id)
            );
CREATE TABLE asset_campaigns (
                campaign_id TEXT    NOT NULL REFERENCES music_campaigns(id) ON DELETE CASCADE,
                asset_id    INTEGER NOT NULL REFERENCES creatures(id)       ON DELETE CASCADE,
                PRIMARY KEY (campaign_id, asset_id)
            );
CREATE TABLE music_track_campaigns (
                track_id    TEXT NOT NULL REFERENCES music_tracks(id)    ON DELETE CASCADE,
                campaign_id TEXT NOT NULL REFERENCES music_campaigns(id) ON DELETE CASCADE,
                PRIMARY KEY (track_id, campaign_id)
            );
CREATE TABLE music_track_sessions (
                track_id   TEXT NOT NULL REFERENCES music_tracks(id) ON DELETE CASCADE,
                session_id TEXT NOT NULL REFERENCES sessions(id)     ON DELETE CASCADE,
                PRIMARY KEY (track_id, session_id)
            );
CREATE TABLE app_meta (
                key   TEXT PRIMARY KEY,
                value TEXT
            );
CREATE TABLE asset_thumbs (
                asset_id INTEGER PRIMARY KEY REFERENCES creatures(id) ON DELETE CASCADE,
                thumb    BLOB NOT NULL
            );
CREATE INDEX idx_creatures_meta ON creatures (id DESC, kind, pinned, name, size, created, aliases);
