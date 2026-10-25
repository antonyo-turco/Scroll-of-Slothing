//! Combat tracker service. Spec: `docs/features/combat-tracker.md`.
//!
//! The tracker is autosaved in the workspace key `combat.tracker` (5etools
//! serial form) and every change is logged as `combat/tracker`, so the UI and
//! the MCP process stay in sync. Creatures come from a local copy of a 5etools
//! mirror's bestiary (`<data dir>/fivetools/bestiary/`), synced by a Job.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sleuth_core::combat::creature::{self, ColumnKind};
use sleuth_core::combat::link::{self, EncounterImport, EncounterItem, LinkKind};
use sleuth_core::combat::{CONDITIONS, Row, Sort, Tracker};
use sleuth_store::Store;

use crate::{JobId, Result, Service, ServiceError};

const KEY: &str = "combat.tracker";
const LOCK_KEY: &str = "_sleuthLocked";
const ENCOUNTERS_KEY: &str = "combat.encounters";
const PARTIES_KEY: &str = "combat.parties";
/// 5etools lair actions and regional effects, synced next to the bestiary files.
const GROUPS_FILE: &str = "legendarygroups.json";
/// Homebrew creatures, in `<data dir>/fivetools/` (outside the synced bestiary).
const HOMEBREW_FILE: &str = "homebrew-bestiary.json";

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
    /// Tiny … Gargantuan.
    pub size: String,
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
    pub hp_temp: Option<i64>,
    /// Damage (positive, temporary HP first) or healing (negative) applied to the current HP.
    pub damage: Option<i64>,
    pub initiative: Option<i64>,
    pub notes: Option<String>,
    /// Condition ids to remove.
    pub remove_conditions: Option<Vec<String>>,
    /// 5etools stat block fields to change for this creature only (merged over
    /// its earlier changes; a field set to null goes back to the bestiary).
    pub stats: Option<serde_json::Map<String, Value>>,
    /// Drop every stat change (back to the bestiary entry).
    pub reset_stats: Option<bool>,
}

/// One creature line of a saved encounter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterCreature {
    /// 5etools creature hash, e.g. `goblin_mm`.
    pub hash: String,
    pub count: u32,
    /// 5etools stat block fields that differ for these copies (e.g. `{"ac": [17]}`),
    /// applied over the bestiary entry when the encounter starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<serde_json::Map<String, Value>>,
}

/// A saved encounter: a creature list rolled into the tracker when started
/// (`kind: encounter`), or a snapshot of a whole tracker (`kind: combat`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedEncounter {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub notes: String,
    pub kind: LinkKind,
    #[serde(default)]
    pub creatures: Vec<EncounterCreature>,
    /// The tracker (5etools serial form), for `kind: combat`.
    #[serde(default)]
    pub combat: Option<Value>,
    #[serde(default)]
    pub pinned: bool,
    pub created: String,
    pub updated: String,
}

