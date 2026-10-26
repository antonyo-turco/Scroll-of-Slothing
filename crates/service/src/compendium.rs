//! Spell and item cards from a 5etools mirror. Spec: `docs/features/spell-item-cards.md`.
//!
//! The mirror's spells (`data/spells/`) and items (`data/items.json`,
//! `data/items-base.json`, `data/magicvariants.json`) are synced by a Job into
//! `<data dir>/fivetools/spells/` and `<data dir>/fivetools/items/`; search and
//! card building then work offline.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sleuth_core::combat::creature::{creature_hash, resolve_copy};
use sleuth_core::compendium::{self as cc, ItemRefs};

use crate::{JobId, Result, Service, ServiceError};

const SPELL_EXTRA: &str = "sources.json";
const ITEM_FILES: [&str; 3] = ["items.json", "items-base.json", "magicvariants.json"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompendiumKind {
    Spell,
    Item,
}

/// A search in the synced spells or items. Empty filters match everything.
#[derive(Debug, Clone, Deserialize)]
pub struct CompendiumQuery {
    pub kind: CompendiumKind,
    #[serde(default)]
    pub query: String,
    /// Spell level, 0 for cantrips.
    pub level: Option<i64>,
    /// A class that has the spell, e.g. "Wizard".
    pub class: Option<String>,
    /// Item rarity: common, uncommon, rare, very rare, legendary, artifact, none…
    pub rarity: Option<String>,
    /// Leave out entries reprinted in a newer book (e.g. PHB spells that are in XPHB).
    #[serde(default)]
    pub newest_only: bool,
    pub limit: Option<usize>,
}

/// One search hit.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompendiumEntry {
    pub kind: CompendiumKind,
    /// 5etools hash, e.g. `fireball_xphb`.
    pub id: String,
    pub name: String,
    pub source: String,
    /// "3rd-level Evocation", "Wondrous item, uncommon".
    pub detail: String,
    pub level: Option<i64>,
    pub rarity: Option<String>,
    pub classes: Vec<String>,
    pub concentration: bool,
    /// Reprinted in a newer book.
    pub reprinted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompendiumStatus {
    pub base_url: String,
    pub synced: bool,
    pub spells: usize,
    pub items: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompendiumRef {
    pub kind: CompendiumKind,
    pub id: String,
}

/// Cards built from spells and items, as deck markdown.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompendiumCards {
    pub markdown: String,
    pub cards: usize,
    /// Ids not in the synced data.
    pub missing: Vec<String>,
}

struct Compendium {
    spells: HashMap<String, (Value, Vec<String>)>,
    items: HashMap<String, Value>,
    refs: ItemRefs,
    list: Vec<CompendiumEntry>,
}

fn cache() -> &'static Mutex<HashMap<PathBuf, Arc<Compendium>>> {
    static C: OnceLock<Mutex<HashMap<PathBuf, Arc<Compendium>>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

fn spells_dir(root: &Path) -> PathBuf {
    root.join("fivetools").join("spells")
}

fn items_dir(root: &Path) -> PathBuf {
    root.join("fivetools").join("items")
}

fn io(e: impl std::fmt::Display) -> ServiceError {
    ServiceError::Io(e.to_string())
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// The 5etools hash, lower-cased throughout (`%2b1%20weapon_xdmg`) so lookups
/// ignore case.
fn hash_of(v: &Value) -> Option<String> {
    Some(creature_hash(v.get("name")?.as_str()?, v.get("source")?.as_str()?).to_lowercase())
}

fn array<'a>(v: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    v.get(key).and_then(Value::as_array).into_iter().flatten()
}

fn reprinted(v: &Value) -> bool {
    v.get("reprintedAs").and_then(Value::as_array).is_some_and(|a| !a.is_empty())
}

/// Resolves `_copy` entries against their base, over a few passes for copies of copies.
fn resolve_copies(by_hash: &mut HashMap<String, Value>, mut copies: Vec<Value>) {
    for _ in 0..4 {
        let mut left = vec![];
        for c in copies {
            let base = c.get("_copy").and_then(hash_of).and_then(|h| by_hash.get(&h).cloned());
            match (base, hash_of(&c)) {
                (Some(b), Some(h)) => {
                    by_hash.insert(h, resolve_copy(&c, &b));
                }
                _ => left.push(c),
            }
        }
        copies = left;
        if copies.is_empty() {
            break;
        }
    }
}

fn load(root: &Path) -> Compendium {
    // spells, with their class lists from sources.json
    let sdir = spells_dir(root);
    let index: HashMap<String, String> = read_json(&sdir.join("index.json"))
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let sources = read_json(&sdir.join(SPELL_EXTRA)).unwrap_or(Value::Null);
    let mut raw: HashMap<String, Value> = HashMap::new();
    let mut copies = vec![];
    for file in index.values() {
        let Some(v) = read_json(&sdir.join(file)) else { continue };
        for s in array(&v, "spell") {
            if s.get("_copy").is_some() {
                copies.push(s.clone());
            } else if let Some(h) = hash_of(s) {
                raw.insert(h, s.clone());
            }
        }
    }
    resolve_copies(&mut raw, copies);
    let mut spells = HashMap::new();
    for (h, s) in raw {
        let (name, source) = (s["name"].as_str().unwrap_or(""), s["source"].as_str().unwrap_or(""));
        let mut classes: Vec<String> = vec![];
        let src = sources.get(source).or_else(|| sources.get(source.to_uppercase()));
        for c in src.and_then(|x| x.get(name)).map(|x| array(x, "class")).into_iter().flatten() {
            if let Some(n) = c.get("name").and_then(Value::as_str)
                && !classes.iter().any(|x| x == n)
            {
                classes.push(n.into());
            }
        }
        spells.insert(h, (s, classes));
    }

    // items: magic and mundane items, base items, generic magic variants
    let idir = items_dir(root);
    let base = read_json(&idir.join("items-base.json")).unwrap_or(Value::Null);
    let refs = ItemRefs::from_base(&base);
    let mut items: HashMap<String, Value> = HashMap::new();
    let mut copies = vec![];
    for i in array(&base, "baseitem") {
        if let Some(h) = hash_of(i) {
            items.insert(h, i.clone());
        }
    }
    if let Some(v) = read_json(&idir.join("items.json")) {
        for i in array(&v, "item") {
            if i.get("_copy").is_some() {
                copies.push(i.clone());
            } else if let Some(h) = hash_of(i) {
                items.insert(h, i.clone());
            }
        }
    }
    resolve_copies(&mut items, copies);
    if let Some(v) = read_json(&idir.join("magicvariants.json")) {
        for mv in array(&v, "magicvariant") {
            let item = cc::variant_as_item(mv);
            if let Some(h) = hash_of(&item) {
                items.entry(h).or_insert(item);
            }
        }
    }

    let mut list: Vec<CompendiumEntry> = spells
        .iter()
        .map(|(h, (s, classes))| CompendiumEntry {
            kind: CompendiumKind::Spell,
            id: h.clone(),
            name: s["name"].as_str().unwrap_or("").into(),
            source: s["source"].as_str().unwrap_or("").into(),
            detail: cc::spell_level_school(s),
            level: s.get("level").and_then(Value::as_i64),
            rarity: None,
            classes: classes.clone(),
            concentration: cc::spell_concentration(s),
            reprinted: reprinted(s),
        })
        .collect();
    list.extend(items.iter().map(|(h, i)| {
        let rarity = cc::item_rarity(i);
        let kind = cc::item_kind(i, &refs);
        let detail = match (kind.is_empty(), rarity.as_str()) {
            (true, "none") => "Item".to_string(),
            (true, r) => r.to_string(),
            (false, "none") => kind,
            (false, r) => format!("{kind}, {r}"),
        };
        CompendiumEntry {
            kind: CompendiumKind::Item,
            id: h.clone(),
            name: i["name"].as_str().unwrap_or("").into(),
            source: i["source"].as_str().unwrap_or("").into(),
            detail,
            level: None,
            rarity: Some(rarity),
            classes: vec![],
            concentration: false,
            reprinted: reprinted(i),
        }
    }));
    list.sort_by(|a, b| {
        a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.source.cmp(&b.source))
    });
    Compendium { spells, items, refs, list }
}

