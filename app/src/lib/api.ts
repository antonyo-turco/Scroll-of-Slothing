// Typed access to the Rust service layer. Inside Tauri this goes through IPC;
// in a plain browser (vite dev, vitest, Playwright) an in-memory mock is used
// so the UI can be developed and tested without the native shell.

import type {
  AppInfo, Asset, AssetPage, AssetPatch, AssetQuery, Border, Campaign, ExportWhat, ImportFile,
  ImportReport, JobStatus, PageFormat, RenameOutcome, ServiceEvent, Session, Settings, SettingsUpdate,
  SheetItemPatch, SheetLayout, SourceImage, TagCount, TokenDraft, TokenParams, TokenSheet,
  CutoutDraft, CutoutParams, ObjectItemPatch, ObjectSheet, SheetMode,
  CardOptions, CardTemplates, Deck, DeckLayout, DeckSummary,
  MapDoc, MapLayout, FilterCatalog, FilterDef, FilterPreset, AoeConfig, AoeInfo, AoePreset, AoeSheet, AoeSheetLayout,
} from './types';
import { createMockBackend, mockBlobUrl, type Backend } from './mock';

const EVENT_CHANNEL = 'sleuth://event';

export const isTauri = (): boolean =>
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

function tauriBackend(): Backend {
  return {
    async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
      const { invoke } = await import('@tauri-apps/api/core');
      return invoke<T>(cmd, args);
    },
    async listen(handler: (e: ServiceEvent) => void): Promise<() => void> {
      const { listen } = await import('@tauri-apps/api/event');
      return listen<ServiceEvent>(EVENT_CHANNEL, (e) => handler(e.payload));
    },
  };
}

let backend: Backend | null = null;

export function getBackend(): Backend {
  backend ??= isTauri() ? tauriBackend() : createMockBackend();
  return backend;
}

/** Test hook: replace the backend (e.g. a fresh mock per test). */
export function setBackend(b: Backend): void {
  backend = b;
}

/** URL for a stored blob served by the `sleuth://` protocol (or a mock image). */
export function blobUrl(hash: string | null | undefined): string {
  if (!hash) return '';
  if (!isTauri()) return mockBlobUrl(hash);
  // WebView2 (Windows) maps custom schemes to http://<scheme>.localhost
  return navigator.userAgent.includes('Windows')
    ? `http://sleuth.localhost/blob/${hash}`
    : `sleuth://localhost/blob/${hash}`;
}

const call = <T>(cmd: string, args?: Record<string, unknown>) => getBackend().invoke<T>(cmd, args);

/** Binary IPC result → Blob (PNG in the app; the mock returns SVG text). */
const toBlob = (r: ArrayBuffer | string): Blob =>
  typeof r === 'string' ? new Blob([r], { type: 'image/svg+xml' }) : new Blob([r], { type: 'image/png' });

