//! Session planner service. Spec: docs/features/session-planner.md.
//! The planner is autosaved in the `planner` workspace. Items are references
//! into the other tools; the view resolves them, and starred (pinned or
//! favourite) items join the current session on their own.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sleuth_core::planner::{
    CheckItem, PlanItem, Planner, RefKind, Section, SessionPlan, file_stem, prints_on_sheet,
    section, starred_extras,
};
use sleuth_store::{AssetQuery, Store, now};

use crate::{JobId, Result, Service, ServiceError};

const KEY: &str = "planner";

fn load(s: &Store) -> Result<Planner> {
    Ok(s.workspace_get::<Planner>(KEY)?.unwrap_or_default())
}

fn save(s: &Store, p: &Planner) -> Result<()> {
    s.workspace_set(KEY, p)?;
    s.record_change("planner", KEY, "update")?;
    Ok(())
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Changes to a session; `None` leaves a field as it is.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionPatch {
    pub name: Option<String>,
    /// `Some("")` clears the date.
    pub date: Option<String>,
    pub notes: Option<String>,
    /// Replaces the whole checklist (items without an id get one).
    pub checklist: Option<Vec<CheckItem>>,
    pub include_token_sheet: Option<bool>,
}

/// One resolved line of a session.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanEntry {
    /// The planned item; `None` for a starred item that joined on its own.
    pub item_id: Option<String>,
    pub kind: RefKind,
    pub ref_id: String,
    pub section: Section,
    pub name: String,
    /// Short description: the asset kind, the creatures, the track count …
    pub detail: String,
    /// Thumbnail blob, when the item has one.
    pub thumb: Option<String>,
    /// Asset kind (`token`, `cutout`, `map` …) for assets.
    pub asset_kind: Option<String>,
    pub copies: u32,
    /// Pinned or favourite in its own tool.
    pub starred: bool,
    /// The referenced item no longer exists.
    pub missing: bool,
}

/// A session with its items resolved.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionView {
    #[serde(flatten)]
    pub plan: SessionPlan,
    pub current: bool,
    /// Section order, then the order items were added; starred extras last
    /// within their section.
    pub entries: Vec<PlanEntry>,
}

/// The planner overview: sessions (without resolved items) and the current one.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlannerSummary {
    pub sessions: Vec<SessionSummary>,
    pub current: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub date: Option<String>,
    pub items: usize,
    /// Checklist done / total.
    pub done: usize,
    pub total: usize,
    pub updated: String,
}

/// What a reference resolves to.
struct Resolved {
    name: String,
    detail: String,
    thumb: Option<String>,
    asset_kind: Option<String>,
    starred: bool,
}

/// Everything referenceable that is cheap to list once per view.
struct Catalog {
    encounters: HashMap<String, Resolved>,
    soundscapes: HashMap<String, Resolved>,
    playlists: HashMap<String, Resolved>,
    decks: HashMap<String, Resolved>,
}

impl Catalog {
    fn load(s: &Store) -> Result<Self> {
        let encounters = crate::combat::load_encounters(s)?
            .into_iter()
            .map(|e| {
                let n: u32 = e.creatures.iter().map(|c| c.count).sum();
                let detail = if n > 0 {
                    format!("{n} creature{}", if n == 1 { "" } else { "s" })
                } else {
                    "saved combat".to_string()
                };
                let r = Resolved {
                    name: e.name,
                    detail,
                    thumb: None,
                    asset_kind: None,
                    starred: e.pinned,
                };
                (e.id, r)
            })
            .collect();
        let soundscapes = s
            .music_soundscapes(None)?
            .into_iter()
            .map(|x| {
                let n = x.channels.len();
                let r = Resolved {
                    name: x.name,
                    detail: format!("soundscape, {n} channel{}", if n == 1 { "" } else { "s" }),
                    thumb: None,
                    asset_kind: None,
                    starred: x.pinned,
                };
                (x.id, r)
            })
            .collect();
        let playlists = s
            .music_playlists(None)?
            .into_iter()
            .map(|x| {
                let n = x.items.len();
                let r = Resolved {
                    name: x.name,
                    detail: format!("playlist, {n} track{}", if n == 1 { "" } else { "s" }),
                    thumb: None,
                    asset_kind: None,
                    starred: false,
                };
                (x.id, r)
            })
            .collect();
        let decks = s
            .list_decks()?
            .into_iter()
            .map(|d| {
                let r = Resolved {
                    name: d.name,
                    detail: "card deck".into(),
                    thumb: None,
                    asset_kind: None,
                    starred: false,
                };
                (d.id.to_string(), r)
            })
            .collect();
        Ok(Catalog { encounters, soundscapes, playlists, decks })
    }

