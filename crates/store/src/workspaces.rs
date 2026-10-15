//! Autosaved editor / sheet state (`key -> json`).

use rusqlite::{OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};

use crate::{Result, Store, now};

impl Store {
    pub fn workspace_get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = self
            .conn()
            .query_row("SELECT json FROM workspaces WHERE key = ?1", [key], |r| r.get(0))
            .optional()?;
        match raw {
            None => Ok(None),
            Some(j) => Ok(Some(serde_json::from_str(&j)?)),
        }
    }

    pub fn workspace_set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let json = serde_json::to_string(value)?;
        self.conn().execute(
            "INSERT INTO workspaces (key, json, updated) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET json = excluded.json, updated = excluded.updated",
            params![key, json, now()],
        )?;
        Ok(())
    }

    pub fn workspace_delete(&self, key: &str) -> Result<()> {
        self.conn().execute("DELETE FROM workspaces WHERE key = ?1", [key])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::temp_store;

    #[test]
    fn set_get_overwrite_delete() {
        let (_d, s) = temp_store();
        assert_eq!(s.workspace_get::<Value>("token_sheet").unwrap(), None);
        s.workspace_set("token_sheet", &json!({"items": [1]})).unwrap();
        s.workspace_set("token_sheet", &json!({"items": [1, 2]})).unwrap();
        assert_eq!(
            s.workspace_get::<Value>("token_sheet").unwrap(),
            Some(json!({"items": [1, 2]}))
        );
        s.workspace_delete("token_sheet").unwrap();
        assert_eq!(s.workspace_get::<Value>("token_sheet").unwrap(), None);
    }
}
