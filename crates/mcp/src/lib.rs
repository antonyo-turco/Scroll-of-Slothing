//! MCP server: every tool is a thin wrapper around one `sleuth_service` call.
//! Reference: docs/mcp-tools.md (kept in sync by the `tools_are_documented` test).
//!
//! Errors from the service are returned as *tool errors* (`isError: true` with a
//! `code: message` text) so the model can read and react to them.

pub mod http;

use std::path::PathBuf;

use base64::Engine;
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ErrorData, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sleuth_core::cards::Flip;
use sleuth_core::cutout::CutoutParams;
use sleuth_core::pack::shelf::SheetMode;
use sleuth_core::settings::SttEngine;
use sleuth_core::stt::{Correction, ExportFormat, Segment};
use sleuth_core::token::TokenParams;
use sleuth_core::units::PageFormat;
use sleuth_service::{
    AssetPatch, AssetQuery, AssetSort, CardOptions, ExportWhat, ImportFile, LegacyImportOptions,
    ObjectItemPatch, Service, ServiceError, SheetItemPatch, TranscribeOptions,
};

type ToolResult = Result<CallToolResult, ErrorData>;

const INSTRUCTIONS: &str = "Kastanomata's Scroll of Sleuthing: a tabletop (D&D) prep toolkit. \
Manage an asset library of tokens, cut-outs and maps (search, tag, pin, \
import/export images), print maps and AoE templates, and run live session music (mixer, soundscapes, playlists, soundboards). Use library_search to find asset ids, then act on them. \
Long operations return a job_id: poll jobs_status. File paths are absolute paths on this machine.";

fn ok<T: Serialize>(v: &T) -> ToolResult {
    Ok(CallToolResult::success(vec![ContentBlock::json(v)?]))
}

fn fail(e: &ServiceError) -> ToolResult {
    let code = serde_json::to_value(e.code())
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    Ok(CallToolResult::error(vec![ContentBlock::text(format!("{code}: {e}"))]))
}

fn done<T: Serialize>(r: Result<T, ServiceError>) -> ToolResult {
    match r {
        Ok(v) => ok(&v),
        Err(e) => fail(&e),
    }
}