impl Service {
    fn compendium(&self) -> Arc<Compendium> {
        let root = self.data_dir().to_path_buf();
        if let Some(c) = cache().lock().unwrap_or_else(|p| p.into_inner()).get(&root) {
            return c.clone();
        }
        let c = Arc::new(load(&root));
        cache().lock().unwrap_or_else(|p| p.into_inner()).insert(root, c.clone());
        c
    }

    pub async fn compendium_status(&self) -> Result<CompendiumStatus> {
        let svc = self.clone();
        self.blocking(move |s| {
            let c = svc.compendium();
            Ok(CompendiumStatus {
                base_url: s.load_settings()?.fivetools_base_url,
                synced: !c.spells.is_empty() || !c.items.is_empty(),
                spells: c.spells.len(),
                items: c.items.len(),
            })
        })
        .await
    }

    /// Spells or items whose name contains the query (prefix matches first).
    pub async fn compendium_search(&self, q: CompendiumQuery) -> Result<Vec<CompendiumEntry>> {
        let svc = self.clone();
        self.blocking(move |_| {
            let c = svc.compendium();
            let text = q.query.trim().to_lowercase();
            let class = q.class.as_deref().map(str::to_lowercase).filter(|x| !x.is_empty());
            let rarity = q.rarity.as_deref().map(str::to_lowercase).filter(|x| !x.is_empty());
            let (mut pre, mut mid) = (vec![], vec![]);
            for e in c.list.iter().filter(|e| e.kind == q.kind) {
                if q.newest_only && e.reprinted
                    || q.level.is_some_and(|l| e.level != Some(l))
                    || class.as_ref().is_some_and(|k| !e.classes.iter().any(|x| x.to_lowercase() == *k))
                    || rarity.as_ref().is_some_and(|r| e.rarity.as_ref() != Some(r))
                {
                    continue;
                }
                let n = e.name.to_lowercase();
                if n.starts_with(&text) {
                    pre.push(e.clone());
                } else if n.contains(&text) || e.id == text {
                    mid.push(e.clone());
                }
            }
            pre.extend(mid);
            pre.truncate(q.limit.unwrap_or(100).clamp(1, 2000));
            Ok(pre)
        })
        .await
    }

