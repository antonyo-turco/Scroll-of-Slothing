//! Read-only access to Orb of Many Functions data. This crate only *reads* the
//! old format; writing into the new store is done by `sleuth_service`
//! (`legacy_import`). Spec: docs/legacy-import.md.
//!
//! The database is opened with `mode=ro`, so the source is never modified.
//! Columns added by later Python migrations are optional.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, types::ValueRef};
use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, thiserror::Error)]
pub enum LegacyError {
    #[error("cannot read legacy database: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("not an Orb of Many Functions database (missing table `creatures`)")]
    NotLegacy,
}

pub type Result<T> = std::result::Result<T, LegacyError>;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LegacyCampaign {
    pub id: String,
    pub name: String,
    pub created: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LegacySession {
    pub id: String,
    pub campaign_id: String,
    pub name: String,
    pub notes: String,
    pub created: String,
}

/// One row of the old `creatures` table (tokens, cut-outs and maps).
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyAsset {
    pub id: i64,
    pub name: String,
    pub kind: String,
    /// Python size label, e.g. `"Medium (1 inch)"`.
    pub size: String,
    pub aliases: String,
    pub pinned: bool,
    pub created: Option<String>,
    pub original: Vec<u8>,
    /// The rendered image (token / cut-out / gridded map).
    pub token: Vec<u8>,
    pub pdf: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LegacyCounts {
    pub assets_by_kind: std::collections::BTreeMap<String, u64>,
    pub tags: u64,
    pub campaigns: u64,
    pub sessions: u64,
    pub music_tables: std::collections::BTreeMap<String, u64>,
}

pub struct LegacyDb {
    conn: Connection,
    creature_cols: HashSet<String>,
}

impl LegacyDb {
    /// Opens `path` strictly read-only.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_URI,
        )?;
        let db = LegacyDb { creature_cols: HashSet::new(), conn };
        if !db.has_table("creatures")? {
            return Err(LegacyError::NotLegacy);
        }
        let creature_cols = db.columns("creatures")?;
        Ok(LegacyDb { creature_cols, ..db })
    }

    fn has_table(&self, name: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [name],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    fn columns(&self, table: &str) -> Result<HashSet<String>> {
        let mut st = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        Ok(st.query_map([], |r| r.get::<_, String>(1))?.collect::<rusqlite::Result<_>>()?)
    }

    fn col_or(&self, col: &str, fallback: &str) -> String {
        if self.creature_cols.contains(col) {
            col.to_string()
        } else {
            format!("{fallback} AS {col}")
        }
    }

    pub fn campaigns(&self) -> Result<Vec<LegacyCampaign>> {
        if !self.has_table("music_campaigns")? {
            return Ok(vec![]);
        }
        let mut st = self
            .conn
            .prepare("SELECT id, name, created_at FROM music_campaigns ORDER BY created_at")?;
        let rows = st.query_map([], |r| {
            Ok(LegacyCampaign { id: r.get(0)?, name: r.get(1)?, created: r.get(2)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn sessions(&self) -> Result<Vec<LegacySession>> {
        if !self.has_table("sessions")? {
            return Ok(vec![]);
        }
        let mut st = self
            .conn
            .prepare("SELECT id, campaign_id, name, coalesce(notes, ''), created_at FROM sessions ORDER BY created_at")?;
        let rows = st.query_map([], |r| {
            Ok(LegacySession {
                id: r.get(0)?,
                campaign_id: r.get(1)?,
                name: r.get(2)?,
                notes: r.get(3)?,
                created: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Ids of all assets, oldest first (preserves the original ordering on import).
    pub fn asset_ids(&self) -> Result<Vec<i64>> {
        let mut st = self.conn.prepare("SELECT id FROM creatures ORDER BY id")?;
        Ok(st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    }

    /// Loads one asset including its image blobs (one row at a time keeps memory bounded).
    pub fn asset(&self, id: i64) -> Result<LegacyAsset> {
        let sql = format!(
            "SELECT id, name, {kind}, {size}, {aliases}, {pinned}, created, original, token, {pdf}
             FROM creatures WHERE id = ?1",
            kind = self.col_or("kind", "'token'"),
            size = self.col_or("size", "'Medium (1 inch)'"),
            aliases = self.col_or("aliases", "''"),
            pinned = self.col_or("pinned", "0"),
            pdf = self.col_or("pdf", "NULL"),
        );
        Ok(self.conn.query_row(&sql, [id], |r| {
            Ok(LegacyAsset {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: r.get(2)?,
                size: r.get(3)?,
                aliases: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                pinned: r.get::<_, Option<i64>>(5)?.unwrap_or(0) != 0,
                created: r.get(6)?,
                original: r.get(7)?,
                token: r.get(8)?,
                pdf: r.get(9)?,
            })
        })?)
    }

    /// `(asset_id, tag name)` pairs.
    pub fn asset_tags(&self) -> Result<Vec<(i64, String)>> {
        if !self.has_table("asset_tag_links")? {
            return Ok(vec![]);
        }
        let mut st = self.conn.prepare(
            "SELECT l.asset_id, t.name FROM asset_tag_links l JOIN asset_tags t ON t.id = l.tag_id ORDER BY l.asset_id",
        )?;
        Ok(st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
    }

    /// `(asset_id, session_id)` pairs.
    pub fn session_assets(&self) -> Result<Vec<(i64, String)>> {
        if !self.has_table("session_assets")? {
            return Ok(vec![]);
        }
        let mut st = self.conn.prepare("SELECT asset_id, session_id FROM session_assets")?;
        Ok(st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
    }

    /// `(asset_id, campaign_id)` pairs.
    pub fn asset_campaigns(&self) -> Result<Vec<(i64, String)>> {
        if !self.has_table("asset_campaigns")? {
            return Ok(vec![]);
        }
        let mut st = self.conn.prepare("SELECT asset_id, campaign_id FROM asset_campaigns")?;
        Ok(st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
    }

    fn music_table_names(&self) -> Result<Vec<String>> {
        let mut st = self.conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'music\\_%' ESCAPE '\\' ORDER BY name",
        )?;
        Ok(st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    }

    /// Every `music_*` table as `{table: [row objects]}` (raw, for the future music phase).
    pub fn music_dump(&self) -> Result<Map<String, Value>> {
        let mut out = Map::new();
        for table in self.music_table_names()? {
            let mut st = self.conn.prepare(&format!("SELECT * FROM \"{table}\""))?;
            let cols: Vec<String> = st.column_names().into_iter().map(String::from).collect();
            let rows = st.query_map([], |r| {
                let mut obj = Map::new();
                for (i, c) in cols.iter().enumerate() {
                    let v = match r.get_ref(i)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(n) => Value::from(n),
                        ValueRef::Real(f) => Value::from(f),
                        ValueRef::Text(t) => Value::from(String::from_utf8_lossy(t).into_owned()),
                        ValueRef::Blob(b) => Value::from(format!("<blob {} bytes>", b.len())),
                    };
                    obj.insert(c.clone(), v);
                }
                Ok(Value::Object(obj))
            })?;
            out.insert(table, Value::Array(rows.collect::<rusqlite::Result<_>>()?));
        }
        Ok(out)
    }

    pub fn counts(&self) -> Result<LegacyCounts> {
        let mut c = LegacyCounts::default();
        let kind = self.col_or("kind", "'token'");
        let mut st =
            self.conn.prepare(&format!("SELECT {kind}, count(*) FROM creatures GROUP BY 1"))?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (k, n) = row?;
            c.assets_by_kind.insert(k, n as u64);
        }
        let count = |t: &str| -> Result<u64> {
            if !self.has_table(t)? {
                return Ok(0);
            }
            Ok(self
                .conn
                .query_row(&format!("SELECT count(*) FROM \"{t}\""), [], |r| r.get::<_, i64>(0))?
                as u64)
        };
        c.tags = count("asset_tags")?;
        c.campaigns = count("music_campaigns")?;
        c.sessions = count("sessions")?;
        for t in self.music_table_names()? {
            let n = count(&t)?;
            c.music_tables.insert(t, n);
        }
        Ok(c)
    }
}

/// Parses the legacy `aoe_presets.json` (`{name: config}`), skipping malformed entries.
pub fn read_aoe_presets(json: &str) -> Vec<(String, Value)> {
    match serde_json::from_str::<Value>(json) {
        Ok(Value::Object(m)) => m.into_iter().filter(|(_, v)| v.is_object()).collect(),
        _ => vec![],
    }
}

#[cfg(any(test, feature = "fixture"))]
pub mod fixture {
    //! Builds a small database with the exact Python schema (dumped from a real
    //! `creatures.db`) for tests.
    use rusqlite::{Connection, params};

    pub const LEGACY_DDL: &str = include_str!("../tests/legacy_schema.sql");

    pub fn build(path: &std::path::Path, png: &[u8]) {
        let c = Connection::open(path).unwrap();
        c.execute_batch(LEGACY_DDL).unwrap();
        c.execute_batch(
            "INSERT INTO music_campaigns VALUES ('c1', 'Strahd', '2026-01-01T00:00:00');
             INSERT INTO sessions VALUES ('s1', 'Session 1', 'c1', 'notes', '2026-01-02T00:00:00');
             INSERT INTO asset_tags (id, name) VALUES (1, 'Undead'), (2, 'boss');
             INSERT INTO music_tracks (id, url, title, campaign_id, added_at) VALUES ('t1', 'https://youtu.be/x', 'Crypt', 'c1', 'now');",
        )
        .unwrap();
        let ins =
            "INSERT INTO creatures (id, name, original, token, size, kind, aliases, pdf, pinned)
                   VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8)";
        c.execute(
            ins,
            params![1, "Zombie", png, "Medium (1 inch)", "token", "walker", None::<Vec<u8>>, 1],
        )
        .unwrap();
        c.execute(
            ins,
            params![
                2,
                "Crypt Map",
                png,
                "Medium (1 inch)",
                "map",
                "",
                Some(b"%PDF-1.4 fake".to_vec()),
                0
            ],
        )
        .unwrap();
        c.execute(
            ins,
            params![3, "Lamp", png, "Tiny (0.5 inch)", "cutout", "", None::<Vec<u8>>, 0],
        )
        .unwrap();
        c.execute_batch(
            "INSERT INTO asset_tag_links VALUES (1, 1), (1, 2), (2, 1);
             INSERT INTO session_assets VALUES ('s1', 1);
             INSERT INTO asset_campaigns VALUES ('c1', 2);",
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (tempfile::TempDir, LegacyDb) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("creatures.db");
        fixture::build(&p, b"\x89PNG fake");
        let db = LegacyDb::open(&p).unwrap();
        (d, db)
    }

    #[test]
    fn reads_all_entities() {
        let (_d, db) = db();
        assert_eq!(db.campaigns().unwrap()[0].name, "Strahd");
        assert_eq!(db.sessions().unwrap()[0].campaign_id, "c1");
        assert_eq!(db.asset_ids().unwrap(), vec![1, 2, 3]);
        let z = db.asset(1).unwrap();
        assert_eq!(
            (z.name.as_str(), z.kind.as_str(), z.pinned, z.aliases.as_str()),
            ("Zombie", "token", true, "walker")
        );
        assert_eq!(db.asset(2).unwrap().pdf.as_deref(), Some(&b"%PDF-1.4 fake"[..]));
        assert_eq!(db.asset_tags().unwrap().len(), 3);
        assert_eq!(db.session_assets().unwrap(), vec![(1, "s1".to_string())]);
        assert_eq!(db.asset_campaigns().unwrap(), vec![(2, "c1".to_string())]);
    }

    #[test]
    fn counts_and_music_dump() {
        let (_d, db) = db();
        let c = db.counts().unwrap();
        assert_eq!(c.assets_by_kind.get("token"), Some(&1));
        assert_eq!(c.assets_by_kind.get("map"), Some(&1));
        assert_eq!((c.tags, c.campaigns, c.sessions), (2, 1, 1));
        assert_eq!(c.music_tables.get("music_tracks"), Some(&1));
        let dump = db.music_dump().unwrap();
        assert_eq!(dump["music_tracks"][0]["title"], "Crypt");
        assert!(dump.contains_key("music_soundboard_buttons"));
        assert!(!dump.contains_key("sessions"), "only music_* tables");
    }

    #[test]
    fn opening_is_read_only_and_validates_schema() {
        let (d, db) = db();
        assert!(db.conn.execute("DELETE FROM creatures", []).is_err(), "read-only connection");
        let other = d.path().join("other.db");
        Connection::open(&other).unwrap().execute_batch("CREATE TABLE x (a)").unwrap();
        assert!(matches!(LegacyDb::open(&other), Err(LegacyError::NotLegacy)));
    }

    #[test]
    fn tolerates_very_old_schema_without_later_columns() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("old.db");
        let c = Connection::open(&p).unwrap();
        c.execute_batch(
            "CREATE TABLE creatures (id INTEGER PRIMARY KEY, name TEXT, original BLOB, token BLOB, created TEXT);
             INSERT INTO creatures VALUES (7, 'Old', x'00', x'00', NULL);",
        )
        .unwrap();
        drop(c);
        let db = LegacyDb::open(&p).unwrap();
        let a = db.asset(7).unwrap();
        assert_eq!((a.kind.as_str(), a.size.as_str(), a.pdf), ("token", "Medium (1 inch)", None));
        assert!(db.campaigns().unwrap().is_empty());
        assert!(db.music_dump().unwrap().is_empty());
    }

    #[test]
    fn aoe_presets_parse_leniently() {
        let p = read_aoe_presets(r#"{"10 ft cube": {"shape": "square", "a_ft": 10}, "bad": 3}"#);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].0, "10 ft cube");
        assert!(read_aoe_presets("not json").is_empty());
    }
}
