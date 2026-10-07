//! Tauri commands. Each one is a one-line delegation to the service layer —
//! keep logic out of here so MCP/CLI get identical behaviour.
//! Command names match the MCP tool names where both exist.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Map, Value};
use sleuth_core::settings::Settings;
use sleuth_service::{
    Asset, AssetPage, AssetPatch, AssetQuery, Campaign, ExportWhat, ImportFile, ImportReport,
    JobId, JobStatus, LegacyImportOptions, RenameOutcome, Service, ServiceError, Session,
    SettingsUpdate, TagCount,
};
use tauri::State;

type Res<T> = Result<T, ServiceError>;
type Svc<'a> = State<'a, Service>;

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    data_dir: String,
}

#[tauri::command]
pub fn app_info(svc: Svc<'_>) -> AppInfo {
    AppInfo { version: env!("CARGO_PKG_VERSION"), data_dir: svc.data_dir().display().to_string() }
}

// ── settings & workspaces ──

#[tauri::command]
pub async fn settings_get(svc: Svc<'_>) -> Res<Settings> {
    svc.settings_get().await
}

#[tauri::command]
pub async fn settings_set(svc: Svc<'_>, patch: Map<String, Value>) -> Res<SettingsUpdate> {
    svc.settings_set(patch).await
}

#[tauri::command]
pub async fn settings_reset(svc: Svc<'_>) -> Res<Settings> {
    svc.settings_reset().await
}

#[tauri::command]
pub async fn workspace_get(svc: Svc<'_>, key: String) -> Res<Option<Value>> {
    svc.workspace_get(key).await
}

#[tauri::command]
pub async fn workspace_set(svc: Svc<'_>, key: String, value: Value) -> Res<()> {
    svc.workspace_set(key, value).await
}

// ── jobs ──

#[tauri::command]
pub fn jobs_list(svc: Svc<'_>) -> Vec<JobStatus> {
    svc.jobs_list()
}

#[tauri::command]
pub fn jobs_cancel(svc: Svc<'_>, id: String) -> Res<()> {
    svc.job_cancel(&id)
}

#[tauri::command]
pub fn legacy_import(
    svc: Svc<'_>,
    db_path: PathBuf,
    settings_path: Option<PathBuf>,
    aoe_presets_path: Option<PathBuf>,
) -> JobId {
    svc.legacy_import(LegacyImportOptions { db_path, settings_path, aoe_presets_path })
}

// ── library ──

#[tauri::command]
pub async fn library_search(svc: Svc<'_>, query: AssetQuery) -> Res<AssetPage> {
    svc.library_search(query).await
}

#[tauri::command]
pub async fn library_get(svc: Svc<'_>, id: i64) -> Res<Asset> {
    svc.library_get(id).await
}

#[tauri::command]
pub async fn library_update(svc: Svc<'_>, id: i64, patch: AssetPatch) -> Res<Asset> {
    svc.library_update(id, patch).await
}

#[tauri::command]
pub async fn library_set_tags(svc: Svc<'_>, id: i64, values: Vec<String>) -> Res<Asset> {
    svc.library_set_tags(id, values).await
}

#[tauri::command]
pub async fn library_set_campaigns(svc: Svc<'_>, id: i64, values: Vec<String>) -> Res<Asset> {
    svc.library_set_campaigns(id, values).await
}

#[tauri::command]
pub async fn library_set_sessions(svc: Svc<'_>, id: i64, values: Vec<String>) -> Res<Asset> {
    svc.library_set_sessions(id, values).await
}

#[tauri::command]
pub async fn library_delete(svc: Svc<'_>, ids: Vec<i64>) -> Res<usize> {
    svc.library_delete(ids).await
}

#[tauri::command]
pub async fn library_import(svc: Svc<'_>, files: Vec<ImportFile>) -> Res<ImportReport> {
    svc.library_import(files).await
}

