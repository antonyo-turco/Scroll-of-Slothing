// In-memory backend used outside Tauri (browser dev, unit tests, Playwright).
// It mirrors the validation rules of crates/core/src/settings.rs closely enough
// for UI development; the Rust implementation remains the source of truth.

import { DEFAULT_SETTINGS, type ServiceEvent, type Settings } from './types';

export interface Backend {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  listen(handler: (e: ServiceEvent) => void): Promise<() => void>;
}

const ROUTES = [
  'library', 'tokens.cutter', 'tokens.sheet', 'cutouts.cutter', 'cutouts.sheet',
  'cards', 'maps.paginator', 'maps.aoe', 'campaigns', 'settings',
];

function validate(key: string, value: unknown): string | null {
  const d = (DEFAULT_SETTINGS as unknown as Record<string, unknown>)[key];
  if (d === undefined) return 'unknown setting';
  if (key === 'start_view') return ROUTES.includes(value as string) ? null : 'unknown view';
  if (typeof d === 'boolean') return typeof value === 'boolean' ? null : 'expected a boolean';
  if (typeof d === 'number') {
    if (typeof value !== 'number' || !Number.isFinite(value)) return 'expected a number';
    const positive = ['cut_line_width', 'map_square_in', 'map_calibration_mm'];
    if (positive.includes(key)) return value > 0 ? null : 'must be > 0';
    return value >= 0 ? null : 'must be ≥ 0';
  }
  if (key.endsWith('_page_format')) return value === 'Letter' || value === 'A4' ? null : 'invalid format';
  if (key === 'cut_line_style') return ['Dashed', 'Dotted', 'Solid'].includes(value as string) ? null : 'invalid style';
  if (key === 'theme') return ['system', 'light', 'dark'].includes(value as string) ? null : 'invalid theme';
  if (key === 'cut_line_color') return /^#[0-9a-fA-F]{6}$/.test(value as string) ? null : 'invalid colour';
  return null;
}

export function createMockBackend(initial: Partial<Settings> = {}): Backend {
  let settings: Settings = { ...DEFAULT_SETTINGS, ...initial };
  const listeners = new Set<(e: ServiceEvent) => void>();

  const handlers: Record<string, (args: Record<string, unknown>) => unknown> = {
    app_info: () => ({ version: '0.0.0-mock', data_dir: '(in-memory mock)' }),
    settings_get: () => ({ ...settings }),
    settings_set: ({ patch }) => {
      const errors: Record<string, string> = {};
      const next = { ...settings } as Record<string, unknown>;
      for (const [k, v] of Object.entries(patch as Record<string, unknown>)) {
        const err = validate(k, v);
        if (err) errors[k] = err;
        else next[k] = v;
      }
      settings = next as unknown as Settings;
      return { settings: { ...settings }, errors };
    },
    settings_reset: () => {
      settings = { ...DEFAULT_SETTINGS };
      return { ...settings };
    },
    jobs_list: () => [],
    jobs_cancel: () => null,
  };

  return {
    async invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
      const h = handlers[cmd];
      if (!h) throw { code: 'internal', message: `mock: unknown command ${cmd}` };
      return structuredClone(h(args)) as T;
    },
    async listen(handler) {
      listeners.add(handler);
      return () => listeners.delete(handler);
    },
  };
}