    fn resolve(&self, s: &Store, kind: RefKind, ref_id: &str) -> Option<Resolved> {
        let copy = |r: &Resolved| Resolved {
            name: r.name.clone(),
            detail: r.detail.clone(),
            thumb: r.thumb.clone(),
            asset_kind: r.asset_kind.clone(),
            starred: r.starred,
        };
        match kind {
            RefKind::Encounter => self.encounters.get(ref_id).map(copy),
            RefKind::Soundscape => self.soundscapes.get(ref_id).map(copy),
            RefKind::Playlist => self.playlists.get(ref_id).map(copy),
            RefKind::Deck => self.decks.get(ref_id).map(copy),
            RefKind::Track => s.music_track_get(ref_id).ok().map(|t| Resolved {
                name: t.title,
                detail: format!("{} track", t.category),
                thumb: None,
                asset_kind: None,
                starred: t.favorite,
            }),
            RefKind::Asset => {
                let a = s.get_asset(ref_id.parse().ok()?).ok()?;
                Some(Resolved {
                    detail: a.kind.clone(),
                    thumb: a.thumb_blob.or(Some(a.rendered_blob)),
                    asset_kind: Some(a.kind),
                    name: a.name,
                    starred: a.pinned,
                })
            }
        }
    }
}

/// Starred references in their tools: pinned assets, encounters and
/// soundscapes, favourite tracks.
fn starred(s: &Store) -> Result<Vec<(RefKind, String)>> {
    let mut out = vec![];
    let q = AssetQuery { pinned_only: true, limit: Some(500), ..Default::default() };
    out.extend(s.search_assets(&q)?.items.into_iter().map(|a| (RefKind::Asset, a.id.to_string())));
    out.extend(
        crate::combat::load_encounters(s)?
            .into_iter()
            .filter(|e| e.pinned)
            .map(|e| (RefKind::Encounter, e.id)),
    );
    out.extend(
        s.music_soundscapes(None)?
            .into_iter()
            .filter(|x| x.pinned)
            .map(|x| (RefKind::Soundscape, x.id)),
    );
    let tq = sleuth_store::MusicTrackQuery { favorites_only: true, ..Default::default() };
    out.extend(s.music_tracks_list(&tq)?.into_iter().map(|t| (RefKind::Track, t.id)));
    Ok(out)
}

fn view(s: &Store, pl: &Planner, id: &str) -> Result<SessionView> {
    let plan = pl.get(id).ok_or_else(|| ServiceError::NotFound(format!("session {id}")))?;
    let cat = Catalog::load(s)?;
    let entry = |item_id: Option<String>, kind: RefKind, ref_id: &str, copies: u32| match cat
        .resolve(s, kind, ref_id)
    {
        Some(r) => PlanEntry {
            item_id,
            kind,
            ref_id: ref_id.to_string(),
            section: section(kind, r.asset_kind.as_deref()),
            name: r.name,
            detail: r.detail,
            thumb: r.thumb,
            asset_kind: r.asset_kind,
            copies,
            starred: r.starred,
            missing: false,
        },
        None => PlanEntry {
            item_id,
            kind,
            ref_id: ref_id.to_string(),
            section: section(kind, None),
            name: "(deleted)".into(),
            detail: "no longer in its tool".into(),
            thumb: None,
            asset_kind: None,
            copies,
            starred: false,
            missing: true,
        },
    };
    let mut entries: Vec<PlanEntry> =
        plan.items.iter().map(|i| entry(Some(i.id.clone()), i.kind, &i.ref_id, i.copies)).collect();
    if pl.is_current(id) {
        for (k, r) in starred_extras(plan, &starred(s)?) {
            entries.push(entry(None, k, &r, 1));
        }
    }
    // stable: keeps the added order (and starred extras after) inside each section
    entries.sort_by_key(|e| e.section);
    Ok(SessionView { plan: plan.clone(), current: pl.is_current(id), entries })
}