export const api = {
  appInfo: () => call<AppInfo>('app_info'),
  /** Endpoint and bearer token of the in-app MCP HTTP server while it runs. */
  mcpHttpStatus: () => call<{ url: string; token: string } | null>('mcp_http_status'),
  settingsGet: () => call<Settings>('settings_get'),
  settingsSet: (patch: Partial<Settings>) => call<SettingsUpdate>('settings_set', { patch }),
  settingsReset: () => call<Settings>('settings_reset'),
  workspaceGet: <T>(key: string) => call<T | null>('workspace_get', { key }),
  workspaceSet: (key: string, value: unknown) => call<void>('workspace_set', { key, value }),
  jobsList: () => call<JobStatus[]>('jobs_list'),
  jobsCancel: (id: string) => call<void>('jobs_cancel', { id }),
  legacyImport: (dbPath: string, settingsPath?: string | null, aoePresetsPath?: string | null) =>
    call<string>('legacy_import', { dbPath, settingsPath, aoePresetsPath }),
  onEvent: (handler: (e: ServiceEvent) => void) => getBackend().listen(handler),

  librarySearch: (query: AssetQuery) => call<AssetPage>('library_search', { query }),
  libraryGet: (id: number) => call<Asset>('library_get', { id }),
  libraryUpdate: (id: number, patch: AssetPatch) => call<Asset>('library_update', { id, patch }),
  librarySetTags: (id: number, values: string[]) => call<Asset>('library_set_tags', { id, values }),
  librarySetCampaigns: (id: number, values: string[]) => call<Asset>('library_set_campaigns', { id, values }),
  librarySetSessions: (id: number, values: string[]) => call<Asset>('library_set_sessions', { id, values }),
  libraryDelete: (ids: number[]) => call<number>('library_delete', { ids }),
  libraryImport: (files: ImportFile[]) => call<ImportReport>('library_import', { files }),
  libraryExport: (id: number, what: ExportWhat, path: string) => call<string>('library_export', { id, what, path }),
  libraryExportMany: (ids: number[], dir: string) => call<string[]>('library_export_many', { ids, dir }),
  libraryCopyToClipboard: (id: number) => call<void>('library_copy_to_clipboard', { id }),
  libraryBulkTag: (ids: number[], tag: string) => call<void>('library_bulk_tag', { ids, tag }),
  libraryBulkPin: (ids: number[], pinned: boolean) => call<void>('library_bulk_pin', { ids, pinned }),
  libraryBulkLinkSession: (ids: number[], sessionId: string) =>
    call<void>('library_bulk_link_session', { ids, sessionId }),
  libraryBulkLinkCampaign: (ids: number[], campaignId: string) =>
    call<void>('library_bulk_link_campaign', { ids, campaignId }),

  tagsList: () => call<TagCount[]>('tags_list'),
  tagsRename: (id: number, name: string) => call<RenameOutcome>('tags_rename', { id, name }),
  tagsMerge: (src: number, dst: number) => call<void>('tags_merge', { src, dst }),
  tagsDelete: (id: number) => call<void>('tags_delete', { id }),

  campaignsList: () => call<Campaign[]>('campaigns_list'),
  campaignsCreate: (name: string) => call<Campaign>('campaigns_create', { name }),
  campaignsRename: (id: string, name: string) => call<void>('campaigns_rename', { id, name }),
  campaignsDelete: (id: string) => call<void>('campaigns_delete', { id }),
  sessionsList: (campaignId?: string | null) => call<Session[]>('sessions_list', { campaignId }),
  sessionsCreate: (campaignId: string, name: string, notes = '') =>
    call<Session>('sessions_create', { campaignId, name, notes }),
  sessionsUpdate: (id: string, name?: string | null, notes?: string | null) =>
    call<void>('sessions_update', { id, name, notes }),
  sessionsDelete: (id: string) => call<void>('sessions_delete', { id }),
  sessionsSetAssets: (id: string, assetIds: number[]) => call<void>('sessions_set_assets', { id, assetIds }),

  tokenBordersList: () => call<Border[]>('token_borders_list'),
  tokenBordersAdd: (path: string, name?: string | null) => call<Border>('token_borders_add', { path, name }),
  tokenBordersRemove: (id: string) => call<void>('token_borders_remove', { id }),
  imageSourceFromPath: (path: string) => call<SourceImage>('image_source_from_path', { path }),
  imageSourceFromClipboard: () => call<SourceImage>('image_source_from_clipboard'),
  imageSourceInfo: (hash: string) => call<SourceImage>('image_source_info', { hash }),
  /** Renders a token; resolves to an image Blob (PNG in the app, SVG in the mock). */
  tokenRender: async (source: string, params: TokenParams, fast = true): Promise<Blob> => {
    const r = await call<ArrayBuffer | string>('token_render', { source, params, fast });
    return typeof r === 'string' ? new Blob([r], { type: 'image/svg+xml' }) : new Blob([r], { type: 'image/png' });
  },
  tokenSavePng: (source: string, params: TokenParams, path: string) =>
    call<string>('token_save_png', { source, params, path }),
  tokenSaveToLibrary: (source: string, params: TokenParams, name: string, size: string, update: number | null) =>
    call<Asset>('token_save_to_library', { source, params, name, size, update }),
  tokenOpenAsset: (id: number) => call<TokenDraft>('token_open_asset', { id }),

  tokenSheetGet: () => call<TokenSheet>('token_sheet_get'),
  tokenSheetAddAssets: (ids: number[]) => call<TokenSheet>('token_sheet_add_assets', { ids }),
  tokenSheetAddFiles: (paths: string[], size?: string | null) => call<TokenSheet>('token_sheet_add_files', { paths, size }),
  tokenSheetAddComposed: (source: string, params: TokenParams, label: string, size: string) =>
    call<TokenSheet>('token_sheet_add_composed', { source, params, label, size }),
  tokenSheetUpdate: (id: string, patch: SheetItemPatch) => call<TokenSheet>('token_sheet_update', { id, patch }),
  tokenSheetRemove: (id: string) => call<TokenSheet>('token_sheet_remove', { id }),
  tokenSheetClear: () => call<TokenSheet>('token_sheet_clear'),
  tokenSheetSetOptions: (format?: PageFormat | null, cutBorder?: boolean | null) =>
    call<TokenSheet>('token_sheet_set_options', { format, cutBorder }),
  tokenSheetLayout: () => call<SheetLayout>('token_sheet_layout'),
  tokenSheetExportPdf: (path: string) => call<string>('token_sheet_export_pdf', { path }),

  /** Renders a cut-out; resolves to an image Blob (PNG in the app, SVG in the mock). */
  cutoutRender: async (source: string, params: CutoutParams, fast = true): Promise<Blob> => {
    const r = await call<ArrayBuffer | string>('cutout_render', { source, params, fast });
    return typeof r === 'string' ? new Blob([r], { type: 'image/svg+xml' }) : new Blob([r], { type: 'image/png' });
  },
  cutoutSavePng: (source: string, params: CutoutParams, path: string) =>
    call<string>('cutout_save_png', { source, params, path }),
  cutoutSaveToLibrary: (source: string, params: CutoutParams, name: string, size: string, update: number | null) =>
    call<Asset>('cutout_save_to_library', { source, params, name, size, update }),
  cutoutOpenAsset: (id: number) => call<CutoutDraft>('cutout_open_asset', { id }),

  objectSheetGet: () => call<ObjectSheet>('object_sheet_get'),
  objectSheetAddAssets: (ids: number[]) => call<ObjectSheet>('object_sheet_add_assets', { ids }),
  objectSheetAddFiles: (paths: string[], size?: string | null) => call<ObjectSheet>('object_sheet_add_files', { paths, size }),
  objectSheetAddComposed: (source: string, params: CutoutParams, label: string, size: string) =>
    call<ObjectSheet>('object_sheet_add_composed', { source, params, label, size }),
  objectSheetUpdate: (id: string, patch: ObjectItemPatch) => call<ObjectSheet>('object_sheet_update', { id, patch }),
  objectSheetRemove: (id: string) => call<ObjectSheet>('object_sheet_remove', { id }),
  objectSheetClear: () => call<ObjectSheet>('object_sheet_clear'),
  objectSheetSetOptions: (format?: PageFormat | null, mode?: SheetMode | null) =>
    call<ObjectSheet>('object_sheet_set_options', { format, mode }),
  objectSheetLayout: () => call<SheetLayout>('object_sheet_layout'),
  objectSheetExportPdf: (path: string) => call<string>('object_sheet_export_pdf', { path }),

  cardsTemplates: () => call<CardTemplates>('cards_templates'),
  cardsLayout: (markdown: string, opts: CardOptions) => call<DeckLayout>('cards_layout', { markdown, opts }),
  cardsExportPdf: (markdown: string, opts: CardOptions, path: string) =>
    call<string>('cards_export_pdf', { markdown, opts, path }),
  decksList: () => call<DeckSummary[]>('decks_list'),
  decksGet: (id: number) => call<Deck>('decks_get', { id }),
  decksSave: (id: number | null, name: string, markdown: string, opts: CardOptions | null) =>
    call<Deck>('decks_save', { id, name, markdown, opts }),
  decksDelete: (id: number) => call<void>('decks_delete', { id }),
  deckReadFile: (path: string) => call<string>('deck_read_file', { path }),
  deckWriteFile: (markdown: string, path: string) => call<string>('deck_write_file', { markdown, path }),

  // ── Maps (Map Paginator) ──────────────────────────────────────────────
  mapGet: () => call<MapDoc | null>('map_get'),
  mapSet: (doc: MapDoc) => call<MapDoc>('map_set', { doc }),
  mapOpenFile: (path: string) => call<MapDoc>('map_open_file', { path }),
  mapOpenSource: (hash: string, name?: string | null) => call<MapDoc>('map_open_source', { hash, name: name ?? null }),
  mapOpenAsset: (id: number) => call<MapDoc>('map_open_asset', { id }),
  /** Processed working proxy (grid removal + filters, no grid overlay). */
  mapPreview: async (doc: MapDoc): Promise<Blob> => toBlob(await call<ArrayBuffer | string>('map_preview', { doc })),
  mapLayout: (doc: MapDoc) => call<MapLayout>('map_layout', { doc }),
  mapPreviewPage: async (doc: MapDoc, index: number, pxPerPt: number): Promise<Blob> =>
    toBlob(await call<ArrayBuffer | string>('map_preview_page', { doc, index, pxPerPt })),
  mapDewarp: (doc: MapDoc) => call<MapDoc>('map_dewarp', { doc }),
  mapUndoDewarp: (doc: MapDoc) => call<MapDoc>('map_undo_dewarp', { doc }),
  mapExportPdf: (doc: MapDoc, path: string) => call<string>('map_export_pdf', { doc, path }),
  mapSaveAsset: (doc: MapDoc, name: string, update?: number | null) =>
    call<string>('map_save_asset', { doc, name, update: update ?? null }),
  mapFilterCatalog: () => call<FilterCatalog>('map_filter_catalog'),
  mapFilterDefs: async (): Promise<FilterDef[]> => (await call<FilterCatalog>('map_filter_catalog')).defs,
  mapFilterPresets: async (): Promise<FilterPreset[]> => (await call<FilterCatalog>('map_filter_catalog')).presets,

  // ── AoE templates ─────────────────────────────────────────────────────
  aoePresetsList: () => call<AoePreset[]>('aoe_presets_list'),
  aoePresetsSave: (name: string, config: AoeConfig, overwrite: boolean) =>
    call<AoePreset>('aoe_presets_save', { name, config, overwrite }),
  aoePresetsDelete: (name: string) => call<void>('aoe_presets_delete', { name }),
  aoeRender: async (config: AoeConfig): Promise<Blob> => toBlob(await call<ArrayBuffer | string>('aoe_render', { config })),
  aoeInfo: (config: AoeConfig) => call<AoeInfo>('aoe_info', { config }),
  aoeExportPng: (config: AoeConfig, path: string) => call<string>('aoe_export_png', { config, path }),
  aoeExportTemplatePdf: (config: AoeConfig, path: string) => call<string>('aoe_export_template_pdf', { config, path }),
  aoeSheetGet: () => call<AoeSheet>('aoe_sheet_get'),
  aoeSheetAdd: (config: AoeConfig) => call<AoeSheet>('aoe_sheet_add', { config }),
  aoeSheetRemove: (ids: string[]) => call<AoeSheet>('aoe_sheet_remove', { ids }),
  aoeSheetClear: () => call<AoeSheet>('aoe_sheet_clear'),
  aoeSheetSetGap: (gapIn: number) => call<AoeSheet>('aoe_sheet_set_gap', { gapIn }),
  aoeSheetLayout: (inch: number, format: PageFormat) => call<AoeSheetLayout>('aoe_sheet_layout', { inch, format }),
  aoeSheetPreview: async (inch: number, format: PageFormat, page: number, maxPx: number): Promise<Blob> =>
    toBlob(await call<ArrayBuffer | string>('aoe_sheet_preview', { inch, format, page, maxPx })),
  aoeSheetExportPdf: (inch: number, format: PageFormat, path: string) =>
    call<string>('aoe_sheet_export_pdf', { inch, format, path }),
};

