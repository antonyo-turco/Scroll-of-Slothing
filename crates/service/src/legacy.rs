//! One-shot import from Orb of Many Functions. Spec: docs/legacy-import.md.
//!
//! Idempotent: assets are matched on `assets.legacy_id`, campaigns/sessions on
//! their (preserved) uuids, so re-running only adds what is missing.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sleuth_core::settings::Settings;
use sleuth_core::units::SizeLabel;
use sleuth_legacy::{LegacyAsset, LegacyDb};
use sleuth_store::{LinkTarget, NewAsset, Store};

use crate::library::{check_kind, store_encoded};
use crate::{JobCtx, JobId, Result, Service, ServiceError};

/// Max concurrent asset conversions. Legacy maps decode to hundreds of MB each,
/// so this is deliberately small to bound memory.
const IMPORT_THREADS: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyImportOptions {
    /// Path to the old `db/creatures.db` (opened read-only).
    pub db_path: PathBuf,
    /// Optional old `app_settings.json`.
    pub settings_path: Option<PathBuf>,
    /// Optional old `aoe_presets.json`.
    pub aoe_presets_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyFailure {
    pub legacy_id: i64,
    pub name: String,
    pub error: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LegacyImportReport {
    pub assets: BTreeMap<String, u64>,
    pub skipped_existing: u64,
    pub failed: Vec<LegacyFailure>,
    pub campaigns: u64,
    pub sessions: u64,
    pub tag_links: u64,
    pub asset_campaign_links: u64,
    pub asset_session_links: u64,
    pub music_rows: BTreeMap<String, u64>,
    pub music_json: Option<PathBuf>,
    pub settings_imported: bool,
    pub aoe_presets: u64,
}

fn convert(store: &Store, a: &LegacyAsset) -> Result<NewAsset> {
    let kind = if a.kind.is_empty() { "token".to_string() } else { a.kind.clone() };
    check_kind(&kind)?;
    let stored = store_encoded(store, &a.original, &a.token)?;
    let pdf_blob = match &a.pdf {
        Some(pdf) if !pdf.is_empty() => Some(store.put_blob(pdf, "application/pdf", None)?.hash),
        _ => None,
    };
    let aliases = a.aliases.replace(',', "\n").lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
    Ok(NewAsset {
        kind,
        name: if a.name.trim().is_empty() { format!("Asset {}", a.id) } else { a.name.trim().to_string() },
        aliases,
        size_label: SizeLabel::parse(&a.size).unwrap_or_default().as_str().into(),
        original_blob: stored.original_blob,
        rendered_blob: stored.rendered_blob,
        pdf_blob,
        thumb_blob: Some(stored.thumb_blob),
        preview_blob: Some(stored.preview_blob),
        params_json: None,
        legacy_id: Some(a.id),
        pinned: a.pinned,
    })
}

/// Runs the import synchronously (call from a blocking context).
pub(crate) fn run(store: &Store, opts: &LegacyImportOptions, ctx: Option<&JobCtx>) -> Result<LegacyImportReport> {
    let progress = |p: f32, m: String| {
        if let Some(c) = ctx {
            c.progress(p, m);
        }
    };
    let check = || ctx.map(JobCtx::check).unwrap_or(Ok(()));
    let legacy = LegacyDb::open(&opts.db_path).map_err(|e| ServiceError::InvalidInput(e.to_string()))?;
    let lerr = |e: sleuth_legacy::LegacyError| ServiceError::Io(e.to_string());
    let mut report = LegacyImportReport::default();

    // Campaigns & sessions (uuids preserved).
    let existing_c: Vec<String> = store.list_campaigns()?.into_iter().map(|c| c.id).collect();
    for c in legacy.campaigns().map_err(lerr)? {
        if !existing_c.contains(&c.id) {
            store.create_campaign_with_id(&c.id, &c.name, Some(&c.created))?;
            report.campaigns += 1;
        }
    }
    let existing_s: Vec<String> = store.list_sessions(None)?.into_iter().map(|s| s.id).collect();
    for s in legacy.sessions().map_err(lerr)? {
        if !existing_s.contains(&s.id) {
            store.create_session_with_id(&s.id, &s.campaign_id, &s.name, &s.notes, Some(&s.created))?;
            report.sessions += 1;
        }
    }

    // Assets: skip already-imported ones, convert the rest with bounded parallelism.
    let ids = legacy.asset_ids().map_err(lerr)?;
    let total = ids.len().max(1) as f32;
    let mut id_map: HashMap<i64, i64> = HashMap::new();
    let mut todo = Vec::new();
    for lid in ids {
        match store.asset_id_by_legacy(lid)? {
            Some(nid) => {
                id_map.insert(lid, nid);
                report.skipped_existing += 1;
            }
            None => todo.push(lid),
        }
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(IMPORT_THREADS)
        .build()
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
    let done = std::sync::atomic::AtomicUsize::new(report.skipped_existing as usize);
    // The legacy connection is not Sync; read rows sequentially, convert in parallel per chunk.
    for chunk in todo.chunks(IMPORT_THREADS * 2) {
        check()?;
        let rows: Vec<std::result::Result<LegacyAsset, LegacyFailure>> = chunk
            .iter()
            .map(|lid| {
                legacy.asset(*lid).map_err(|e| LegacyFailure { legacy_id: *lid, name: String::new(), error: e.to_string() })
            })
            .collect();
        let converted: Vec<std::result::Result<(i64, NewAsset), LegacyFailure>> = pool.install(|| {
            rows.into_par_iter()
                .map(|r| {
                    let a = r?;
                    let out = convert(store, &a)
                        .map(|n| (a.id, n))
                        .map_err(|e| LegacyFailure { legacy_id: a.id, name: a.name.clone(), error: e.to_string() });
                    let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    progress(0.05 + 0.85 * n as f32 / total, format!("{} ({n}/{})", a.name, total as usize));
                    out
                })
                .collect()
        });
        for c in converted {
            match c {
                Ok((lid, asset)) => {
                    *report.assets.entry(asset.kind.clone()).or_default() += 1;
                    id_map.insert(lid, store.insert_asset(&asset)?);
                }
                Err(f) => report.failed.push(f),
            }
        }
    }

    // Tags and links (idempotent: set/insert-or-ignore).
    progress(0.92, "tags and links".into());
    let mut tags_by_asset: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    for (lid, name) in legacy.asset_tags().map_err(lerr)? {
        tags_by_asset.entry(lid).or_default().push(name);
    }
    for (lid, names) in tags_by_asset {
        if let Some(nid) = id_map.get(&lid) {
            let mut current = store.get_asset(*nid)?.tags;
            current.extend(names.iter().cloned());
            store.set_asset_tags(*nid, &current)?;
            report.tag_links += names.len() as u64;
        }
    }
    for (lid, sid) in legacy.session_assets().map_err(lerr)? {
        if let Some(nid) = id_map.get(&lid) {
            store.link_assets(&[*nid], LinkTarget::Session(&sid))?;
            report.asset_session_links += 1;
        }
    }
    for (lid, cid) in legacy.asset_campaigns().map_err(lerr)? {
        if let Some(nid) = id_map.get(&lid) {
            store.link_assets(&[*nid], LinkTarget::Campaign(&cid))?;
            report.asset_campaign_links += 1;
        }
    }

    // Music: preserved raw for the future music phase.
    progress(0.95, "music data".into());
    let music = legacy.music_dump().map_err(lerr)?;
    if !music.is_empty() {
        for (t, rows) in &music {
            report.music_rows.insert(t.clone(), rows.as_array().map_or(0, |a| a.len() as u64));
        }
        let dir = store.root().join("legacy");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("music.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&Value::Object(music)).map_err(|e| ServiceError::Internal(e.to_string()))?)?;
        report.music_json = Some(path);
    }

    if let Some(p) = &opts.settings_path {
        let raw = std::fs::read_to_string(p)?;
        let v: Value = serde_json::from_str(&raw).map_err(|e| ServiceError::InvalidInput(format!("app_settings.json: {e}")))?;
        store.save_settings(&Settings::from_legacy_json(&v))?;
        report.settings_imported = true;
    }
    if let Some(p) = &opts.aoe_presets_path {
        for (name, cfg) in sleuth_legacy::read_aoe_presets(&std::fs::read_to_string(p)?) {
            store.upsert_aoe_preset(&name, &cfg)?;
            report.aoe_presets += 1;
        }
    }
    progress(1.0, "done".into());
    Ok(report)
}

impl Service {
    /// Starts the legacy import as a background job. The job result is a
    /// [`LegacyImportReport`] (as JSON).
    pub fn legacy_import(&self, opts: LegacyImportOptions) -> JobId {
        self.spawn_job("legacy import", move |ctx| {
            let store = ctx.service().store();
            let report = run(&store, &opts, Some(&ctx))?;
            serde_json::to_value(report).map_err(|e| ServiceError::Internal(e.to_string()))
        })
    }

    /// Runs the import to completion (CLI / tests).
    pub async fn legacy_import_blocking(&self, opts: LegacyImportOptions) -> Result<LegacyImportReport> {
        self.blocking(move |s| run(s, &opts, None)).await
    }
}

#[cfg(test)]
mod tests {
    use sleuth_store::AssetQuery;

    use super::*;
    use crate::library::tests::write_test_png;
    use crate::temp_service;

    fn setup() -> (tempfile::TempDir, Service, LegacyImportOptions) {
        let (d, svc) = temp_service();
        let png = std::fs::read(write_test_png(d.path(), "src.png", 300, 200)).unwrap();
        let db = d.path().join("creatures.db");
        sleuth_legacy::fixture::build(&db, &png);
        let settings = d.path().join("app_settings.json");
        std::fs::write(&settings, r#"{"start_tab": "Card Printer", "map_overlap_in": "0.5"}"#).unwrap();
        let aoe = d.path().join("aoe_presets.json");
        std::fs::write(&aoe, r#"{"10 ft cube": {"shape": "square", "a_ft": 10.0}}"#).unwrap();
        let opts = LegacyImportOptions { db_path: db, settings_path: Some(settings), aoe_presets_path: Some(aoe) };
        (d, svc, opts)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn imports_everything_and_is_idempotent() {
        let (_d, svc, opts) = setup();
        let r = svc.legacy_import_blocking(opts.clone()).await.unwrap();
        assert!(r.failed.is_empty(), "{:?}", r.failed);
        assert_eq!(r.assets.values().sum::<u64>(), 3);
        assert_eq!((r.campaigns, r.sessions, r.tag_links), (1, 1, 3));
        assert_eq!((r.asset_session_links, r.asset_campaign_links), (1, 1));
        assert_eq!(r.music_rows.get("music_tracks"), Some(&1));
        assert!(r.music_json.as_ref().unwrap().exists());
        assert!(r.settings_imported);
        assert_eq!(r.aoe_presets, 1);

        let all = svc.library_search(AssetQuery::default()).await.unwrap();
        let zombie = all.items.iter().find(|a| a.name == "Zombie").unwrap();
        assert!(zombie.pinned);
        assert_eq!(zombie.tags, vec!["boss".to_string(), "Undead".to_string()]);
        assert_eq!(zombie.session_ids, vec!["s1".to_string()]);
        assert_eq!(zombie.legacy_id, Some(1));
        let map = all.items.iter().find(|a| a.kind == "map").unwrap();
        assert!(map.pdf_blob.is_some());
        let lamp = all.items.iter().find(|a| a.name == "Lamp").unwrap();
        assert_eq!(lamp.size_label, "Tiny");
        let by_campaign =
            svc.library_search(AssetQuery { campaign_id: Some("c1".into()), ..Default::default() }).await.unwrap();
        assert_eq!(by_campaign.total, 2, "map (direct) + zombie (via session)");

        let st = svc.settings_get().await.unwrap();
        assert_eq!((st.start_view.as_str(), st.map_overlap_in), ("cards", 0.5));

        let again = svc.legacy_import_blocking(opts).await.unwrap();
        assert_eq!(again.skipped_existing, 3);
        assert_eq!(again.assets.values().sum::<u64>(), 0);
        assert_eq!((again.campaigns, again.sessions), (0, 0));
        assert_eq!(svc.library_search(AssetQuery::default()).await.unwrap().total, 3);
        assert_eq!(svc.library_get(zombie.id).await.unwrap().tags.len(), 2, "no duplicate tags");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn corrupt_images_are_reported_not_fatal() {
        let (d, svc) = temp_service();
        let db = d.path().join("creatures.db");
        sleuth_legacy::fixture::build(&db, b"not an image");
        let r = svc
            .legacy_import_blocking(LegacyImportOptions { db_path: db, settings_path: None, aoe_presets_path: None })
            .await
            .unwrap();
        assert_eq!(r.failed.len(), 3);
        assert_eq!(r.campaigns, 1, "metadata still imported");
    }

    #[tokio::test]
    async fn rejects_non_legacy_db() {
        let (d, svc) = temp_service();
        let err = svc
            .legacy_import_blocking(LegacyImportOptions {
                db_path: d.path().join("sleuth.db"),
                settings_path: None,
                aoe_presets_path: None,
            })
            .await
            .unwrap_err();
        assert_eq!(err.code(), crate::ErrorCode::InvalidInput);
    }
}
