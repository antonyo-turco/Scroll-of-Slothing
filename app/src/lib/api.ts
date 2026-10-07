// Typed access to the Rust service layer. Inside Tauri this goes through IPC;
// in a plain browser (vite dev, vitest, Playwright) an in-memory mock is used
// so the UI can be developed and tested without the native shell.

import type { AppInfo, JobStatus, ServiceEvent, Settings, SettingsUpdate } from './types';
import { createMockBackend, type Backend } from './mock';

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

/** URL for a stored blob served by the `sleuth://` protocol. */
export function blobUrl(hash: string): string {
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
  jobsList: () => call<JobStatus[]>('jobs_list'),
  jobsCancel: (id: string) => call<void>('jobs_cancel', { id }),
  onEvent: (handler: (e: ServiceEvent) => void) => getBackend().listen(handler),
};