#[tauri::command]
pub async fn library_export(
    svc: Svc<'_>,
    id: i64,
    what: ExportWhat,
    path: PathBuf,
) -> Res<PathBuf> {
    svc.library_export(id, what, path).await
}

#[tauri::command]
pub async fn library_export_many(svc: Svc<'_>, ids: Vec<i64>, dir: PathBuf) -> Res<Vec<PathBuf>> {
    svc.library_export_many(ids, dir).await
}

#[tauri::command]
pub async fn library_copy_to_clipboard(svc: Svc<'_>, id: i64) -> Res<()> {
    svc.library_copy_to_clipboard(id).await
}

#[tauri::command]
pub async fn library_bulk_tag(svc: Svc<'_>, ids: Vec<i64>, tag: String) -> Res<()> {
    svc.library_bulk_tag(ids, tag).await
}

#[tauri::command]
pub async fn library_bulk_pin(svc: Svc<'_>, ids: Vec<i64>, pinned: bool) -> Res<()> {
    svc.library_bulk_pin(ids, pinned).await
}

#[tauri::command]
pub async fn library_bulk_link_session(svc: Svc<'_>, ids: Vec<i64>, session_id: String) -> Res<()> {
    svc.library_bulk_link_session(ids, session_id).await
}

#[tauri::command]
pub async fn library_bulk_link_campaign(
    svc: Svc<'_>,
    ids: Vec<i64>,
    campaign_id: String,
) -> Res<()> {
    svc.library_bulk_link_campaign(ids, campaign_id).await
}

// ── tags ──

#[tauri::command]
pub async fn tags_list(svc: Svc<'_>) -> Res<Vec<TagCount>> {
    svc.tags_list().await
}

#[tauri::command]
pub async fn tags_rename(svc: Svc<'_>, id: i64, name: String) -> Res<RenameOutcome> {
    svc.tags_rename(id, name).await
}

#[tauri::command]
pub async fn tags_merge(svc: Svc<'_>, src: i64, dst: i64) -> Res<()> {
    svc.tags_merge(src, dst).await
}

#[tauri::command]
pub async fn tags_delete(svc: Svc<'_>, id: i64) -> Res<()> {
    svc.tags_delete(id).await
}

// ── campaigns & sessions ──

#[tauri::command]
pub async fn campaigns_list(svc: Svc<'_>) -> Res<Vec<Campaign>> {
    svc.campaigns_list().await
}

#[tauri::command]
pub async fn campaigns_create(svc: Svc<'_>, name: String) -> Res<Campaign> {
    svc.campaigns_create(name).await
}

#[tauri::command]
pub async fn campaigns_rename(svc: Svc<'_>, id: String, name: String) -> Res<()> {
    svc.campaigns_rename(id, name).await
}

#[tauri::command]
pub async fn campaigns_delete(svc: Svc<'_>, id: String) -> Res<()> {
    svc.campaigns_delete(id).await
}

#[tauri::command]
pub async fn sessions_list(svc: Svc<'_>, campaign_id: Option<String>) -> Res<Vec<Session>> {
    svc.sessions_list(campaign_id).await
}

#[tauri::command]
pub async fn sessions_create(
    svc: Svc<'_>,
    campaign_id: String,
    name: String,
    notes: Option<String>,
) -> Res<Session> {
    svc.sessions_create(campaign_id, name, notes.unwrap_or_default()).await
}

#[tauri::command]
pub async fn sessions_update(
    svc: Svc<'_>,
    id: String,
    name: Option<String>,
    notes: Option<String>,
) -> Res<()> {
    svc.sessions_update(id, name, notes).await
}

#[tauri::command]
pub async fn sessions_delete(svc: Svc<'_>, id: String) -> Res<()> {
    svc.sessions_delete(id).await
}

#[tauri::command]
pub async fn sessions_set_assets(svc: Svc<'_>, id: String, asset_ids: Vec<i64>) -> Res<()> {
    svc.sessions_set_assets(id, asset_ids).await
}
