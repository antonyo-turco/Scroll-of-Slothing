//! Assets (tokens, cut-outs, maps): CRUD, search (FTS5) and link management.
//! Spec: docs/features/library.md, docs/data-model.md.

use rusqlite::{
    Connection, OptionalExtension, Row, Transaction, params, params_from_iter,
    types::Value as SqlValue,
};
use serde::{Deserialize, Serialize};

use crate::{Result, Store, StoreError, changes, now};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewAsset {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub aliases: String,
    pub size_label: String,
    pub original_blob: String,
    pub rendered_blob: String,
    pub pdf_blob: Option<String>,
    pub thumb_blob: Option<String>,
    pub preview_blob: Option<String>,
    pub params_json: Option<String>,
    pub legacy_id: Option<i64>,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Asset {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub aliases: String,
    pub size_label: String,
    pub pinned: bool,
    pub original_blob: String,
    pub rendered_blob: String,
    pub pdf_blob: Option<String>,
    pub thumb_blob: Option<String>,
    pub preview_blob: Option<String>,
    pub params_json: Option<String>,
    pub legacy_id: Option<i64>,
    pub created: String,
    pub updated: String,
    pub tags: Vec<String>,
    pub campaign_ids: Vec<String>,
    pub session_ids: Vec<String>,
}

/// Metadata edits; `None` leaves a field unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetPatch {
    pub name: Option<String>,
    pub aliases: Option<String>,
    pub size_label: Option<String>,
    pub pinned: Option<bool>,
}

/// New image set for an existing asset (re-edit "update existing").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetImages {
    pub original_blob: String,
    pub rendered_blob: String,
    pub pdf_blob: Option<String>,
    pub thumb_blob: Option<String>,
    pub preview_blob: Option<String>,
    pub params_json: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetSort {
    #[default]
    Newest,
    Oldest,
    NameAsc,
    NameDesc,
    Kind,
    Size,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetQuery {
    /// Free text: prefix-matched against name, aliases and tag names (all words must match).
    pub q: Option<String>,
    /// Restrict to these kinds (empty = all).
    pub kinds: Vec<String>,
    /// Tag names; an asset must carry *all* of them.
    pub tags: Vec<String>,
    /// Direct campaign link **or** via any session of the campaign.
    pub campaign_id: Option<String>,
    pub session_id: Option<String>,
    pub pinned_only: bool,
    pub sort: AssetSort,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetPage {
    pub items: Vec<Asset>,
    pub total: u64,
}

const ASSET_COLS: &str = "id, kind, name, aliases, size_label, pinned, original_blob, rendered_blob, \
     pdf_blob, thumb_blob, preview_blob, params_json, legacy_id, created, updated";

fn row_to_asset(r: &Row<'_>) -> rusqlite::Result<Asset> {
    Ok(Asset {
        id: r.get(0)?,
        kind: r.get(1)?,
        name: r.get(2)?,
        aliases: r.get(3)?,
        size_label: r.get(4)?,
        pinned: r.get::<_, i64>(5)? != 0,
        original_blob: r.get(6)?,
        rendered_blob: r.get(7)?,
        pdf_blob: r.get(8)?,
        thumb_blob: r.get(9)?,
        preview_blob: r.get(10)?,
        params_json: r.get(11)?,
        legacy_id: r.get(12)?,
        created: r.get(13)?,
        updated: r.get(14)?,
        tags: Vec::new(),
        campaign_ids: Vec::new(),
        session_ids: Vec::new(),
    })
}

/// Builds an FTS5 query: every whitespace-separated word as a quoted prefix term.
pub(crate) fn fts_query(q: &str) -> Option<String> {
    let terms: Vec<String> = q
        .split_whitespace()
        .map(|w| w.replace('"', ""))
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    if terms.is_empty() { None } else { Some(terms.join(" ")) }
}

/// Re-indexes one asset in `assets_fts` (name, aliases, tag names).
pub(crate) fn reindex(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM assets_fts WHERE rowid = ?1", [id])?;
    conn.execute(
        "INSERT INTO assets_fts (rowid, name, aliases, tags)
         SELECT a.id, a.name, a.aliases,
                coalesce((SELECT group_concat(t.name, ' ') FROM asset_tags at
                          JOIN tags t ON t.id = at.tag_id WHERE at.asset_id = a.id), '')
         FROM assets a WHERE a.id = ?1",
        [id],
    )?;
    Ok(())
}

fn ensure_exists(conn: &Connection, id: i64) -> Result<()> {
    let found: Option<i64> =
        conn.query_row("SELECT id FROM assets WHERE id = ?1", [id], |r| r.get(0)).optional()?;
    found.map(|_| ()).ok_or_else(|| StoreError::NotFound(format!("asset {id}")))
}

/// Returns the tag id for `name` (case-insensitive), creating it if needed.
pub(crate) fn tag_id(conn: &Connection, name: &str) -> Result<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::Invalid("empty tag name".into()));
    }
    conn.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [name])?;
    Ok(conn.query_row("SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE", [name], |r| r.get(0))?)
}

