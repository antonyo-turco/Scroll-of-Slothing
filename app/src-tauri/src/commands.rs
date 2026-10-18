//! Tauri commands. Each one is a one-line delegation to the service layer —
//! keep logic out of here so MCP/CLI get identical behaviour.
//! Command names match the MCP tool names where both exist.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Map, Value};
use sleuth_core::cards::DeckLayout;
use sleuth_core::cutout::CutoutParams;
use sleuth_core::pack::shelf::SheetMode;
use sleuth_core::settings::Settings;
use sleuth_core::stt::{ExportFormat, Segment};
use sleuth_core::token::TokenParams;
use sleuth_core::units::PageFormat;
use sleuth_service::{
    Asset, AssetPage, AssetPatch, AssetQuery, Border, CardOptions, CardSummary, CloudKeyInfo,
    CloudKeyStatus, CutoutDraft, Deck, DeckSummary, ExportWhat, ImportFile, ImportReport, JobId,
    JobStatus, LegacyImportOptions, ObjectItemPatch, ObjectSheet, RenameOutcome, Service,
    ServiceError, SettingsUpdate, SheetItemPatch, SheetLayout, SourceImage, SttModel, TagCount,
    TokenDraft, TokenSheet, TranscribeOptions,
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

// ── cards ──

#[tauri::command]
pub fn cards_templates(svc: Svc<'_>) -> Value {
    svc.cards_templates()
}

#[tauri::command]
pub async fn cards_layout(svc: Svc<'_>, markdown: String, opts: CardOptions) -> Res<DeckLayout> {
    svc.cards_layout(markdown, opts).await
}

#[tauri::command]
pub async fn cards_validate(svc: Svc<'_>, markdown: String, opts: CardOptions) -> Res<CardSummary> {
    svc.cards_validate(markdown, opts).await
}

#[tauri::command]
pub fn cards_export_pdf(svc: Svc<'_>, markdown: String, opts: CardOptions, path: PathBuf) -> JobId {
    svc.cards_export_pdf(markdown, opts, path)
}

#[tauri::command]
pub async fn decks_list(svc: Svc<'_>) -> Res<Vec<DeckSummary>> {
    svc.decks_list().await
}

#[tauri::command]
pub async fn decks_get(svc: Svc<'_>, id: i64) -> Res<Deck> {
    svc.decks_get(id).await
}

#[tauri::command]
pub async fn decks_save(
    svc: Svc<'_>,
    id: Option<i64>,
    name: String,
    markdown: String,
    opts: Option<CardOptions>,
) -> Res<Deck> {
    svc.decks_save(id, name, markdown, opts).await
}

#[tauri::command]
pub async fn decks_delete(svc: Svc<'_>, id: i64) -> Res<()> {
    svc.decks_delete(id).await
}

#[tauri::command]
pub async fn deck_read_file(svc: Svc<'_>, path: PathBuf) -> Res<String> {
    svc.deck_read_file(path).await
}

#[tauri::command]
pub async fn deck_write_file(svc: Svc<'_>, markdown: String, path: PathBuf) -> Res<PathBuf> {
    svc.deck_write_file(markdown, path).await
}

// ── maps (Map Paginator) ──

use sleuth_core::aoe::AoeConfig;
use sleuth_service::{
    AoeInfo, AoePreset, AoeSheet, AoeSheetLayout, FilterCatalog, MapDoc, MapLayout,
};
use sleuth_service::{CloudModelSpec, TranscribeEstimate};

#[tauri::command]
pub async fn map_get(svc: Svc<'_>) -> Res<Option<MapDoc>> {
    svc.map_get().await
}

#[tauri::command]
pub async fn map_set(svc: Svc<'_>, doc: MapDoc) -> Res<MapDoc> {
    svc.map_set(doc).await
}

#[tauri::command]
pub async fn map_open_file(svc: Svc<'_>, path: PathBuf) -> Res<MapDoc> {
    svc.map_open_file(path).await
}

#[tauri::command]
pub async fn map_open_source(svc: Svc<'_>, hash: String, name: Option<String>) -> Res<MapDoc> {
    svc.map_open_source(hash, name).await
}

#[tauri::command]
pub async fn map_open_asset(svc: Svc<'_>, id: i64) -> Res<MapDoc> {
    svc.map_open_asset(id).await
}

#[tauri::command]
pub async fn map_preview(svc: Svc<'_>, doc: MapDoc) -> Res<Response> {
    Ok(Response::new(svc.map_preview(doc).await?))
}

#[tauri::command]
pub async fn map_layout(svc: Svc<'_>, doc: MapDoc) -> Res<MapLayout> {
    svc.map_layout(doc).await
}

#[tauri::command]
pub async fn map_preview_page(
    svc: Svc<'_>,
    doc: MapDoc,
    index: usize,
    px_per_pt: Option<f64>,
) -> Res<Response> {
    Ok(Response::new(svc.map_preview_page(doc, index, px_per_pt.unwrap_or(1.0)).await?))
}

#[tauri::command]
pub async fn map_dewarp(svc: Svc<'_>, doc: MapDoc) -> Res<MapDoc> {
    svc.map_dewarp(doc).await
}

#[tauri::command]
pub async fn map_detect_grid(svc: Svc<'_>, doc: MapDoc) -> Res<MapDoc> {
    svc.map_detect_grid(doc).await
}

#[tauri::command]
pub async fn map_fit_region(svc: Svc<'_>, doc: MapDoc) -> Res<MapDoc> {
    svc.map_fit_region(doc).await
}

#[tauri::command]
pub async fn map_fewest_pages(svc: Svc<'_>, doc: MapDoc) -> Res<MapDoc> {
    svc.map_fewest_pages(doc).await
}

#[tauri::command]
pub async fn map_undo_dewarp(svc: Svc<'_>, doc: MapDoc) -> Res<MapDoc> {
    svc.map_undo_dewarp(doc).await
}

#[tauri::command]
pub fn map_export_pdf(svc: Svc<'_>, doc: MapDoc, path: PathBuf) -> JobId {
    svc.map_export_pdf(doc, path)
}

#[tauri::command]
pub fn map_save_asset(svc: Svc<'_>, doc: MapDoc, name: String, update: Option<i64>) -> JobId {
    svc.map_save_asset(doc, name, update)
}

#[tauri::command]
pub fn map_filter_catalog(svc: Svc<'_>) -> FilterCatalog {
    svc.map_filter_catalog()
}

#[tauri::command]
pub fn token_filter_catalog(svc: Svc<'_>) -> FilterCatalog {
    svc.token_filter_catalog()
}

// ── AoE templates ──

#[tauri::command]
pub async fn aoe_presets_list(svc: Svc<'_>) -> Res<Vec<AoePreset>> {
    svc.aoe_presets_list().await
}

#[tauri::command]
pub async fn aoe_presets_save(
    svc: Svc<'_>,
    name: String,
    config: AoeConfig,
    overwrite: Option<bool>,
) -> Res<AoePreset> {
    svc.aoe_presets_save(name, config, overwrite.unwrap_or(false)).await
}

#[tauri::command]
pub async fn aoe_presets_delete(svc: Svc<'_>, name: String) -> Res<()> {
    svc.aoe_presets_delete(name).await
}

#[tauri::command]
pub async fn aoe_render(svc: Svc<'_>, config: AoeConfig) -> Res<Response> {
    Ok(Response::new(svc.aoe_render(config).await?))
}

#[tauri::command]
pub async fn aoe_info(svc: Svc<'_>, config: AoeConfig) -> Res<AoeInfo> {
    svc.aoe_info(config).await
}

#[tauri::command]
pub async fn aoe_export_png(svc: Svc<'_>, config: AoeConfig, path: PathBuf) -> Res<PathBuf> {
    svc.aoe_export_png(config, path).await
}

#[tauri::command]
pub fn aoe_export_template_pdf(svc: Svc<'_>, config: AoeConfig, path: PathBuf) -> JobId {
    svc.aoe_export_template_pdf(config, path)
}

#[tauri::command]
pub async fn aoe_sheet_get(svc: Svc<'_>) -> Res<AoeSheet> {
    svc.aoe_sheet_get().await
}

#[tauri::command]
pub async fn aoe_sheet_add(svc: Svc<'_>, config: AoeConfig) -> Res<AoeSheet> {
    svc.aoe_sheet_add(config).await
}

#[tauri::command]
pub async fn aoe_sheet_remove(svc: Svc<'_>, ids: Vec<String>) -> Res<AoeSheet> {
    svc.aoe_sheet_remove(ids).await
}

#[tauri::command]
pub async fn aoe_sheet_clear(svc: Svc<'_>) -> Res<AoeSheet> {
    svc.aoe_sheet_clear().await
}

#[tauri::command]
pub async fn aoe_sheet_set_gap(svc: Svc<'_>, gap_in: f64) -> Res<AoeSheet> {
    svc.aoe_sheet_set_gap(gap_in).await
}

#[tauri::command]
pub async fn aoe_sheet_layout(svc: Svc<'_>, inch: f64, format: PageFormat) -> Res<AoeSheetLayout> {
    svc.aoe_sheet_layout(inch, format).await
}

#[tauri::command]
pub async fn aoe_sheet_preview(
    svc: Svc<'_>,
    inch: f64,
    format: PageFormat,
    page: usize,
    max_px: Option<u32>,
) -> Res<Response> {
    Ok(Response::new(svc.aoe_sheet_preview(inch, format, page, max_px.unwrap_or(1200)).await?))
}

#[tauri::command]
pub fn aoe_sheet_export_pdf(svc: Svc<'_>, inch: f64, format: PageFormat, path: PathBuf) -> JobId {
    svc.aoe_sheet_export_pdf(inch, format, path)
}

// ── music ──

use sleuth_service::{
    DuckMode, MixImportResult, MixerState, MusicChannelPatch, MusicEngineEvent, MusicImportReport,
    MusicPlaylist, MusicSoundboard, MusicSoundscape, MusicTrack, MusicTrackQuery, PlaylistInput,
    SoundbitePatch, SoundboardInput, SoundscapeInput, TrackInput, TrackMeta,
};

#[tauri::command]
pub async fn music_tracks_list(svc: Svc<'_>, query: MusicTrackQuery) -> Res<Vec<MusicTrack>> {
    svc.music_tracks_list(query).await
}

#[tauri::command]
pub async fn music_track_get(svc: Svc<'_>, id: String) -> Res<MusicTrack> {
    svc.music_track_get(id).await
}

#[tauri::command]
pub async fn music_track_create(svc: Svc<'_>, input: TrackInput) -> Res<MusicTrack> {
    svc.music_track_create(input).await
}

#[tauri::command]
pub async fn music_track_update(svc: Svc<'_>, id: String, input: TrackInput) -> Res<MusicTrack> {
    svc.music_track_update(id, input).await
}

#[tauri::command]
pub async fn music_track_set_favorite(svc: Svc<'_>, id: String, favorite: bool) -> Res<MusicTrack> {
    svc.music_track_set_favorite(id, favorite).await
}

#[tauri::command]
pub async fn music_track_delete(svc: Svc<'_>, id: String) -> Res<()> {
    svc.music_track_delete(id).await
}

#[tauri::command]
pub async fn music_fetch_youtube_meta(svc: Svc<'_>, url: String) -> Res<TrackMeta> {
    svc.music_fetch_youtube_meta(url).await
}

#[tauri::command]
pub async fn music_probe_file(svc: Svc<'_>, path: PathBuf) -> Res<TrackMeta> {
    svc.music_probe_file(path).await
}

#[tauri::command]
pub async fn music_state(svc: Svc<'_>) -> Res<MixerState> {
    svc.music_state().await
}

#[tauri::command]
pub async fn music_channel_add(
    svc: Svc<'_>,
    track_id: String,
    role: Option<String>,
    volume: Option<i64>,
    looping: Option<bool>,
    play: bool,
) -> Res<MixerState> {
    svc.music_channel_add(track_id, role, volume, looping, play).await
}

#[tauri::command]
pub async fn music_channel_update(
    svc: Svc<'_>,
    id: String,
    patch: MusicChannelPatch,
) -> Res<MixerState> {
    svc.music_channel_update(id, patch).await
}

#[tauri::command]
pub async fn music_channel_remove(svc: Svc<'_>, id: String) -> Res<MixerState> {
    svc.music_channel_remove(id).await
}

#[tauri::command]
pub async fn music_set_all_playing(svc: Svc<'_>, playing: bool) -> Res<MixerState> {
    svc.music_set_all_playing(playing).await
}

#[tauri::command]
pub async fn music_stop_all(svc: Svc<'_>) -> Res<MixerState> {
    svc.music_stop_all().await
}

#[tauri::command]
pub async fn music_soundbite_add(
    svc: Svc<'_>,
    track_id: String,
    min_interval: f64,
    max_interval: f64,
    volume: i64,
) -> Res<MixerState> {
    svc.music_soundbite_add(track_id, min_interval, max_interval, volume).await
}

#[tauri::command]
pub async fn music_soundbite_update(
    svc: Svc<'_>,
    id: String,
    patch: SoundbitePatch,
) -> Res<MixerState> {
    svc.music_soundbite_update(id, patch).await
}

#[tauri::command]
pub async fn music_soundbite_remove(svc: Svc<'_>, id: String) -> Res<MixerState> {
    svc.music_soundbite_remove(id).await
}

#[tauri::command]
pub async fn music_soundbite_play(svc: Svc<'_>, id: String) -> Res<MixerState> {
    svc.music_soundbite_play(id).await
}

#[tauri::command]
pub async fn music_set_duck(svc: Svc<'_>, mode: DuckMode) -> Res<MixerState> {
    svc.music_set_duck(mode).await
}

#[tauri::command]
pub async fn music_set_master(svc: Svc<'_>, volume: i64) -> Res<MixerState> {
    svc.music_set_master(volume).await
}

#[tauri::command]
pub async fn music_preview(svc: Svc<'_>, track_id: String, seconds: Option<f64>) -> Res<()> {
    svc.music_preview(track_id, seconds).await
}

#[tauri::command]
pub async fn music_stop_preview(svc: Svc<'_>) -> Res<()> {
    svc.music_stop_preview().await
}

#[tauri::command]
pub async fn music_toggle_video(svc: Svc<'_>) -> Res<()> {
    svc.music_toggle_video().await
}

#[tauri::command]
pub async fn music_engine_event(svc: Svc<'_>, event: MusicEngineEvent) -> Res<()> {
    svc.music_engine_event(event).await
}

#[tauri::command]
pub async fn music_sync(svc: Svc<'_>) -> Res<()> {
    svc.music_sync().await
}

#[tauri::command]
pub async fn music_soundscapes_list(svc: Svc<'_>) -> Res<Vec<MusicSoundscape>> {
    svc.music_soundscapes_list().await
}

#[tauri::command]
pub async fn music_soundscape_save(svc: Svc<'_>, input: SoundscapeInput) -> Res<MusicSoundscape> {
    svc.music_soundscape_save(input).await
}

#[tauri::command]
pub async fn music_soundscape_update(svc: Svc<'_>, id: String) -> Res<MusicSoundscape> {
    svc.music_soundscape_update(id).await
}

#[tauri::command]
pub async fn music_soundscape_load(svc: Svc<'_>, id: String) -> Res<MixerState> {
    svc.music_soundscape_load(id).await
}

#[tauri::command]
pub async fn music_soundscape_set_pinned(
    svc: Svc<'_>,
    id: String,
    pinned: bool,
) -> Res<MusicSoundscape> {
    svc.music_soundscape_set_pinned(id, pinned).await
}

#[tauri::command]
pub async fn music_soundscape_delete(svc: Svc<'_>, id: String) -> Res<()> {
    svc.music_soundscape_delete(id).await
}

#[tauri::command]
pub async fn music_playlists_list(svc: Svc<'_>) -> Res<Vec<MusicPlaylist>> {
    svc.music_playlists_list().await
}

#[tauri::command]
pub async fn music_playlist_save(svc: Svc<'_>, input: PlaylistInput) -> Res<MusicPlaylist> {
    svc.music_playlist_save(input).await
}

#[tauri::command]
pub async fn music_playlist_delete(svc: Svc<'_>, id: String) -> Res<()> {
    svc.music_playlist_delete(id).await
}

#[tauri::command]
pub async fn music_playlist_play(
    svc: Svc<'_>,
    id: String,
    index: Option<usize>,
) -> Res<MixerState> {
    svc.music_playlist_play(id, index).await
}

#[tauri::command]
pub async fn music_playlist_control(svc: Svc<'_>, action: String) -> Res<MixerState> {
    svc.music_playlist_control(action).await
}

#[tauri::command]
pub async fn music_soundboards_list(svc: Svc<'_>) -> Res<Vec<MusicSoundboard>> {
    svc.music_soundboards_list().await
}

#[tauri::command]
pub async fn music_soundboard_save(svc: Svc<'_>, input: SoundboardInput) -> Res<MusicSoundboard> {
    svc.music_soundboard_save(input).await
}

#[tauri::command]
pub async fn music_soundboard_delete(svc: Svc<'_>, id: String) -> Res<()> {
    svc.music_soundboard_delete(id).await
}

#[tauri::command]
pub async fn music_soundboard_press(svc: Svc<'_>, button_id: String) -> Res<MixerState> {
    svc.music_soundboard_press(button_id).await
}

#[tauri::command]
pub async fn music_mix_export(
    svc: Svc<'_>,
    path: PathBuf,
    soundscape_id: Option<String>,
) -> Res<PathBuf> {
    svc.music_mix_export(path, soundscape_id).await
}

#[tauri::command]
pub async fn music_mix_import(svc: Svc<'_>, path: PathBuf, play: bool) -> Res<MixImportResult> {
    svc.music_mix_import(path, play).await
}

#[tauri::command]
pub async fn music_import_legacy(svc: Svc<'_>) -> Res<MusicImportReport> {
    svc.music_import_legacy().await
}

// ── speech to text ──

#[tauri::command]
pub async fn stt_models_list(svc: Svc<'_>) -> Res<Vec<SttModel>> {
    svc.stt_models_list().await
}

#[tauri::command]
pub fn stt_model_download(svc: Svc<'_>, id: String) -> Res<JobId> {
    svc.stt_model_download(id)
}

#[tauri::command]
pub async fn stt_model_delete(svc: Svc<'_>, id: String) -> Res<()> {
    svc.stt_model_delete(id).await
}

#[tauri::command]
pub fn stt_cloud_models_list(svc: Svc<'_>) -> Vec<CloudModelSpec> {
    svc.stt_cloud_models_list()
}

#[tauri::command]
pub async fn transcribe_estimate(
    svc: Svc<'_>,
    path: PathBuf,
    beam_size: Option<u32>,
    cloud_model: Option<String>,
) -> Res<TranscribeEstimate> {
    svc.transcribe_estimate(path, beam_size, cloud_model).await
}

#[tauri::command]
pub async fn stt_cloud_key_status(svc: Svc<'_>) -> Res<CloudKeyStatus> {
    svc.stt_cloud_key_status().await
}

#[tauri::command]
pub async fn stt_cloud_key_set(svc: Svc<'_>, key: String) -> Res<CloudKeyStatus> {
    svc.stt_cloud_key_set(key).await
}

#[tauri::command]
pub async fn stt_cloud_key_clear(svc: Svc<'_>) -> Res<CloudKeyStatus> {
    svc.stt_cloud_key_clear().await
}

#[tauri::command]
pub async fn stt_cloud_key_test(svc: Svc<'_>) -> Res<CloudKeyInfo> {
    svc.stt_cloud_key_test().await
}

#[tauri::command]
pub fn transcribe_file(svc: Svc<'_>, path: PathBuf, opts: TranscribeOptions) -> Res<JobId> {
    svc.transcribe_file(path, opts)
}

#[tauri::command]
pub async fn transcript_export(
    svc: Svc<'_>,
    segments: Vec<Segment>,
    format: ExportFormat,
    title: Option<String>,
    path: Option<PathBuf>,
) -> Res<String> {
    svc.transcript_export(segments, format, title, path).await
}
