-- Favourite tracks and pinned soundscapes (docs/features/music/data-model.md).
ALTER TABLE music_tracks ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0;
ALTER TABLE music_soundscapes ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