// ── parameter types ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    /// Free text, prefix-matched against names, aliases and tags (all words must match).
    pub q: Option<String>,
    /// Filter by kind: token, cutout, map.
    #[serde(default)]
    pub kinds: Vec<String>,
    /// Tag names; assets must carry all of them.
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub pinned_only: bool,
    /// newest (default) | oldest | name_asc | name_desc | kind | size
    pub sort: Option<String>,
    /// Default 50, max 500.
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IdParams {
    pub id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IdsParams {
    pub ids: Vec<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateParams {
    pub id: i64,
    pub name: Option<String>,
    /// One alias per line.
    pub aliases: Option<String>,
    /// Tiny | Small | Medium | Large | Huge | Gargantuan
    pub size: Option<String>,
    pub pinned: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetListParams {
    pub id: i64,
    /// Replaces the full set.
    pub values: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ImportFileParam {
    /// Absolute path to a png/jpg/webp/gif/bmp file.
    pub path: String,
    pub name: Option<String>,
    /// token | cutout | map
    pub kind: String,
    pub size: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ImportParams {
    pub files: Vec<ImportFileParam>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExportParams {
    pub id: i64,
    /// original | rendered | pdf
    pub what: String,
    /// Target file path, or an existing directory.
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameTagParams {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MergeTagParams {
    pub src: i64,
    pub dst: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NameParams {
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StrIdParams {
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameParams {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SettingsSetParams {
    /// Keys and values to change, e.g. {"token_page_format": "A4"}. See settings_get for keys.
    pub values: Map<String, Value>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobParams {
    pub job_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LegacyImportParams {
    /// Absolute path to the old Orb of Many Functions db/creatures.db (read-only).
    pub db_path: String,
    pub settings_path: Option<String>,
    pub aoe_presets_path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BorderAddParams {
    pub path: String,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TokenComposeParams {
    /// Image file to make a token from (absolute path)...
    pub source_path: Option<String>,
    /// ...or an existing library asset (re-edit; its saved framing is the starting point).
    pub asset_id: Option<i64>,
    /// Border id from token_borders_list (default: first border).
    pub border: Option<String>,
    /// "#rrggbb" fill behind the image, or "none" for transparent.
    pub background: Option<String>,
    /// Ring thickness: inset of the image circle, % of radius (0-50).
    pub ring_inset_pct: Option<f64>,
    /// Zoom on top of cover-fit (0.1-10).
    pub zoom: Option<f64>,
    /// Pan in border pixels (positive = view moves right / down over the image).
    pub pan_x: Option<f64>,
    pub pan_y: Option<f64>,
    /// Filters on the image (never the ring): a preset name from token_filter_catalog...
    pub filter_preset: Option<String>,
    /// ...or a full pipeline [{type, enabled, params}] of token filters.
    pub filters: Option<Vec<Value>>,
    pub name: Option<String>,
    /// Tiny | Small | Medium | Large | Huge | Gargantuan
    pub size: Option<String>,
    #[serde(default)]
    pub save_to_library: bool,
    /// With asset_id + save_to_library: replace that asset instead of creating a new one.
    #[serde(default)]
    pub update_existing: bool,
    #[serde(default)]
    pub add_to_sheet: bool,
    /// Also write the token PNG here.
    pub out_png: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SheetAddParams {
    #[serde(default)]
    pub asset_ids: Vec<i64>,
    /// Image files (absolute paths) added as tokens.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Size for added files (default Medium).
    pub size: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SheetUpdateParams {
    pub id: String,
    pub label: Option<String>,
    pub size: Option<String>,
    pub copies: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SheetOptionsParams {
    /// Letter | A4
    pub format: Option<String>,
    pub cut_border: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SttModelParams {
    /// Model id from `stt_models_list`, e.g. `small`, `medium-q5_0`, `large-v3-turbo-q5_0`.
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CorrectionParam {
    /// Text to find (whole words, case-insensitive).
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TranscribeFileParams {
    /// Audio file (absolute path): wav, mp3, flac, ogg, m4a, ...
    pub path: String,
    /// `local` (Whisper on this computer) or `cloud` (OpenRouter; needs a key saved in the app's Settings). Default: the `stt_engine` setting.
    pub engine: Option<String>,
    /// Local: a model id, default `large-v3-turbo-q5_0`, must be installed (`stt_model_download`). Cloud: an OpenRouter model id such as `openai/whisper-1`; default: the `stt_cloud_model` setting.
    pub model: Option<String>,
    /// `it` (default), `en`, ... or `auto`.
    pub language: Option<String>,
    /// Local only: terms whose spelling Whisper should favour, e.g. monster or NPC names.
    #[serde(default)]
    pub glossary: Vec<String>,
    /// Find/replace rules applied to the result (either engine).
    #[serde(default)]
    pub corrections: Vec<CorrectionParam>,
    /// Local only. Beam width; 1 is greedy (faster). Default 5.
    pub beam_size: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SegmentParam {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TranscriptExportParams {
    /// Segments, as in the `transcribe_file` job result.
    pub segments: Vec<SegmentParam>,
    /// `txt`, `md`, `srt` or `vtt`.
    pub format: String,
    /// Heading for Markdown.
    pub title: Option<String>,
    /// Also write the text to this absolute path.
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PathParams {
    /// Absolute output path.
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CutoutCreateParams {
    /// Image file to cut from (absolute path)...
    pub source_path: Option<String>,
    /// ...or an existing library cut-out (re-edit; its saved trace is the starting point).
    pub asset_id: Option<i64>,
    /// Selection polygon [[x, y], ...] in source-image pixels (at least 3 points).
    pub points: Option<Vec<[f64; 2]>>,
    /// Outline thickness in pixels (0-40, default 6).
    pub outline_px: Option<u32>,
    /// Outline colour "#rrggbb" (default white).
    pub outline_color: Option<String>,
    pub name: Option<String>,
    /// Tiny | Small | Medium | Large | Huge | Gargantuan (longest side when printed)
    pub size: Option<String>,
    #[serde(default)]
    pub save_to_library: bool,
    #[serde(default)]
    pub update_existing: bool,
    /// Add to the object sheet.
    #[serde(default)]
    pub add_to_sheet: bool,
    pub out_png: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ObjectUpdateParams {
    pub id: String,
    pub label: Option<String>,
    pub size: Option<String>,
    /// 10-400 %
    pub scale: Option<u32>,
    pub copies: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ObjectOptionsParams {
    /// Letter | A4
    pub format: Option<String>,
    /// flat | standee
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CardsParams {
    /// The deck: cards separated by lines of `---` (see cards_templates for the format).
    pub markdown: String,
    /// Letter | A4 (default: the card_page_format setting)
    pub format: Option<String>,
    /// Print overflowing text on back pages (duplex).
    #[serde(default)]
    pub double_sided: bool,
    /// long (default) | short: the duplex flip edge.
    pub flip: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CardsExportParams {
    pub markdown: Option<String>,
    /// Use a saved deck instead of markdown.
    pub deck_id: Option<i64>,
    pub format: Option<String>,
    #[serde(default)]
    pub double_sided: bool,
    pub flip: Option<String>,
    /// Absolute output path for the PDF.
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeckSaveParams {
    /// Omit to create a new deck.
    pub id: Option<i64>,
    pub name: String,
    pub markdown: String,
}

// ── server ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SleuthServer {
    svc: Service,
    tool_router: ToolRouter<Self>,
}

impl SleuthServer {
    pub fn new(svc: Service) -> Self {
        SleuthServer { svc, tool_router: Self::tool_router() }
    }

    /// Names of every registered tool (used by the docs test).
    pub fn tool_names() -> Vec<String> {
        Self::tool_router().list_all().into_iter().map(|t| t.name.to_string()).collect()
    }
}

#[tool_router]
impl SleuthServer {
    // ── settings ──
    #[tool(
        description = "Get all application settings (page formats, cut-line style, map print defaults, ...)."
    )]
    async fn settings_get(&self) -> ToolResult {
        done(self.svc.settings_get().await)
    }

    #[tool(
        description = "Change settings. Valid keys are applied; invalid ones are returned in `errors`."
    )]
    async fn settings_set(&self, Parameters(p): Parameters<SettingsSetParams>) -> ToolResult {
        done(self.svc.settings_set(p.values).await)
    }

    // ── library ──
    #[tool(
        description = "Search the asset library (tokens, cut-outs, maps). Returns {items, total}."
    )]
    async fn library_search(&self, Parameters(p): Parameters<SearchParams>) -> ToolResult {
        let sort = match p.sort.as_deref() {
            None => AssetSort::Newest,
            Some(s) => match serde_json::from_value(Value::String(s.to_string())) {
                Ok(v) => v,
                Err(_) => return fail(&ServiceError::InvalidInput(format!("unknown sort {s:?}"))),
            },
        };
        let q = AssetQuery {
            q: p.q,
            kinds: p.kinds,
            tags: p.tags,
            campaign_id: None,
            session_id: None,
            pinned_only: p.pinned_only,
            sort,
            limit: Some(p.limit.unwrap_or(50).min(500)),
            offset: p.offset,
        };
        done(self.svc.library_search(q).await)
    }

    #[tool(description = "Get one asset's metadata plus a thumbnail image so you can see it.")]
    async fn library_get(&self, Parameters(p): Parameters<IdParams>) -> ToolResult {
        let asset = match self.svc.library_get(p.id).await {
            Ok(a) => a,
            Err(e) => return fail(&e),
        };
        let mut content = vec![ContentBlock::json(&asset)?];
        if let Some(thumb) = &asset.thumb_blob
            && let Ok((bytes, mime)) = self.svc.blob_bytes(thumb).await
        {
            content.push(ContentBlock::image(
                base64::engine::general_purpose::STANDARD.encode(bytes),
                mime,
            ));
        }
        Ok(CallToolResult::success(content))
    }

    #[tool(description = "Edit an asset's name, aliases, print size or pinned flag.")]
    async fn library_update(&self, Parameters(p): Parameters<UpdateParams>) -> ToolResult {
        let patch =
            AssetPatch { name: p.name, aliases: p.aliases, size_label: p.size, pinned: p.pinned };
        done(self.svc.library_update(p.id, patch).await)
    }

    #[tool(description = "Replace an asset's tags (tag names; created if missing).")]
    async fn library_set_tags(&self, Parameters(p): Parameters<SetListParams>) -> ToolResult {
        done(self.svc.library_set_tags(p.id, p.values).await)
    }

    #[tool(description = "Delete assets by id. Returns how many were deleted.")]
    async fn library_delete(&self, Parameters(p): Parameters<IdsParams>) -> ToolResult {
        done(self.svc.library_delete(p.ids).await)
    }

    #[tool(
        description = "Import image files into the library unprocessed. Returns imported ids and per-file failures."
    )]
    async fn library_import(&self, Parameters(p): Parameters<ImportParams>) -> ToolResult {
        let files = p
            .files
            .into_iter()
            .map(|f| ImportFile {
                path: PathBuf::from(f.path),
                name: f.name,
                kind: f.kind,
                size: f.size,
            })
            .collect();
        done(self.svc.library_import(files).await)
    }

    #[tool(
        description = "Write an asset's original, rendered image or PDF to a path (file or directory)."
    )]
    async fn library_export(&self, Parameters(p): Parameters<ExportParams>) -> ToolResult {
        let what: ExportWhat = match serde_json::from_value(Value::String(p.what.clone())) {
            Ok(w) => w,
            Err(_) => {
                return fail(&ServiceError::InvalidInput(format!("unknown export {:?}", p.what)));
            }
        };
        done(self.svc.library_export(p.id, what, PathBuf::from(p.path)).await)
    }

    // ── tags ──
    #[tool(description = "List tags with usage counts.")]
    async fn tags_list(&self) -> ToolResult {
        done(self.svc.tags_list().await)
    }

    #[tool(
        description = "Rename a tag. If the name exists already, the tags are merged (see merged_into)."
    )]
    async fn tags_rename(&self, Parameters(p): Parameters<RenameTagParams>) -> ToolResult {
        done(self.svc.tags_rename(p.id, p.name).await)
    }

    #[tool(description = "Merge tag `src` into `dst` (src is deleted).")]
    async fn tags_merge(&self, Parameters(p): Parameters<MergeTagParams>) -> ToolResult {
        done(self.svc.tags_merge(p.src, p.dst).await)
    }

    #[tool(description = "Delete a tag (assets are kept).")]
    async fn tags_delete(&self, Parameters(p): Parameters<IdParams>) -> ToolResult {
        done(self.svc.tags_delete(p.id).await)
    }

    // ── jobs ──
    #[tool(
        description = "Status of a background job: state, progress 0..1, message, result or error."
    )]
    async fn jobs_status(&self, Parameters(p): Parameters<JobParams>) -> ToolResult {
        done(self.svc.job_status(&p.job_id))
    }

    #[tool(description = "Request cancellation of a running job.")]
    async fn jobs_cancel(&self, Parameters(p): Parameters<JobParams>) -> ToolResult {
        done(self.svc.job_cancel(&p.job_id))
    }

    // ── speech to text ──
    #[tool(
        description = "Whisper models for local transcription: id, label, size in bytes, installed."
    )]
    async fn stt_models_list(&self) -> ToolResult {
        done(self.svc.stt_models_list().await)
    }

    #[tool(description = "Download a Whisper model (hash-checked). Returns a job_id.")]
    async fn stt_model_download(&self, Parameters(p): Parameters<SttModelParams>) -> ToolResult {
        done(
            self.svc.stt_model_download(p.id).map(|job_id| serde_json::json!({ "job_id": job_id })),
        )
    }

    #[tool(description = "Delete a downloaded Whisper model.")]
    async fn stt_model_delete(&self, Parameters(p): Parameters<SttModelParams>) -> ToolResult {
        done(self.svc.stt_model_delete(p.id).await)
    }

    #[tool(
        description = "Whether an OpenRouter API key is available for cloud transcription (never returns the key). Keys are added in the app's Settings, not over MCP."
    )]
    async fn stt_cloud_key_status(&self) -> ToolResult {
        done(self.svc.stt_cloud_key_status().await)
    }

    #[tool(
        description = "Check the saved OpenRouter key against the API: label, credit limit and usage."
    )]
    async fn stt_cloud_key_test(&self) -> ToolResult {
        done(self.svc.stt_cloud_key_test().await)
    }

    #[tool(
        description = "Transcribe an audio file (Italian by default), locally with Whisper or in the cloud via OpenRouter (engine). Returns a job_id; the job result has engine, model, language, duration_ms, cost (cloud) and segments [{start_ms, end_ms, text}]."
    )]
    async fn transcribe_file(&self, Parameters(p): Parameters<TranscribeFileParams>) -> ToolResult {
        let engine = match p.engine.as_deref().map(str::trim) {
            None | Some("") => None,
            Some("local") => Some(SttEngine::Local),
            Some("cloud") => Some(SttEngine::Cloud),
            Some(other) => {
                return fail(&ServiceError::InvalidInput(format!(
                    "unknown engine {other:?} (local|cloud)"
                )));
            }
        };
        let opts = TranscribeOptions {
            engine,
            model: p.model,
            language: p.language,
            glossary: p.glossary,
            corrections: p
                .corrections
                .into_iter()
                .map(|c| Correction { from: c.from, to: c.to })
                .collect(),
            beam_size: p.beam_size,
            threads: None,
        };
        done(
            self.svc
                .transcribe_file(PathBuf::from(p.path), opts)
                .map(|job_id| serde_json::json!({ "job_id": job_id })),
        )
    }

    #[tool(
        description = "Render transcript segments as txt, md, srt or vtt; optionally write to a path. Returns the text."
    )]
    async fn transcript_export(
        &self,
        Parameters(p): Parameters<TranscriptExportParams>,
    ) -> ToolResult {
        let format: ExportFormat = match serde_json::from_value(Value::String(p.format.clone())) {
            Ok(f) => f,
            Err(_) => {
                return fail(&ServiceError::InvalidInput(format!(
                    "unknown format {:?} (txt|md|srt|vtt)",
                    p.format
                )));
            }
        };
        let segments = p
            .segments
            .into_iter()
            .map(|s| Segment { start_ms: s.start_ms, end_ms: s.end_ms, text: s.text })
            .collect();
        done(self.svc.transcript_export(segments, format, p.title, p.path.map(PathBuf::from)).await)
    }

    #[tool(
        description = "Import data from the old Orb of Many Functions app (source is read-only; idempotent). Returns a job_id."
    )]
    async fn legacy_import(&self, Parameters(p): Parameters<LegacyImportParams>) -> ToolResult {
        let job_id = self.svc.legacy_import(LegacyImportOptions {
            db_path: p.db_path.into(),
            settings_path: p.settings_path.map(Into::into),
            aoe_presets_path: p.aoe_presets_path.map(Into::into),
        });
        ok(&serde_json::json!({ "job_id": job_id }))
    }

    // ── tokens ──
    #[tool(description = "List token borders (built-in and user-added) with their ids.")]
    async fn token_borders_list(&self) -> ToolResult {
        done(self.svc.token_borders_list().await)
    }

    #[tool(description = "Add a border image (PNG with a transparent centre) from a file path.")]
    async fn token_borders_add(&self, Parameters(p): Parameters<BorderAddParams>) -> ToolResult {
        done(self.svc.token_borders_add(PathBuf::from(p.path), p.name).await)
    }

    #[tool(
        description = "List the filters a token can use (colour/tone map filters, applied to the image, never the ring) and the presets built from them."
    )]
    async fn token_filter_catalog(&self) -> ToolResult {
        ok(&self.svc.token_filter_catalog())
    }

    #[tool(
        description = "Make a round token from an image (file path or library asset) inside a border ring. \
        Optionally save it to the library, add it to the token sheet, and/or write a PNG. Returns the result plus the token image."
    )]
    async fn token_compose(&self, Parameters(p): Parameters<TokenComposeParams>) -> ToolResult {
        match self.token_compose_inner(p).await {
            Ok((summary, png)) => Ok(CallToolResult::success(vec![
                ContentBlock::json(&summary)?,
                ContentBlock::image(
                    base64::engine::general_purpose::STANDARD.encode(png),
                    "image/png",
                ),
            ])),
            Err(e) => fail(&e),
        }
    }

    #[tool(
        description = "Get the token sheet: items (id, label, size, copies), page format and cut-border option."
    )]
    async fn token_sheet_list(&self) -> ToolResult {
        done(self.svc.token_sheet_get().await)
    }

    #[tool(
        description = "Add library assets (asset_ids) and/or image files (paths) to the token sheet."
    )]
    async fn token_sheet_add(&self, Parameters(p): Parameters<SheetAddParams>) -> ToolResult {
        if !p.asset_ids.is_empty()
            && let Err(e) = self.svc.token_sheet_add_assets(p.asset_ids).await
        {
            return fail(&e);
        }
        if !p.paths.is_empty() {
            return done(
                self.svc
                    .token_sheet_add_files(p.paths.into_iter().map(PathBuf::from).collect(), p.size)
                    .await,
            );
        }
        done(self.svc.token_sheet_get().await)
    }

    #[tool(description = "Change a sheet item's label, size or copies (1-99).")]
    async fn token_sheet_update(&self, Parameters(p): Parameters<SheetUpdateParams>) -> ToolResult {
        let patch = SheetItemPatch { label: p.label, size: p.size, copies: p.copies };
        done(self.svc.token_sheet_update(p.id, patch).await)
    }

    #[tool(description = "Remove one item from the token sheet.")]
    async fn token_sheet_remove(&self, Parameters(p): Parameters<StrIdParams>) -> ToolResult {
        done(self.svc.token_sheet_remove(p.id).await)
    }

    #[tool(description = "Remove all items from the token sheet.")]
    async fn token_sheet_clear(&self) -> ToolResult {
        done(self.svc.token_sheet_clear().await)
    }

    #[tool(description = "Set the token sheet page format (Letter|A4) and/or cut-border circles.")]
    async fn token_sheet_set_options(
        &self,
        Parameters(p): Parameters<SheetOptionsParams>,
    ) -> ToolResult {
        let format = match p.format.as_deref().map(PageFormat::parse_sheet) {
            Some(None) => {
                return fail(&ServiceError::InvalidInput("format must be Letter or A4".into()));
            }
            Some(f) => f,
            None => None,
        };
        done(self.svc.token_sheet_set_options(format, p.cut_border).await)
    }

    #[tool(
        description = "Compute the token sheet pages: placements in PDF points (bottom-left origin) and skipped items."
    )]
    async fn token_sheet_layout(&self) -> ToolResult {
        done(self.svc.token_sheet_layout().await)
    }

    #[tool(
        description = "Export the token sheet as a PDF to `path`. Returns a job_id (poll jobs_status)."
    )]
    async fn token_sheet_export_pdf(&self, Parameters(p): Parameters<PathParams>) -> ToolResult {
        ok(&serde_json::json!({ "job_id": self.svc.token_sheet_export_pdf(PathBuf::from(p.path)) }))
    }

    // ── cut-outs ──
    #[tool(
        description = "Cut an object out of an image with a polygon (points in source-image pixels, at least 3) and an \
        optional sticker outline. Optionally save to the library, add to the object sheet and/or write a PNG. Returns the result plus the image."
    )]
    async fn cutout_create(&self, Parameters(p): Parameters<CutoutCreateParams>) -> ToolResult {
        match self.cutout_inner(p).await {
            Ok((summary, png)) => Ok(CallToolResult::success(vec![
                ContentBlock::json(&summary)?,
                ContentBlock::image(
                    base64::engine::general_purpose::STANDARD.encode(png),
                    "image/png",
                ),
            ])),
            Err(e) => fail(&e),
        }
    }

    #[tool(
        description = "Get the object sheet: items (id, label, size, scale %, copies), page format and mode (flat|standee)."
    )]
    async fn object_sheet_list(&self) -> ToolResult {
        done(self.svc.object_sheet_get().await)
    }

    #[tool(
        description = "Add library assets (asset_ids) and/or image files (paths) to the object sheet."
    )]
    async fn object_sheet_add(&self, Parameters(p): Parameters<SheetAddParams>) -> ToolResult {
        if !p.asset_ids.is_empty()
            && let Err(e) = self.svc.object_sheet_add_assets(p.asset_ids).await
        {
            return fail(&e);
        }
        if !p.paths.is_empty() {
            return done(
                self.svc
                    .object_sheet_add_files(
                        p.paths.into_iter().map(PathBuf::from).collect(),
                        p.size,
                    )
                    .await,
            );
        }
        done(self.svc.object_sheet_get().await)
    }

    #[tool(
        description = "Change an object sheet item's label, size, scale (10-400 %) or copies (1-99)."
    )]
    async fn object_sheet_update(
        &self,
        Parameters(p): Parameters<ObjectUpdateParams>,
    ) -> ToolResult {
        let patch =
            ObjectItemPatch { label: p.label, size: p.size, scale: p.scale, copies: p.copies };
        done(self.svc.object_sheet_update(p.id, patch).await)
    }

    #[tool(description = "Remove one item from the object sheet.")]
    async fn object_sheet_remove(&self, Parameters(p): Parameters<StrIdParams>) -> ToolResult {
        done(self.svc.object_sheet_remove(p.id).await)
    }

    #[tool(description = "Remove all items from the object sheet.")]
    async fn object_sheet_clear(&self) -> ToolResult {
        done(self.svc.object_sheet_clear().await)
    }

    #[tool(
        description = "Set the object sheet page format (Letter|A4) and/or mode (flat|standee: fold-over, mirrored copy on top)."
    )]
    async fn object_sheet_set_options(
        &self,
        Parameters(p): Parameters<ObjectOptionsParams>,
    ) -> ToolResult {
        let format = match p.format.as_deref().map(PageFormat::parse_sheet) {
            Some(None) => {
                return fail(&ServiceError::InvalidInput("format must be Letter or A4".into()));
            }
            Some(f) => f,
            None => None,
        };
        let mode = match p.mode.as_deref() {
            None => None,
            Some("flat") => Some(SheetMode::Flat),
            Some("standee") => Some(SheetMode::Standee),
            Some(other) => {
                return fail(&ServiceError::InvalidInput(format!(
                    "mode {other:?} must be flat or standee"
                )));
            }
        };
        done(self.svc.object_sheet_set_options(format, mode).await)
    }

    #[tool(
        description = "Compute the object sheet pages: placements in PDF points (bottom-left origin) and skipped items."
    )]
    async fn object_sheet_layout(&self) -> ToolResult {
        done(self.svc.object_sheet_layout().await)
    }

    #[tool(
        description = "Export the object sheet as a PDF to `path`. Returns a job_id (poll jobs_status)."
    )]
    async fn object_sheet_export_pdf(&self, Parameters(p): Parameters<PathParams>) -> ToolResult {
        ok(
            &serde_json::json!({ "job_id": self.svc.object_sheet_export_pdf(PathBuf::from(p.path)) }),
        )
    }

    // ── cards ──
    #[tool(
        description = "Card markdown format reference and the Spell/Feat/Item/Ability templates. Read this before writing a deck."
    )]
    async fn cards_templates(&self) -> ToolResult {
        ok(&self.svc.cards_templates())
    }

    #[tool(
        description = "Parse a card deck (markdown) into cards: title, type, stats, body, colour, copies."
    )]
    async fn cards_parse(&self, Parameters(p): Parameters<CardsParams>) -> ToolResult {
        done(self.svc.cards_parse(p.markdown).await)
    }

    #[tool(
        description = "Check a deck before printing: card count (copies expanded), pages, and which cards' text overflows \
        the front (even at the smallest font). With double_sided, overflowing text continues on back pages."
    )]
    async fn cards_validate(&self, Parameters(p): Parameters<CardsParams>) -> ToolResult {
        match self.card_opts(&p).await {
            Ok(opts) => done(self.svc.cards_validate(p.markdown, opts).await),
            Err(e) => fail(&e),
        }
    }

    #[tool(
        description = "Export a deck (markdown, or a saved deck_id) to a PDF of 3x3 poker cards. Returns a job_id."
    )]
    async fn cards_export_pdf(&self, Parameters(p): Parameters<CardsExportParams>) -> ToolResult {
        let markdown = match (p.deck_id, p.markdown) {
            (Some(id), _) => match self.svc.decks_get(id).await {
                Ok(d) => d.markdown,
                Err(e) => return fail(&e),
            },
            (None, Some(md)) => md,
            (None, None) => {
                return fail(&ServiceError::InvalidInput("give markdown or deck_id".into()));
            }
        };
        let cp = CardsParams {
            markdown: markdown.clone(),
            format: p.format,
            double_sided: p.double_sided,
            flip: p.flip,
        };
        match self.card_opts(&cp).await {
            Ok(opts) => ok(
                &serde_json::json!({ "job_id": self.svc.cards_export_pdf(markdown, opts, PathBuf::from(p.path)) }),
            ),
            Err(e) => fail(&e),
        }
    }

    #[tool(description = "List saved card decks.")]
    async fn decks_list(&self) -> ToolResult {
        done(self.svc.decks_list().await)
    }

    #[tool(description = "Get a saved deck (markdown and print settings).")]
    async fn decks_get(&self, Parameters(p): Parameters<IdParams>) -> ToolResult {
        done(self.svc.decks_get(p.id).await)
    }

    #[tool(description = "Save a deck: create (no id) or overwrite (id).")]
    async fn decks_save(&self, Parameters(p): Parameters<DeckSaveParams>) -> ToolResult {
        done(self.svc.decks_save(p.id, p.name, p.markdown, None).await)
    }

    #[tool(description = "Delete a saved deck.")]
    async fn decks_delete(&self, Parameters(p): Parameters<IdParams>) -> ToolResult {
        done(self.svc.decks_delete(p.id).await)
    }

    // ── maps (Map Paginator) ── tools act on the autosaved map doc ──

    #[tool(
        description = "Get the current Map Paginator document (grid, filters, print settings, region, excluded pages), or null if no map is open."
    )]
    async fn map_get(&self) -> ToolResult {
        done(self.svc.map_get().await)
    }

    #[tool(
        description = "Open a map for pagination from an image file path or a stored blob hash. Starts a fresh document with print defaults from Settings and the image's DPI."
    )]
    async fn map_open(&self, Parameters(p): Parameters<MapOpenParams>) -> ToolResult {
        let r = match (p.path, p.hash) {
            (Some(path), _) => self.svc.map_open_file(PathBuf::from(path)).await,
            (None, Some(hash)) => self.svc.map_open_source(hash, p.name).await,
            _ => Err(ServiceError::InvalidInput("give path or hash".into())),
        };
        done(r)
    }

    #[tool(description = "Open a library map asset in the paginator, restoring its saved state.")]
    async fn map_open_asset(&self, Parameters(p): Parameters<IdParams>) -> ToolResult {
        done(self.svc.map_open_asset(p.id).await)
    }

    #[tool(
        description = "Merge a partial MapDoc JSON object (any top-level fields, see map_get) into the current map document."
    )]
    async fn map_update(&self, Parameters(p): Parameters<MapPatchParams>) -> ToolResult {
        done(self.map_patch(p.patch).await)
    }

    #[tool(
        description = "Set the grid sizing: mode rows_cols|reference|ppi|cell_side and its inputs (rows, cols, square_cells, ref_n, ref_quad as 4 [x,y] corners in image px, ppi, side), plus manual offset_x/offset_y."
    )]
    async fn map_set_grid(&self, Parameters(p): Parameters<MapGridParams>) -> ToolResult {
        done(self.map_patch(to_obj(&p)).await)
    }

    #[tool(
        description = "Set the grid overlay look: color [r,g,b], thickness 1-10, opacity 0-255, visible, style solid|dashed|dotted|broken."
    )]
    async fn map_set_style(&self, Parameters(p): Parameters<MapStyleParams>) -> ToolResult {
        done(self.map_patch(to_obj(&p)).await)
    }

    #[tool(
        description = "Set the map filters: a preset name (see map_filter_catalog) or a full pipeline [{type, enabled, params}]."
    )]
    async fn map_set_filters(&self, Parameters(p): Parameters<MapFiltersParams>) -> ToolResult {
        let pipeline = match (p.preset, p.pipeline) {
            (Some(name), _) => match sleuth_core::maps::filters::preset_pipeline(&name) {
                Some(pl) => pl,
                None => {
                    return fail(&ServiceError::InvalidInput(format!("unknown preset {name:?}")));
                }
            },
            (None, Some(pl)) => match serde_json::from_value::<
                Vec<sleuth_core::maps::filters::FilterStep>,
            >(Value::Array(pl))
            {
                Ok(pl) => pl,
                Err(e) => return fail(&ServiceError::InvalidInput(e.to_string())),
            },
            _ => return fail(&ServiceError::InvalidInput("give preset or pipeline".into())),
        };
        let mut m = Map::new();
        m.insert("filters".into(), serde_json::to_value(pipeline).unwrap_or_default());
        done(self.map_patch(m).await)
    }

    #[tool(description = "List filter definitions (params, ranges) and the 18 presets.")]
    async fn map_filter_catalog(&self) -> ToolResult {
        ok(&self.svc.map_filter_catalog())
    }

    #[tool(description = "Turn embedded-grid removal (FFT notch) on or off.")]
    async fn map_remove_grid(&self, Parameters(p): Parameters<OnParams>) -> ToolResult {
        let mut m = Map::new();
        m.insert("remove_grid".into(), p.on.into());
        done(self.map_patch(m).await)
    }

    #[tool(
        description = "De-warp (perspective-rectify) the whole map from a reference block: quad = its 4 corners [x,y] in image px (any order), n = blocks per side. Uses the doc's ref_quad/ref_n when omitted."
    )]
    async fn map_dewarp(&self, Parameters(p): Parameters<MapDewarpParams>) -> ToolResult {
        let r = async {
            let mut doc = self.current_map().await?;
            if let Some(q) = p.quad {
                doc.ref_quad = Some(q);
            }
            if let Some(n) = p.n {
                doc.ref_n = n;
            }
            self.svc.map_dewarp(doc).await
        };
        done(r.await)
    }

    #[tool(
        description = "Detect the current map's embedded grid and set the square size (cell-side mode) and line offset from it. Errors when no regular grid stands out; then set the size with map_set_grid."
    )]
    async fn map_detect_grid(&self) -> ToolResult {
        let r = async { self.svc.map_detect_grid(self.current_map().await?).await };
        done(r.await)
    }

    #[tool(
        description = "Set the print region to the map's content: trims uniform borders (scanner bars, blank paper) and snaps each edge to the nearest grid line. Run map_detect_grid first if the grid is not set. Clears page exclusions."
    )]
    async fn map_fit_region(&self) -> ToolResult {
        let r = async { self.svc.map_fit_region(self.current_map().await?).await };
        done(r.await)
    }

    #[tool(
        description = "Switch the current map to the orientation and page-grid shift that print it on the fewest pages (clears page exclusions). Returns the map; map_layout's `fewest` shows the saving first."
    )]
    async fn map_fewest_pages(&self) -> ToolResult {
        let r = async { self.svc.map_fewest_pages(self.current_map().await?).await };
        done(r.await)
    }

    #[tool(description = "Undo the last de-warp.")]
    async fn map_undo_dewarp(&self) -> ToolResult {
        let r = async { self.svc.map_undo_dewarp(self.current_map().await?).await };
        done(r.await)
    }

    #[tool(
        description = "Set print options: format Letter|A4|A3, landscape, square_in (printed inches per square), calibration_mm (measured mm of a printed inch), margin_in, overlap_in, align_to_grid, cut_border (cut lines along every page edge, out to the paper edge)."
    )]
    async fn map_set_print(&self, Parameters(p): Parameters<MapPrintParams>) -> ToolResult {
        done(self.map_patch(to_obj(&p)).await)
    }

    #[tool(
        description = "Set the print region [x, y, w, h] in image px (null = whole map) and optionally the page grid offset [dx, dy] px."
    )]
    async fn map_set_region(&self, Parameters(p): Parameters<MapRegionParams>) -> ToolResult {
        let mut m = Map::new();
        m.insert("region".into(), serde_json::to_value(p.region).unwrap_or_default());
        if let Some(o) = p.page_offset {
            m.insert("page_offset".into(), serde_json::to_value(o).unwrap_or_default());
        }
        done(self.map_patch(m).await)
    }

    #[tool(
        description = "Include/exclude one page (col, row) from export; toggles unless `include` is given."
    )]
    async fn map_toggle_page(&self, Parameters(p): Parameters<MapPageParams>) -> ToolResult {
        let r = async {
            let mut doc = self.current_map().await?;
            let key = (p.col, p.row);
            let excluded = doc.excluded.contains(&key);
            let include = p.include.unwrap_or(excluded);
            doc.excluded.retain(|k| *k != key);
            if !include {
                doc.excluded.push(key);
            }
            self.svc.map_set(doc).await
        };
        done(r.await)
    }

    #[tool(
        description = "Compute the page tiling of the current map: grid, tiles (col,row,crop,page rects), selection summary and fit check."
    )]
    async fn map_layout(&self) -> ToolResult {
        let r = async { self.svc.map_layout(self.current_map().await?).await };
        done(r.await)
    }

    #[tool(
        description = "See the working preview of the map (filters and grid removal applied, without the grid overlay)."
    )]
    async fn map_preview(&self) -> ToolResult {
        let r = async { self.svc.map_preview(self.current_map().await?).await };
        png_result(r.await)
    }

    #[tool(description = "See one printed page (0-based index into map_layout tiles) as an image.")]
    async fn map_preview_page(&self, Parameters(p): Parameters<IndexParams>) -> ToolResult {
        let r = async { self.svc.map_preview_page(self.current_map().await?, p.index, 1.0).await };
        png_result(r.await)
    }

    #[tool(
        description = "Export the map as a true-scale tiled PDF (included pages only). Returns a job_id."
    )]
    async fn map_export_pdf(&self, Parameters(p): Parameters<PathParams>) -> ToolResult {
        match self.current_map().await {
            Ok(doc) => ok(&serde_json::json!({
                "job_id": self.svc.map_export_pdf(doc, PathBuf::from(p.path))
            })),
            Err(e) => fail(&e),
        }
    }

    #[tool(
        description = "Save the map to the library (original, gridded composite, PDF, full state). asset_id updates that asset instead of creating one. Returns a job_id."
    )]
    async fn map_save_asset(&self, Parameters(p): Parameters<MapSaveParams>) -> ToolResult {
        match self.current_map().await {
            Ok(doc) => ok(&serde_json::json!({
                "job_id": self.svc.map_save_asset(doc, p.name, p.asset_id)
            })),
            Err(e) => fail(&e),
        }
    }

    // ── AoE templates ──

    #[tool(
        description = "Render an area-of-effect template. config: {shape circle|cone|line|square, a_ft, b_ft (line width), feet_per_square, inch (printed inches per square), page_format, fill_rgb, fill_alpha, line_rgb, line_width, grid_on, grid_rgb, grid_alpha, label_on, label}. Returns info (footprint, inches, pages) and the image."
    )]
    async fn aoe_render(&self, Parameters(p): Parameters<AoeParams>) -> ToolResult {
        let info = match self.svc.aoe_info(p.config.clone()).await {
            Ok(i) => i,
            Err(e) => return fail(&e),
        };
        match self.svc.aoe_render(p.config).await {
            Ok(png) => Ok(CallToolResult::success(vec![
                ContentBlock::json(&info)?,
                ContentBlock::image(
                    base64::engine::general_purpose::STANDARD.encode(png),
                    "image/png",
                ),
            ])),
            Err(e) => fail(&e),
        }
    }

    #[tool(
        description = "Footprint, size on paper and page count of an AoE template config (no image)."
    )]
    async fn aoe_info(&self, Parameters(p): Parameters<AoeParams>) -> ToolResult {
        done(self.svc.aoe_info(p.config).await)
    }

    #[tool(description = "List AoE presets: 11 built-ins, then user presets.")]
    async fn aoe_presets_list(&self) -> ToolResult {
        done(self.svc.aoe_presets_list().await)
    }

    #[tool(
        description = "Save a user AoE preset; an existing name is replaced only with overwrite=true."
    )]
    async fn aoe_presets_save(&self, Parameters(p): Parameters<AoePresetSaveParams>) -> ToolResult {
        done(self.svc.aoe_presets_save(p.name, p.config, p.overwrite.unwrap_or(false)).await)
    }

    #[tool(description = "Delete a user AoE preset by name.")]
    async fn aoe_presets_delete(&self, Parameters(p): Parameters<NameParams>) -> ToolResult {
        done(self.svc.aoe_presets_delete(p.name).await)
    }

    #[tool(description = "Add a template config to the AoE sheet.")]
    async fn aoe_sheet_add(&self, Parameters(p): Parameters<AoeParams>) -> ToolResult {
        done(self.svc.aoe_sheet_add(p.config).await)
    }

    #[tool(description = "Get the AoE sheet (items with ids and configs, gap).")]
    async fn aoe_sheet_list(&self) -> ToolResult {
        done(self.svc.aoe_sheet_get().await)
    }

    #[tool(description = "Remove items from the AoE sheet by id.")]
    async fn aoe_sheet_remove(&self, Parameters(p): Parameters<StrIdsParams>) -> ToolResult {
        done(self.svc.aoe_sheet_remove(p.ids).await)
    }

    #[tool(description = "Remove all items from the AoE sheet.")]
    async fn aoe_sheet_clear(&self) -> ToolResult {
        done(self.svc.aoe_sheet_clear().await)
    }

    #[tool(description = "Set the gap between packed templates, inches.")]
    async fn aoe_sheet_set_gap(&self, Parameters(p): Parameters<GapParams>) -> ToolResult {
        done(self.svc.aoe_sheet_set_gap(p.gap_in).await)
    }

    #[tool(
        description = "Pack the AoE sheet at `inch` per square on `format` pages: placements, pages and skipped (too big) items."
    )]
    async fn aoe_sheet_layout(&self, Parameters(p): Parameters<AoeSheetParams>) -> ToolResult {
        match parse_format(&p.format) {
            Ok(f) => done(self.svc.aoe_sheet_layout(p.inch, f).await),
            Err(e) => fail(&e),
        }
    }

    #[tool(
        description = "Export the AoE sheet PDF. Returns a job_id; the job result lists skipped items."
    )]
    async fn aoe_sheet_export_pdf(
        &self,
        Parameters(p): Parameters<AoeSheetExportParams>,
    ) -> ToolResult {
        match parse_format(&p.format) {
            Ok(f) => ok(&serde_json::json!({
                "job_id": self.svc.aoe_sheet_export_pdf(p.inch, f, PathBuf::from(p.path))
            })),
            Err(e) => fail(&e),
        }
    }

    #[tool(
        description = "Export one AoE template tiled at true scale across pages (PDF). Returns a job_id."
    )]
    async fn aoe_export_template_pdf(
        &self,
        Parameters(p): Parameters<AoeExportParams>,
    ) -> ToolResult {
        ok(&serde_json::json!({
            "job_id": self.svc.aoe_export_template_pdf(p.config, PathBuf::from(p.path))
        }))
    }

    #[tool(description = "Save one AoE template as a PNG file.")]
    async fn aoe_export_png(&self, Parameters(p): Parameters<AoeExportParams>) -> ToolResult {
        done(self.svc.aoe_export_png(p.config, PathBuf::from(p.path)).await)
    }

    // ── UI remote control (in-app server only) ──

    #[tool(
        description = "Show a view in the desktop app. route: library, tokens.cutter, tokens.sheet, cutouts.cutter, cutouts.sheet, cards, maps.paginator, maps.aoe, music, settings. Works only through the in-app MCP server."
    )]
    async fn ui_navigate(&self, Parameters(p): Parameters<RouteParams>) -> ToolResult {
        done(self.svc.ui_navigate(p.route).await)
    }

    #[tool(
        description = "Open a library asset in its editor in the desktop app (tokens and cut-outs in their cutter with saved framing, maps in the paginator). Works only through the in-app MCP server."
    )]
    async fn ui_open_asset(&self, Parameters(p): Parameters<IdParams>) -> ToolResult {
        done(self.svc.ui_open_asset(p.id).await)
    }

    // ── music ──
    // Audio plays in the desktop app's engine. From the standalone server the
    // state changes are stored and the running app applies them.

    #[tool(
        description = "List music tracks. Optional query: {tags (all must match), text (title/notes)}."
    )]
    async fn music_tracks_list(&self, Parameters(p): Parameters<MusicQueryParams>) -> ToolResult {
        done(self.svc.music_tracks_list(p.query.unwrap_or_default()).await)
    }

    #[tool(
        description = "Add a music track. input: {source youtube|upload, url (youtube), file_path (absolute, upload), title, notes, loop_by_default, tags}."
    )]
    async fn music_track_add(&self, Parameters(p): Parameters<MusicTrackParams>) -> ToolResult {
        done(self.svc.music_track_create(p.input).await)
    }

    #[tool(
        description = "Delete a music track by id (also removes it from mixes, playlists and boards)."
    )]
    async fn music_track_delete(&self, Parameters(p): Parameters<StrIdParams>) -> ToolResult {
        done(self.svc.music_track_delete(p.id).await)
    }

    #[tool(
        description = "Live mixer state: channels (track, role, volume, looping, playing), soundbites, duck mode, master volume, loaded soundscape, playlist position."
    )]
    async fn music_state(&self) -> ToolResult {
        done(self.svc.music_state().await)
    }

    #[tool(
        description = "Add a track to the live mix as a channel. role music|ambient|sfx (default music), volume 0-100, looping, play (default true)."
    )]
    async fn music_channel_add(
        &self,
        Parameters(p): Parameters<MusicChannelAddParams>,
    ) -> ToolResult {
        done(
            self.svc
                .music_channel_add(p.track_id, p.role, p.volume, p.looping, p.play.unwrap_or(true))
                .await,
        )
    }

    #[tool(
        description = "Change a live channel. patch: {volume 0-100, is_looping, is_playing, role}."
    )]
    async fn music_channel_update(
        &self,
        Parameters(p): Parameters<MusicChannelPatchParams>,
    ) -> ToolResult {
        done(self.svc.music_channel_update(p.id, p.patch).await)
    }

    #[tool(description = "Remove a channel from the live mix.")]
    async fn music_channel_remove(&self, Parameters(p): Parameters<StrIdParams>) -> ToolResult {
        done(self.svc.music_channel_remove(p.id).await)
    }

    #[tool(
        description = "Play (on=true) or pause (on=false) every channel; pausing also stops the soundbite timers."
    )]
    async fn music_set_all_playing(&self, Parameters(p): Parameters<OnParams>) -> ToolResult {
        done(self.svc.music_set_all_playing(p.on).await)
    }

    #[tool(
        description = "Global stop: stops all audio, clears the live mix and unloads the soundscape."
    )]
    async fn music_stop_all(&self) -> ToolResult {
        done(self.svc.music_stop_all().await)
    }

    #[tool(
        description = "Add a soundbite: a track played at random intervals (min/max seconds, defaults 30/90) at volume 0-100 (default 80), ducking the music."
    )]
    async fn music_soundbite_add(
        &self,
        Parameters(p): Parameters<MusicSoundbiteParams>,
    ) -> ToolResult {
        done(
            self.svc
                .music_soundbite_add(
                    p.track_id,
                    p.min_interval.unwrap_or(30.0),
                    p.max_interval.unwrap_or(90.0),
                    p.volume.unwrap_or(80),
                )
                .await,
        )
    }

    #[tool(description = "Remove a soundbite from the live mix.")]
    async fn music_soundbite_remove(&self, Parameters(p): Parameters<StrIdParams>) -> ToolResult {
        done(self.svc.music_soundbite_remove(p.id).await)
    }

    #[tool(description = "Play a soundbite now (through the SFX queue, ducking the music).")]
    async fn music_soundbite_play(&self, Parameters(p): Parameters<StrIdParams>) -> ToolResult {
        done(self.svc.music_soundbite_play(p.id).await)
    }

    #[tool(
        description = "Set ducking while soundbites and SFX play: off | soft (50%) | strong (25%)."
    )]
    async fn music_set_duck(&self, Parameters(p): Parameters<MusicDuckParams>) -> ToolResult {
        done(self.svc.music_set_duck(p.mode).await)
    }

    #[tool(description = "Set the master volume (0-100).")]
    async fn music_set_master(&self, Parameters(p): Parameters<MusicVolumeParams>) -> ToolResult {
        done(self.svc.music_set_master(p.volume).await)
    }

    #[tool(description = "List saved soundscapes (mixes).")]
    async fn music_soundscapes_list(&self) -> ToolResult {
        done(self.svc.music_soundscapes_list().await)
    }

    #[tool(description = "Save the current live mix as a soundscape. input: {name, notes, tags}.")]
    async fn music_soundscape_save(
        &self,
        Parameters(p): Parameters<MusicSoundscapeParams>,
    ) -> ToolResult {
        done(self.svc.music_soundscape_save(p.input).await)
    }

    #[tool(
        description = "Load a soundscape into the live mix and play it (replaces the current mix)."
    )]
    async fn music_play_soundscape(&self, Parameters(p): Parameters<StrIdParams>) -> ToolResult {
        done(self.svc.music_soundscape_load(p.id).await)
    }

    #[tool(description = "Delete a soundscape.")]
    async fn music_soundscape_delete(&self, Parameters(p): Parameters<StrIdParams>) -> ToolResult {
        done(self.svc.music_soundscape_delete(p.id).await)
    }

    #[tool(description = "List playlists.")]
    async fn music_playlists_list(&self) -> ToolResult {
        done(self.svc.music_playlists_list().await)
    }

    #[tool(
        description = "Create (no id) or replace (with id) a playlist. input: {id, name, track_ids (in order)}."
    )]
    async fn music_playlist_save(
        &self,
        Parameters(p): Parameters<MusicPlaylistParams>,
    ) -> ToolResult {
        done(self.svc.music_playlist_save(p.input).await)
    }

    #[tool(description = "Play a playlist from track index (default 0); it auto-advances.")]
    async fn music_playlist_play(
        &self,
        Parameters(p): Parameters<MusicPlaylistPlayParams>,
    ) -> ToolResult {
        done(self.svc.music_playlist_play(p.id, p.index).await)
    }

    #[tool(
        description = "Control the playing playlist. action: next | prev | pause | resume | stop."
    )]
    async fn music_playlist_control(
        &self,
        Parameters(p): Parameters<MusicActionParams>,
    ) -> ToolResult {
        done(self.svc.music_playlist_control(p.action).await)
    }

    #[tool(description = "List soundboards with their buttons.")]
    async fn music_soundboards_list(&self) -> ToolResult {
        done(self.svc.music_soundboards_list().await)
    }

    #[tool(
        description = "Press a soundboard button by button id (one-shot, restart or toggle, per its mode)."
    )]
    async fn music_soundboard_press(&self, Parameters(p): Parameters<StrIdParams>) -> ToolResult {
        done(self.svc.music_soundboard_press(p.id).await)
    }

    #[tool(
        description = "Write the live mix (or soundscape_id) to a portable .dmix file at an absolute path (uploads embedded)."
    )]
    async fn music_mix_export(
        &self,
        Parameters(p): Parameters<MusicMixExportParams>,
    ) -> ToolResult {
        done(self.svc.music_mix_export(PathBuf::from(p.path), p.soundscape_id).await)
    }

    #[tool(
        description = "Import a .dmix file: creates or reuses tracks, saves a soundscape named after the file and loads it (plays unless play=false)."
    )]
    async fn music_mix_import(
        &self,
        Parameters(p): Parameters<MusicMixImportParams>,
    ) -> ToolResult {
        done(self.svc.music_mix_import(PathBuf::from(p.path), p.play.unwrap_or(true)).await)
    }

    #[tool(description = "Import the music data saved by the legacy importer (legacy/music.json).")]
    async fn music_import_legacy(&self) -> ToolResult {
        done(self.svc.music_import_legacy().await)
    }
}

