//! User AoE presets (`name -> config json`). Built-in presets live in code.

use rusqlite::params;
use serde_json::Value;

use crate::{Result, Store, StoreError, changes, now};

impl Store {
    pub fn list_aoe_presets(&self) -> Result<Vec<(String, Value)>> {
        let conn = self.conn();
        let mut st =
            conn.prepare("SELECT name, config_json FROM aoe_presets ORDER BY name COLLATE NOCASE")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (name, json) = row?;
            out.push((name, serde_json::from_str(&json)?));
        }
        Ok(out)
    }

    /// Inserts or overwrites a preset. Returns `true` if it replaced an existing one.
    pub fn upsert_aoe_preset(&self, name: &str, config: &Value) -> Result<bool> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Invalid("preset name must not be empty".into()));
        }
        let conn = self.conn();
        let existed = conn.execute(
            "UPDATE aoe_presets SET config_json = ?2 WHERE name = ?1",
            params![name, config.to_string()],
        )? > 0;
        if !existed {
            conn.execute(
                "INSERT INTO aoe_presets (name, config_json, created) VALUES (?1, ?2, ?3)",
                params![name, config.to_string(), now()],
            )?;
        }
        changes::record(&conn, "aoe_preset", name, if existed { "update" } else { "insert" })?;
        Ok(existed)
    }

    pub fn delete_aoe_preset(&self, name: &str) -> Result<()> {
        let conn = self.conn();
        if conn.execute("DELETE FROM aoe_presets WHERE name = ?1", [name])? == 0 {
            return Err(StoreError::NotFound(format!("preset {name}")));
        }
        changes::record(&conn, "aoe_preset", name, "delete")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::temp_store;

    #[test]
    fn upsert_list_delete() {
        let (_d, s) = temp_store();
        assert!(!s.upsert_aoe_preset("10 ft cube", &json!({"shape": "square"})).unwrap());
        assert!(
            s.upsert_aoe_preset("10 ft cube", &json!({"shape": "square", "a_ft": 10})).unwrap()
        );
        assert_eq!(
            s.list_aoe_presets().unwrap(),
            vec![("10 ft cube".into(), json!({"shape": "square", "a_ft": 10}))]
        );
        s.delete_aoe_preset("10 ft cube").unwrap();
        assert!(s.delete_aoe_preset("10 ft cube").is_err());
        assert!(s.upsert_aoe_preset(" ", &json!({})).is_err());
    }
}
