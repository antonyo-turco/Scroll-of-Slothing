//! Tauri commands. Each one is a one-line delegation to the service layer —
//! keep logic out of here so MCP/CLI get identical behaviour.
//! Command names match the MCP tool names where both exist.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Map, Value};
use sleuth_core::cutout::CutoutParams;
use sleuth_core::pack::shelf::SheetMode;
use sleuth_core::settings::Settings;
use sleuth_core::token::TokenParams;
use sleuth_core::units::PageFormat;
use sleuth_service::{
    Asset, AssetPage, AssetPatch, AssetQuery, Border, Campaign, CutoutDraft, ExportWhat,
    ImportFile, ImportReport, JobId, JobStatus, LegacyImportOptions, ObjectItemPatch, ObjectSheet,
    RenameOutcome, Service, ServiceError, Session, SettingsUpdate, SheetItemPatch, SheetLayout,
    SourceImage, TagCount, TokenDraft, TokenSheet,
};
use tauri::State;
use tauri::ipc::Response;

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

// ── tokens ──

#[tauri::command]
pub async fn token_borders_list(svc: Svc<'_>) -> Res<Vec<Border>> {
    svc.token_borders_list().await
}

#[tauri::command]
pub async fn token_borders_add(svc: Svc<'_>, path: PathBuf, name: Option<String>) -> Res<Border> {
    svc.token_borders_add(path, name).await
}

#[tauri::command]
pub async fn token_borders_remove(svc: Svc<'_>, id: String) -> Res<()> {
    svc.token_borders_remove(id).await
}

#[tauri::command]
pub async fn image_source_from_path(svc: Svc<'_>, path: PathBuf) -> Res<SourceImage> {
    svc.image_source_from_path(path).await
}

#[tauri::command]
pub async fn image_source_from_clipboard(svc: Svc<'_>) -> Res<SourceImage> {
    svc.image_source_from_clipboard().await
}

#[tauri::command]
pub async fn image_source_info(svc: Svc<'_>, hash: String) -> Res<SourceImage> {
    svc.image_source_info(hash).await
}

/// Returns raw PNG bytes (binary IPC, no base64).
#[tauri::command]
pub async fn token_render(
    svc: Svc<'_>,
    source: String,
    params: TokenParams,
    fast: Option<bool>,
) -> Res<Response> {
    Ok(Response::new(svc.token_render(source, params, fast.unwrap_or(true)).await?))
}

#[tauri::command]
pub async fn token_save_png(
    svc: Svc<'_>,
    source: String,
    params: TokenParams,
    path: PathBuf,
) -> Res<PathBuf> {
    svc.token_save_png(source, params, path).await
}

#[tauri::command]
pub async fn token_save_to_library(
    svc: Svc<'_>,
    source: String,
    params: TokenParams,
    name: String,
    size: String,
    update: Option<i64>,
) -> Res<Asset> {
    svc.token_save_to_library(source, params, name, size, update).await
}

#[tauri::command]
pub async fn token_open_asset(svc: Svc<'_>, id: i64) -> Res<TokenDraft> {
    svc.token_open_asset(id).await
}

// ── token sheet ──

#[tauri::command]
pub async fn token_sheet_get(svc: Svc<'_>) -> Res<TokenSheet> {
    svc.token_sheet_get().await
}

#[tauri::command]
pub async fn token_sheet_add_assets(svc: Svc<'_>, ids: Vec<i64>) -> Res<TokenSheet> {
    svc.token_sheet_add_assets(ids).await
}

#[tauri::command]
pub async fn token_sheet_add_files(
    svc: Svc<'_>,
    paths: Vec<PathBuf>,
    size: Option<String>,
) -> Res<TokenSheet> {
    svc.token_sheet_add_files(paths, size).await
}

#[tauri::command]
pub async fn token_sheet_add_composed(
    svc: Svc<'_>,
    source: String,
    params: TokenParams,
    label: String,
    size: String,
) -> Res<TokenSheet> {
    svc.token_sheet_add_composed(source, params, label, size).await
}

#[tauri::command]
pub async fn token_sheet_update(
    svc: Svc<'_>,
    id: String,
    patch: SheetItemPatch,
) -> Res<TokenSheet> {
    svc.token_sheet_update(id, patch).await
}

