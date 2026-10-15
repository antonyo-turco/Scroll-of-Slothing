//! Settings persistence: one row per key, JSON value.

use rusqlite::params;
use serde_json::{Map, Value};
use sleuth_core::settings::Settings;

use crate::{Result, Store, changes};

impl Store {
    pub fn load_settings(&self) -> Result<Settings> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT key, json FROM settings")?;
        let mut map = Map::new();
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (k, j) = row?;
            if let Ok(v) = serde_json::from_str::<Value>(&j) {
                map.insert(k, v);
            }
        }
        Ok(Settings::from_map(&map))
    }

    /// Persists every key of `settings` atomically.
    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for (k, v) in settings.to_map() {
            tx.execute(
                "INSERT INTO settings (key, json) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET json = excluded.json",
                params![k, v.to_string()],
            )?;
        }
        changes::record(&tx, "settings", "*", "update")?;
        tx.commit()?;
        Ok(())
    }

    pub fn reset_settings(&self) -> Result<Settings> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM settings", [])?;
        changes::record(&tx, "settings", "*", "update")?;
        tx.commit()?;
        Ok(Settings::default())
    }
}

#[cfg(test)]
mod tests {
    use sleuth_core::settings::{CutLineStyle, Settings};

    use crate::temp_store;

    #[test]
    fn empty_store_yields_defaults() {
        let (_d, s) = temp_store();
        assert_eq!(s.load_settings().unwrap(), Settings::default());
    }

    #[test]
    fn save_load_reset_roundtrip() {
        let (_d, s) = temp_store();
        let st = Settings {
            cut_line_style: CutLineStyle::Solid,
            map_margin_in: 0.25,
            ..Settings::default()
        };
        s.save_settings(&st).unwrap();
        assert_eq!(s.load_settings().unwrap(), st);
        assert_eq!(s.reset_settings().unwrap(), Settings::default());
        assert_eq!(s.load_settings().unwrap(), Settings::default());
        assert_eq!(s.changes_since(0).unwrap().len(), 2);
    }

    #[test]
    fn corrupt_rows_fall_back_to_defaults() {
        let (_d, s) = temp_store();
        s.conn()
            .execute_batch(
                "INSERT INTO settings VALUES ('map_square_in', '\"oops\"');
                 INSERT INTO settings VALUES ('cut_line_width', 'not json');
                 INSERT INTO settings VALUES ('map_overlap_in', '0.5');",
            )
            .unwrap();
        let st = s.load_settings().unwrap();
        assert_eq!(st.map_square_in, 1.0);
        assert_eq!(st.cut_line_width, 0.5);
        assert_eq!(st.map_overlap_in, 0.5);
    }
}