    /// A synced spell's or item's full 5etools JSON.
    pub async fn compendium_get(&self, r: CompendiumRef) -> Result<Value> {
        let svc = self.clone();
        self.blocking(move |_| {
            let c = svc.compendium();
            let id = r.id.to_lowercase();
            let hit = match r.kind {
                CompendiumKind::Spell => c.spells.get(&id).map(|(s, _)| s.clone()),
                CompendiumKind::Item => c.items.get(&id).cloned(),
            };
            hit.ok_or_else(|| {
                ServiceError::NotFound(format!("{:?} {:?} (sync spells and items first?)", r.kind, r.id))
            })
        })
        .await
    }

    /// Card markdown for spells and items, in the order given, ready to append
    /// to a deck. Unknown ids are reported, not dropped silently.
    pub async fn compendium_cards(&self, refs: Vec<CompendiumRef>) -> Result<CompendiumCards> {
        let svc = self.clone();
        self.blocking(move |_| {
            let c = svc.compendium();
            let mut cards = vec![];
            let mut missing = vec![];
            for r in refs {
                let id = r.id.to_lowercase();
                let md = match r.kind {
                    CompendiumKind::Spell => {
                        c.spells.get(&id).map(|(s, classes)| cc::spell_card(s, classes))
                    }
                    CompendiumKind::Item => c.items.get(&id).map(|i| cc::item_card(i, &c.refs)),
                };
                match md {
                    Some(md) => cards.push(md),
                    None => missing.push(r.id),
                }
            }
            Ok(CompendiumCards { cards: cards.len(), markdown: cards.join("\n\n---\n\n"), missing })
        })
        .await
    }