// ── native dialogs (no-ops returning null outside Tauri) ──

export async function pickFiles(title: string, extensions: string[]): Promise<string[] | null> {
  if (!isTauri()) return null;
  const { open } = await import('@tauri-apps/plugin-dialog');
  const r = await open({ title, multiple: true, filters: [{ name: 'Images', extensions }] });
  return r === null ? null : Array.isArray(r) ? r : [r];
}

export async function pickFile(title: string, extensions: string[], name = 'Files'): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import('@tauri-apps/plugin-dialog');
  const r = await open({ title, multiple: false, filters: [{ name, extensions }] });
  return typeof r === 'string' ? r : null;
}

export async function pickDirectory(title: string): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import('@tauri-apps/plugin-dialog');
  const r = await open({ title, directory: true });
  return typeof r === 'string' ? r : null;
}

export async function pickSavePath(title: string, defaultPath: string, extensions: string[]): Promise<string | null> {
  if (!isTauri()) return null;
  const { save } = await import('@tauri-apps/plugin-dialog');
  return save({ title, defaultPath, filters: [{ name: extensions[0].toUpperCase(), extensions }] });
}

/** Subscribe to files dropped on the window (Tauri only). */
export async function onFileDrop(handler: (paths: string[]) => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const { getCurrentWebview } = await import('@tauri-apps/api/webview');
  return getCurrentWebview().onDragDropEvent((e) => {
    if (e.payload.type === 'drop') handler(e.payload.paths);
  });
}

export const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp'];
