//! Change log used to propagate writes between processes (app <-> standalone MCP).

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::{Result, Store, now};

const KEEP_ROWS: i64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub seq: i64,
    pub entity: String,
    pub entity_id: String,
    pub op: String,
}

/// Records a change inside the caller's transaction/connection.
pub(crate) fn record(conn: &Connection, entity: &str, entity_id: &str, op: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO change_log (entity, entity_id, op, at) VALUES (?1, ?2, ?3, ?4)",
        params![entity, entity_id, op, now()],
    )?;
    Ok(())
}

impl Store {
    /// Records a change on the shared connection.
    pub fn record_change(&self, entity: &str, entity_id: &str, op: &str) -> Result<()> {
        record(&self.conn(), entity, entity_id, op)
    }

    /// Changes with `seq > after`, oldest first.
    pub fn changes_since(&self, after: i64) -> Result<Vec<Change>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT seq, entity, entity_id, op FROM change_log WHERE seq > ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map([after], |r| {
            Ok(Change { seq: r.get(0)?, entity: r.get(1)?, entity_id: r.get(2)?, op: r.get(3)? })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn latest_change_seq(&self) -> Result<i64> {
        Ok(self
            .conn()
            .query_row("SELECT coalesce(max(seq), 0) FROM change_log", [], |r| r.get(0))?)
    }

    /// `PRAGMA data_version`: changes whenever *another* connection commits.
    pub fn data_version(&self) -> Result<i64> {
        Ok(self.conn().query_row("PRAGMA data_version", [], |r| r.get(0))?)
    }

    /// Drops old rows, keeping the most recent ones.
    pub fn trim_changes(&self) -> Result<usize> {
        Ok(self.conn().execute(
            "DELETE FROM change_log WHERE seq <= (SELECT max(seq) FROM change_log) - ?1",
            [KEEP_ROWS],
        )?)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Store, temp_store};

    #[test]
    fn changes_are_ordered_and_filtered() {
        let (_d, s) = temp_store();
        s.record_change("asset", "1", "insert").unwrap();
        s.record_change("tag", "7", "delete").unwrap();
        let all = s.changes_since(0).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].entity, "tag");
        assert_eq!(s.changes_since(all[0].seq).unwrap().len(), 1);
        assert_eq!(s.latest_change_seq().unwrap(), all[1].seq);
    }

    #[test]
    fn data_version_detects_other_connection_commits() {
        let (d, a) = temp_store();
        let b = Store::open(d.path()).unwrap();
        let v0 = a.data_version().unwrap();
        b.record_change("asset", "1", "update").unwrap();
        assert_ne!(a.data_version().unwrap(), v0);
        assert_eq!(a.changes_since(0).unwrap().len(), 1);
    }
}