    /// Downloads the mirror's spells and items into the cache (job result:
    /// `{spells, items}`). The old cache stays until the sync succeeds.
    pub async fn compendium_sync(&self) -> Result<JobId> {
        let base = self.blocking(|s| Ok(s.load_settings()?.fivetools_base_url)).await?;
        let root = self.data_dir().to_path_buf();
        Ok(self.spawn_job("spells and items sync", move |ctx| {
            let client = reqwest::blocking::Client::builder()
                .user_agent(concat!("sleuth/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .map_err(io)?;
            let get = |path: &str| -> Result<Vec<u8>> {
                let url = format!("{base}/data/{path}");
                let r = client
                    .get(&url)
                    .send()
                    .and_then(|r| r.error_for_status())
                    .map_err(|e| ServiceError::Io(format!("{url}: {e}")))?;
                Ok(r.bytes().map_err(io)?.to_vec())
            };
            let fresh = |dir: &Path| -> Result<PathBuf> {
                let tmp = dir.with_extension("part");
                let _ = std::fs::remove_dir_all(&tmp);
                std::fs::create_dir_all(&tmp).map_err(io)?;
                Ok(tmp)
            };

            ctx.progress(0.0, format!("reading {base}/data/spells/index.json"));
            let sdir = spells_dir(&root);
            let stmp = fresh(&sdir)?;
            let index_bytes = get("spells/index.json")?;
            let index: HashMap<String, String> =
                serde_json::from_slice(&index_bytes).map_err(|e| {
                    ServiceError::InvalidInput(format!("index.json is not a 5etools spell index: {e}"))
                })?;
            std::fs::write(stmp.join("index.json"), &index_bytes).map_err(io)?;
            let files: Vec<&String> =
                index.values().filter(|f| !f.contains(['/', '\\']) && !f.contains("..")).collect();
            let total = (files.len() + 1 + ITEM_FILES.len()) as f32;
            for (i, file) in files.iter().enumerate() {
                ctx.check()?;
                ctx.progress(i as f32 / total, format!("{file} ({}/{})", i + 1, files.len()));
                std::fs::write(stmp.join(file), get(&format!("spells/{file}"))?).map_err(io)?;
            }
            ctx.check()?;
            // class lists; an older mirror without the file is fine
            if let Ok(b) = get(&format!("spells/{SPELL_EXTRA}")) {
                std::fs::write(stmp.join(SPELL_EXTRA), b).map_err(io)?;
            }

            let idir = items_dir(&root);
            let itmp = fresh(&idir)?;
            for (i, file) in ITEM_FILES.iter().enumerate() {
                ctx.check()?;
                ctx.progress((files.len() + 1 + i) as f32 / total, (*file).to_string());
                match get(file) {
                    Ok(b) => std::fs::write(itmp.join(file), b).map_err(io)?,
                    // magic variants are optional; items and base items are not
                    Err(_) if *file == "magicvariants.json" => {}
                    Err(e) => return Err(e),
                }
            }
            ctx.check()?;
            for (dir, tmp) in [(&sdir, &stmp), (&idir, &itmp)] {
                let _ = std::fs::remove_dir_all(dir);
                std::fs::rename(tmp, dir).map_err(io)?;
            }
            let c = load(&root);
            let out = json!({"spells": c.spells.len(), "items": c.items.len()});
            cache().lock().unwrap_or_else(|p| p.into_inner()).insert(root.clone(), Arc::new(c));
            let _ = ctx.service().store().record_change("compendium", "cache", "update");
            Ok(out)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{JobState, temp_service};

    fn seed(root: &Path) {
        let s = spells_dir(root);
        std::fs::create_dir_all(&s).unwrap();
        std::fs::write(s.join("index.json"), r#"{"PHB": "spells-phb.json", "XPHB": "spells-xphb.json"}"#)
            .unwrap();
        let spell = |src: &str, extra: Value| {
            let mut v = json!({"name": "Fireball", "source": src, "level": 3, "school": "V",
                "time": [{"number": 1, "unit": "action"}],
                "range": {"type": "point", "distance": {"type": "feet", "amount": 150}},
                "components": {"v": true, "s": true}, "duration": [{"type": "instant"}],
                "entries": ["Boom {@damage 8d6}."]});
            v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            v
        };
        std::fs::write(
            s.join("spells-phb.json"),
            json!({"spell": [spell("PHB", json!({"reprintedAs": ["Fireball|XPHB"]})),
                {"name": "Light", "source": "PHB", "level": 0, "school": "V", "entries": ["Glow."]}]})
            .to_string(),
        )
        .unwrap();
        std::fs::write(s.join("spells-xphb.json"), json!({"spell": [spell("XPHB", json!({}))]}).to_string())
            .unwrap();
        std::fs::write(
            s.join(SPELL_EXTRA),
            json!({"XPHB": {"Fireball": {"class": [{"name": "Sorcerer"}, {"name": "Wizard"}]}},
                   "PHB": {"Light": {"class": [{"name": "Cleric"}]}}})
            .to_string(),
        )
        .unwrap();
        let i = items_dir(root);
        std::fs::create_dir_all(&i).unwrap();
        std::fs::write(
            i.join("items.json"),
            json!({"item": [
                {"name": "Bag of Holding", "source": "XDMG", "rarity": "uncommon", "wondrous": true, "entries": ["Big inside."]},
                {"name": "Bag of Tricks", "source": "XDMG", "_copy": {"name": "Bag of Holding", "source": "XDMG"}, "rarity": "rare"}
            ]})
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            i.join("items-base.json"),
            json!({"baseitem": [{"name": "Longsword", "source": "XPHB", "type": "M|XPHB", "rarity": "none",
                    "dmg1": "1d8", "dmgType": "S"}],
                   "itemType": [{"abbreviation": "M", "name": "Melee Weapon"}]})
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            i.join("magicvariants.json"),
            json!({"magicvariant": [{"name": "+1 Weapon", "requires": [{"weapon": true}],
                "inherits": {"source": "XDMG", "rarity": "uncommon", "bonusWeapon": "+1",
                    "entries": ["A {=bonusWeapon} bonus."]}}]})
            .to_string(),
        )
        .unwrap();
    }

    fn q(kind: CompendiumKind, query: &str) -> CompendiumQuery {
        CompendiumQuery {
            kind,
            query: query.into(),
            level: None,
            class: None,
            rarity: None,
            newest_only: false,
            limit: None,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn search_filters_and_builds_cards() {
        let (d, svc) = temp_service();
        seed(d.path());
        let st = svc.compendium_status().await.unwrap();
        assert_eq!((st.synced, st.spells, st.items), (true, 3, 4));

        let all = svc.compendium_search(q(CompendiumKind::Spell, "")).await.unwrap();
        assert_eq!(all.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["fireball_phb", "fireball_xphb", "light_phb"]);
        let newest = svc
            .compendium_search(CompendiumQuery { newest_only: true, ..q(CompendiumKind::Spell, "fire") })
            .await
            .unwrap();
        assert_eq!(newest.len(), 1);
        assert_eq!(newest[0].classes, ["Sorcerer", "Wizard"]);
        let wiz = svc
            .compendium_search(CompendiumQuery { class: Some("wizard".into()), ..q(CompendiumKind::Spell, "") })
            .await
            .unwrap();
        assert_eq!(wiz.len(), 1);
        let cantrips = svc
            .compendium_search(CompendiumQuery { level: Some(0), ..q(CompendiumKind::Spell, "") })
            .await
            .unwrap();
        assert_eq!(cantrips[0].detail, "Evocation cantrip");

        let rare = svc
            .compendium_search(CompendiumQuery { rarity: Some("Rare".into()), ..q(CompendiumKind::Item, "bag") })
            .await
            .unwrap();
        assert_eq!(rare.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Bag of Tricks"]);
        assert_eq!(rare[0].detail, "Wondrous item, rare");
        let sword = svc.compendium_search(q(CompendiumKind::Item, "longsword")).await.unwrap();
        assert_eq!(sword[0].detail, "Melee Weapon");

        let out = svc
            .compendium_cards(vec![
                CompendiumRef { kind: CompendiumKind::Spell, id: "fireball_xphb".into() },
                CompendiumRef { kind: CompendiumKind::Item, id: "%2B1%20weapon_xdmg".into() },
                CompendiumRef { kind: CompendiumKind::Item, id: "nope_x".into() },
            ])
            .await
            .unwrap();
        assert_eq!((out.cards, out.missing.as_slice()), (2, ["nope_x".to_string()].as_slice()));
        let parsed = svc.cards_parse(out.markdown.clone()).await.unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].title, "Fireball");
        assert!(out.markdown.contains("**Classes:** Sorcerer, Wizard"));
        assert_eq!(parsed[1].body, "A +1 bonus.");

        let full = svc
            .compendium_get(CompendiumRef { kind: CompendiumKind::Item, id: "Bag%20of%20Tricks_XDMG".into() })
            .await
            .unwrap();
        assert_eq!(full["entries"][0], "Big inside.");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_reports_an_unreachable_mirror() {
        let (_d, svc) = temp_service();
        let mut p = serde_json::Map::new();
        p.insert("fivetools_base_url".into(), json!("http://127.0.0.1:9"));
        svc.settings_set(p).await.unwrap();
        let job = svc.compendium_sync().await.unwrap();
        let mut st = svc.job_status(&job).unwrap();
        for _ in 0..2000 {
            st = svc.job_status(&job).unwrap();
            if st.state != JobState::Running {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(st.state, JobState::Failed);
        assert!(st.error.unwrap_or_default().contains("127.0.0.1:9"));
        assert!(!svc.compendium_status().await.unwrap().synced);
    }

    /// Every spell and item of a real mirror becomes a card that parses back
    /// with no 5etools tags left. Needs a running mirror:
    /// `SLEUTH_TEST_MIRROR=http://localhost:8080 cargo test -p sleuth-service real_mirror -- --ignored`
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn real_mirror_cards() {
        let Ok(base) = std::env::var("SLEUTH_TEST_MIRROR") else { return };
        let (_d, svc) = temp_service();
        let mut p = serde_json::Map::new();
        p.insert("fivetools_base_url".into(), json!(base));
        svc.settings_set(p).await.unwrap();
        let job = svc.compendium_sync().await.unwrap();
        let st = loop {
            let st = svc.job_status(&job).unwrap();
            if st.state != JobState::Running {
                break st;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        };
        assert_eq!(st.state, JobState::Done, "{:?}", st.error);
        for kind in [CompendiumKind::Spell, CompendiumKind::Item] {
            let all = svc
                .compendium_search(CompendiumQuery { limit: Some(2000), ..q(kind, "") })
                .await
                .unwrap();
            assert!(all.len() > 100, "{kind:?}: {}", all.len());
            let refs: Vec<CompendiumRef> =
                all.iter().map(|e| CompendiumRef { kind, id: e.id.clone() }).collect();
            let out = svc.compendium_cards(refs).await.unwrap();
            assert!(out.missing.is_empty(), "{:?}", out.missing);
            let left: Vec<&str> =
                out.markdown.lines().filter(|l| l.contains("{@")).take(5).collect();
            assert!(left.is_empty(), "tags left in the {kind:?} cards: {left:#?}");
            let parsed = svc.cards_parse(out.markdown.clone()).await.unwrap();
            assert_eq!(parsed.len(), out.cards, "{kind:?}: card count after parsing");
            for (c, e) in parsed.iter().zip(&all) {
                assert_eq!(c.title, e.name.replace(['*', '_'], ""), "{kind:?} {}", e.id);
            }
        }
        let fb = svc
            .compendium_cards(vec![CompendiumRef { kind: CompendiumKind::Spell, id: "fireball_xphb".into() }])
            .await
            .unwrap();
        println!("{}", fb.markdown);
    }
}
