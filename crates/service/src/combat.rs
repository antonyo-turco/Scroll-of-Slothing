//! Combat tracker service. Spec: `docs/features/combat-tracker.md`.
//!
//! The tracker is autosaved in the workspace key `combat.tracker` (5etools
//! serial form) and every change is logged as `combat/tracker`, so the UI and
//! the MCP process stay in sync. Creatures come from a local copy of a 5etools
//! mirror's bestiary (`<data dir>/fivetools/bestiary/`), synced by a Job.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use serde_json::{Value, json};
use sleuth_core::combat::creature::{self, ColumnKind};
use sleuth_core::combat::link::{self, EncounterImport, LinkKind};
use sleuth_core::combat::{CONDITIONS, Row, Sort, Tracker};
use sleuth_store::Store;

use crate::{JobId, Result, Service, ServiceError};

const KEY: &str = "combat.tracker";
const LOCK_KEY: &str = "_sleuthLocked";

/// One search hit in the cached bestiary.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BestiaryEntry {
    pub hash: String,
    pub name: String,
    pub source: String,
    pub cr: Option<String>,
    pub hp: Option<i64>,
    pub ac: Value,
    pub token_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BestiaryStatus {
    pub base_url: String,
    pub synced: bool,
    pub files: usize,
    pub creatures: usize,
}

/// What `combat_import_link` did.
#[derive(Debug, Clone, Serialize)]
pub struct CombatImport {
    pub kind: LinkKind,
    pub tracker: Tracker,
    /// For encounters: rows added, creatures not in the cache, scaled ones.
    pub encounter: Option<EncounterImport>,
}

/// Fields to change on one row (MCP); None leaves a field as it is.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct RowPatch {
    pub name: Option<String>,
    /// Display name; an empty string clears it.
    pub custom_name: Option<String>,
    pub hp_current: Option<i64>,
    pub hp_max: Option<i64>,
    /// Damage (positive) or healing (negative) applied to the current HP.
    pub damage: Option<i64>,
    pub initiative: Option<i64>,
    pub notes: Option<String>,
    pub is_player_visible: Option<bool>,
    /// Condition ids to remove.
    pub remove_conditions: Option<Vec<String>>,
}

/// Built-in conditions and stat column types, for pickers.
#[derive(Debug, Clone, Serialize)]
pub struct CombatCatalog {
    pub conditions: Vec<(String, String)>,
    pub columns: Vec<ColumnKind>,
}

struct Bestiary {
    files: usize,
    by_hash: HashMap<String, Value>,
    list: Vec<BestiaryEntry>,
}

/// Loaded bestiaries per data dir (tests use several).
fn cache() -> &'static Mutex<HashMap<PathBuf, Arc<Bestiary>>> {
    static C: OnceLock<Mutex<HashMap<PathBuf, Arc<Bestiary>>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

