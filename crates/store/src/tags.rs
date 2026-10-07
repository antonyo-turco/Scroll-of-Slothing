//! Tag management: counts, rename (auto-merge), merge, delete. Spec: docs/features/tags.md.

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

use crate::assets::reindex;
use crate::{Result, Store, StoreError, changes};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagCount {
    pub id: i64,
    pub name: String,
    pub count: u64,
}

/// Outcome of a rename: either renamed in place or merged into an existing tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameOutcome {
    pub id: i64,
    pub merged_into: Option<i64>,
}

fn asset_ids_with_tag(tx: &Transaction<'_>, tag: i64) -> Result<Vec<i64>> {
    let mut st = tx.prepare("SELECT asset_id FROM asset_tags WHERE tag_id = ?1")?;
    Ok(st.query_map([tag], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

fn merge_in_tx(tx: &Transaction<'_>, src: i64, dst: i64) -> Result<()> {
    let affected = asset_ids_with_tag(tx, src)?;
    tx.execute(
        "INSERT OR IGNORE INTO asset_tags (asset_id, tag_id) SELECT asset_id, ?2 FROM asset_tags WHERE tag_id = ?1",
        params![src, dst],
    )?;
    tx.execute("DELETE FROM tags WHERE id = ?1", [src])?; // cascades asset_tags rows of src
    for a in affected {
        reindex(tx, a)?;
        changes::record(tx, "asset", &a.to_string(), "update")?;
    }
    changes::record(tx, "tag", &src.to_string(), "delete")?;
    changes::record(tx, "tag", &dst.to_string(), "update")?;
    Ok(())
}

impl Store {
    pub fn list_tags(&self) -> Result<Vec<TagCount>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT t.id, t.name, count(at.asset_id) FROM tags t
             LEFT JOIN asset_tags at ON at.tag_id = t.id
             GROUP BY t.id ORDER BY t.name COLLATE NOCASE",
        )?;
        let rows = st.query_map([], |r| {
            Ok(TagCount { id: r.get(0)?, name: r.get(1)?, count: r.get::<_, i64>(2)? as u64 })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Renames a tag; if another tag already has that name (case-insensitive),
    /// merges into it instead.
    pub fn rename_tag(&self, id: i64, new_name: &str) -> Result<RenameOutcome> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(StoreError::Invalid("empty tag name".into()));
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let exists: Option<i64> =
            tx.query_row("SELECT id FROM tags WHERE id = ?1", [id], |r| r.get(0)).optional()?;
        exists.ok_or_else(|| StoreError::NotFound(format!("tag {id}")))?;
        let clash: Option<i64> = tx
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE AND id != ?2",
                params![new_name, id],
                |r| r.get(0),
            )
            .optional()?;
        let outcome = if let Some(dst) = clash {
            merge_in_tx(&tx, id, dst)?;
            RenameOutcome { id: dst, merged_into: Some(dst) }
        } else {
            tx.execute("UPDATE tags SET name = ?2 WHERE id = ?1", params![id, new_name])?;
            for a in asset_ids_with_tag(&tx, id)? {
                reindex(&tx, a)?;
            }
            changes::record(&tx, "tag", &id.to_string(), "update")?;
            RenameOutcome { id, merged_into: None }
        };
        tx.commit()?;
        Ok(outcome)
    }

    /// Moves every use of `src` onto `dst` and deletes `src`.
    pub fn merge_tags(&self, src: i64, dst: i64) -> Result<()> {
        if src == dst {
            return Err(StoreError::Invalid("cannot merge a tag into itself".into()));
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for t in [src, dst] {
            let e: Option<i64> =
                tx.query_row("SELECT id FROM tags WHERE id = ?1", [t], |r| r.get(0)).optional()?;
            e.ok_or_else(|| StoreError::NotFound(format!("tag {t}")))?;
        }
        merge_in_tx(&tx, src, dst)?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_tag(&self, id: i64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let affected = asset_ids_with_tag(&tx, id)?;
        if tx.execute("DELETE FROM tags WHERE id = ?1", [id])? == 0 {
            return Err(StoreError::NotFound(format!("tag {id}")));
        }
        for a in affected {
            reindex(&tx, a)?;
            changes::record(&tx, "asset", &a.to_string(), "update")?;
        }
        changes::record(&tx, "tag", &id.to_string(), "delete")?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::assets::AssetQuery;
    use crate::assets::tests::new_asset;
    use crate::temp_store;

    fn tag(s: &crate::Store, name: &str) -> i64 {
        s.list_tags().unwrap().into_iter().find(|t| t.name.eq_ignore_ascii_case(name)).unwrap().id
    }

    #[test]
    fn counts_and_case_insensitive_uniqueness() {
        let (_d, s) = temp_store();
        let a = new_asset(&s, "token", "a");
        let b = new_asset(&s, "token", "b");
        s.set_asset_tags(a, &["Undead".into()]).unwrap();
        s.set_asset_tags(b, &["undead".into(), "boss".into()]).unwrap();
        let tags = s.list_tags().unwrap();
        assert_eq!(
            tags.iter().map(|t| (t.name.as_str(), t.count)).collect::<Vec<_>>(),
            [("boss", 1), ("Undead", 2)]
        );
    }

    #[test]
    fn rename_in_place_updates_search() {
        let (_d, s) = temp_store();
        let a = new_asset(&s, "token", "a");
        s.set_asset_tags(a, &["foo".into()]).unwrap();
        let out = s.rename_tag(tag(&s, "foo"), "bar").unwrap();
        assert_eq!(out.merged_into, None);
        let hits =
            s.search_assets(&AssetQuery { q: Some("bar".into()), ..Default::default() }).unwrap();
        assert_eq!(hits.total, 1);
    }

    #[test]
    fn rename_onto_existing_name_merges() {
        let (_d, s) = temp_store();
        let a = new_asset(&s, "token", "a");
        let b = new_asset(&s, "token", "b");
        s.set_asset_tags(a, &["goblin".into(), "Goblins".into()]).unwrap();
        s.set_asset_tags(b, &["Goblins".into()]).unwrap();
        let dst = tag(&s, "goblin");
        let out = s.rename_tag(tag(&s, "Goblins"), "GOBLIN").unwrap();
        assert_eq!(out.merged_into, Some(dst));
        let tags = s.list_tags().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(
            (tags[0].name.as_str(), tags[0].count),
            ("goblin", 2),
            "duplicate link on `a` collapsed"
        );
    }

    #[test]
    fn merge_and_delete() {
        let (_d, s) = temp_store();
        let a = new_asset(&s, "token", "a");
        s.set_asset_tags(a, &["x".into(), "y".into(), "z".into()]).unwrap();
        s.merge_tags(tag(&s, "x"), tag(&s, "y")).unwrap();
        assert!(s.merge_tags(tag(&s, "y"), tag(&s, "y")).is_err());
        s.delete_tag(tag(&s, "z")).unwrap();
        assert_eq!(s.get_asset(a).unwrap().tags, vec!["y".to_string()]);
        assert!(s.delete_tag(9999).is_err());
        assert!(s.rename_tag(tag(&s, "y"), " ").is_err());
    }
}