#[tauri::command]
pub async fn token_sheet_remove(svc: Svc<'_>, id: String) -> Res<TokenSheet> {
    svc.token_sheet_remove(id).await
}

#[tauri::command]
pub async fn token_sheet_clear(svc: Svc<'_>) -> Res<TokenSheet> {
    svc.token_sheet_clear().await
}

#[tauri::command]
pub async fn token_sheet_set_options(
    svc: Svc<'_>,
    format: Option<PageFormat>,
    cut_border: Option<bool>,
) -> Res<TokenSheet> {
    svc.token_sheet_set_options(format, cut_border).await
}

#[tauri::command]
pub async fn token_sheet_layout(svc: Svc<'_>) -> Res<SheetLayout> {
    svc.token_sheet_layout().await
}

#[tauri::command]
pub fn token_sheet_export_pdf(svc: Svc<'_>, path: PathBuf) -> JobId {
    svc.token_sheet_export_pdf(path)
}

// ── cut-outs ──

#[tauri::command]
pub async fn cutout_render(
    svc: Svc<'_>,
    source: String,
    params: CutoutParams,
    fast: Option<bool>,
) -> Res<Response> {
    Ok(Response::new(svc.cutout_render(source, params, fast.unwrap_or(true)).await?))
}

#[tauri::command]
pub async fn cutout_save_png(
    svc: Svc<'_>,
    source: String,
    params: CutoutParams,
    path: PathBuf,
) -> Res<PathBuf> {
    svc.cutout_save_png(source, params, path).await
}

#[tauri::command]
pub async fn cutout_save_to_library(
    svc: Svc<'_>,
    source: String,
    params: CutoutParams,
    name: String,
    size: String,
    update: Option<i64>,
) -> Res<Asset> {
    svc.cutout_save_to_library(source, params, name, size, update).await
}

#[tauri::command]
pub async fn cutout_open_asset(svc: Svc<'_>, id: i64) -> Res<CutoutDraft> {
    svc.cutout_open_asset(id).await
}

// ── object sheet ──

#[tauri::command]
pub async fn object_sheet_get(svc: Svc<'_>) -> Res<ObjectSheet> {
    svc.object_sheet_get().await
}

#[tauri::command]
pub async fn object_sheet_add_assets(svc: Svc<'_>, ids: Vec<i64>) -> Res<ObjectSheet> {
    svc.object_sheet_add_assets(ids).await
}

#[tauri::command]
pub async fn object_sheet_add_files(
    svc: Svc<'_>,
    paths: Vec<PathBuf>,
    size: Option<String>,
) -> Res<ObjectSheet> {
    svc.object_sheet_add_files(paths, size).await
}

#[tauri::command]
pub async fn object_sheet_add_composed(
    svc: Svc<'_>,
    source: String,
    params: CutoutParams,
    label: String,
    size: String,
) -> Res<ObjectSheet> {
    svc.object_sheet_add_composed(source, params, label, size).await
}

#[tauri::command]
pub async fn object_sheet_update(
    svc: Svc<'_>,
    id: String,
    patch: ObjectItemPatch,
) -> Res<ObjectSheet> {
    svc.object_sheet_update(id, patch).await
}

#[tauri::command]
pub async fn object_sheet_remove(svc: Svc<'_>, id: String) -> Res<ObjectSheet> {
    svc.object_sheet_remove(id).await
}

#[tauri::command]
pub async fn object_sheet_clear(svc: Svc<'_>) -> Res<ObjectSheet> {
    svc.object_sheet_clear().await
}

#[tauri::command]
pub async fn object_sheet_set_options(
    svc: Svc<'_>,
    format: Option<PageFormat>,
    mode: Option<SheetMode>,
) -> Res<ObjectSheet> {
    svc.object_sheet_set_options(format, mode).await
}

#[tauri::command]
pub async fn object_sheet_layout(svc: Svc<'_>) -> Res<SheetLayout> {
    svc.object_sheet_layout().await
}

#[tauri::command]
pub fn object_sheet_export_pdf(svc: Svc<'_>, path: PathBuf) -> JobId {
    svc.object_sheet_export_pdf(path)
}
