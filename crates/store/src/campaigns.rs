//! Campaigns and sessions. Spec: docs/features/campaigns-sessions.md.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{Result, Store, StoreError, changes, now};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Campaign {
    pub id: String,
    pub name: String,
    pub created: String,
    pub session_count: u64,
    pub asset_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub campaign_id: String,
    pub name: String,
    pub notes: String,
    pub created: String,
    pub asset_count: u64,
}

fn non_empty(s: &str, what: &str) -> Result<String> {
    let t = s.trim();
    if t.is_empty() { Err(StoreError::Invalid(format!("{what} must not be empty"))) } else { Ok(t.to_string()) }
}

impl Store {
    pub fn list_campaigns(&self) -> Result<Vec<Campaign>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT c.id, c.name, c.created,
                    (SELECT count(*) FROM sessions s WHERE s.campaign_id = c.id),
                    (SELECT count(*) FROM (
                        SELECT asset_id FROM asset_campaigns WHERE campaign_id = c.id
                        UNION SELECT a.asset_id FROM asset_sessions a
                              JOIN sessions s ON s.id = a.session_id WHERE s.campaign_id = c.id))
             FROM campaigns c ORDER BY c.name COLLATE NOCASE",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Campaign {
                id: r.get(0)?,
                name: r.get(1)?,
                created: r.get(2)?,
                session_count: r.get::<_, i64>(3)? as u64,
                asset_count: r.get::<_, i64>(4)? as u64,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn get_campaign(&self, id: &str) -> Result<Campaign> {
        self.list_campaigns()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| StoreError::NotFound(format!("campaign {id}")))
    }

    pub fn create_campaign(&self, name: &str) -> Result<Campaign> {
        self.create_campaign_with_id(&uuid_v4(), name, None)
    }

    /// Creates a campaign with a caller-chosen id (used by the legacy importer to keep uuids).
    pub fn create_campaign_with_id(&self, id: &str, name: &str, created: Option<&str>) -> Result<Campaign> {
        let name = non_empty(name, "campaign name")?;
        let ts = created.map(str::to_string).unwrap_or_else(now);
        let conn = self.conn();
        conn.execute("INSERT INTO campaigns (id, name, created) VALUES (?1, ?2, ?3)", params![id, name, ts])?;
        changes::record(&conn, "campaign", id, "insert")?;
        drop(conn);
        self.get_campaign(id)
    }

    pub fn rename_campaign(&self, id: &str, name: &str) -> Result<()> {
        let name = non_empty(name, "campaign name")?;
        let conn = self.conn();
        if conn.execute("UPDATE campaigns SET name = ?2 WHERE id = ?1", params![id, name])? == 0 {
            return Err(StoreError::NotFound(format!("campaign {id}")));
        }
        changes::record(&conn, "campaign", id, "update")?;
        Ok(())
    }

    /// Deletes the campaign, its sessions and all links. Assets are never deleted.
    pub fn delete_campaign(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        if conn.execute("DELETE FROM campaigns WHERE id = ?1", [id])? == 0 {
            return Err(StoreError::NotFound(format!("campaign {id}")));
        }
        changes::record(&conn, "campaign", id, "delete")?;
        Ok(())
    }

    pub fn list_sessions(&self, campaign_id: Option<&str>) -> Result<Vec<Session>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT s.id, s.campaign_id, s.name, s.notes, s.created,
                    (SELECT count(*) FROM asset_sessions a WHERE a.session_id = s.id)
             FROM sessions s WHERE (?1 IS NULL OR s.campaign_id = ?1)
             ORDER BY s.created, s.name COLLATE NOCASE",
        )?;
        let rows = st.query_map([campaign_id], |r| {
            Ok(Session {
                id: r.get(0)?,
                campaign_id: r.get(1)?,
                name: r.get(2)?,
                notes: r.get(3)?,
                created: r.get(4)?,
                asset_count: r.get::<_, i64>(5)? as u64,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn create_session(&self, campaign_id: &str, name: &str, notes: &str) -> Result<Session> {
        self.create_session_with_id(&uuid_v4(), campaign_id, name, notes, None)
    }

    pub fn create_session_with_id(
        &self,
        id: &str,
        campaign_id: &str,
        name: &str,
        notes: &str,
        created: Option<&str>,
    ) -> Result<Session> {
        let name = non_empty(name, "session name")?;
        let ts = created.map(str::to_string).unwrap_or_else(now);
        let conn = self.conn();
        let exists: Option<String> =
            conn.query_row("SELECT id FROM campaigns WHERE id = ?1", [campaign_id], |r| r.get(0)).optional()?;
        exists.ok_or_else(|| StoreError::NotFound(format!("campaign {campaign_id}")))?;
        conn.execute(
            "INSERT INTO sessions (id, campaign_id, name, notes, created) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, campaign_id, name, notes, ts],
        )?;
        changes::record(&conn, "session", id, "insert")?;
        Ok(Session {
            id: id.into(),
            campaign_id: campaign_id.into(),
            name,
            notes: notes.into(),
            created: ts,
            asset_count: 0,
        })
    }

    pub fn update_session(&self, id: &str, name: Option<&str>, notes: Option<&str>) -> Result<()> {
        let conn = self.conn();
        let exists: Option<String> =
            conn.query_row("SELECT id FROM sessions WHERE id = ?1", [id], |r| r.get(0)).optional()?;
        exists.ok_or_else(|| StoreError::NotFound(format!("session {id}")))?;
        if let Some(n) = name {
            conn.execute("UPDATE sessions SET name = ?2 WHERE id = ?1", params![id, non_empty(n, "session name")?])?;
        }
        if let Some(n) = notes {
            conn.execute("UPDATE sessions SET notes = ?2 WHERE id = ?1", params![id, n])?;
        }
        changes::record(&conn, "session", id, "update")?;
        Ok(())
    }

    /// Deletes a session and its asset links (assets are kept).
    pub fn delete_session(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        if conn.execute("DELETE FROM sessions WHERE id = ?1", [id])? == 0 {
            return Err(StoreError::NotFound(format!("session {id}")));
        }
        changes::record(&conn, "session", id, "delete")?;
        Ok(())
    }

    /// Replaces the full asset set of a session.
    pub fn set_session_assets(&self, session_id: &str, asset_ids: &[i64]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let exists: Option<String> =
            tx.query_row("SELECT id FROM sessions WHERE id = ?1", [session_id], |r| r.get(0)).optional()?;
        exists.ok_or_else(|| StoreError::NotFound(format!("session {session_id}")))?;
        tx.execute("DELETE FROM asset_sessions WHERE session_id = ?1", [session_id])?;
        for a in asset_ids {
            tx.execute("INSERT OR IGNORE INTO asset_sessions (asset_id, session_id) VALUES (?1, ?2)", params![a, session_id])
                .map_err(|_| StoreError::NotFound(format!("asset {a}")))?;
        }
        changes::record(&tx, "session", session_id, "update")?;
        tx.commit()?;
        Ok(())
    }
}

fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::tests::new_asset;
    use crate::temp_store;

    #[test]
    fn campaign_crud_and_counts() {
        let (_d, s) = temp_store();
        let c = s.create_campaign("  Tomb ").unwrap();
        assert_eq!(c.name, "Tomb");
        assert!(s.create_campaign("").is_err());
        let sess = s.create_session(&c.id, "One", "notes").unwrap();
        let a = new_asset(&s, "token", "a");
        let b = new_asset(&s, "token", "b");
        s.set_session_assets(&sess.id, &[a, b]).unwrap();
        s.set_asset_campaigns(a, std::slice::from_ref(&c.id)).unwrap();
        let got = s.get_campaign(&c.id).unwrap();
        assert_eq!((got.session_count, got.asset_count), (1, 2), "union of direct + session links");
        s.rename_campaign(&c.id, "Tomb of Annihilation").unwrap();
        assert_eq!(s.list_campaigns().unwrap()[0].name, "Tomb of Annihilation");
    }

    #[test]
    fn deleting_campaign_cascades_sessions_but_keeps_assets() {
        let (_d, s) = temp_store();
        let c = s.create_campaign("C").unwrap();
        let sess = s.create_session(&c.id, "S", "").unwrap();
        let a = new_asset(&s, "token", "keep me");
        s.set_session_assets(&sess.id, &[a]).unwrap();
        s.delete_campaign(&c.id).unwrap();
        assert!(s.list_sessions(None).unwrap().is_empty());
        let asset = s.get_asset(a).unwrap();
        assert!(asset.session_ids.is_empty());
        assert!(s.delete_campaign(&c.id).is_err());
    }

    #[test]
    fn sessions_require_existing_campaign_and_support_updates() {
        let (_d, s) = temp_store();
        assert!(matches!(s.create_session("missing", "S", ""), Err(StoreError::NotFound(_))));
        let c = s.create_campaign("C").unwrap();
        let sess = s.create_session(&c.id, "S", "").unwrap();
        s.update_session(&sess.id, Some("Session 2"), Some("prep notes")).unwrap();
        let got = &s.list_sessions(Some(&c.id)).unwrap()[0];
        assert_eq!((got.name.as_str(), got.notes.as_str()), ("Session 2", "prep notes"));
        s.delete_session(&sess.id).unwrap();
        assert!(s.update_session(&sess.id, None, None).is_err());
    }

    #[test]
    fn uuids_are_unique_and_well_formed() {
        let a = uuid_v4();
        let b = uuid_v4();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
    }
}
