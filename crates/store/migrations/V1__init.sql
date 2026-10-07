-- V1: initial schema. See docs/data-model.md. Never edit once released; add V2+.

CREATE TABLE blobs (
    hash    TEXT PRIMARY KEY,
    mime    TEXT NOT NULL,
    width   INTEGER,
    height  INTEGER,
    bytes   INTEGER NOT NULL,
    created TEXT NOT NULL
);

CREATE TABLE assets (
    id            INTEGER PRIMARY KEY,
    kind          TEXT NOT NULL,
    name          TEXT NOT NULL,
    aliases       TEXT NOT NULL DEFAULT '',
    size_label    TEXT NOT NULL DEFAULT 'Medium',
    pinned        INTEGER NOT NULL DEFAULT 0,
    original_blob TEXT NOT NULL REFERENCES blobs(hash),
    rendered_blob TEXT NOT NULL REFERENCES blobs(hash),
    pdf_blob      TEXT REFERENCES blobs(hash),
    thumb_blob    TEXT REFERENCES blobs(hash),
    preview_blob  TEXT REFERENCES blobs(hash),
    params_json   TEXT,
    legacy_id     INTEGER UNIQUE,
    created       TEXT NOT NULL,
    updated       TEXT NOT NULL
);
CREATE INDEX idx_assets_kind ON assets(kind);
CREATE INDEX idx_assets_pinned ON assets(pinned) WHERE pinned = 1;

CREATE TABLE tags (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE asset_tags (
    asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    tag_id   INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (asset_id, tag_id)
);
CREATE INDEX idx_asset_tags_tag ON asset_tags(tag_id);

CREATE TABLE campaigns (
    id      TEXT PRIMARY KEY,
    name    TEXT NOT NULL,
    created TEXT NOT NULL
);

CREATE TABLE sessions (
    id          TEXT PRIMARY KEY,
    campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    notes       TEXT NOT NULL DEFAULT '',
    created     TEXT NOT NULL
);
CREATE INDEX idx_sessions_campaign ON sessions(campaign_id);

CREATE TABLE asset_campaigns (
    asset_id    INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    PRIMARY KEY (asset_id, campaign_id)
);
CREATE INDEX idx_asset_campaigns_campaign ON asset_campaigns(campaign_id);

CREATE TABLE asset_sessions (
    asset_id   INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    PRIMARY KEY (asset_id, session_id)
);
CREATE INDEX idx_asset_sessions_session ON asset_sessions(session_id);

CREATE TABLE aoe_presets (
    name        TEXT PRIMARY KEY,
    config_json TEXT NOT NULL,
    created     TEXT NOT NULL
);

CREATE TABLE card_decks (
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL,
    markdown      TEXT NOT NULL,
    settings_json TEXT NOT NULL DEFAULT '{}',
    created       TEXT NOT NULL,
    updated       TEXT NOT NULL
);

CREATE TABLE workspaces (
    key     TEXT PRIMARY KEY,
    json    TEXT NOT NULL,
    updated TEXT NOT NULL
);

CREATE TABLE settings (
    key  TEXT PRIMARY KEY,
    json TEXT NOT NULL
);

CREATE TABLE change_log (
    seq       INTEGER PRIMARY KEY AUTOINCREMENT,
    entity    TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    op        TEXT NOT NULL,
    at        TEXT NOT NULL
);

CREATE VIRTUAL TABLE assets_fts USING fts5(
    name, aliases, tags,
    tokenize = 'unicode61 remove_diacritics 2'
);
