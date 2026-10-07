//! MCP server: every tool is a thin wrapper around one `sleuth_service` call.
//! Reference: docs/mcp-tools.md (kept in sync by the `tools_are_documented` test).
//!
//! Errors from the service are returned as *tool errors* (`isError: true` with a
//! `code: message` text) so the model can read and react to them.

use std::path::PathBuf;

use base64::Engine;
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ErrorData, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sleuth_core::token::TokenParams;
use sleuth_core::units::PageFormat;
use sleuth_service::{
    AssetPatch, AssetQuery, AssetSort, ExportWhat, ImportFile, LegacyImportOptions, Service,
    ServiceError, SheetItemPatch,
};

type ToolResult = Result<CallToolResult, ErrorData>;

const INSTRUCTIONS: &str = "Kastanomata's Scroll of Sleuthing: a tabletop (D&D) prep toolkit. \
Manage an asset library of tokens, cut-outs and maps (search, tag, organise into campaigns and \
sessions, import/export images). Use library_search to find asset ids, then act on them. \
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
    pub campaign_id: Option<String>,
    pub session_id: Option<String>,
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
pub struct SessionsListParams {
    pub campaign_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionCreateParams {
    pub campaign_id: String,
    pub name: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionUpdateParams {
    pub id: String,
    pub name: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionAssetsParams {
    pub id: String,
    pub asset_ids: Vec<i64>,
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
pub struct PathParams {
    /// Absolute output path.
    pub path: String,
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
            campaign_id: p.campaign_id,
            session_id: p.session_id,
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

    #[tool(description = "Replace an asset's campaign links (campaign ids).")]
    async fn library_set_campaigns(&self, Parameters(p): Parameters<SetListParams>) -> ToolResult {
        done(self.svc.library_set_campaigns(p.id, p.values).await)
    }

    #[tool(description = "Replace an asset's session links (session ids).")]
    async fn library_set_sessions(&self, Parameters(p): Parameters<SetListParams>) -> ToolResult {
        done(self.svc.library_set_sessions(p.id, p.values).await)
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

    // ── campaigns & sessions ──
    #[tool(description = "List campaigns with session and asset counts.")]
    async fn campaigns_list(&self) -> ToolResult {
        done(self.svc.campaigns_list().await)
    }

    #[tool(description = "Create a campaign.")]
    async fn campaigns_create(&self, Parameters(p): Parameters<NameParams>) -> ToolResult {
        done(self.svc.campaigns_create(p.name).await)
    }

    #[tool(description = "Rename a campaign.")]
    async fn campaigns_rename(&self, Parameters(p): Parameters<RenameParams>) -> ToolResult {
        done(self.svc.campaigns_rename(p.id, p.name).await)
    }

    #[tool(description = "Delete a campaign and its sessions. Assets are never deleted.")]
    async fn campaigns_delete(&self, Parameters(p): Parameters<StrIdParams>) -> ToolResult {
        done(self.svc.campaigns_delete(p.id).await)
    }

    #[tool(description = "List sessions, optionally of one campaign.")]
    async fn sessions_list(&self, Parameters(p): Parameters<SessionsListParams>) -> ToolResult {
        done(self.svc.sessions_list(p.campaign_id).await)
    }

    #[tool(description = "Create a session in a campaign.")]
    async fn sessions_create(&self, Parameters(p): Parameters<SessionCreateParams>) -> ToolResult {
        done(self.svc.sessions_create(p.campaign_id, p.name, p.notes).await)
    }

    #[tool(description = "Rename a session and/or change its notes.")]
    async fn sessions_update(&self, Parameters(p): Parameters<SessionUpdateParams>) -> ToolResult {
        done(self.svc.sessions_update(p.id, p.name, p.notes).await)
    }

    #[tool(description = "Delete a session (assets are kept).")]
    async fn sessions_delete(&self, Parameters(p): Parameters<StrIdParams>) -> ToolResult {
        done(self.svc.sessions_delete(p.id).await)
    }

    #[tool(description = "Replace the full set of assets in a session.")]
    async fn sessions_set_assets(
        &self,
        Parameters(p): Parameters<SessionAssetsParams>,
    ) -> ToolResult {
        done(self.svc.sessions_set_assets(p.id, p.asset_ids).await)
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
        let format = match p.format.as_deref().map(PageFormat::parse) {
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
