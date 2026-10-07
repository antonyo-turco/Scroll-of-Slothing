// In-memory backend used outside Tauri (browser dev, unit tests, Playwright).
// It mirrors the behaviour of the Rust service closely enough for UI work and
// tests; the Rust implementation (and its tests) remain the source of truth.

import {
  DEFAULT_SETTINGS, SIZE_LABELS,
  type Asset, type AssetQuery, type Campaign, type ServiceEvent, type Session, type Settings,
} from './types';

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

/** Deterministic placeholder image for a mock blob hash. */
export function mockBlobUrl(hash: string): string {
  let h = 0;
  for (const c of hash) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  const hue = h % 360;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256">
    <rect width="256" height="256" fill="hsl(${hue} 45% 55%)"/>
    <circle cx="128" cy="110" r="56" fill="hsl(${(hue + 40) % 360} 50% 75%)"/>
    <rect x="56" y="180" width="144" height="36" rx="18" fill="hsl(${(hue + 40) % 360} 50% 75%)"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

class MockError {
  constructor(public code: string, public message: string) {}
}
const notFound = (what: string) => new MockError('not_found', `not found: ${what}`);
const invalid = (msg: string) => new MockError('invalid_input', `invalid input: ${msg}`);

const SEED: [string, string, string, string[]][] = [
  ['Goblin Boss', 'token', 'Small', ['goblinoid', 'boss']],
  ['Owlbear', 'token', 'Large', ['beast']],
  ['Ancient Red Dragon', 'token', 'Gargantuan', ['dragon', 'boss']],
  ['Zombie', 'token', 'Medium', ['undead']],
  ['Brazier', 'cutout', 'Small', ['prop']],
  ['Crypt of the Lich', 'map', 'Medium', ['undead', 'dungeon']],
  ['Forest Road', 'map', 'Medium', []],
];

export function createMockBackend(initial: Partial<Settings> = {}, opts: { seed?: boolean } = {}): Backend {
  let settings: Settings = { ...DEFAULT_SETTINGS, ...initial };
  const listeners = new Set<(e: ServiceEvent) => void>();
  const workspaces = new Map<string, unknown>();
  let assets: Asset[] = [];
  let campaigns: Campaign[] = [];
  let sessions: Session[] = [];
  let tags: { id: number; name: string }[] = [];
  let seq = 0;
  const now = () => new Date().toISOString();

  const tagId = (name: string) => {
    let t = tags.find((x) => x.name.toLowerCase() === name.trim().toLowerCase());
    if (!t) tags.push((t = { id: ++seq, name: name.trim() }));
    return t.id;
  };
  const getAsset = (id: number) => {
    const a = assets.find((x) => x.id === id);
    if (!a) throw notFound(`asset ${id}`);
    return a;
  };
  const newAsset = (name: string, kind: string, size: string, tagNames: string[]): Asset => {
    const id = ++seq;
    tagNames.forEach(tagId);
    const blob = `mock-${kind}-${id}`;
    return {
      id, kind, name, aliases: '', size_label: size, pinned: false,
      original_blob: blob, rendered_blob: blob, pdf_blob: kind === 'map' ? `${blob}-pdf` : null,
      thumb_blob: blob, preview_blob: blob, params_json: null, legacy_id: null,
      created: now(), updated: now(), tags: [...tagNames], campaign_ids: [], session_ids: [],
    };
  };
  if (opts.seed !== false) {
    for (const [n, k, s, t] of SEED) assets.push(newAsset(n, k, s, t));
    const c = { id: 'mock-c1', name: 'Curse of Strahd', created: now(), session_count: 1, asset_count: 0 };
    campaigns.push(c);
    sessions.push({ id: 'mock-s1', campaign_id: c.id, name: 'Death House', notes: '', created: now(), asset_count: 0 });
    assets[3].session_ids.push('mock-s1');
    assets[5].campaign_ids.push(c.id);
  }

  const emit = (entity: string, entity_id: string, op = 'update') =>
    listeners.forEach((l) => l({ type: 'store_changed', entity, entity_id, op }));

  const sizeOrder = (s: string) => {
    const i = SIZE_LABELS.indexOf(s as never);
    return i < 0 ? 99 : i;
  };

  const search = (q: AssetQuery) => {
    let items = assets.slice();
    const words = (q.q ?? '').toLowerCase().split(/\s+/).filter(Boolean);
    if (words.length) {
      items = items.filter((a) => {
        const hay = [a.name, a.aliases, ...a.tags].join(' ').toLowerCase().split(/\s+/);
        return words.every((w) => hay.some((h) => h.startsWith(w)));
      });
    }
    if (q.kinds?.length) items = items.filter((a) => q.kinds!.includes(a.kind));
    for (const t of q.tags ?? []) items = items.filter((a) => a.tags.some((x) => x.toLowerCase() === t.toLowerCase()));
    if (q.campaign_id) {
      const sess = sessions.filter((s) => s.campaign_id === q.campaign_id).map((s) => s.id);
      items = items.filter((a) => a.campaign_ids.includes(q.campaign_id!) || a.session_ids.some((s) => sess.includes(s)));
    }
    if (q.session_id) items = items.filter((a) => a.session_ids.includes(q.session_id!));
    if (q.pinned_only) items = items.filter((a) => a.pinned);
    const byName = (a: Asset, b: Asset) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
    const sorters: Record<string, (a: Asset, b: Asset) => number> = {
      newest: (a, b) => b.id - a.id,
      oldest: (a, b) => a.id - b.id,
      name_asc: byName,
      name_desc: (a, b) => byName(b, a),
      kind: (a, b) => a.kind.localeCompare(b.kind) || byName(a, b),
      size: (a, b) => sizeOrder(a.size_label) - sizeOrder(b.size_label) || byName(a, b),
    };
    items.sort(sorters[q.sort ?? 'newest']);
    const total = items.length;
    const off = q.offset ?? 0;
    return { items: items.slice(off, q.limit ? off + q.limit : undefined), total };
  };

  const campaignView = () =>
    campaigns.map((c) => {
      const sess = sessions.filter((s) => s.campaign_id === c.id).map((s) => s.id);
      const n = assets.filter((a) => a.campaign_ids.includes(c.id) || a.session_ids.some((s) => sess.includes(s))).length;
      return { ...c, session_count: sess.length, asset_count: n };
    });
  const sessionView = (cid?: string | null) =>
    sessions
      .filter((s) => !cid || s.campaign_id === cid)
      .map((s) => ({ ...s, asset_count: assets.filter((a) => a.session_ids.includes(s.id)).length }));

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
    settings_reset: () => (settings = { ...DEFAULT_SETTINGS }),
    workspace_get: ({ key }) => workspaces.get(key as string) ?? null,
    workspace_set: ({ key, value }) => void workspaces.set(key as string, value),
    jobs_list: () => [],
    jobs_cancel: () => null,
    legacy_import: () => 'mock-job',

    library_search: ({ query }) => search((query ?? {}) as AssetQuery),
    library_get: ({ id }) => getAsset(id as number),
    library_update: ({ id, patch }) => {
      const a = getAsset(id as number);
      const p = patch as Record<string, unknown>;
      if (typeof p.name === 'string') {
        if (!p.name.trim()) throw invalid('name must not be empty');
        a.name = p.name.trim();
      }
      if (typeof p.aliases === 'string') a.aliases = p.aliases;
      if (typeof p.size_label === 'string') {
        const s = SIZE_LABELS.find((x) => x.toLowerCase() === (p.size_label as string).split(' (')[0].toLowerCase());
        if (!s) throw invalid(`unknown size ${p.size_label}`);
        a.size_label = s;
      }
      if (typeof p.pinned === 'boolean') a.pinned = p.pinned;
      emit('asset', String(a.id));
      return a;
    },
    library_set_tags: ({ id, values }) => {
      const a = getAsset(id as number);
      const seen = new Set<string>();
      a.tags = (values as string[])
        .map((v) => v.trim())
        .filter((v) => v && !seen.has(v.toLowerCase()) && seen.add(v.toLowerCase()))
        .map((v) => {
          const id = tagId(v); // may create the tag; resolve before searching
          return tags.find((t) => t.id === id)!.name;
        })
        .sort((x, y) => x.localeCompare(y, undefined, { sensitivity: 'base' }));
      return a;
    },
    library_set_campaigns: ({ id, values }) => {
      const a = getAsset(id as number);
      for (const c of values as string[]) if (!campaigns.some((x) => x.id === c)) throw notFound(`campaign_id ${c}`);
      a.campaign_ids = [...(values as string[])];
      return a;
    },
    library_set_sessions: ({ id, values }) => {
      const a = getAsset(id as number);
      for (const s of values as string[]) if (!sessions.some((x) => x.id === s)) throw notFound(`session_id ${s}`);
      a.session_ids = [...(values as string[])];
      return a;
    },
    library_delete: ({ ids }) => {
      const before = assets.length;
      assets = assets.filter((a) => !(ids as number[]).includes(a.id));
      return before - assets.length;
    },
    library_import: ({ files }) => {
      const imported: number[] = [];
      for (const f of files as { path: string; name?: string; kind: string; size?: string }[]) {
        const stem = f.path.split(/[\\/]/).pop()!.replace(/\.[^.]+$/, '');
        const a = newAsset(f.name || stem, f.kind, f.size ?? 'Medium', []);
        assets.push(a);
        imported.push(a.id);
      }
      return { imported, failed: [] };
    },
    library_export: ({ path }) => path,
    library_export_many: ({ ids, dir }) => (ids as number[]).map((id) => `${dir}/${getAsset(id).name}.png`),
    library_copy_to_clipboard: () => null,
    library_bulk_tag: ({ ids, tag }) => {
      tagId(tag as string);
      for (const id of ids as number[]) {
        const a = getAsset(id);
        if (!a.tags.some((t) => t.toLowerCase() === (tag as string).toLowerCase())) a.tags.push(tag as string);
      }
    },
    library_bulk_pin: ({ ids, pinned }) => (ids as number[]).forEach((id) => (getAsset(id).pinned = pinned as boolean)),
    library_bulk_link_session: ({ ids, sessionId }) =>
      (ids as number[]).forEach((id) => {
        const a = getAsset(id);
        if (!a.session_ids.includes(sessionId as string)) a.session_ids.push(sessionId as string);
      }),
    library_bulk_link_campaign: ({ ids, campaignId }) =>
      (ids as number[]).forEach((id) => {
        const a = getAsset(id);
        if (!a.campaign_ids.includes(campaignId as string)) a.campaign_ids.push(campaignId as string);
      }),

    tags_list: () =>
      tags
        .map((t) => ({ ...t, count: assets.filter((a) => a.tags.some((x) => x.toLowerCase() === t.name.toLowerCase())).length }))
        .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })),
    tags_rename: ({ id, name }) => {
      const t = tags.find((x) => x.id === id);
      if (!t) throw notFound(`tag ${id}`);
      const clash = tags.find((x) => x.id !== id && x.name.toLowerCase() === (name as string).trim().toLowerCase());
      for (const a of assets) {
        a.tags = a.tags.map((x) => (x.toLowerCase() === t.name.toLowerCase() ? (clash?.name ?? (name as string)) : x));
        a.tags = [...new Map(a.tags.map((x) => [x.toLowerCase(), x])).values()];
      }
      if (clash) {
        tags = tags.filter((x) => x.id !== id);
        return { id: clash.id, merged_into: clash.id };
      }
      t.name = (name as string).trim();
      return { id, merged_into: null };
    },
    tags_merge: ({ src, dst }) => handlers.tags_rename({ id: src, name: tags.find((t) => t.id === dst)!.name }),
    tags_delete: ({ id }) => {
      const t = tags.find((x) => x.id === id);
      if (!t) throw notFound(`tag ${id}`);
      tags = tags.filter((x) => x.id !== id);
      for (const a of assets) a.tags = a.tags.filter((x) => x.toLowerCase() !== t.name.toLowerCase());
    },

    campaigns_list: () => campaignView(),
    campaigns_create: ({ name }) => {
      if (!(name as string).trim()) throw invalid('campaign name must not be empty');
      const c = { id: `mock-c${++seq}`, name: (name as string).trim(), created: now(), session_count: 0, asset_count: 0 };
      campaigns.push(c);
      return c;
    },
    campaigns_rename: ({ id, name }) => {
      const c = campaigns.find((x) => x.id === id);
      if (!c) throw notFound(`campaign ${id}`);
      c.name = (name as string).trim();
    },
    campaigns_delete: ({ id }) => {
      const sess = sessions.filter((s) => s.campaign_id === id).map((s) => s.id);
      campaigns = campaigns.filter((c) => c.id !== id);
      sessions = sessions.filter((s) => s.campaign_id !== id);
      for (const a of assets) {
        a.campaign_ids = a.campaign_ids.filter((c) => c !== id);
        a.session_ids = a.session_ids.filter((s) => !sess.includes(s));
      }
    },
    sessions_list: ({ campaignId }) => sessionView(campaignId as string | null),
    sessions_create: ({ campaignId, name, notes }) => {
      if (!campaigns.some((c) => c.id === campaignId)) throw notFound(`campaign ${campaignId}`);
      const s = { id: `mock-s${++seq}`, campaign_id: campaignId as string, name: (name as string).trim(), notes: (notes as string) ?? '', created: now(), asset_count: 0 };
      sessions.push(s);
      return s;
    },
    sessions_update: ({ id, name, notes }) => {
      const s = sessions.find((x) => x.id === id);
      if (!s) throw notFound(`session ${id}`);
      if (typeof name === 'string') s.name = name;
      if (typeof notes === 'string') s.notes = notes;
    },
    sessions_delete: ({ id }) => {
      sessions = sessions.filter((s) => s.id !== id);
      for (const a of assets) a.session_ids = a.session_ids.filter((s) => s !== id);
    },
    sessions_set_assets: ({ id, assetIds }) => {
      for (const a of assets) {
        const has = (assetIds as number[]).includes(a.id);
        a.session_ids = a.session_ids.filter((s) => s !== id);
        if (has) a.session_ids.push(id as string);
      }
    },
  };

  return {
    async invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
      const h = handlers[cmd];
      if (!h) throw { code: 'internal', message: `mock: unknown command ${cmd}` };
      try {
        return structuredClone(h(args) ?? null) as T;
      } catch (e) {
        if (e instanceof MockError) throw { code: e.code, message: e.message };
        throw e;
      }
    },
    async listen(handler) {
      listeners.add(handler);
      return () => listeners.delete(handler);
    },
  };
}