fn bestiary_dir(root: &Path) -> PathBuf {
    root.join("fivetools").join("bestiary")
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn rng(n: u32) -> u32 {
    fastrand::u32(0..n.max(1))
}

fn io(e: impl std::fmt::Display) -> ServiceError {
    ServiceError::Io(e.to_string())
}

/// Reads the cached `index.json` and bestiary files; `_copy` creatures are
/// resolved against their base.
fn load_bestiary(dir: &Path, base_url: &str) -> Result<Bestiary> {
    let index: HashMap<String, String> = match std::fs::read(dir.join("index.json")) {
        Ok(b) => serde_json::from_slice(&b).map_err(io)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
        Err(e) => return Err(io(e)),
    };
    let mut mons: Vec<Value> = vec![];
    let mut files = 0;
    for file in index.values() {
        let Ok(b) = std::fs::read(dir.join(file)) else { continue };
        let Ok(v) = serde_json::from_slice::<Value>(&b) else { continue };
        files += 1;
        if let Some(Value::Array(a)) = v.get("monster") {
            mons.extend(a.iter().cloned());
        }
    }
    let hash_of = |m: &Value| {
        let n = m.get("name").and_then(Value::as_str)?;
        let s = m.get("source").and_then(Value::as_str)?;
        Some(creature::creature_hash(n, s))
    };
    let mut by_hash: HashMap<String, Value> = HashMap::new();
    let mut copies = vec![];
    for m in mons {
        if m.get("_copy").is_some() {
            copies.push(m);
        } else if let Some(h) = hash_of(&m) {
            by_hash.insert(h, m);
        }
    }
    // copies of copies resolve over a few passes
    for _ in 0..4 {
        let mut left = vec![];
        for c in copies {
            let base = c.get("_copy").and_then(hash_of).and_then(|h| by_hash.get(&h).cloned());
            match (base, hash_of(&c)) {
                (Some(b), Some(h)) => {
                    by_hash.insert(h, creature::resolve_copy(&c, &b));
                }
                _ => left.push(c),
            }
        }
        copies = left;
        if copies.is_empty() {
            break;
        }
    }
    let mut list: Vec<BestiaryEntry> = by_hash
        .iter()
        .map(|(h, m)| BestiaryEntry {
            hash: h.clone(),
            name: m["name"].as_str().unwrap_or("").into(),
            source: m["source"].as_str().unwrap_or("").into(),
            cr: m
                .get("cr")
                .map(|c| c.get("cr").unwrap_or(c))
                .and_then(Value::as_str)
                .map(Into::into),
            hp: creature::hp_average(m),
            ac: m
                .get("ac")
                .and_then(|a| a.get(0))
                .map(|a| a.get("ac").unwrap_or(a).clone())
                .unwrap_or(Value::Null),
            token_url: m
                .get("hasToken")
                .and_then(Value::as_bool)
                .filter(|t| *t)
                .and_then(|_| creature::token_url(m, base_url)),
        })
        .collect();
    list.sort_by(|a, b| {
        a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.source.cmp(&b.source))
    });
    Ok(Bestiary { files, by_hash, list })
}

impl Service {
    fn fivetools_base(&self, s: &Store) -> Result<String> {
        Ok(s.load_settings()?.fivetools_base_url)
    }

    fn bestiary(&self, s: &Store) -> Result<Arc<Bestiary>> {
        let root = self.data_dir().to_path_buf();
        if let Some(b) = cache().lock().unwrap_or_else(|p| p.into_inner()).get(&root) {
            return Ok(b.clone());
        }
        let b = Arc::new(load_bestiary(&bestiary_dir(&root), &self.fivetools_base(s)?)?);
        cache().lock().unwrap_or_else(|p| p.into_inner()).insert(root, b.clone());
        Ok(b)
    }

    fn creature(&self, s: &Store, hash: &str) -> Result<Value> {
        self.bestiary(s)?.by_hash.get(&hash.to_lowercase()).cloned().ok_or_else(|| {
            ServiceError::NotFound(format!("creature {hash:?} (sync the bestiary first?)"))
        })
    }

    /// The lock is temporary in 5etools (not in links), so the autosave keeps
    /// it under its own key.
    fn load_tracker(s: &Store) -> Result<Tracker> {
        Ok(match s.workspace_get::<Value>(KEY)? {
            Some(v) => {
                let mut t = Tracker::from_serial(&v, &mut new_id);
                t.is_locked = t.extra.remove(LOCK_KEY).and_then(|l| l.as_bool()).unwrap_or(false);
                t
            }
            None => Tracker::default(),
        })
    }

    fn save_tracker(s: &Store, t: &Tracker) -> Result<()> {
        let mut v = t.to_serial();
        v[LOCK_KEY] = json!(t.is_locked);
        s.workspace_set(KEY, &v)?;
        s.record_change("combat", "tracker", "update")?;
        Ok(())
    }