fn check_copies(c: u32) -> Result<u32> {
    if (1..=99).contains(&c) {
        Ok(c)
    } else {
        Err(ServiceError::InvalidInput("copies must be 1–99".into()))
    }
}

impl Service {
    async fn planner_mutate<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Store, &mut Planner) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        self.blocking(move |s| {
            let mut pl = load(s)?;
            let out = f(s, &mut pl)?;
            save(s, &pl)?;
            Ok(out)
        })
        .await
    }

    /// The sessions (newest last) and the current one.
    pub async fn planner_list(&self) -> Result<PlannerSummary> {
        self.blocking(|s| {
            let pl = load(s)?;
            let sessions = pl
                .sessions
                .iter()
                .map(|p| {
                    let (done, total) = p.progress();
                    SessionSummary {
                        id: p.id.clone(),
                        name: p.name.clone(),
                        date: p.date.clone(),
                        items: p.items.len(),
                        done,
                        total,
                        updated: p.updated.clone(),
                    }
                })
                .collect();
            Ok(PlannerSummary { sessions, current: pl.current })
        })
        .await
    }

    /// A session with every item resolved (plus starred items when it is current).
    pub async fn planner_session_get(&self, id: String) -> Result<SessionView> {
        self.blocking(move |s| view(s, &load(s)?, &id)).await
    }

    /// Creates a session (default name `Session N`); the first one becomes current.
    /// `current: true` makes it current anyway.
    pub async fn planner_session_create(
        &self,
        name: Option<String>,
        current: Option<bool>,
    ) -> Result<SessionView> {
        self.planner_mutate(move |s, pl| {
            let name = match name.map(|n| n.trim().to_string()) {
                Some(n) if !n.is_empty() => n,
                _ => format!("Session {}", pl.sessions.len() + 1),
            };
            let id = new_id();
            pl.push(SessionPlan::new(id.clone(), name, now()));
            if current == Some(true) {
                pl.set_current(&id);
            }
            view(s, pl, &id)
        })
        .await
    }

    pub async fn planner_session_update(
        &self,
        id: String,
        patch: SessionPatch,
    ) -> Result<SessionView> {
        self.planner_mutate(move |s, pl| {
            let p =
                pl.get_mut(&id).ok_or_else(|| ServiceError::NotFound(format!("session {id}")))?;
            if let Some(n) = patch.name {
                let n = n.trim();
                if n.is_empty() {
                    return Err(ServiceError::InvalidInput("a session needs a name".into()));
                }
                p.name = n.to_string();
            }
            if let Some(d) = patch.date {
                p.date = Some(d.trim().to_string()).filter(|d| !d.is_empty());
            }
            if let Some(n) = patch.notes {
                p.notes = n;
            }
            if let Some(mut list) = patch.checklist {
                for c in &mut list {
                    if c.id.is_empty() {
                        c.id = new_id();
                    }
                }
                p.checklist = list;
            }
            if let Some(b) = patch.include_token_sheet {
                p.include_token_sheet = b;
            }
            p.updated = now();
            view(s, pl, &id)
        })
        .await
    }

    pub async fn planner_session_delete(&self, id: String) -> Result<PlannerSummary> {
        self.planner_mutate(move |_, pl| {
            if !pl.remove(&id) {
                return Err(ServiceError::NotFound(format!("session {id}")));
            }
            Ok(())
        })
        .await?;
        self.planner_list().await
    }

    /// Makes a session the current one (starred items join it).
    pub async fn planner_set_current(&self, id: String) -> Result<SessionView> {
        self.planner_mutate(move |s, pl| {
            if !pl.set_current(&id) {
                return Err(ServiceError::NotFound(format!("session {id}")));
            }
            view(s, pl, &id)
        })
        .await
    }

    /// Adds references to a session (once each; re-adding is a no-op). Each
    /// reference must exist in its tool.
    pub async fn planner_items_add(
        &self,
        session: String,
        items: Vec<PlanRef>,
    ) -> Result<SessionView> {
        self.planner_mutate(move |s, pl| {
            let cat = Catalog::load(s)?;
            let mut missing = vec![];
            for r in &items {
                check_copies(r.copies.unwrap_or(1))?;
                if cat.resolve(s, r.kind, &r.ref_id).is_none() {
                    missing.push(format!("{:?} {}", r.kind, r.ref_id).to_lowercase());
                }
            }
            if !missing.is_empty() {
                return Err(ServiceError::NotFound(format!("not found: {}", missing.join(", "))));
            }
            let p = pl
                .get_mut(&session)
                .ok_or_else(|| ServiceError::NotFound(format!("session {session}")))?;
            for r in items {
                p.add(new_id(), r.kind, r.ref_id, r.copies.unwrap_or(1));
            }
            p.updated = now();
            view(s, pl, &session)
        })
        .await
    }

    pub async fn planner_item_update(
        &self,
        session: String,
        item: String,
        copies: u32,
    ) -> Result<SessionView> {
        let copies = check_copies(copies)?;
        self.planner_mutate(move |s, pl| {
            let p = pl
                .get_mut(&session)
                .ok_or_else(|| ServiceError::NotFound(format!("session {session}")))?;
            let it: &mut PlanItem = p
                .items
                .iter_mut()
                .find(|i| i.id == item)
                .ok_or_else(|| ServiceError::NotFound(format!("planned item {item}")))?;
            it.copies = copies;
            p.updated = now();
            view(s, pl, &session)
        })
        .await
    }

    pub async fn planner_item_remove(&self, session: String, item: String) -> Result<SessionView> {
        self.planner_mutate(move |s, pl| {
            let p = pl
                .get_mut(&session)
                .ok_or_else(|| ServiceError::NotFound(format!("session {session}")))?;
            if !p.remove(&item) {
                return Err(ServiceError::NotFound(format!("planned item {item}")));
            }
            p.updated = now();
            view(s, pl, &session)
        })
        .await
    }

    /// Drops references whose item was deleted in its tool. Returns how many.
    pub async fn planner_prune(&self, session: String) -> Result<usize> {
        self.planner_mutate(move |s, pl| {
            let cat = Catalog::load(s)?;
            let p = pl
                .get_mut(&session)
                .ok_or_else(|| ServiceError::NotFound(format!("session {session}")))?;
            let n = p.prune(|i| cat.resolve(s, i.kind, &i.ref_id).is_some());
            if n > 0 {
                p.updated = now();
            }
            Ok(n)
        })
        .await
    }

    /// Prints everything printable of a session into folder `dir` as a job:
    /// one sheet of its tokens and cut-outs (plus the Token Sheet when the
    /// session asks for it), each map's saved PDF, each card deck. Result:
    /// `{dir, files: [path], skipped: [reason]}`; nothing is dropped silently.
    pub fn planner_print(&self, session: String, dir: PathBuf) -> JobId {
        self.spawn_job("session print", move |ctx| {
            let svc = ctx.service().clone();
            let store = svc.store();
            let pl = load(&store)?;
            let v = view(&store, &pl, &session)?;
            std::fs::create_dir_all(&dir)?;
            let prefix = file_stem(&v.plan.name);
            let mut files: Vec<PathBuf> = vec![];
            let mut skipped: Vec<String> = vec![];
            let live: Vec<&PlanEntry> = v.entries.iter().filter(|e| !e.missing).collect();
            for e in v.entries.iter().filter(|e| e.missing) {
                skipped.push(format!("a deleted {:?} (ref {})", e.kind, e.ref_id).to_lowercase());
            }

            // tokens and cut-outs: one sheet
            let mut sheet = crate::token_sheet::load(&store)?;
            let mut objects = crate::object_sheet::load(&store)?;
            if !v.plan.include_token_sheet {
                sheet.items.clear();
                objects.items.clear();
            }
            let mut sheet_assets = vec![];
            for e in &live {
                if e.kind == RefKind::Asset && e.asset_kind.as_deref().is_some_and(prints_on_sheet)
                {
                    let id: i64 = e.ref_id.parse().map_err(|_| {
                        ServiceError::InvalidInput(format!("bad asset id {}", e.ref_id))
                    })?;
                    sheet_assets.push((store.get_asset(id)?, e.copies));
                }
            }
            ctx.progress(0.02, "collecting tokens");
            crate::token_sheet::add_assets_to(&store, &sheet_assets, &mut sheet, &mut objects)?;
            let maps: Vec<&&PlanEntry> =
                live.iter().filter(|e| e.asset_kind.as_deref() == Some("map")).collect();
            let decks: Vec<&&PlanEntry> = live.iter().filter(|e| e.kind == RefKind::Deck).collect();
            let parts = usize::from(!sheet.items.is_empty() || !objects.items.is_empty())
                + maps.len()
                + decks.len();
            if parts == 0 {
                return Err(ServiceError::InvalidInput(
                    "nothing to print: add tokens, cut-outs, maps or card decks".into(),
                ));
            }
            let span = 0.95 / parts as f32;
            let mut at = 0.03_f32;
            let mut next_path = |name: &str| {
                let mut p = dir.join(format!("{prefix} - {}.pdf", file_stem(name)));
                let mut n = 2;
                while files.contains(&p) || p.exists() {
                    p = dir.join(format!("{prefix} - {} ({n}).pdf", file_stem(name)));
                    n += 1;
                }
                files.push(p.clone());
                p
            };
            if !sheet.items.is_empty() || !objects.items.is_empty() {
                let (bytes, _) =
                    crate::token_sheet::render_sheet(&store, &sheet, &objects, &ctx, (at, span))?;
                std::fs::write(next_path("Tokens and cut-outs"), bytes)?;
                at += span;
            }
            for e in maps {
                if ctx.is_cancelled() {
                    return Err(ServiceError::Cancelled);
                }
                ctx.progress(at, format!("map {}", e.name));
                let a = store.get_asset(e.ref_id.parse().unwrap_or_default())?;
                let doc = a
                    .params_json
                    .as_deref()
                    .and_then(|j| serde_json::from_str::<crate::MapDoc>(j).ok());
                let pdf = match (a.pdf_blob, doc) {
                    (Some(b), _) => Some(store.read_blob(&b)?),
                    (None, Some(doc)) => Some(svc.render_map_pdf(&store, &doc, &ctx)?.0),
                    (None, None) => None,
                };
                match pdf {
                    Some(bytes) => std::fs::write(next_path(&e.name), bytes)?,
                    None => skipped.push(format!(
                        "map \"{}\": no saved pages (open it in the Map Paginator and save it)",
                        e.name
                    )),
                }
                at += span;
            }
            for e in decks {
                let d = store.get_deck(e.ref_id.parse().unwrap_or_default())?;
                match crate::cards::render_deck(
                    &d.markdown,
                    crate::cards::deck_options(&d.settings_json),
                    &ctx,
                    (at, span),
                ) {
                    Ok((bytes, _)) => std::fs::write(next_path(&e.name), bytes)?,
                    Err(ServiceError::Cancelled) => return Err(ServiceError::Cancelled),
                    Err(err) => skipped.push(format!("deck \"{}\": {err}", e.name)),
                }
                at += span;
            }
            Ok(serde_json::json!({ "dir": dir, "files": files, "skipped": skipped }))
        })
    }
}