impl SleuthServer {
    /// Card options from tool params, defaulting the format to the `card_page_format` setting.
    async fn card_opts(&self, p: &CardsParams) -> Result<CardOptions, ServiceError> {
        let format = match p.format.as_deref() {
            Some(f) => PageFormat::parse_sheet(f)
                .ok_or_else(|| ServiceError::InvalidInput("format must be Letter or A4".into()))?,
            None => self.svc.settings_get().await?.card_page_format,
        };
        let flip = match p.flip.as_deref() {
            None | Some("long") => Flip::Long,
            Some("short") => Flip::Short,
            Some(o) => {
                return Err(ServiceError::InvalidInput(format!(
                    "flip {o:?} must be long or short"
                )));
            }
        };
        Ok(CardOptions { format, double_sided: p.double_sided, flip })
    }
}

impl SleuthServer {
    async fn cutout_inner(&self, p: CutoutCreateParams) -> Result<(Value, Vec<u8>), ServiceError> {
        let (source, mut params, name, size, asset_id) = match (p.asset_id, p.source_path) {
            (Some(id), _) => {
                let d = self.svc.cutout_open_asset(id).await?;
                (d.source.hash, d.params, d.name, d.size, Some(id))
            }
            (None, Some(path)) => {
                let src = self.svc.image_source_from_path(PathBuf::from(&path)).await?;
                let stem = std::path::Path::new(&path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Cut-Out".into());
                (src.hash, CutoutParams::default(), stem, "Medium".to_string(), None)
            }
            (None, None) => {
                return Err(ServiceError::InvalidInput("give source_path or asset_id".into()));
            }
        };
        if let Some(pts) = p.points {
            params.points = pts;
        }
        if let Some(px) = p.outline_px {
            params.outline_px = px.min(40);
        }
        if let Some(c) = p.outline_color {
            params.outline_rgb = sleuth_core::settings::HexColor::parse(&c)
                .ok_or_else(|| {
                    ServiceError::InvalidInput(format!("outline_color {c:?} is not #rrggbb"))
                })?
                .0;
        }
        let name = p.name.unwrap_or(name);
        let size = p.size.unwrap_or(size);
        let mut summary = serde_json::json!({ "params": params, "name": name, "size": size });
        if p.save_to_library {
            let update = if p.update_existing { asset_id } else { None };
            let a = self
                .svc
                .cutout_save_to_library(
                    source.clone(),
                    params.clone(),
                    name.clone(),
                    size.clone(),
                    update,
                )
                .await?;
            summary["asset_id"] = a.id.into();
        }
        if p.add_to_sheet {
            let sheet = self
                .svc
                .object_sheet_add_composed(
                    source.clone(),
                    params.clone(),
                    name.clone(),
                    size.clone(),
                )
                .await?;
            summary["sheet_items"] = sheet.items.len().into();
        }
        if let Some(out) = p.out_png {
            let path = self
                .svc
                .cutout_save_png(source.clone(), params.clone(), PathBuf::from(out))
                .await?;
            summary["png_path"] = serde_json::to_value(path).unwrap_or_default();
        }
        let png = self.svc.cutout_render(source, params, false).await?;
        Ok((summary, png))
    }
}

impl SleuthServer {
    async fn token_compose_inner(
        &self,
        p: TokenComposeParams,
    ) -> Result<(Value, Vec<u8>), ServiceError> {
        let (source, mut params, name, size, asset_id) = match (p.asset_id, p.source_path) {
            (Some(id), _) => {
                let d = self.svc.token_open_asset(id).await?;
                (d.source.hash, d.params, d.name, d.size, Some(id))
            }
            (None, Some(path)) => {
                let src = self.svc.image_source_from_path(PathBuf::from(&path)).await?;
                let stem = std::path::Path::new(&path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Token".into());
                (src.hash, TokenParams::default(), stem, "Medium".to_string(), None)
            }
            (None, None) => {
                return Err(ServiceError::InvalidInput("give source_path or asset_id".into()));
            }
        };
        if let Some(b) = p.border {
            params.border = b;
        }
        if params.border.is_empty() {
            params.border = self
                .svc
                .token_borders_list()
                .await?
                .first()
                .map(|b| b.id.clone())
                .unwrap_or_default();
        }
        if let Some(bg) = p.background {
            params.bg = if bg.trim().is_empty() || bg == "none" {
                None
            } else {
                Some(
                    sleuth_core::settings::HexColor::parse(&bg)
                        .ok_or_else(|| {
                            ServiceError::InvalidInput(format!("background {bg:?} is not #rrggbb"))
                        })?
                        .0,
                )
            };
        }
        if let Some(v) = p.ring_inset_pct {
            params.ring_inset_pct = v;
        }
        if let Some(v) = p.zoom {
            params.zoom = v;
        }
        if let Some(v) = p.pan_x {
            params.pan[0] = v;
        }
        if let Some(v) = p.pan_y {
            params.pan[1] = v;
        }
        if let Some(name) = p.filter_preset {
            params.filters = sleuth_core::token::token_preset_pipeline(&name).ok_or_else(|| {
                ServiceError::InvalidInput(format!("unknown token filter preset {name:?}"))
            })?;
        } else if let Some(pl) = p.filters {
            params.filters = serde_json::from_value(Value::Array(pl))
                .map_err(|e| ServiceError::InvalidInput(e.to_string()))?;
        }
        let params = params.clamped();
        let name = p.name.unwrap_or(name);
        let size = p.size.unwrap_or(size);
        let mut summary = serde_json::json!({ "params": params, "name": name, "size": size });
        if p.save_to_library {
            let update = if p.update_existing { asset_id } else { None };
            let a = self
                .svc
                .token_save_to_library(
                    source.clone(),
                    params.clone(),
                    name.clone(),
                    size.clone(),
                    update,
                )
                .await?;
            summary["asset_id"] = a.id.into();
        }
        if p.add_to_sheet {
            let sheet = self
                .svc
                .token_sheet_add_composed(
                    source.clone(),
                    params.clone(),
                    name.clone(),
                    size.clone(),
                )
                .await?;
            summary["sheet_items"] = sheet.items.len().into();
        }
        if let Some(out) = p.out_png {
            let path =
                self.svc.token_save_png(source.clone(), params.clone(), PathBuf::from(out)).await?;
            summary["png_path"] = serde_json::to_value(path).unwrap_or_default();
        }
        let png = self.svc.token_render(source, params, false).await?;
        Ok((summary, png))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SleuthServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("sleuth", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

// ── maps / AoE helpers and params ───────────────────────────────────────────

impl SleuthServer {
    async fn current_map(&self) -> Result<sleuth_service::MapDoc, ServiceError> {
        self.svc
            .map_get()
            .await?
            .ok_or_else(|| ServiceError::InvalidInput("open a map first (map_open)".into()))
    }

    /// Shallow-merges `patch` into the current doc and saves it.
    async fn map_patch(
        &self,
        patch: Map<String, Value>,
    ) -> Result<sleuth_service::MapDoc, ServiceError> {
        let doc = self.current_map().await?;
        let mut v =
            serde_json::to_value(&doc).map_err(|e| ServiceError::Internal(e.to_string()))?;
        if let Some(obj) = v.as_object_mut() {
            for (k, val) in patch {
                if !obj.contains_key(&k) {
                    return Err(ServiceError::InvalidInput(format!("unknown map field {k:?}")));
                }
                obj.insert(k, val);
            }
        }
        let doc: sleuth_service::MapDoc =
            serde_json::from_value(v).map_err(|e| ServiceError::InvalidInput(e.to_string()))?;
        self.svc.map_set(doc).await
    }
}

/// Serializes a params struct, dropping `None` fields.
fn to_obj<T: Serialize>(p: &T) -> Map<String, Value> {
    match serde_json::to_value(p) {
        Ok(Value::Object(m)) => m.into_iter().filter(|(_, v)| !v.is_null()).collect(),
        _ => Map::new(),
    }
}

fn png_result(r: Result<Vec<u8>, ServiceError>) -> ToolResult {
    match r {
        Ok(png) => Ok(CallToolResult::success(vec![ContentBlock::image(
            base64::engine::general_purpose::STANDARD.encode(png),
            "image/png",
        )])),
        Err(e) => fail(&e),
    }
}

/// Map and AoE formats: Letter, A4 or A3.
fn parse_format(f: &str) -> Result<PageFormat, ServiceError> {
    PageFormat::parse(f)
        .ok_or_else(|| ServiceError::InvalidInput("format must be Letter, A4 or A3".into()))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MapOpenParams {
    /// Absolute path to the map image.
    pub path: Option<String>,
    /// Or a stored blob hash.
    pub hash: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MapPatchParams {
    /// Partial MapDoc: top-level fields to replace.
    pub patch: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MapGridParams {
    /// rows_cols | reference | ppi | cell_side
    pub mode: Option<String>,
    pub rows: Option<i64>,
    pub cols: Option<i64>,
    pub square_cells: Option<bool>,
    pub ref_n: Option<i64>,
    /// 4 corners [[x,y],...] in image px.
    pub ref_quad: Option<[(f64, f64); 4]>,
    pub ppi: Option<f64>,
    pub side: Option<f64>,
    pub offset_x: Option<f64>,
    pub offset_y: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MapStyleParams {
    pub color: Option<[u8; 3]>,
    pub thickness: Option<u32>,
    pub opacity: Option<u8>,
    pub visible: Option<bool>,
    /// solid | dashed | dotted | broken
    pub style: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MapFiltersParams {
    pub preset: Option<String>,
    /// [{type, enabled, params}]
    pub pipeline: Option<Vec<Value>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OnParams {
    pub on: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MapDewarpParams {
    pub quad: Option<[(f64, f64); 4]>,
    pub n: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MapPrintParams {
    /// Letter | A4 | A3
    pub format: Option<String>,
    /// Turn the pages sideways.
    pub landscape: Option<bool>,
    pub square_in: Option<f64>,
    pub calibration_mm: Option<f64>,
    pub margin_in: Option<f64>,
    pub overlap_in: Option<f64>,
    pub align_to_grid: Option<bool>,
    pub cut_border: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MapRegionParams {
    /// [x, y, w, h] image px, or null for the whole map.
    pub region: Option<(f64, f64, f64, f64)>,
    pub page_offset: Option<(f64, f64)>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MapPageParams {
    pub col: u32,
    pub row: u32,
    pub include: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IndexParams {
    pub index: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MapSaveParams {
    pub name: String,
    pub asset_id: Option<i64>,
}

/// An AoE template config; missing fields take the defaults (20 ft circle).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AoeParams {
    #[schemars(with = "Value")]
    pub config: sleuth_core::aoe::AoeConfig,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AoePresetSaveParams {
    pub name: String,
    #[schemars(with = "Value")]
    pub config: sleuth_core::aoe::AoeConfig,
    pub overwrite: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StrIdsParams {
    pub ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GapParams {
    pub gap_in: f64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AoeSheetParams {
    pub inch: f64,
    /// Letter | A4 | A3
    pub format: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AoeSheetExportParams {
    pub inch: f64,
    /// Letter | A4 | A3
    pub format: String,
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AoeExportParams {
    #[schemars(with = "Value")]
    pub config: sleuth_core::aoe::AoeConfig,
    pub path: String,
}

// ── music params ──

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MusicQueryParams {
    /// {tags, text}
    #[schemars(with = "Option<Value>")]
    pub query: Option<sleuth_service::MusicTrackQuery>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MusicTrackParams {
    #[schemars(with = "Value")]
    pub input: sleuth_service::TrackInput,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MusicChannelAddParams {
    pub track_id: String,
    pub role: Option<String>,
    pub volume: Option<i64>,
    pub looping: Option<bool>,
    pub play: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MusicChannelPatchParams {
    pub id: String,
    #[schemars(with = "Value")]
    pub patch: sleuth_service::MusicChannelPatch,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MusicSoundbiteParams {
    pub track_id: String,
    pub min_interval: Option<f64>,
    pub max_interval: Option<f64>,
    pub volume: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MusicDuckParams {
    /// off | soft | strong
    #[schemars(with = "String")]
    pub mode: sleuth_service::DuckMode,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MusicVolumeParams {
    pub volume: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MusicSoundscapeParams {
    #[schemars(with = "Value")]
    pub input: sleuth_service::SoundscapeInput,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MusicPlaylistParams {
    #[schemars(with = "Value")]
    pub input: sleuth_service::PlaylistInput,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MusicPlaylistPlayParams {
    pub id: String,
    pub index: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MusicActionParams {
    pub action: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MusicMixExportParams {
    /// Absolute output path (.dmix).
    pub path: String,
    pub soundscape_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MusicMixImportParams {
    /// Absolute path of a .dmix file.
    pub path: String,
    pub play: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RouteParams {
    pub route: String,
}
