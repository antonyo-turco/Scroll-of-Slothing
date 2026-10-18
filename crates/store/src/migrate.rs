//! Minimal forward-only migration runner keyed on `PRAGMA user_version`.
//! Each migration runs in its own transaction together with the version bump.

use rusqlite::Connection;

use crate::{Result, StoreError};

/// Ordered list of migrations; index + 1 is the version they produce.
const MIGRATIONS: &[(&str, &str)] = &[
    ("V1__init", include_str!("../migrations/V1__init.sql")),
    ("V2__music", include_str!("../migrations/V2__music.sql")),
    ("V3__music_favorites", include_str!("../migrations/V3__music_favorites.sql")),
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