    /// Loads the tracker, applies `f` (refused while locked when `edit`), saves it.
    async fn combat_edit<T, F>(&self, edit: bool, f: F) -> Result<(Tracker, T)>
    where
        T: Send + 'static,
        F: FnOnce(&Service, &Store, &mut Tracker) -> Result<T> + Send + 'static,
    {
        let svc = self.clone();
        self.blocking(move |s| {
            let mut t = Self::load_tracker(s)?;
            if edit && t.is_locked {
                return Err(ServiceError::InvalidInput("the tracker is locked".into()));
            }
            let out = f(&svc, s, &mut t)?;
            Self::save_tracker(s, &t)?;
            Ok((t, out))
        })
        .await
    }

    // ── tracker ──

    pub async fn combat_get(&self) -> Result<Tracker> {
        self.blocking(Self::load_tracker).await
    }

    /// Replaces the whole tracker (UI edits of names, HP, notes, settings …).
    /// Cells missing for a stat column (a column just added) are filled from
    /// each creature's stat block, as 5etools does.
    pub async fn combat_set(&self, tracker: Tracker) -> Result<Tracker> {
        self.combat_edit(false, move |svc, s, t| {
            *t = tracker;
            let b = svc.bestiary(s)?;
            let base = svc.fivetools_base(s)?;
            for i in 0..t.rows.len() {
                let missing: Vec<_> = t
                    .stats_cols
                    .iter()
                    .filter(|c| !t.rows[i].cells.iter().any(|x| x.id == c.id))
                    .cloned()
                    .collect();
                if missing.is_empty() {
                    continue;
                }
                let r = &t.rows[i];
                let mon = r
                    .source
                    .as_deref()
                    .and_then(|src| b.by_hash.get(&creature::creature_hash(&r.name, src)));
                let probe = Tracker { stats_cols: missing, ..Tracker::default() };
                let cells = probe.initial_cells(mon, &base);
                t.rows[i].cells.extend(cells);
            }
            t.sort_rows();
            Ok(())
        })
        .await
        .map(|(t, _)| t)
    }

    pub async fn combat_next_turn(&self) -> Result<Tracker> {
        self.combat_edit(false, |svc, s, t| {
            let b = svc.bestiary(s)?;
            let bonus = |r: &Row| {
                let h = creature::creature_hash(&r.name, r.source.as_deref()?);
                creature::initiative_bonus(b.by_hash.get(&h)?)
            };
            t.next_turn(&mut rng, &bonus);
            Ok(())
        })
        .await
        .map(|(t, _)| t)
    }

    pub async fn combat_prev_turn(&self) -> Result<Tracker> {
        self.combat_edit(false, |_, _, t| {
            t.prev_turn();
            Ok(())
        })
        .await
        .map(|(t, _)| t)
    }

    pub async fn combat_sort(&self, sort: Sort) -> Result<Tracker> {
        self.combat_edit(false, move |_, _, t| {
            t.set_sort(sort);
            Ok(())
        })
        .await
        .map(|(t, _)| t)
    }

    pub async fn combat_add_player(&self, name: Option<String>) -> Result<Tracker> {
        self.combat_edit(true, move |_, _, t| {
            let mut row = t.new_player(&mut new_id);
            row.name = name.unwrap_or_default();
            t.rows.push(row);
            t.sort_rows();
            Ok(())
        })
        .await
        .map(|(t, _)| t)
    }

    /// Adds `count` copies of a cached creature (by 5etools hash, e.g. `goblin_mm`).
    pub async fn combat_add_creature(
        &self,
        hash: String,
        count: u32,
        roll_hp: Option<bool>,
        custom_name: Option<String>,
    ) -> Result<Tracker> {
        if !(1..=100).contains(&count) {
            return Err(ServiceError::InvalidInput("count must be 1 to 100".into()));
        }
        self.combat_edit(true, move |svc, s, t| {
            let mon = svc.creature(s, &hash)?;
            let base = svc.fivetools_base(s)?;
            let roll = roll_hp.unwrap_or(t.is_roll_hp);
            t.add_creatures(&mon, count, roll, custom_name, &base, &mut rng, &mut new_id);
            Ok(())
        })
        .await
        .map(|(t, _)| t)
    }

