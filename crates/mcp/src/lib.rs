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
use sleuth_service::{
    AssetPatch, AssetQuery, AssetSort, ExportWhat, ImportFile, LegacyImportOptions, Service,
    ServiceError,
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
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SleuthServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("sleuth", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}