fn load_links(conn: &Connection, asset: &mut Asset) -> Result<()> {
    let mut st = conn.prepare_cached(
        "SELECT t.name FROM asset_tags at JOIN tags t ON t.id = at.tag_id
         WHERE at.asset_id = ?1 ORDER BY t.name COLLATE NOCASE",
    )?;
    asset.tags = st.query_map([asset.id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    let mut st = conn.prepare_cached(
        "SELECT campaign_id FROM asset_campaigns WHERE asset_id = ?1 ORDER BY campaign_id",
    )?;
    asset.campaign_ids =
        st.query_map([asset.id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    let mut st = conn.prepare_cached(
        "SELECT session_id FROM asset_sessions WHERE asset_id = ?1 ORDER BY session_id",
    )?;
    asset.session_ids = st.query_map([asset.id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(())
}

fn touch(tx: &Transaction<'_>, id: i64) -> Result<()> {
    tx.execute("UPDATE assets SET updated = ?2 WHERE id = ?1", params![id, now()])?;
    Ok(())
}

impl Store {
    pub fn insert_asset(&self, a: &NewAsset) -> Result<i64> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let ts = now();
        tx.execute(
            "INSERT INTO assets (kind, name, aliases, size_label, pinned, original_blob, rendered_blob,
                 pdf_blob, thumb_blob, preview_blob, params_json, legacy_id, created, updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
            params![
                a.kind,
                a.name,
                a.aliases,
                a.size_label,
                a.pinned as i64,
                a.original_blob,
                a.rendered_blob,
                a.pdf_blob,
                a.thumb_blob,
                a.preview_blob,
                a.params_json,
                a.legacy_id,
                ts
            ],
        )?;
        let id = tx.last_insert_rowid();
        reindex(&tx, id)?;
        changes::record(&tx, "asset", &id.to_string(), "insert")?;
        tx.commit()?;
        Ok(id)
    }

    pub fn get_asset(&self, id: i64) -> Result<Asset> {
        let conn = self.conn();
        let mut asset = conn
            .query_row(
                &format!("SELECT {ASSET_COLS} FROM assets WHERE id = ?1"),
                [id],
                row_to_asset,
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("asset {id}")))?;
        load_links(&conn, &mut asset)?;
        Ok(asset)
    }

    pub fn asset_id_by_legacy(&self, legacy_id: i64) -> Result<Option<i64>> {
        Ok(self
            .conn()
            .query_row("SELECT id FROM assets WHERE legacy_id = ?1", [legacy_id], |r| r.get(0))
            .optional()?)
    }

    pub fn search_assets(&self, q: &AssetQuery) -> Result<AssetPage> {
        let mut wh: Vec<String> = Vec::new();
        let mut args: Vec<SqlValue> = Vec::new();
        if let Some(fts) = q.q.as_deref().and_then(fts_query) {
            wh.push("a.id IN (SELECT rowid FROM assets_fts WHERE assets_fts MATCH ?)".into());
            args.push(fts.into());
        }
        if !q.kinds.is_empty() {
            wh.push(format!("a.kind IN ({})", vec!["?"; q.kinds.len()].join(",")));
            args.extend(q.kinds.iter().cloned().map(SqlValue::from));
        }
        for t in &q.tags {
            wh.push(
                "a.id IN (SELECT at.asset_id FROM asset_tags at JOIN tags t ON t.id = at.tag_id
                          WHERE t.name = ? COLLATE NOCASE)"
                    .into(),
            );
            args.push(t.clone().into());
        }
        if let Some(c) = &q.campaign_id {
            wh.push(
                "(a.id IN (SELECT asset_id FROM asset_campaigns WHERE campaign_id = ?)
                  OR a.id IN (SELECT s.asset_id FROM asset_sessions s
                              JOIN sessions ss ON ss.id = s.session_id WHERE ss.campaign_id = ?))"
                    .into(),
            );
            args.push(c.clone().into());
            args.push(c.clone().into());
        }
        if let Some(s) = &q.session_id {
            wh.push("a.id IN (SELECT asset_id FROM asset_sessions WHERE session_id = ?)".into());
            args.push(s.clone().into());
        }
        if q.pinned_only {
            wh.push("a.pinned = 1".into());
        }
        let where_sql =
            if wh.is_empty() { String::new() } else { format!("WHERE {}", wh.join(" AND ")) };
        let order = match q.sort {
            AssetSort::Newest => "a.id DESC",
            AssetSort::Oldest => "a.id ASC",
            AssetSort::NameAsc => "a.name COLLATE NOCASE ASC, a.id DESC",
            AssetSort::NameDesc => "a.name COLLATE NOCASE DESC, a.id DESC",
            AssetSort::Kind => "a.kind ASC, a.name COLLATE NOCASE ASC",
            AssetSort::Size => {
                "CASE a.size_label WHEN 'Tiny' THEN 0 WHEN 'Small' THEN 1 WHEN 'Medium' THEN 2 \
                 WHEN 'Large' THEN 3 WHEN 'Huge' THEN 4 WHEN 'Gargantuan' THEN 5 ELSE 6 END, \
                 a.name COLLATE NOCASE ASC"
            }
        };
        let conn = self.conn();
        let total: i64 = conn.query_row(
            &format!("SELECT count(*) FROM assets a {where_sql}"),
            params_from_iter(args.iter()),
            |r| r.get(0),
        )?;
        let limit = q.limit.map(i64::from).unwrap_or(-1);
        let offset = i64::from(q.offset.unwrap_or(0));
        let sql = format!(
            "SELECT {} FROM assets a {where_sql} ORDER BY {order} LIMIT {limit} OFFSET {offset}",
            ASSET_COLS.split(", ").map(|c| format!("a.{c}")).collect::<Vec<_>>().join(", ")
        );
        let mut st = conn.prepare(&sql)?;
        let mut items: Vec<Asset> = st
            .query_map(params_from_iter(args.iter()), row_to_asset)?
            .collect::<rusqlite::Result<_>>()?;
        for a in &mut items {
            load_links(&conn, a)?;
        }
        Ok(AssetPage { items, total: total as u64 })
    }

    pub fn update_asset_meta(&self, id: i64, p: &AssetPatch) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        ensure_exists(&tx, id)?;
        if let Some(n) = &p.name {
            if n.trim().is_empty() {
                return Err(StoreError::Invalid("name must not be empty".into()));
            }
            tx.execute("UPDATE assets SET name = ?2 WHERE id = ?1", params![id, n.trim()])?;
        }
        if let Some(a) = &p.aliases {
            tx.execute("UPDATE assets SET aliases = ?2 WHERE id = ?1", params![id, a])?;
        }
        if let Some(s) = &p.size_label {
            tx.execute("UPDATE assets SET size_label = ?2 WHERE id = ?1", params![id, s])?;
        }
        if let Some(pin) = p.pinned {
            tx.execute("UPDATE assets SET pinned = ?2 WHERE id = ?1", params![id, pin as i64])?;
        }
        touch(&tx, id)?;
        reindex(&tx, id)?;
        changes::record(&tx, "asset", &id.to_string(), "update")?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_asset_images(&self, id: i64, img: &AssetImages) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        ensure_exists(&tx, id)?;
        tx.execute(
            "UPDATE assets SET original_blob = ?2, rendered_blob = ?3, pdf_blob = ?4, thumb_blob = ?5,
                 preview_blob = ?6, params_json = ?7 WHERE id = ?1",
            params![
                id,
                img.original_blob,
                img.rendered_blob,
                img.pdf_blob,
                img.thumb_blob,
                img.preview_blob,
                img.params_json
            ],
        )?;
        touch(&tx, id)?;
        changes::record(&tx, "asset", &id.to_string(), "update")?;
        tx.commit()?;
        Ok(())
    }

    /// Deletes assets (links cascade). Returns how many existed.
    pub fn delete_assets(&self, ids: &[i64]) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut n = 0;
        for id in ids {
            let deleted = tx.execute("DELETE FROM assets WHERE id = ?1", [id])?;
            if deleted > 0 {
                tx.execute("DELETE FROM assets_fts WHERE rowid = ?1", [id])?;
                changes::record(&tx, "asset", &id.to_string(), "delete")?;
                n += deleted;
            }
        }
        tx.commit()?;
        Ok(n)
    }

    /// Replaces the asset's tag set (names, case-insensitive; created as needed).
    pub fn set_asset_tags(&self, id: i64, names: &[String]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        ensure_exists(&tx, id)?;
        tx.execute("DELETE FROM asset_tags WHERE asset_id = ?1", [id])?;
        for n in names.iter().filter(|n| !n.trim().is_empty()) {
            let t = tag_id(&tx, n)?;
            tx.execute(
                "INSERT OR IGNORE INTO asset_tags (asset_id, tag_id) VALUES (?1, ?2)",
                params![id, t],
            )?;
        }
        touch(&tx, id)?;
        reindex(&tx, id)?;
        changes::record(&tx, "asset", &id.to_string(), "update")?;
        tx.commit()?;
        Ok(())
    }

    /// Adds one tag to many assets (bulk action).
    pub fn add_tag_to_assets(&self, ids: &[i64], name: &str) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let t = tag_id(&tx, name)?;
        for id in ids {
            ensure_exists(&tx, *id)?;
            tx.execute(
                "INSERT OR IGNORE INTO asset_tags (asset_id, tag_id) VALUES (?1, ?2)",
                params![id, t],
            )?;
            touch(&tx, *id)?;
            reindex(&tx, *id)?;
            changes::record(&tx, "asset", &id.to_string(), "update")?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_asset_campaigns(&self, id: i64, campaign_ids: &[String]) -> Result<()> {
        self.replace_links(id, "asset_campaigns", "campaign_id", campaign_ids)
    }

    pub fn set_asset_sessions(&self, id: i64, session_ids: &[String]) -> Result<()> {
        self.replace_links(id, "asset_sessions", "session_id", session_ids)
    }

    fn replace_links(&self, id: i64, table: &str, col: &str, targets: &[String]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        ensure_exists(&tx, id)?;
        tx.execute(&format!("DELETE FROM {table} WHERE asset_id = ?1"), [id])?;
        for t in targets {
            tx.execute(
                &format!("INSERT OR IGNORE INTO {table} (asset_id, {col}) VALUES (?1, ?2)"),
                params![id, t],
            )
            .map_err(|e| match e {
                rusqlite::Error::SqliteFailure(f, _)
                    if f.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    StoreError::NotFound(format!("{col} {t}"))
                }
                other => other.into(),
            })?;
        }
        touch(&tx, id)?;
        changes::record(&tx, "asset", &id.to_string(), "update")?;
        tx.commit()?;
        Ok(())
    }

    /// Adds many assets to a session or campaign (bulk actions).
    pub fn link_assets(&self, ids: &[i64], target: LinkTarget<'_>) -> Result<()> {
        let (table, col, val) = match target {
            LinkTarget::Campaign(c) => ("asset_campaigns", "campaign_id", c),
            LinkTarget::Session(s) => ("asset_sessions", "session_id", s),
        };
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for id in ids {
            ensure_exists(&tx, *id)?;
            tx.execute(
                &format!("INSERT OR IGNORE INTO {table} (asset_id, {col}) VALUES (?1, ?2)"),
                params![id, val],
            )
            .map_err(|_| StoreError::NotFound(format!("{col} {val}")))?;
            changes::record(&tx, "asset", &id.to_string(), "update")?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn unlink_asset(&self, id: i64, target: LinkTarget<'_>) -> Result<()> {
        let (table, col, val) = match target {
            LinkTarget::Campaign(c) => ("asset_campaigns", "campaign_id", c),
            LinkTarget::Session(s) => ("asset_sessions", "session_id", s),
        };
        let conn = self.conn();
        conn.execute(
            &format!("DELETE FROM {table} WHERE asset_id = ?1 AND {col} = ?2"),
            params![id, val],
        )?;
        changes::record(&conn, "asset", &id.to_string(), "update")?;
        Ok(())
    }

    /// Hashes of every blob still referenced by an asset (for garbage collection).
    pub fn referenced_blobs(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT original_blob FROM assets UNION SELECT rendered_blob FROM assets
             UNION SELECT pdf_blob FROM assets WHERE pdf_blob IS NOT NULL
             UNION SELECT thumb_blob FROM assets WHERE thumb_blob IS NOT NULL
             UNION SELECT preview_blob FROM assets WHERE preview_blob IS NOT NULL",
        )?;
        Ok(st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LinkTarget<'a> {
    Campaign(&'a str),
    Session(&'a str),
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::temp_store;

    pub(crate) fn new_asset(s: &Store, kind: &str, name: &str) -> i64 {
        let b = s.put_blob(name.as_bytes(), "image/png", Some((1, 1))).unwrap();
        s.insert_asset(&NewAsset {
            kind: kind.into(),
            name: name.into(),
            aliases: String::new(),
            size_label: "Medium".into(),
            original_blob: b.hash.clone(),
            rendered_blob: b.hash,
            pdf_blob: None,
            thumb_blob: None,
            preview_blob: None,
            params_json: None,
            legacy_id: None,
            pinned: false,
        })
        .unwrap()
    }

    fn names(page: &AssetPage) -> Vec<&str> {
        page.items.iter().map(|a| a.name.as_str()).collect()
    }

    #[test]
    fn insert_get_and_not_found() {
        let (_d, s) = temp_store();
        let id = new_asset(&s, "token", "Goblin");
        let a = s.get_asset(id).unwrap();
        assert_eq!((a.kind.as_str(), a.name.as_str(), a.pinned), ("token", "Goblin", false));
        assert!(matches!(s.get_asset(999), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn search_text_matches_name_aliases_and_tags_by_prefix() {
        let (_d, s) = temp_store();
        let g = new_asset(&s, "token", "Goblin Boss");
        let o = new_asset(&s, "token", "Orc");
        new_asset(&s, "map", "Dungeon");
        s.update_asset_meta(
            o,
            &AssetPatch { aliases: Some("Grishnak\nbrute".into()), ..Default::default() },
        )
        .unwrap();
        s.set_asset_tags(g, &["Goblinoid".into(), "boss".into()]).unwrap();

        let q = |text: &str| {
            s.search_assets(&AssetQuery { q: Some(text.into()), ..Default::default() }).unwrap()
        };
        assert_eq!(names(&q("gob")), ["Goblin Boss"]);
        assert_eq!(names(&q("grish")), ["Orc"], "aliases are searchable");
        assert_eq!(names(&q("goblinoid")), ["Goblin Boss"], "tags are searchable");
        assert_eq!(names(&q("gob boss")), ["Goblin Boss"], "all words must match");
        assert!(q("dragon").items.is_empty());
        assert_eq!(q("  ").total, 3, "blank query matches everything");
        assert_eq!(q("\"unbalanced").total, 0, "quotes are sanitized, no SQL error");
    }

    #[test]
    fn filters_kind_tags_pinned_and_sorting() {
        let (_d, s) = temp_store();
        let a = new_asset(&s, "token", "beta");
        let b = new_asset(&s, "token", "Alpha");
        let c = new_asset(&s, "map", "gamma");
        s.set_asset_tags(a, &["x".into(), "y".into()]).unwrap();
        s.set_asset_tags(b, &["X".into()]).unwrap();
        s.update_asset_meta(c, &AssetPatch { pinned: Some(true), ..Default::default() }).unwrap();
        s.update_asset_meta(
            b,
            &AssetPatch { size_label: Some("Tiny".into()), ..Default::default() },
        )
        .unwrap();

        let run = |q: AssetQuery| s.search_assets(&q).unwrap();
        assert_eq!(
            names(&run(AssetQuery { kinds: vec!["map".into()], ..Default::default() })),
            ["gamma"]
        );
        assert_eq!(
            names(&run(AssetQuery { tags: vec!["x".into(), "Y".into()], ..Default::default() })),
            ["beta"],
            "tags are AND-ed and case-insensitive"
        );
        assert_eq!(names(&run(AssetQuery { pinned_only: true, ..Default::default() })), ["gamma"]);
        assert_eq!(names(&run(AssetQuery::default())), ["gamma", "Alpha", "beta"], "newest first");
        assert_eq!(
            names(&run(AssetQuery { sort: AssetSort::NameAsc, ..Default::default() })),
            ["Alpha", "beta", "gamma"]
        );
        assert_eq!(
            names(&run(AssetQuery { sort: AssetSort::Size, ..Default::default() })),
            ["Alpha", "beta", "gamma"],
            "Tiny before Medium"
        );
        let page = run(AssetQuery { limit: Some(1), offset: Some(1), ..Default::default() });
        assert_eq!((names(&page), page.total), (vec!["Alpha"], 3));
    }

    #[test]
    fn campaign_filter_includes_session_linked_assets() {
        let (_d, s) = temp_store();
        let c = s.create_campaign("Curse of Strahd").unwrap();
        let sess = s.create_session(&c.id, "Session 1", "").unwrap();
        let direct = new_asset(&s, "token", "direct");
        let via = new_asset(&s, "token", "via session");
        new_asset(&s, "token", "unrelated");
        s.set_asset_campaigns(direct, std::slice::from_ref(&c.id)).unwrap();
        s.link_assets(&[via], LinkTarget::Session(&sess.id)).unwrap();

        let by_c = s
            .search_assets(&AssetQuery { campaign_id: Some(c.id.clone()), ..Default::default() })
            .unwrap();
        assert_eq!(names(&by_c), ["via session", "direct"]);
        let by_s = s
            .search_assets(&AssetQuery { session_id: Some(sess.id.clone()), ..Default::default() })
            .unwrap();
        assert_eq!(names(&by_s), ["via session"]);
        assert_eq!(s.get_asset(via).unwrap().session_ids, vec![sess.id.clone()]);
    }

    #[test]
    fn linking_unknown_campaign_is_not_found() {
        let (_d, s) = temp_store();
        let a = new_asset(&s, "token", "x");
        assert!(matches!(s.set_asset_campaigns(a, &["nope".into()]), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn delete_cascades_links_and_fts() {
        let (_d, s) = temp_store();
        let a = new_asset(&s, "token", "Zombie");
        s.set_asset_tags(a, &["undead".into()]).unwrap();
        assert_eq!(s.delete_assets(&[a, 12345]).unwrap(), 1);
        let n: i64 =
            s.conn().query_row("SELECT count(*) FROM asset_tags", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        let q = s
            .search_assets(&AssetQuery { q: Some("zombie".into()), ..Default::default() })
            .unwrap();
        assert_eq!(q.total, 0);
    }

    #[test]
    fn empty_name_rejected_and_writes_are_logged() {
        let (_d, s) = temp_store();
        let a = new_asset(&s, "token", "x");
        assert!(
            s.update_asset_meta(a, &AssetPatch { name: Some("  ".into()), ..Default::default() })
                .is_err()
        );
        let before = s.latest_change_seq().unwrap();
        s.add_tag_to_assets(&[a], "new").unwrap();
        assert!(s.latest_change_seq().unwrap() > before);
        assert_eq!(s.get_asset(a).unwrap().tags, vec!["new".to_string()]);
    }
}