    /// "Add Another": copies a row; `reroll` rolls fresh initiative and HP.
    pub async fn combat_duplicate(&self, id: String, reroll: bool) -> Result<Tracker> {
        self.combat_edit(true, move |svc, s, t| {
            let row = t
                .rows
                .iter()
                .find(|r| r.id == id)
                .ok_or_else(|| ServiceError::NotFound(format!("row {id}")))?;
            let mon = match (reroll, row.source.as_deref()) {
                (true, Some(src)) => svc.creature(s, &creature::creature_hash(&row.name, src)).ok(),
                _ => None,
            };
            t.duplicate(&id, mon.as_ref(), &mut rng, &mut new_id);
            Ok(())
        })
        .await
        .map(|(t, _)| t)
    }

    /// Deletes a row, or every similar row.
    pub async fn combat_delete(&self, id: String, similar: bool) -> Result<Tracker> {
        self.combat_edit(true, move |_, _, t| {
            if t.delete(&id, similar) == 0 {
                return Err(ServiceError::NotFound(format!("row {id}")));
            }
            Ok(())
        })
        .await
        .map(|(t, _)| t)
    }

    pub async fn combat_add_condition(
        &self,
        row_id: String,
        name: String,
        color: Option<String>,
        turns: Option<i64>,
    ) -> Result<Tracker> {
        self.combat_edit(true, move |_, _, t| {
            let color = color
                .or_else(|| {
                    let builtin = CONDITIONS
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case(&name))
                        .map(|(_, c)| *c);
                    let custom =
                        t.conditions_custom.iter().find(|c| c.name.eq_ignore_ascii_case(&name));
                    builtin.map(Into::into).or_else(|| custom.map(|c| c.color.clone()))
                })
                .unwrap_or_else(|| "#888888".into());
            if !t.add_condition(&row_id, name.trim(), &color, turns, &mut new_id) {
                return Err(ServiceError::NotFound(format!("row {row_id}")));
            }
            Ok(())
        })
        .await
        .map(|(t, _)| t)
    }

    pub async fn combat_update_row(&self, id: String, patch: RowPatch) -> Result<Tracker> {
        self.combat_edit(true, move |_, _, t| {
            let invert = t.is_invert_wound_direction;
            let r = t
                .rows
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or_else(|| ServiceError::NotFound(format!("row {id}")))?;
            if let Some(v) = patch.name {
                r.name = v;
            }
            if let Some(v) = patch.custom_name {
                r.custom_name = Some(v).filter(|v| !v.trim().is_empty());
            }
            if let Some(v) = patch.hp_max {
                r.hp_max = Some(v);
            }
            if let Some(v) = patch.hp_current {
                r.hp_current = Some(v);
            }
            if let Some(d) = patch.damage {
                // as damage, the counter goes up; as HP, down
                let cur = r.hp_current.unwrap_or(if invert { 0 } else { r.hp_max.unwrap_or(0) });
                r.hp_current = Some(if invert { cur + d } else { cur - d });
            }
            if let Some(v) = patch.initiative {
                r.initiative = Some(v);
            }
            if let Some(v) = patch.notes {
                r.notes = v;
            }
            if let Some(v) = patch.is_player_visible {
                r.is_player_visible = v;
            }
            if let Some(ids) = patch.remove_conditions {
                r.conditions.retain(|c| !ids.contains(&c.id));
            }
            t.sort_rows();
            Ok(())
        })
        .await
        .map(|(t, _)| t)
    }

    /// Reset: the default party, round 1.
    pub async fn combat_reset(&self) -> Result<Tracker> {
        self.combat_edit(true, |_, _, t| {
            t.reset(&mut new_id);
            Ok(())
        })
        .await
        .map(|(t, _)| t)
    }

    /// The 5etools "Copy Combat Link" URL for the current tracker.
    pub async fn combat_link(&self) -> Result<String> {
        let svc = self.clone();
        self.blocking(move |s| {
            Ok(link::combat_url(&svc.fivetools_base(s)?, &Self::load_tracker(s)?))
        })
        .await
    }

    /// Opens a `#combat=` link (replacing the tracker) or rolls an
    /// `#encounter=` link into it with the import settings.
    pub async fn combat_import_link(&self, text: String) -> Result<CombatImport> {
        let (kind, data) =
            link::parse(&text).map_err(|e| ServiceError::InvalidInput(e.to_string()))?;
        let (tracker, encounter) = self
            .combat_edit(true, move |svc, s, t| {
                Ok(match kind {
                    LinkKind::Combat => {
                        *t = Tracker::from_serial(&data, &mut new_id);
                        t.sort_rows();
                        None
                    }
                    LinkKind::Encounter => {
                        let b = svc.bestiary(s)?;
                        let base = svc.fivetools_base(s)?;
                        let lookup = |h: &str| b.by_hash.get(h).cloned();
                        Some(link::import_encounter(
                            t,
                            &data,
                            &lookup,
                            &base,
                            &mut rng,
                            &mut new_id,
                        ))
                    }
                })
            })
            .await?;
        Ok(CombatImport { kind, tracker, encounter })
    }

    pub fn combat_catalog(&self) -> CombatCatalog {
        CombatCatalog {
            conditions: CONDITIONS.iter().map(|(n, c)| (n.to_string(), c.to_string())).collect(),
            columns: creature::column_kinds(),
        }
    }

    // ── bestiary ──

    pub async fn bestiary_status(&self) -> Result<BestiaryStatus> {
        let svc = self.clone();
        self.blocking(move |s| {
            let b = svc.bestiary(s)?;
            Ok(BestiaryStatus {
                base_url: svc.fivetools_base(s)?,
                synced: b.files > 0,
                files: b.files,
                creatures: b.list.len(),
            })
        })
        .await
    }

    /// Creatures whose name contains `query` (prefix matches first).
    pub async fn bestiary_search(
        &self,
        query: String,
        limit: Option<usize>,
    ) -> Result<Vec<BestiaryEntry>> {
        let svc = self.clone();
        self.blocking(move |s| {
            let b = svc.bestiary(s)?;
            let q = query.trim().to_lowercase();
            let (mut pre, mut mid): (Vec<_>, Vec<_>) = (vec![], vec![]);
            for e in &b.list {
                let n = e.name.to_lowercase();
                if n.starts_with(&q) {
                    pre.push(e.clone());
                } else if n.contains(&q) || e.hash == q {
                    mid.push(e.clone());
                }
            }
            pre.extend(mid);
            pre.truncate(limit.unwrap_or(50).clamp(1, 500));
            Ok(pre)
        })
        .await
    }

    /// A cached creature's full 5etools JSON.
    pub async fn bestiary_get(&self, hash: String) -> Result<Value> {
        let svc = self.clone();
        self.blocking(move |s| svc.creature(s, &hash)).await
    }

    /// Downloads the mirror's bestiary into the cache (job result:
    /// `{files, creatures}`). The old cache stays until the sync succeeds.
    pub async fn bestiary_sync(&self) -> Result<JobId> {
        let base = self.blocking(|s| Ok(s.load_settings()?.fivetools_base_url)).await?;
        let root = self.data_dir().to_path_buf();
        Ok(self.spawn_job("bestiary sync", move |ctx| {
            let dir = bestiary_dir(&root);
            let tmp = dir.with_extension("part");
            let _ = std::fs::remove_dir_all(&tmp);
            std::fs::create_dir_all(&tmp).map_err(io)?;
            let client = reqwest::blocking::Client::builder()
                .user_agent(concat!("sleuth/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .map_err(io)?;
            let get = |path: &str| -> Result<Vec<u8>> {
                let url = format!("{base}/data/bestiary/{path}");
                let r = client
                    .get(&url)
                    .send()
                    .and_then(|r| r.error_for_status())
                    .map_err(|e| ServiceError::Io(format!("{url}: {e}")))?;
                Ok(r.bytes().map_err(io)?.to_vec())
            };
            ctx.progress(0.0, format!("reading {base}/data/bestiary/index.json"));
            let index_bytes = get("index.json")?;
            let index: HashMap<String, String> =
                serde_json::from_slice(&index_bytes).map_err(|e| {
                    ServiceError::InvalidInput(format!(
                        "index.json is not a 5etools bestiary index: {e}"
                    ))
                })?;
            std::fs::write(tmp.join("index.json"), &index_bytes).map_err(io)?;
            let total = index.len().max(1);
            for (i, file) in index.values().enumerate() {
                ctx.check()?;
                if file.contains(['/', '\\']) || file.contains("..") {
                    continue;
                }
                ctx.progress(i as f32 / total as f32, format!("{file} ({}/{total})", i + 1));
                std::fs::write(tmp.join(file), get(file)?).map_err(io)?;
            }
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::rename(&tmp, &dir).map_err(io)?;
            let b = load_bestiary(&dir, &base)?;
            let out = json!({"files": b.files, "creatures": b.list.len()});
            cache().lock().unwrap_or_else(|p| p.into_inner()).insert(root.clone(), Arc::new(b));
            let _ = ctx.service().store().record_change("bestiary", "cache", "update");
            Ok(out)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temp_service;

    fn seed_bestiary(root: &Path) {
        let dir = bestiary_dir(root);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.json"), r#"{"MM": "bestiary-mm.json"}"#).unwrap();
        let mm = json!({"monster": [
            {"name": "Goblin", "source": "MM", "hp": {"average": 7, "formula": "2d6"}, "ac": [{"ac": 15}],
             "dex": 14, "cr": "1/4", "hasToken": true},
            {"name": "Goblin Boss", "source": "MM", "_copy": {"name": "Goblin", "source": "MM"}, "cr": "1"},
            {"name": "Ogre", "source": "MM", "hp": {"average": 59, "formula": "7d10+21"}, "ac": [11], "dex": 8}
        ]});
        std::fs::write(dir.join("bestiary-mm.json"), mm.to_string()).unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn search_add_turns_and_links() {
        let (d, svc) = temp_service();
        seed_bestiary(d.path());
        let st = svc.bestiary_status().await.unwrap();
        assert_eq!(
            (st.synced, st.creatures, st.base_url.as_str()),
            (true, 3, "http://localhost:8080")
        );
        let hits = svc.bestiary_search("gob".into(), None).await.unwrap();
        let names: Vec<_> = hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["Goblin", "Goblin Boss"]);
        assert_eq!(hits[1].hp, Some(7), "copies take the base's stats");
        assert_eq!(
            hits[0].token_url.as_deref(),
            Some("http://localhost:8080/img/bestiary/tokens/MM/Goblin.webp")
        );
        assert_eq!(svc.bestiary_search("ogre".into(), None).await.unwrap()[0].ac, json!(11));

        svc.combat_add_player(Some("Vex".into())).await.unwrap();
        let t = svc.combat_add_creature("goblin_mm".into(), 2, Some(false), None).await.unwrap();
        assert_eq!(t.rows.len(), 3);
        assert!(t.rows.iter().filter(|r| r.name == "Goblin").all(|r| r.hp_max == Some(7)));
        assert!(svc.combat_add_creature("nope_mm".into(), 1, None, None).await.is_err());
        assert!(svc.combat_add_creature("goblin_mm".into(), 0, None, None).await.is_err());

        let t = svc.combat_next_turn().await.unwrap();
        assert_eq!(t.active().len(), 1);
        let goblin = t.rows.iter().find(|r| r.name == "Goblin").unwrap().id.clone();
        let t =
            svc.combat_add_condition(goblin.clone(), "prone".into(), None, Some(2)).await.unwrap();
        let c = &t.rows.iter().find(|r| r.id == goblin).unwrap().conditions[0];
        assert_eq!((c.color.as_str(), c.turns), ("#5e60a0", Some(2)));

        // a link restores the same combat
        let url = svc.combat_link().await.unwrap();
        svc.combat_reset().await.unwrap();
        assert!(svc.combat_get().await.unwrap().rows.is_empty());
        let imp = svc.combat_import_link(url).await.unwrap();
        assert_eq!((imp.kind, imp.tracker.rows.len()), (LinkKind::Combat, 3));
        assert_eq!(svc.combat_get().await.unwrap(), imp.tracker, "autosaved");

        // a locked tracker refuses edits but still turns
        let mut locked = imp.tracker.clone();
        locked.is_locked = true;
        svc.combat_set(locked).await.unwrap();
        assert!(svc.combat_add_player(None).await.is_err());
        assert!(svc.combat_next_turn().await.is_ok());
        assert!(svc.combat_update_row(goblin.clone(), RowPatch::default()).await.is_err());
        let mut open = svc.combat_get().await.unwrap();
        open.is_locked = false;
        svc.combat_set(open).await.unwrap();
        let p = RowPatch {
            damage: Some(3),
            remove_conditions: Some(vec![c.id.clone()]),
            ..RowPatch::default()
        };
        let t = svc.combat_update_row(goblin.clone(), p).await.unwrap();
        let g = t.rows.iter().find(|r| r.id == goblin).unwrap();
        assert_eq!((g.hp_current, g.conditions.len()), (Some(4), 0));

        // a new column fills from each creature's stat block
        let mut t = svc.combat_get().await.unwrap();
        t.stats_cols.push(sleuth_core::combat::Column {
            id: "ac".into(),
            is_editable: true,
            is_player_visible: 0,
            populate_with: "armorClass".into(),
            abbreviation: "AC".into(),
        });
        let t = svc.combat_set(t).await.unwrap();
        let g = t.rows.iter().find(|r| r.id == goblin).unwrap();
        assert_eq!(g.cells.iter().find(|c| c.id == "ac").unwrap().entity["value"], json!(15));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn encounter_links_report_missing_creatures() {
        let (d, svc) = temp_service();
        seed_bestiary(d.path());
        let enc = json!({"items": [{"h": "ogre_mm", "c": 2}, {"h": "kraken_mm"}]});
        let text =
            format!("http://localhost:8080/dmscreen.html#encounter={}", link::compress(&enc));
        let imp = svc.combat_import_link(text).await.unwrap();
        let r = imp.encounter.unwrap();
        assert_eq!((r.rows_added, r.missing), (2, vec!["kraken_mm".to_string()]));
        assert!(imp.tracker.rows.iter().all(|r| r.name == "Ogre" && r.initiative.is_some()));
        assert!(svc.combat_import_link("not a link".into()).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_reports_an_unreachable_mirror() {
        let (_d, svc) = temp_service();
        let mut p = serde_json::Map::new();
        p.insert("fivetools_base_url".into(), json!("http://127.0.0.1:9"));
        svc.settings_set(p).await.unwrap();
        let id = svc.bestiary_sync().await.unwrap();
        let st = loop {
            let st = svc.job_status(&id).unwrap();
            if st.state != crate::JobState::Running {
                break st;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };
        assert_eq!(st.state, crate::JobState::Failed);
        assert!(st.error.unwrap().contains("127.0.0.1:9"));
    }
}
