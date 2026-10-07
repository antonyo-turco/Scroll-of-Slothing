// Global reactive app state (Svelte 5 runes): current route, settings, jobs, toasts.

import { api } from './api';
import { isRoute } from './nav';
import type { JobStatus, ServiceError, ServiceEvent, Settings } from './types';
import { DEFAULT_SETTINGS } from './types';

export interface Toast {
  id: number;
  kind: 'info' | 'success' | 'error';
  text: string;
  detail?: string;
}

class AppState {
  route = $state('tokens.sheet');
  settings = $state<Settings>({ ...DEFAULT_SETTINGS });
  sidebarCollapsed = $state(false);
  jobs = $state<JobStatus[]>([]);
  toasts = $state<Toast[]>([]);
  paletteOpen = $state(false);
  ready = $state(false);
  /** Global campaign context (persisted in workspace `ui.campaign`). */
  campaignId = $state<string | null>(null);
  /** Bumped (debounced) whenever persistent data changes; views refetch on change. */
  dataVersion = $state(0);
  /** One-shot filter handed to the Library when navigating there (e.g. from a session). */
  libraryPreset = $state<{ sessionId?: string; campaignId?: string } | null>(null);
  /** One-shot request for an editor view to open a library asset. */
  openRequest = $state<{ route: string; assetId: number } | null>(null);
  #bumpTimer: ReturnType<typeof setTimeout> | null = null;
  #toastSeq = 0;
  #unlisten: (() => void) | null = null;

  async init(): Promise<void> {
    try {
      this.settings = await api.settingsGet();
    } catch (e) {
      this.error('Could not load settings', e);
    }
    this.sidebarCollapsed = this.settings.sidebar_collapsed;
    const fromHash = location.hash.replace(/^#\/?/, '');
    this.route = isRoute(fromHash) ? fromHash : isRoute(this.settings.start_view) ? this.settings.start_view : 'tokens.sheet';
    this.applyTheme();
    this.#unlisten = await api.onEvent((e) => this.onEvent(e));
    this.jobs = await api.jobsList().catch(() => []);
    this.campaignId = await api.workspaceGet<string>('ui.campaign').catch(() => null);
    window.addEventListener('hashchange', () => {
      const id = location.hash.replace(/^#\/?/, '');
      if (isRoute(id)) this.route = id;
    });
    this.ready = true;
  }

  dispose(): void {
    this.#unlisten?.();
  }

  navigate(id: string): void {
    if (!isRoute(id)) return;
    this.route = id;
    if (location.hash !== `#/${id}`) history.replaceState(null, '', `#/${id}`);
  }

  async updateSettings(patch: Partial<Settings>): Promise<Record<string, string>> {
    try {
      const res = await api.settingsSet(patch);
      this.settings = res.settings;
      this.applyTheme();
      return res.errors;
    } catch (e) {
      this.error('Could not save settings', e);
      return {};
    }
  }

  async resetSettings(): Promise<void> {
    this.settings = await api.settingsReset();
    this.applyTheme();
  }

  toggleSidebar(): void {
    this.sidebarCollapsed = !this.sidebarCollapsed;
    void this.updateSettings({ sidebar_collapsed: this.sidebarCollapsed });
  }

  applyTheme(): void {
    const t = this.settings.theme;
    if (t === 'system') document.documentElement.removeAttribute('data-theme');
    else document.documentElement.setAttribute('data-theme', t);
  }

  onEvent(e: ServiceEvent): void {
    if (e.type === 'job') {
      const { type: _t, ...job } = e;
      const i = this.jobs.findIndex((j) => j.id === job.id);
      if (i >= 0) this.jobs[i] = job;
      else this.jobs.push(job);
      if (job.state === 'failed') this.toast('error', `${job.kind} failed`, job.error ?? undefined);
      if (job.state === 'done') this.toast('success', `${job.kind} finished`);
    } else if (e.type === 'store_changed' && e.entity === 'settings') {
      void api.settingsGet().then((s) => {
        this.settings = s;
        this.applyTheme();
      });
    } else if (e.type === 'store_changed') {
      this.bumpData();
    }
  }

  /** Signals views to refetch; coalesces bursts (e.g. a bulk import) into one refresh. */
  bumpData(): void {
    if (this.#bumpTimer) return;
    this.#bumpTimer = setTimeout(() => {
      this.#bumpTimer = null;
      this.dataVersion++;
    }, 150);
  }

  async setCampaign(id: string | null): Promise<void> {
    this.campaignId = id;
    await api.workspaceSet('ui.campaign', id).catch((e) => this.error('Could not save campaign', e));
  }

  toast(kind: Toast['kind'], text: string, detail?: string): void {
    const id = ++this.#toastSeq;
    this.toasts.push({ id, kind, text, detail });
    if (kind !== 'error') setTimeout(() => this.dismissToast(id), 3500);
  }

  dismissToast(id: number): void {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  error(text: string, e: unknown): void {
    const detail = (e as ServiceError)?.message ?? String(e);
    this.toast('error', text, detail);
  }
}

export const app = new AppState();