/// A saved encounter with what the bestiary cache knows about its creatures.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EncounterSummary {
    #[serde(flatten)]
    pub encounter: SavedEncounter,
    /// One line per creature: name, source, CR (unknown hashes keep the hash as name).
    pub lines: Vec<EncounterLine>,
    pub total: u32,
    /// Sum of creature XP (by CR), when every creature is known.
    pub xp: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EncounterLine {
    pub hash: String,
    pub name: String,
    pub source: String,
    pub count: u32,
    pub cr: Option<String>,
    /// The bestiary's CR when the line is scaled (`cr` is then the scaled one).
    pub base_cr: Option<String>,
    pub known: bool,
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
    /// Lair actions and regional effects, by `name|source` lower-cased.
    groups: HashMap<String, Value>,
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
    // homebrew creatures (made here or over MCP) live next to the synced copy
    let homebrew = dir.parent().map(|d| d.join(HOMEBREW_FILE));
    if let Some(Ok(b)) = homebrew.map(std::fs::read) {
        if let Ok(v) = serde_json::from_slice::<Value>(&b) {
            files += 1;
            if let Some(Value::Array(a)) = v.get("monster") {
                mons.extend(a.iter().cloned());
            }
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
            size: creature_size_label(m),
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
    let mut groups = HashMap::new();
    if let Ok(b) = std::fs::read(dir.join(GROUPS_FILE))
        && let Ok(v) = serde_json::from_slice::<Value>(&b)
    {
        for g in v.get("legendaryGroup").and_then(Value::as_array).into_iter().flatten() {
            let n = g.get("name").and_then(Value::as_str).unwrap_or("");
            let s = g.get("source").and_then(Value::as_str).unwrap_or("");
            groups.insert(format!("{n}|{s}").to_lowercase(), g.clone());
        }
    }
    Ok(Bestiary { files, by_hash, list, groups })
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
                apply_settings(&mut t, &s.load_settings()?);
                t
            }
            None => {
                let mut t = Tracker::default();
                apply_settings(&mut t, &s.load_settings()?);
                t
            }
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
        side: Option<String>,
    ) -> Result<Tracker> {
        let side = match side.as_deref() {
            None | Some("enemy") => None,
            Some("ally") => Some("ally".to_string()),
            Some("neutral") => Some("neutral".to_string()),
            Some(s) => {
                return Err(ServiceError::InvalidInput(format!(
                    "side must be enemy, ally or neutral, not {s:?}"
                )));
            }
        };
        if !(1..=100).contains(&count) {
            return Err(ServiceError::InvalidInput("count must be 1 to 100".into()));
        }
        self.combat_edit(true, move |svc, s, t| {
            let mon = svc.creature(s, &hash)?;
            let base = svc.fivetools_base(s)?;
            let roll = roll_hp.unwrap_or(t.is_roll_hp);
            let ids = t.add_creatures(&mon, count, roll, custom_name, &base, &mut rng, &mut new_id);
            for r in t.rows.iter_mut().filter(|r| ids.contains(&r.id)) {
                r.side = side.clone();
            }
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
            let cols = t.stats_cols.clone();
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
            if let Some(v) = patch.hp_temp {
                r.hp_temp = Some(v).filter(|v| *v > 0);
            }
            if let Some(mut d) = patch.damage {
                // temporary HP soak damage first
                if d > 0
                    && let Some(tmp) = r.hp_temp
                {
                    let soak = tmp.min(d);
                    d -= soak;
                    r.hp_temp = Some(tmp - soak).filter(|v| *v > 0);
                }
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
            if let Some(ids) = patch.remove_conditions {
                r.conditions.retain(|c| !ids.contains(&c.id));
            }
            if patch.reset_stats == Some(true) {
                r.overrides = None;
            }
            if let Some(stats) = patch.stats {
                let mut o = r.overrides.take().unwrap_or_default();
                for (k, v) in stats {
                    if v.is_null() {
                        o.remove(&k);
                    } else {
                        o.insert(k, v);
                    }
                }
                apply_stats_to_row(r, &o, &cols);
                r.overrides = (!o.is_empty()).then_some(o);
            }
            t.sort_rows();
            Ok(())
        })
        .await
        .map(|(t, _)| t)
    }

    /// Reset to round 1: the default party replaces every row, or with
    /// `keep_players` (default: the `combat_reset_keep_players` setting) only
    /// the creatures are removed and the players stay as they are.
    pub async fn combat_reset(&self, keep_players: Option<bool>) -> Result<Tracker> {
        self.combat_edit(true, move |_, s, t| {
            if keep_players.unwrap_or(s.load_settings()?.combat_reset_keep_players) {
                t.rows.retain(|r| !r.is_creature());
                for r in &mut t.rows {
                    r.is_active = false;
                }
                let d = Tracker::default();
                (t.round, t.sort, t.dir) = (d.round, d.sort, d.dir);
                t.sort_rows();
            } else {
                t.reset(&mut new_id);
            }
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
            ctx.check()?;
            // lair actions; an older mirror without the file is fine
            if let Ok(b) = get(GROUPS_FILE) {
                std::fs::write(tmp.join(GROUPS_FILE), b).map_err(io)?;
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
mod tests_support {
    use super::*;

    pub fn seed_bestiary(root: &Path) {
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
}

#[cfg(test)]
mod tests {
    use super::tests_support::seed_bestiary;
    use super::*;
    use crate::temp_service;

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
        let t =
            svc.combat_add_creature("goblin_mm".into(), 2, Some(false), None, None).await.unwrap();
        assert_eq!(t.rows.len(), 3);
        assert!(t.rows.iter().filter(|r| r.name == "Goblin").all(|r| r.hp_max == Some(7)));
        assert!(svc.combat_add_creature("nope_mm".into(), 1, None, None, None).await.is_err());
        assert!(svc.combat_add_creature("goblin_mm".into(), 0, None, None, None).await.is_err());

        let t = svc.combat_next_turn().await.unwrap();
        assert_eq!(t.active().len(), 1);
        let goblin = t.rows.iter().find(|r| r.name == "Goblin").unwrap().id.clone();
        let t =
            svc.combat_add_condition(goblin.clone(), "prone".into(), None, Some(2)).await.unwrap();
        let c = &t.rows.iter().find(|r| r.id == goblin).unwrap().conditions[0];
        assert_eq!((c.color.as_str(), c.turns), ("#5e60a0", Some(2)));

        // a link restores the same combat
        let url = svc.combat_link().await.unwrap();
        svc.combat_reset(Some(false)).await.unwrap();
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

/// The rolling and import options are app settings (Settings → Combat), not
/// per-tracker state; they still travel in links for 5etools.
fn apply_settings(t: &mut Tracker, st: &sleuth_core::settings::Settings) {
    t.is_roll_init = st.combat_roll_init;
    t.is_roll_hp = st.combat_roll_hp;
    t.is_roll_groups = st.combat_roll_groups;
    t.is_reroll_initiative_each_round = st.combat_reroll_each_round;
    t.is_passive_init = st.combat_passive_initiative;
    t.is_invert_wound_direction = st.combat_hp_as_damage;
    t.import_is_roll_groups = st.combat_import_roll_groups;
    t.import_is_add_players = st.combat_import_add_players;
    t.import_is_append = st.combat_import_append;
}

// ── saved encounters ──

fn load_encounters(s: &Store) -> Result<Vec<SavedEncounter>> {
    Ok(s.workspace_get::<Vec<SavedEncounter>>(ENCOUNTERS_KEY)?.unwrap_or_default())
}

fn save_encounters(s: &Store, list: &[SavedEncounter]) -> Result<()> {
    s.workspace_set(ENCOUNTERS_KEY, &list)?;
    s.record_change("combat", "encounters", "update")?;
    Ok(())
}

/// Default name: `3 × Goblin, Ogre`.
fn auto_name(lines: &[EncounterLine]) -> String {
    let parts: Vec<String> = lines
        .iter()
        .take(4)
        .map(|l| if l.count > 1 { format!("{} × {}", l.count, l.name) } else { l.name.clone() })
        .collect();
    let more = if lines.len() > 4 { format!(" +{}", lines.len() - 4) } else { String::new() };
    if parts.is_empty() { "Empty encounter".into() } else { format!("{}{more}", parts.join(", ")) }
}

fn clean_creatures(c: Vec<EncounterCreature>) -> Vec<EncounterCreature> {
    let mut out: Vec<EncounterCreature> = vec![];
    for x in c {
        let hash = x.hash.trim().to_lowercase();
        if hash.is_empty() || x.count == 0 {
            continue;
        }
        // lines merge only when they are the same creature with the same changes
        match out.iter_mut().find(|o| o.hash == hash && o.stats == x.stats) {
            Some(o) => o.count = (o.count + x.count).min(500),
            None => out.push(EncounterCreature { hash, count: x.count.min(500), stats: x.stats }),
        }
    }
    out
}

impl Service {
    fn summarize(&self, s: &Store, e: SavedEncounter) -> Result<EncounterSummary> {
        let b = self.bestiary(s)?;
        let creatures = match (&e.kind, &e.combat) {
            // a combat's creatures come from its rows
            (LinkKind::Combat, Some(v)) => {
                let t = Tracker::from_serial(v, &mut new_id);
                let mut c: Vec<EncounterCreature> = vec![];
                for r in t.rows.iter().filter(|r| r.is_creature()) {
                    let hash = creature::creature_hash(&r.name, r.source.as_deref().unwrap_or(""));
                    c.push(EncounterCreature { hash, count: 1, stats: r.overrides.clone() });
                }
                clean_creatures(c)
            }
            _ => e.creatures.clone(),
        };
        let mut xp = Some(0i64);
        let lines: Vec<EncounterLine> = creatures
            .iter()
            .map(|c| {
                // a line's stat changes (e.g. a scaled CR) count for its CR and XP
                let merged: Option<Value> = b.by_hash.get(&c.hash).map(|m| {
                    let mut m = m.clone();
                    if let (Some(o), Some(st)) = (m.as_object_mut(), c.stats.as_ref()) {
                        for (k, v) in st {
                            o.insert(k.clone(), v.clone());
                        }
                    }
                    m
                });
                let mon = merged.as_ref();
                let base_cr = b
                    .by_hash
                    .get(&c.hash)
                    .and_then(|m| m.get("cr"))
                    .map(|c| c.get("cr").unwrap_or(c))
                    .and_then(Value::as_str)
                    .map(String::from);
                let x = mon.and_then(creature::cr_to_xp);
                xp = xp.zip(x).map(|(a, b)| a + b * i64::from(c.count));
                let (name, source) = c.hash.rsplit_once('_').unwrap_or((&c.hash, ""));
                EncounterLine {
                    hash: c.hash.clone(),
                    name: mon
                        .and_then(|m| m["name"].as_str())
                        .map(Into::into)
                        .unwrap_or_else(|| name.replace("%20", " ")),
                    source: mon
                        .and_then(|m| m["source"].as_str())
                        .map(Into::into)
                        .unwrap_or_else(|| source.to_uppercase()),
                    count: c.count,
                    cr: mon
                        .and_then(|m| m.get("cr"))
                        .map(|c| c.get("cr").unwrap_or(c))
                        .and_then(Value::as_str)
                        .map(Into::into),
                    base_cr,
                    known: mon.is_some(),
                }
            })
            .collect();
        let total = lines.iter().map(|l| l.count).sum();
        if lines.is_empty() {
            xp = None;
        }
        Ok(EncounterSummary { encounter: e, lines, total, xp })
    }

    /// Saved encounters, pinned first, then most recently changed.
    pub async fn encounters_list(&self) -> Result<Vec<EncounterSummary>> {
        let svc = self.clone();
        self.blocking(move |s| {
            let mut list = load_encounters(s)?;
            list.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.updated.cmp(&a.updated)));
            list.into_iter().map(|e| svc.summarize(s, e)).collect()
        })
        .await
    }

    /// Creates (no `id`) or updates a creature-list encounter. An empty name
    /// becomes `3 × Goblin, Ogre`.
    pub async fn encounter_save(
        &self,
        id: Option<String>,
        name: String,
        notes: String,
        creatures: Vec<EncounterCreature>,
    ) -> Result<EncounterSummary> {
        let svc = self.clone();
        self.blocking(move |s| {
            let mut list = load_encounters(s)?;
            let creatures = clean_creatures(creatures);
            let now = sleuth_store::now();
            let ix = match &id {
                Some(id) => list
                    .iter()
                    .position(|e| &e.id == id)
                    .ok_or_else(|| ServiceError::NotFound(format!("encounter {id}")))?,
                None => {
                    list.push(SavedEncounter {
                        id: new_id(),
                        name: String::new(),
                        notes: String::new(),
                        kind: LinkKind::Encounter,
                        creatures: vec![],
                        combat: None,
                        pinned: false,
                        created: now.clone(),
                        updated: now.clone(),
                    });
                    list.len() - 1
                }
            };
            let e = &mut list[ix];
            if e.kind == LinkKind::Encounter {
                e.creatures = creatures;
            }
            e.notes = notes;
            e.updated = now;
            e.name = name.trim().to_string();
            if e.name.is_empty() {
                e.name = auto_name(&svc.summarize(s, e.clone())?.lines);
            }
            let out = e.clone();
            save_encounters(s, &list)?;
            svc.summarize(s, out)
        })
        .await
    }

    pub async fn encounter_set_pinned(&self, id: String, pinned: bool) -> Result<()> {
        self.blocking(move |s| {
            let mut list = load_encounters(s)?;
            let e = list
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or_else(|| ServiceError::NotFound(format!("encounter {id}")))?;
            e.pinned = pinned;
            save_encounters(s, &list)
        })
        .await
    }

    pub async fn encounter_delete(&self, id: String) -> Result<()> {
        self.blocking(move |s| {
            let mut list = load_encounters(s)?;
            let before = list.len();
            list.retain(|e| e.id != id);
            if list.len() == before {
                return Err(ServiceError::NotFound(format!("encounter {id}")));
            }
            save_encounters(s, &list)
        })
        .await
    }

    /// Saves the current tracker as a `combat` snapshot.
    pub async fn encounter_save_current(&self, name: String) -> Result<EncounterSummary> {
        let svc = self.clone();
        self.blocking(move |s| {
            let t = Self::load_tracker(s)?;
            if t.rows.is_empty() {
                return Err(ServiceError::InvalidInput("the tracker is empty".into()));
            }
            let now = sleuth_store::now();
            let mut e = SavedEncounter {
                id: new_id(),
                name: name.trim().to_string(),
                notes: t.notes.clone(),
                kind: LinkKind::Combat,
                creatures: vec![],
                combat: Some(t.to_serial()),
                pinned: false,
                created: now.clone(),
                updated: now,
            };
            if e.name.is_empty() {
                e.name = format!("Combat, round {}", t.round);
            }
            let mut list = load_encounters(s)?;
            list.push(e.clone());
            save_encounters(s, &list)?;
            svc.summarize(s, e)
        })
        .await
    }

    /// Imports 5etools data as a saved encounter: a Bestiary encounter file
    /// ("Save to File", JSON with `items`), an `#encounter=` link, or a
    /// `#combat=` link (saved as a combat snapshot).
    pub async fn encounter_import(
        &self,
        text: String,
        name: Option<String>,
    ) -> Result<EncounterSummary> {
        let t = text.trim();
        let (kind, data) = if t.starts_with('{') {
            let v: Value = serde_json::from_str(t).map_err(|e| {
                ServiceError::InvalidInput(format!("not 5etools encounter JSON: {e}"))
            })?;
            if v.get("items").is_none() {
                return Err(ServiceError::InvalidInput(
                    "not a 5etools encounter (no \"items\")".into(),
                ));
            }
            (LinkKind::Encounter, v)
        } else {
            link::parse(t).map_err(|e| ServiceError::InvalidInput(e.to_string()))?
        };
        let svc = self.clone();
        self.blocking(move |s| {
            let now = sleuth_store::now();
            let given = name
                .or_else(|| data.get("name").and_then(Value::as_str).map(Into::into))
                .unwrap_or_default();
            let mut e = SavedEncounter {
                id: new_id(),
                name: given.trim().to_string(),
                notes: String::new(),
                kind,
                creatures: vec![],
                combat: None,
                pinned: false,
                created: now.clone(),
                updated: now,
            };
            match kind {
                LinkKind::Encounter => {
                    let items = link::encounter_items(&data);
                    if items.is_empty() {
                        return Err(ServiceError::InvalidInput(
                            "the encounter has no creatures".into(),
                        ));
                    }
                    e.creatures = clean_creatures(
                        items
                            .into_iter()
                            .map(|i| EncounterCreature {
                                hash: i.hash,
                                count: i.count,
                                stats: None,
                            })
                            .collect(),
                    );
                }
                LinkKind::Combat => {
                    let tr = Tracker::from_serial(&data, &mut new_id);
                    e.notes = tr.notes.clone();
                    e.combat = Some(tr.to_serial());
                }
            }
            if e.name.is_empty() {
                let lines = svc.summarize(s, e.clone())?.lines;
                e.name = if kind == LinkKind::Combat {
                    format!("Combat: {}", auto_name(&lines))
                } else {
                    auto_name(&lines)
                };
            }
            let mut list = load_encounters(s)?;
            list.push(e.clone());
            save_encounters(s, &list)?;
            svc.summarize(s, e)
        })
        .await
    }

    /// Reads a 5etools encounter file from disk and imports it.
    pub async fn encounter_import_file(
        &self,
        path: std::path::PathBuf,
    ) -> Result<EncounterSummary> {
        let text = std::fs::read_to_string(&path).map_err(io)?;
        let stem = path.file_stem().map(|n| n.to_string_lossy().replace(".encounter", ""));
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| ServiceError::InvalidInput(format!("not 5etools encounter JSON: {e}")))?;
        let name = v.get("name").and_then(Value::as_str).map(Into::into).or(stem);
        self.encounter_import(text, name).await
    }

    /// Starts a saved encounter in the tracker: a creature list is rolled in
    /// with the import settings (`append` overrides "add to existing"); a
    /// combat snapshot replaces the tracker. Returns the tracker and, for
    /// creature lists, what was added or missing.
    pub async fn encounter_start(&self, id: String, append: Option<bool>) -> Result<CombatImport> {
        let (tracker, report, kind) = self
            .combat_edit(true, move |svc, s, t| {
                let e = load_encounters(s)?
                    .into_iter()
                    .find(|e| e.id == id)
                    .ok_or_else(|| ServiceError::NotFound(format!("encounter {id}")))?;
                Ok(match (e.kind, e.combat) {
                    (LinkKind::Combat, Some(v)) => {
                        *t = Tracker::from_serial(&v, &mut new_id);
                        t.sort_rows();
                        (None, LinkKind::Combat)
                    }
                    _ => {
                        let b = svc.bestiary(s)?;
                        let base = svc.fivetools_base(s)?;
                        let items: Vec<EncounterItem> = e
                            .creatures
                            .iter()
                            .map(|c| EncounterItem {
                                hash: c.hash.clone(),
                                count: c.count,
                                custom_hash_id: None,
                            })
                            .collect();
                        let data = link::exported_encounter(&e.name, &items);
                        let keep = t.import_is_append;
                        if let Some(a) = append {
                            t.import_is_append = a;
                        }
                        let lookup = |h: &str| b.by_hash.get(h).cloned();
                        let before: std::collections::HashSet<String> =
                            t.rows.iter().map(|r| r.id.clone()).collect();
                        let r =
                            link::import_encounter(t, &data, &lookup, &base, &mut rng, &mut new_id);
                        t.import_is_append = keep;
                        // lines with stat changes: hand them out to the new rows of that creature
                        let cols = t.stats_cols.clone();
                        let mut pending: Vec<(String, u32, serde_json::Map<String, Value>)> = e
                            .creatures
                            .iter()
                            .filter_map(|c| c.stats.clone().map(|s| (c.hash.clone(), c.count, s)))
                            .collect();
                        for row in t.rows.iter_mut().filter(|r| !before.contains(&r.id)) {
                            let h = creature::creature_hash(
                                &row.name,
                                row.source.as_deref().unwrap_or(""),
                            );
                            if let Some(p) = pending.iter_mut().find(|p| p.0 == h && p.1 > 0) {
                                p.1 -= 1;
                                apply_stats_to_row(row, &p.2, &cols);
                                row.overrides = Some(p.2.clone());
                            }
                        }
                        if !e.notes.is_empty() && t.notes.is_empty() {
                            t.notes = e.notes.clone();
                        }
                        (Some(r), LinkKind::Encounter)
                    }
                })
            })
            .await
            .map(|(t, (r, k))| (t, r, k))?;
        Ok(CombatImport { kind, tracker, encounter: report })
    }

    /// The 5etools link for a saved encounter (`#encounter=` or `#combat=`).
    pub async fn encounter_link(&self, id: String) -> Result<String> {
        let svc = self.clone();
        self.blocking(move |s| {
            let e = load_encounters(s)?
                .into_iter()
                .find(|e| e.id == id)
                .ok_or_else(|| ServiceError::NotFound(format!("encounter {id}")))?;
            let base = svc.fivetools_base(s)?;
            Ok(match (e.kind, e.combat) {
                (LinkKind::Combat, Some(v)) => {
                    format!("{base}/dmscreen.html#{}{}", link::PREFIX_COMBAT, link::compress(&v))
                }
                _ => {
                    let items: Vec<EncounterItem> = e
                        .creatures
                        .iter()
                        .map(|c| EncounterItem {
                            hash: c.hash.clone(),
                            count: c.count,
                            custom_hash_id: None,
                        })
                        .collect();
                    let v = link::exported_encounter(&e.name, &items);
                    format!("{base}/dmscreen.html#{}{}", link::PREFIX_ENCOUNTER, link::compress(&v))
                }
            })
        })
        .await
    }
}

#[cfg(test)]
mod encounter_tests {
    use super::tests_support::seed_bestiary;
    use super::*;
    use crate::temp_service;

    #[tokio::test(flavor = "multi_thread")]
    async fn create_import_start_and_link() {
        let (d, svc) = temp_service();
        seed_bestiary(d.path());
        let c = |h: &str, n| EncounterCreature { hash: h.into(), count: n, stats: None };
        let e = svc
            .encounter_save(
                None,
                " ".into(),
                "ambush".into(),
                vec![c("Goblin_MM", 2), c("ogre_mm", 1), c("goblin_mm", 1)],
            )
            .await
            .unwrap();
        assert_eq!(
            e.encounter.name, "3 × Goblin, Ogre",
            "named from its creatures, duplicates merged"
        );
        assert_eq!((e.total, e.xp), (4, Some(3 * 50 + 450)));
        let id = e.encounter.id.clone();

        // a 5etools "Save to File" encounter and an #encounter= link
        let file = json!({"name": "Bridge", "items": [{"h": "ogre_mm", "c": 2}, {"h": "kraken_mm"}], "fileType": "encounter"});
        let f = svc.encounter_import(file.to_string(), None).await.unwrap();
        assert_eq!((f.encounter.name.as_str(), f.total, f.xp), ("Bridge", 3, None));
        assert!(!f.lines[1].known);
        let link = svc.encounter_link(id.clone()).await.unwrap();
        let l = svc.encounter_import(link, Some("Copy".into())).await.unwrap();
        assert_eq!(l.encounter.creatures, e.encounter.creatures, "the link round-trips");
        assert!(svc.encounter_import("{}".into(), None).await.is_err());

        // start: rolls the creatures into the tracker
        svc.combat_add_player(Some("Vex".into())).await.unwrap();
        let r = svc.encounter_start(id.clone(), Some(true)).await.unwrap();
        assert_eq!((r.kind, r.tracker.rows.len()), (LinkKind::Encounter, 5), "added to Vex");
        assert_eq!(r.tracker.notes, "ambush");
        assert!(!r.tracker.import_is_append, "the override does not stick");

        // snapshot the tracker as a combat, then start it again later
        let snap = svc.encounter_save_current(String::new()).await.unwrap();
        assert_eq!((snap.encounter.kind, snap.total), (LinkKind::Combat, 4));
        svc.combat_reset(Some(false)).await.unwrap();
        let r = svc.encounter_start(snap.encounter.id.clone(), None).await.unwrap();
        assert_eq!(r.tracker.rows.len(), 5);

        svc.encounter_set_pinned(snap.encounter.id.clone(), true).await.unwrap();
        let list = svc.encounters_list().await.unwrap();
        assert_eq!(list.len(), 4);
        assert_eq!(list[0].encounter.id, snap.encounter.id, "pinned first");
        svc.encounter_delete(id.clone()).await.unwrap();
        assert!(svc.encounter_delete(id).await.is_err());
        assert_eq!(svc.encounters_list().await.unwrap().len(), 3);
    }
}

// ── hero parties ──

/// One player character of a saved party.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Hero {
    pub id: String,
    pub name: String,
    pub species: String,
    pub class: String,
    pub subclass: String,
    pub level: Option<i64>,
    pub hp_max: Option<i64>,
    pub ac: Option<i64>,
    pub initiative_bonus: Option<i64>,
    pub passive_perception: Option<i64>,
    pub speed: Option<i64>,
    pub spell_dc: Option<i64>,
    pub player: String,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Party {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub notes: String,
    pub heroes: Vec<Hero>,
    pub updated: String,
}

fn load_parties(s: &Store) -> Result<Vec<Party>> {
    Ok(s.workspace_get::<Vec<Party>>(PARTIES_KEY)?.unwrap_or_default())
}

fn save_parties(s: &Store, list: &[Party]) -> Result<()> {
    s.workspace_set(PARTIES_KEY, &list)?;
    s.record_change("combat", "parties", "update")?;
    Ok(())
}

/// A tracker row for a hero: full HP, initiative rolled when the tracker rolls
/// it, and stat columns filled from the sheet (AC, PP, speed, spell DC, init).
fn hero_row(t: &Tracker, h: &Hero, roll: bool) -> Row {
    let mut row = t.new_player(&mut new_id);
    row.name = h.name.clone();
    row.hp_max = h.hp_max;
    row.hp_current = if t.is_invert_wound_direction { h.hp_max.map(|_| 0) } else { h.hp_max };
    row.initiative = roll.then(|| t.initiative_of(h.initiative_bonus.unwrap_or(0), 0, &mut rng));
    let mut sub: Vec<String> = vec![];
    for x in [&h.species, &h.class, &h.subclass] {
        if !x.trim().is_empty() {
            sub.push(x.trim().to_string());
        }
    }
    if let Some(l) = h.level {
        sub.push(format!("level {l}"));
    }
    row.display_name = (!sub.is_empty()).then(|| sub.join(" · "));
    row.notes = h.notes.clone();
    for (cell, col) in row.cells.iter_mut().zip(&t.stats_cols) {
        let v = match col.populate_with.as_str() {
            "armorClass" => h.ac,
            "passivePerception" => h.passive_perception,
            "speed" => h.speed,
            "spellDc" => h.spell_dc,
            "initiative" => h.initiative_bonus,
            _ => continue,
        };
        cell.entity.insert("value".into(), json!(v));
    }
    row
}

impl Service {
    pub async fn parties_list(&self) -> Result<Vec<Party>> {
        self.blocking(|s| {
            let mut list = load_parties(s)?;
            list.sort_by_key(|p| p.name.to_lowercase());
            Ok(list)
        })
        .await
    }

    /// Creates (no `id`) or replaces a party. Heroes without an id get one.
    pub async fn party_save(
        &self,
        id: Option<String>,
        name: String,
        notes: String,
        heroes: Vec<Hero>,
    ) -> Result<Party> {
        if name.trim().is_empty() {
            return Err(ServiceError::InvalidInput("the party needs a name".into()));
        }
        self.blocking(move |s| {
            let mut list = load_parties(s)?;
            let heroes: Vec<Hero> = heroes
                .into_iter()
                .map(|h| Hero { id: if h.id.is_empty() { new_id() } else { h.id }, ..h })
                .collect();
            let party = Party {
                id: id.clone().unwrap_or_else(new_id),
                name: name.trim().into(),
                notes,
                heroes,
                updated: sleuth_store::now(),
            };
            match id {
                Some(id) => {
                    let p = list
                        .iter_mut()
                        .find(|p| p.id == id)
                        .ok_or_else(|| ServiceError::NotFound(format!("party {id}")))?;
                    *p = party.clone();
                }
                None => list.push(party.clone()),
            }
            save_parties(s, &list)?;
            Ok(party)
        })
        .await
    }

    pub async fn party_delete(&self, id: String) -> Result<()> {
        self.blocking(move |s| {
            let mut list = load_parties(s)?;
            let before = list.len();
            list.retain(|p| p.id != id);
            if list.len() == before {
                return Err(ServiceError::NotFound(format!("party {id}")));
            }
            save_parties(s, &list)
        })
        .await
    }

    /// Adds a party's heroes (all, or `hero_ids`) to the tracker; with
    /// `as_default` they also become the default party put back on Reset.
    pub async fn party_add_to_tracker(
        &self,
        id: String,
        hero_ids: Option<Vec<String>>,
        as_default: bool,
    ) -> Result<Tracker> {
        self.combat_edit(true, move |_, s, t| {
            let party = load_parties(s)?
                .into_iter()
                .find(|p| p.id == id)
                .ok_or_else(|| ServiceError::NotFound(format!("party {id}")))?;
            let heroes: Vec<&Hero> = party
                .heroes
                .iter()
                .filter(|h| hero_ids.as_ref().is_none_or(|ids| ids.contains(&h.id)))
                .collect();
            let roll = t.is_roll_init;
            let rows: Vec<Row> = heroes.iter().map(|h| hero_row(t, h, roll)).collect();
            if as_default {
                t.rows_default_party = heroes.iter().map(|h| hero_row(t, h, false)).collect();
            }
            t.rows.extend(rows);
            t.sort_rows();
            Ok(())
        })
        .await
        .map(|(t, _)| t)
    }
}

#[cfg(test)]
mod party_tests {
    use super::*;
    use crate::temp_service;

    #[tokio::test(flavor = "multi_thread")]
    async fn parties_fill_player_rows() {
        let (_d, svc) = temp_service();
        let vex = Hero {
            name: "Vex".into(),
            species: "Half-elf".into(),
            class: "Ranger".into(),
            level: Some(5),
            hp_max: Some(42),
            ac: Some(16),
            initiative_bonus: Some(4),
            ..Hero::default()
        };
        let pike = Hero { name: "Pike".into(), hp_max: Some(38), ac: Some(18), ..Hero::default() };
        assert!(svc.party_save(None, " ".into(), String::new(), vec![]).await.is_err());
        let p = svc
            .party_save(None, "Vox Machina".into(), String::new(), vec![vex, pike])
            .await
            .unwrap();
        assert!(p.heroes.iter().all(|h| !h.id.is_empty()));

        let mut t = svc.combat_get().await.unwrap();
        t.stats_cols.push(sleuth_core::combat::Column {
            id: "ac".into(),
            is_editable: true,
            is_player_visible: 0,
            populate_with: "armorClass".into(),
            abbreviation: "AC".into(),
        });
        svc.combat_set(t).await.unwrap();
        let only_vex = Some(vec![p.heroes[0].id.clone()]);
        let t = svc.party_add_to_tracker(p.id.clone(), only_vex, false).await.unwrap();
        let r = &t.rows[0];
        assert_eq!((r.name.as_str(), r.hp_current, r.hp_max), ("Vex", Some(42), Some(42)));
        assert_eq!(r.display_name.as_deref(), Some("Half-elf · Ranger · level 5"));
        assert!(r.initiative.is_some_and(|i| (5..=24).contains(&i)));
        assert_eq!(r.cells[0].entity["value"], json!(16));
        assert!(r.source.is_none(), "heroes are player rows");

        let t = svc.party_add_to_tracker(p.id.clone(), None, true).await.unwrap();
        assert_eq!((t.rows.len(), t.rows_default_party.len()), (3, 2));
        let t = svc.combat_reset(Some(false)).await.unwrap();
        assert_eq!(t.rows.len(), 2, "the party comes back on Reset");

        svc.party_delete(p.id.clone()).await.unwrap();
        assert!(svc.parties_list().await.unwrap().is_empty());
    }
}

// ── .denc files ──

/// File format marker of `.denc` encounter files.
pub const DENC_FORMAT: &str = "sleuth-encounters";

impl Service {
    /// Writes saved encounters (all, or `ids`) to a `.denc` file: JSON with
    /// `format`, `version` and the encounters. Returns how many were written.
    pub async fn encounters_export(
        &self,
        ids: Option<Vec<String>>,
        path: std::path::PathBuf,
    ) -> Result<usize> {
        self.blocking(move |s| {
            let list: Vec<SavedEncounter> = load_encounters(s)?
                .into_iter()
                .filter(|e| ids.as_ref().is_none_or(|ids| ids.contains(&e.id)))
                .collect();
            if list.is_empty() {
                return Err(ServiceError::InvalidInput("no encounters to export".into()));
            }
            let v = json!({"format": DENC_FORMAT, "version": 1, "encounters": list});
            let text = serde_json::to_string_pretty(&v).map_err(io)?;
            std::fs::write(&path, text).map_err(io)?;
            Ok(list.len())
        })
        .await
    }

    /// Adds the encounters of a `.denc` file (with new ids, so importing twice
    /// makes copies).
    pub async fn encounters_import_denc(
        &self,
        path: std::path::PathBuf,
    ) -> Result<Vec<EncounterSummary>> {
        let text = std::fs::read_to_string(&path).map_err(io)?;
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| ServiceError::InvalidInput(format!("not a .denc file: {e}")))?;
        if v.get("format").and_then(Value::as_str) != Some(DENC_FORMAT) {
            return Err(ServiceError::InvalidInput("not a .denc encounter file".into()));
        }
        let incoming: Vec<SavedEncounter> =
            serde_json::from_value(v.get("encounters").cloned().unwrap_or_default())
                .map_err(|e| ServiceError::InvalidInput(format!("damaged .denc file: {e}")))?;
        let svc = self.clone();
        self.blocking(move |s| {
            let mut list = load_encounters(s)?;
            let now = sleuth_store::now();
            let added: Vec<SavedEncounter> = incoming
                .into_iter()
                .map(|e| SavedEncounter { id: new_id(), pinned: false, updated: now.clone(), ..e })
                .collect();
            list.extend(added.iter().cloned());
            save_encounters(s, &list)?;
            added.into_iter().map(|e| svc.summarize(s, e)).collect()
        })
        .await
    }
}

#[cfg(test)]
mod denc_tests {
    use super::*;
    use crate::temp_service;

    #[tokio::test(flavor = "multi_thread")]
    async fn denc_round_trip() {
        let (d, svc) = temp_service();
        let c = EncounterCreature { hash: "goblin_mm".into(), count: 3, stats: None };
        let e = svc.encounter_save(None, "Ambush".into(), "at dusk".into(), vec![c]).await.unwrap();
        let path = d.path().join("ambush.denc");
        assert!(svc.encounters_export(Some(vec!["nope".into()]), path.clone()).await.is_err());
        assert_eq!(svc.encounters_export(None, path.clone()).await.unwrap(), 1);
        let back = svc.encounters_import_denc(path).await.unwrap();
        assert_eq!((back[0].encounter.name.as_str(), back[0].total), ("Ambush", 3));
        assert_ne!(back[0].encounter.id, e.encounter.id, "imported as a copy");
        let bad = d.path().join("x.denc");
        std::fs::write(&bad, "{}").unwrap();
        assert!(svc.encounters_import_denc(bad).await.is_err());
    }
}

/// Keeps a row in step with changed stats: HP (from `hp.average`), and the
/// AC, speed and passive Perception columns.
fn apply_stats_to_row(
    r: &mut Row,
    stats: &serde_json::Map<String, Value>,
    cols: &[sleuth_core::combat::Column],
) {
    if let Some(hp) = stats.get("hp").and_then(|h| h.get("average")).and_then(Value::as_i64) {
        r.hp_max = Some(hp);
        if r.hp_current.is_none_or(|c| c > hp) {
            r.hp_current = Some(hp);
        }
    }
    let mon = Value::Object(stats.clone());
    for (cell, col) in r.cells.iter_mut().zip(cols) {
        if matches!(col.populate_with.as_str(), "armorClass" | "speed" | "passivePerception")
            && stats.contains_key(match col.populate_with.as_str() {
                "armorClass" => "ac",
                "speed" => "speed",
                _ => "passive",
            })
            && let Value::Object(v) = creature::initial_cell(&col.populate_with, Some(&mon), "")
        {
            cell.entity.extend(v);
        }
    }
}

// ── homebrew creatures ──

fn homebrew_path(root: &Path) -> PathBuf {
    root.join("fivetools").join(HOMEBREW_FILE)
}

fn load_homebrew(root: &Path) -> Result<Vec<Value>> {
    match std::fs::read(homebrew_path(root)) {
        Ok(b) => {
            let v: Value = serde_json::from_slice(&b).map_err(io)?;
            Ok(v.get("monster").and_then(Value::as_array).cloned().unwrap_or_default())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(io(e)),
    }
}

impl Service {
    fn forget_bestiary(&self) {
        cache().lock().unwrap_or_else(|p| p.into_inner()).remove(self.data_dir());
    }

    /// Saves a homebrew creature (a 5etools monster JSON object with at least
    /// `name` and `source`; same name and source replace it). It joins the
    /// bestiary: search, add and encounters find it by its hash.
    pub async fn bestiary_save_creature(&self, creature: Value) -> Result<BestiaryEntry> {
        let name =
            creature.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty());
        let source =
            creature.get("source").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty());
        let (Some(name), Some(source)) = (name.map(String::from), source.map(String::from)) else {
            return Err(ServiceError::InvalidInput(
                "a creature needs a \"name\" and a \"source\" (e.g. \"Homebrew\")".into(),
            ));
        };
        if !creature.is_object() {
            return Err(ServiceError::InvalidInput("a creature is a JSON object".into()));
        }
        let root = self.data_dir().to_path_buf();
        let hash = creature::creature_hash(&name, &source);
        let mut list = load_homebrew(&root)?;
        list.retain(|m| {
            let n = m.get("name").and_then(Value::as_str).unwrap_or("");
            let s = m.get("source").and_then(Value::as_str).unwrap_or("");
            creature::creature_hash(n, s) != hash
        });
        list.push(creature);
        std::fs::create_dir_all(root.join("fivetools")).map_err(io)?;
        let text = serde_json::to_string_pretty(&json!({"monster": list})).map_err(io)?;
        std::fs::write(homebrew_path(&root), text).map_err(io)?;
        self.forget_bestiary();
        self.blocking(|s| Ok(s.record_change("bestiary", "homebrew", "update")?)).await?;
        let hits = self.bestiary_search(name.clone(), Some(500)).await?;
        hits.into_iter()
            .find(|h| h.hash == hash)
            .ok_or_else(|| ServiceError::Internal("saved creature not found".into()))
    }

    /// Homebrew creatures as saved (full 5etools JSON).
    pub async fn bestiary_homebrew(&self) -> Result<Vec<Value>> {
        let root = self.data_dir().to_path_buf();
        load_homebrew(&root)
    }

    pub async fn bestiary_delete_creature(&self, hash: String) -> Result<()> {
        let root = self.data_dir().to_path_buf();
        let mut list = load_homebrew(&root)?;
        let before = list.len();
        list.retain(|m| {
            let n = m.get("name").and_then(Value::as_str).unwrap_or("");
            let s = m.get("source").and_then(Value::as_str).unwrap_or("");
            creature::creature_hash(n, s) != hash.to_lowercase()
        });
        if list.len() == before {
            return Err(ServiceError::NotFound(format!("homebrew creature {hash}")));
        }
        let text = serde_json::to_string_pretty(&json!({"monster": list})).map_err(io)?;
        std::fs::write(homebrew_path(&root), text).map_err(io)?;
        self.forget_bestiary();
        self.blocking(|s| Ok(s.record_change("bestiary", "homebrew", "update")?)).await
    }
}

#[cfg(test)]
mod homebrew_tests {
    use super::tests_support::seed_bestiary;
    use super::*;
    use crate::temp_service;

    #[tokio::test(flavor = "multi_thread")]
    async fn homebrew_creatures_and_stat_changes() {
        let (d, svc) = temp_service();
        seed_bestiary(d.path());
        assert!(svc.bestiary_save_creature(json!({"name": "Nameless"})).await.is_err());
        let wolf = json!({"name": "Ash Wolf", "source": "Homebrew", "hp": {"average": 22, "formula": "4d8+4"},
                          "ac": [13], "dex": 15, "cr": "1"});
        let e = svc.bestiary_save_creature(wolf).await.unwrap();
        assert_eq!((e.hash.as_str(), e.hp), ("ash%20wolf_homebrew", Some(22)));
        let t = svc.combat_add_creature(e.hash.clone(), 1, Some(false), None, None).await.unwrap();
        assert_eq!(t.rows[0].hp_max, Some(22));

        // per-row stats: merge, null removes, hp follows
        let id = t.rows[0].id.clone();
        let mut stats = serde_json::Map::new();
        stats.insert("ac".into(), json!([16]));
        stats.insert("hp".into(), json!({"average": 30, "formula": "5d8+8"}));
        let p = RowPatch { stats: Some(stats), ..RowPatch::default() };
        let t = svc.combat_update_row(id.clone(), p).await.unwrap();
        assert_eq!(
            (t.rows[0].hp_max, t.rows[0].overrides.as_ref().unwrap()["ac"].clone()),
            (Some(30), json!([16]))
        );
        let mut undo = serde_json::Map::new();
        undo.insert("ac".into(), Value::Null);
        let t = svc
            .combat_update_row(id, RowPatch { stats: Some(undo), ..RowPatch::default() })
            .await
            .unwrap();
        assert!(!t.rows[0].overrides.as_ref().unwrap().contains_key("ac"));

        // an encounter line with changes keeps them when started
        let mut s17 = serde_json::Map::new();
        s17.insert("ac".into(), json!([17]));
        let lines = vec![
            EncounterCreature { hash: "goblin_mm".into(), count: 2, stats: Some(s17.clone()) },
            EncounterCreature { hash: "goblin_mm".into(), count: 1, stats: None },
        ];
        let enc =
            svc.encounter_save(None, "Elite goblins".into(), String::new(), lines).await.unwrap();
        assert_eq!(enc.encounter.creatures.len(), 2, "different changes stay separate lines");
        let r = svc.encounter_start(enc.encounter.id, Some(false)).await.unwrap();
        let elite = r
            .tracker
            .rows
            .iter()
            .filter(|r| r.overrides.as_ref().is_some_and(|o| o["ac"] == json!([17])))
            .count();
        assert_eq!(elite, 2);

        svc.bestiary_delete_creature("ash%20wolf_homebrew".into()).await.unwrap();
        assert!(svc.bestiary_get("ash%20wolf_homebrew".into()).await.is_err());
    }
}

// ── creature images (tokens and art) from the mirror ──

/// Which picture of a creature: its round token, or its full art.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CreatureImage {
    #[default]
    Token,
    Art,
}

/// 5etools size letter → the app's size label (Tiny … Gargantuan).
pub(crate) fn creature_size_label(mon: &Value) -> String {
    let letter = mon.get("size").and_then(|s| s.get(0)).and_then(Value::as_str).unwrap_or("M");
    match letter {
        "T" => "Tiny",
        "S" => "Small",
        "L" => "Large",
        "H" => "Huge",
        "G" => "Gargantuan",
        _ => "Medium",
    }
    .into()
}

fn creature_image_url(mon: &Value, base: &str, kind: CreatureImage) -> Option<String> {
    match kind {
        CreatureImage::Token => creature::token_url(mon, base),
        // 5etools keeps full art at img/bestiary/<SOURCE>/<Name>.webp
        CreatureImage::Art => {
            let name = mon.get("name")?.as_str()?.replace('"', "");
            let source = mon.get("source")?.as_str()?;
            Some(format!("{}/img/bestiary/{source}/{name}.webp", base.trim_end_matches('/')))
        }
    }
}

fn fetch_image(url: &str) -> Result<Vec<u8>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("sleuth/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(io)?;
    // spaces and other characters in names must be escaped in the path
    let url = url.replace(' ', "%20");
    let r = client
        .get(&url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| ServiceError::Io(format!("{url}: {e}")))?;
    Ok(r.bytes().map_err(io)?.to_vec())
}

impl Service {
    /// Downloads a creature's picture from the mirror (or its own `tokenUrl`).
    pub(crate) fn creature_image_bytes(
        &self,
        s: &Store,
        hash: &str,
        kind: CreatureImage,
    ) -> Result<(Value, Vec<u8>)> {
        let mon = self.creature(s, hash)?;
        let url = creature_image_url(&mon, &self.fivetools_base(s)?, kind).ok_or_else(|| {
            ServiceError::NotFound(format!(
                "{} has no {}",
                mon["name"].as_str().unwrap_or(hash),
                if kind == CreatureImage::Art { "art" } else { "token" }
            ))
        })?;
        let bytes = fetch_image(&url)?;
        Ok((mon, bytes))
    }

    /// A creature's token or art as an image source (for the Token Cutter).
    pub async fn image_source_from_creature(
        &self,
        hash: String,
        kind: CreatureImage,
    ) -> Result<crate::SourceImage> {
        let svc = self.clone();
        self.blocking(move |s| {
            let (_, bytes) = svc.creature_image_bytes(s, &hash, kind)?;
            crate::tokens::store_source(s, &bytes)
        })
        .await
    }
}

impl Service {
    /// A creature's lair actions and regional effects (5etools `legendaryGroup`),
    /// by its `legendaryGroup` name and source. None when the cache has none.
    pub async fn bestiary_legendary_group(
        &self,
        name: String,
        source: String,
    ) -> Result<Option<Value>> {
        let svc = self.clone();
        self.blocking(move |s| {
            Ok(svc.bestiary(s)?.groups.get(&format!("{name}|{source}").to_lowercase()).cloned())
        })
        .await
    }
}
