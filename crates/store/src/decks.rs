//! Saved card decks (markdown + print settings).

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{Result, Store, StoreError, changes, now};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deck {
    pub id: i64,
    pub name: String,
    pub markdown: String,
    /// JSON object: `{format, double_sided, flip}`.
    pub settings_json: String,
    pub created: String,
    pub updated: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckSummary {
    pub id: i64,
    pub name: String,
    pub updated: String,
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Deck> {
    Ok(Deck {
        id: r.get(0)?,
        name: r.get(1)?,
        markdown: r.get(2)?,
        settings_json: r.get(3)?,
        created: r.get(4)?,
        updated: r.get(5)?,
    })
}

impl Store {
    pub fn list_decks(&self) -> Result<Vec<DeckSummary>> {
        let conn = self.conn();
        let mut st = conn
            .prepare("SELECT id, name, updated FROM card_decks ORDER BY updated DESC, id DESC")?;
        let rows = st.query_map([], |r| {
            Ok(DeckSummary { id: r.get(0)?, name: r.get(1)?, updated: r.get(2)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn get_deck(&self, id: i64) -> Result<Deck> {
        self.conn()
            .query_row(
                "SELECT id, name, markdown, settings_json, created, updated FROM card_decks WHERE id = ?1",
                [id],
                row,
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("deck {id}")))
    }

    /// Creates (`id = None`) or overwrites a deck. Returns the saved deck.
    pub fn save_deck(
        &self,
        id: Option<i64>,
        name: &str,
        markdown: &str,
        settings_json: &str,
    ) -> Result<Deck> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Invalid("deck name must not be empty".into()));
        }
        let ts = now();
        let id = {
            let conn = self.conn();
            match id {
                Some(id) => {
                    let n = conn.execute(
                        "UPDATE card_decks SET name = ?2, markdown = ?3, settings_json = ?4, updated = ?5 WHERE id = ?1",
                        params![id, name, markdown, settings_json, ts],
                    )?;
                    if n == 0 {
                        return Err(StoreError::NotFound(format!("deck {id}")));
                    }
                    changes::record(&conn, "deck", &id.to_string(), "update")?;
                    id
                }
                None => {
                    conn.execute(
                        "INSERT INTO card_decks (name, markdown, settings_json, created, updated) VALUES (?1, ?2, ?3, ?4, ?4)",
                        params![name, markdown, settings_json, ts],
                    )?;
                    let id = conn.last_insert_rowid();
                    changes::record(&conn, "deck", &id.to_string(), "insert")?;
                    id
                }
            }
        };
        self.get_deck(id)
    }

    pub fn delete_deck(&self, id: i64) -> Result<()> {
        let conn = self.conn();
        if conn.execute("DELETE FROM card_decks WHERE id = ?1", [id])? == 0 {
            return Err(StoreError::NotFound(format!("deck {id}")));
        }
        changes::record(&conn, "deck", &id.to_string(), "delete")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::temp_store;

    #[test]
    fn deck_crud() {
        let (_d, s) = temp_store();
        let d = s.save_deck(None, "Spells", "# Fireball", "{}").unwrap();
        let d2 = s
            .save_deck(Some(d.id), "Spells L3", "# Fireball\n---\n# Haste", "{\"format\":\"A4\"}")
            .unwrap();
        assert_eq!((d2.id, d2.name.as_str()), (d.id, "Spells L3"));
        assert_eq!(s.list_decks().unwrap().len(), 1);
        assert!(s.save_deck(Some(999), "x", "", "{}").is_err());
        assert!(s.save_deck(None, "  ", "", "{}").is_err());
        s.delete_deck(d.id).unwrap();
        assert!(s.get_deck(d.id).is_err());
    }
}