/// One reference for `planner_items_add`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRef {
    pub kind: RefKind,
    pub ref_id: String,
    /// Copies to print (tokens and cut-outs), default 1.
    #[serde(default)]
    pub copies: Option<u32>,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::library::tests::write_test_png;
    use crate::{ImportFile, JobState, temp_service};

    async fn import(svc: &Service, dir: &std::path::Path, name: &str, kind: &str) -> i64 {
        let p = write_test_png(dir, &format!("{name}.png"), 64, 64);
        svc.library_import(vec![ImportFile {
            path: p,
            name: Some(name.into()),
            kind: kind.into(),
            size: Some("Medium".into()),
        }])
        .await
        .unwrap()
        .imported[0]
    }

    fn asset(id: i64) -> PlanRef {
        PlanRef { kind: RefKind::Asset, ref_id: id.to_string(), copies: Some(2) }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sessions_items_and_starred_extras() {
        let (d, svc) = temp_service();
        let first = svc.planner_session_create(None, None).await.unwrap();
        assert_eq!(first.plan.name, "Session 1");
        assert!(first.current, "the first session is current");
        let second = svc.planner_session_create(Some("Cave".into()), None).await.unwrap();
        assert!(!second.current);

        let ogre = import(&svc, d.path(), "Ogre", "token").await;
        let lamp = import(&svc, d.path(), "Lamp", "cutout").await;
        let v = svc
            .planner_items_add(first.plan.id.clone(), vec![asset(ogre), asset(ogre), asset(lamp)])
            .await
            .unwrap();
        assert_eq!(v.entries.len(), 2, "re-adding is a no-op");
        assert!(v.entries.iter().all(|e| e.section == Section::Tokens && e.copies == 2));
        assert!(
            svc.planner_items_add(first.plan.id.clone(), vec![asset(9999)]).await.is_err(),
            "unknown references are refused"
        );

        // a pinned asset joins the current session only
        let map = import(&svc, d.path(), "Road", "map").await;
        svc.library_bulk_pin(vec![map], true).await.unwrap();
        let v = svc.planner_session_get(first.plan.id.clone()).await.unwrap();
        let star = v.entries.iter().find(|e| e.ref_id == map.to_string()).unwrap();
        assert!(star.starred && star.item_id.is_none());
        assert_eq!(star.section, Section::Maps);
        let other = svc.planner_session_get(second.plan.id.clone()).await.unwrap();
        assert!(other.entries.is_empty());
        let other = svc.planner_set_current(second.plan.id.clone()).await.unwrap();
        assert_eq!(other.entries.len(), 1);

        let patch = SessionPatch {
            date: Some("2026-11-01".into()),
            checklist: Some(vec![CheckItem {
                id: String::new(),
                text: "Recap".into(),
                done: true,
            }]),
            ..Default::default()
        };
        let v = svc.planner_session_update(first.plan.id.clone(), patch).await.unwrap();
        assert_eq!(v.plan.date.as_deref(), Some("2026-11-01"));
        assert!(!v.plan.checklist[0].id.is_empty());
        let list = svc.planner_list().await.unwrap();
        assert_eq!((list.sessions[0].done, list.sessions[0].total), (1, 1));

        svc.library_delete(vec![lamp]).await.unwrap();
        let v = svc.planner_session_get(first.plan.id.clone()).await.unwrap();
        assert_eq!(v.entries.iter().filter(|e| e.missing).count(), 1);
        assert_eq!(svc.planner_prune(first.plan.id.clone()).await.unwrap(), 1);

        let left = svc.planner_session_delete(second.plan.id).await.unwrap();
        assert_eq!(left.current.as_deref(), Some(first.plan.id.as_str()));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn print_writes_one_sheet_and_reports_what_it_skips() {
        let (d, svc) = temp_service();
        let s = svc.planner_session_create(Some("Night 3".into()), None).await.unwrap();
        let ogre = import(&svc, d.path(), "Ogre", "token").await;
        let road = import(&svc, d.path(), "Road", "map").await;
        svc.planner_items_add(s.plan.id.clone(), vec![asset(ogre), asset(road)]).await.unwrap();
        let out = d.path().join("print");
        let job = svc.planner_print(s.plan.id.clone(), out.clone());
        let mut st = svc.job_status(&job).unwrap();
        for _ in 0..4000 {
            st = svc.job_status(&job).unwrap();
            if st.state != JobState::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(st.state, JobState::Done, "{:?}", st.error);
        let r = st.result.unwrap();
        assert_eq!(r["files"].as_array().unwrap().len(), 1);
        assert!(out.join("Night 3 - Tokens and cut-outs.pdf").exists());
        let skipped = r["skipped"].as_array().unwrap();
        assert_eq!(skipped.len(), 1, "an imported map has no saved pages: {skipped:?}");
    }
}
