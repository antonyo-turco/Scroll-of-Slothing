//! Minimal forward-only migration runner keyed on `PRAGMA user_version`.
//! Each migration runs in its own transaction together with the version bump.

use rusqlite::Connection;

use crate::{Result, StoreError};

/// Ordered list of migrations; index + 1 is the version they produce.
const MIGRATIONS: &[(&str, &str)] = &[
    ("V1__init", include_str!("../migrations/V1__init.sql")),
    ("V2__music", include_str!("../migrations/V2__music.sql")),
    ("V3__music_favorites", include_str!("../migrations/V3__music_favorites.sql")),
    ("V4__music_categories", include_str!("../migrations/V4__music_categories.sql")),
];

pub const LATEST: i64 = MIGRATIONS.len() as i64;

pub fn run(conn: &mut Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > LATEST {
        return Err(StoreError::Invalid(format!(
            "database schema v{current} is newer than this app (v{LATEST}); update the app"
        )));
    }
    for (i, (name, sql)) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let version = i as i64 + 1;
        tracing::info!(migration = name, version, "applying migration");
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// V4 fills in categories for existing tracks: usage in saved mixes first,
    /// then the title and length.
    #[test]
    fn v4_backfills_categories_from_usage_and_titles() {
        let mut conn = Connection::open_in_memory().unwrap();
        for (_, sql) in &MIGRATIONS[..3] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 3).unwrap();
        conn.execute_batch(
            "INSERT INTO music_tracks (id, source, title, loop_by_default, duration, added_at) VALUES
               ('rain', 'youtube', 'Rain', 1, 3600, 'x'),
               ('drums', 'youtube', 'Swamp Village | Exploration Music', 1, 3600, 'x'),
               ('forest', 'youtube', 'Peaceful Forest Ambience', 1, 3600, 'x'),
               ('rat', 'youtube', 'SFX - Rat Screech', 0, 16, 'x'),
               ('door', 'youtube', 'Door', 0, 3, 'x'),
               ('bell', 'youtube', 'Bell', 1, 600, 'x'),
               ('mixed', 'youtube', 'Cave Ambience', 1, 3600, 'x');
             INSERT INTO music_soundscapes (id, name, created_at) VALUES ('s', 'S', 'x');
             INSERT INTO music_soundscape_channels (id, soundscape_id, track_id, role) VALUES
               ('c1', 's', 'rain', 'ambient'),
               ('c2', 's', 'mixed', 'music'), ('c3', 's', 'mixed', 'music'), ('c4', 's', 'mixed', 'ambient');
             INSERT INTO music_soundscape_soundbites (id, soundscape_id, track_id) VALUES ('b1', 's', 'bell');",
        )
        .unwrap();
        run(&mut conn).unwrap();
        let cat = |id: &str| -> String {
            conn.query_row("SELECT category FROM music_tracks WHERE id = ?1", [id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(cat("rain"), "ambient"); // used as ambient
        assert_eq!(cat("drums"), "music"); // "music" in the title
        assert_eq!(cat("forest"), "ambient"); // unused, ambience title
        assert_eq!(cat("rat"), "soundbite"); // SFX title
        assert_eq!(cat("door"), "soundbite"); // short one-shot
        assert_eq!(cat("bell"), "soundbite"); // used as a soundbite
        assert_eq!(cat("mixed"), "music"); // used as music more often than as ambient
        let bad = conn.execute("UPDATE music_tracks SET category = 'sfx' WHERE id = 'rain'", []);
        assert!(bad.is_err(), "the CHECK constraint rejects unknown categories");
    }

    #[test]
    fn refuses_newer_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", LATEST + 1).unwrap();
        assert!(matches!(run(&mut conn), Err(StoreError::Invalid(_))));
    }

    #[test]
    fn all_tables_exist_after_migration() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        for t in [
            "blobs",
            "assets",
            "tags",
            "asset_tags",
            "campaigns",
            "sessions",
            "asset_campaigns",
            "asset_sessions",
            "aoe_presets",
            "card_decks",
            "workspaces",
            "settings",
            "change_log",
            "assets_fts",
        ] {
            let n: i64 = conn
                .query_row("SELECT count(*) FROM sqlite_master WHERE name = ?1", [t], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 1, "missing table {t}");
        }
    }
}
