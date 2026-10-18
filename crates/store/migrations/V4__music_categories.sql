-- Track category: music, ambient or soundbite (docs/features/music/data-model.md).
-- It decides where "Add to mix" puts a track and is a library filter.
ALTER TABLE music_tracks ADD COLUMN category TEXT NOT NULL DEFAULT 'music'
    CHECK (category IN ('music', 'ambient', 'soundbite'));

-- Backfill existing tracks. Strongest evidence first: how saved mixes use them.
-- Soundbites: used as a soundbite, named SFX, or a short one-shot.
UPDATE music_tracks SET category = 'soundbite'
WHERE id IN (SELECT track_id FROM music_soundscape_soundbites UNION SELECT track_id FROM music_soundbites)
   OR lower(title) LIKE 'sfx%' OR lower(title) LIKE '% sfx%'
   OR (loop_by_default = 0 AND duration IS NOT NULL AND duration <= 120);

-- Ambient: used more often as an ambient channel than as a music channel, or,
-- if never used in a mix, an "ambience"-style title that doesn't say "music".
UPDATE music_tracks SET category = 'ambient'
WHERE category = 'music' AND (
    (SELECT COUNT(*) FROM music_soundscape_channels c WHERE c.track_id = music_tracks.id AND c.role = 'ambient')
  > (SELECT COUNT(*) FROM music_soundscape_channels c WHERE c.track_id = music_tracks.id AND c.role = 'music')
 OR (
    NOT EXISTS (SELECT 1 FROM music_soundscape_channels c WHERE c.track_id = music_tracks.id)
    AND (lower(title) LIKE '%ambien%' OR lower(title) LIKE '%atmospher%' OR lower(title) LIKE '%soundscape%'
         OR lower(title) LIKE '%sounds%' OR lower(title) LIKE '%dripping%')
    AND lower(title) NOT LIKE '%music%' AND lower(title) NOT LIKE '%theme%'
 )
);
