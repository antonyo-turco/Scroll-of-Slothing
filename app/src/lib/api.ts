// Typed access to the Rust service layer. Inside Tauri this goes through IPC;
// in a plain browser (vite dev, vitest, Playwright) an in-memory mock is used
// so the UI can be developed and tested without the native shell.

import type {
  AppInfo, Asset, AssetPage, AssetPatch, AssetQuery, Campaign, ExportWhat, ImportFile,
  ImportReport, JobStatus, RenameOutcome, ServiceEvent, Session, Settings, SettingsUpdate, TagCount,
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

export const api = {
  appInfo: () => call<AppInfo>('app_info'),
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
