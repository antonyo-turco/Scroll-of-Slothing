// In-memory backend used outside Tauri (browser dev, unit tests, Playwright).
// It mirrors the behaviour of the Rust service closely enough for UI work and
// tests; the Rust implementation (and its tests) remain the source of truth.

import {
  DEFAULT_SETTINGS, SIZE_INCHES, SIZE_LABELS,
  type Asset, type AssetQuery, type Border, type Campaign, type PageFormat, type PlacedItem,
  type ServiceEvent, type Session, type Settings, type SheetItem, type SheetItemPatch, type SheetLayout,
  type SizeLabel, type TokenParams, type TokenSheet,
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

  // ── tokens & token sheet ──
  const borders: Border[] = [
    { id: 'builtin:roll20', name: 'Roll20 gold', builtin: true, blob: 'mock-border-gold', width: 442, height: 442 },
    { id: 'builtin:roll20_greyscale', name: 'Roll20 grey', builtin: true, blob: 'mock-border-grey', width: 256, height: 256 },
  ];
  const sources = new Map<string, { width: number; height: number }>();
  const tokenParams = new Map<number, TokenParams>();
  let sheet: TokenSheet = { items: [], format: settings.token_page_format, cut_border: settings.token_cut_border };
  let sheetLoaded = false;
  const getSheet = () => {
    if (!sheetLoaded) {
      sheet = { items: [], format: settings.token_page_format, cut_border: settings.token_cut_border };
      sheetLoaded = true;
    }
    return sheet;
  };
  const sheetItem = (label: string, size: string, image: string, asset_id: number | null): SheetItem => ({
    id: `mock-item-${++seq}`, label, size, copies: 1, image, thumb: image, asset_id,
  });
  const checkSize = (s: string) => {
    const hit = SIZE_LABELS.find((x) => x.toLowerCase() === s.split(' (')[0].trim().toLowerCase());
    if (!hit) throw invalid(`unknown size ${s}`);
    return hit;
  };
  const renderSvg = (p: TokenParams) => {
    const bg = p.bg ? `rgb(${p.bg.join(',')})` : 'transparent';
    const inner = 200 * (1 - p.ring_inset_pct / 100);
    return `<svg xmlns="http://www.w3.org/2000/svg" width="442" height="442" viewBox="0 0 442 442">
      <circle cx="221" cy="221" r="${inner}" fill="${bg}"/>
      <circle cx="${221 - p.pan[0] / 4}" cy="${221 - p.pan[1] / 4}" r="${60 * p.zoom}" fill="#7a5"/>
      <circle cx="221" cy="221" r="210" fill="none" stroke="${p.border.includes('grey') ? '#999' : '#c93'}" stroke-width="22"/></svg>`;
  };

  /** Half-inch first-fit packer (mirror of crates/core/src/pack/token_grid.rs, HalfInch mode). */
  const layoutSheet = (): SheetLayout => {
    const s = getSheet();
    const [pw, ph] = s.format === 'A4' ? [595.2756, 841.8898] : [612, 792];
    const cell = 36;
    const cols = Math.floor((pw - 20) / cell);
    const rows = Math.floor((ph - 20) / cell);
    const ox = (pw - cols * cell) / 2;
    const oy = (ph - rows * cell) / 2;
    const order = s.items.map((it, i) => ({ it, i, pt: SIZE_INCHES[it.size as SizeLabel] * 72 }));
    order.sort((a, b) => b.pt - a.pt);
    const pages: PlacedItem[][] = [];
    const skipped: string[] = [];
    let grid: boolean[][] = [];
    const fresh = () => {
      grid = Array.from({ length: rows }, () => Array(cols).fill(false));
      pages.push([]);
    };
    const find = (n: number) => {
      for (let r = 0; r + n <= rows; r++)
        for (let c = 0; c + n <= cols; c++)
          if (grid.slice(r, r + n).every((row) => row.slice(c, c + n).every((u) => !u))) return [r, c];
      return null;
    };
    for (const { it, pt } of order) {
      const n = Math.max(1, Math.ceil(pt / cell - 1e-9));
      if (n > rows || n > cols) {
        skipped.push(it.id);
        continue;
      }
      for (let k = 0; k < it.copies; k++) {
        if (!pages.length) fresh();
        let spot = find(n);
        if (!spot) {
          fresh();
          spot = find(n);
        }
        const [r, c] = spot!;
        for (let y = r; y < r + n; y++) for (let x = c; x < c + n; x++) grid[y][x] = true;
        const inset = (n * cell - pt) / 2;
        pages[pages.length - 1].push({ id: it.id, x: ox + c * cell + inset, y: ph - oy - (r + n) * cell + inset, w: pt, h: pt });
      }
    }
    return { page_w: pw, page_h: ph, pages, skipped };
  };

  Object.assign(handlers, {
    token_borders_list: () => borders,
    token_borders_add: ({ path, name }: Record<string, unknown>) => {
      const b = { id: `user:${++seq}`, name: (name as string) || String(path).split(/[\\/]/).pop()!.replace(/\.[^.]+$/, ''), builtin: false, blob: `mock-border-${seq}`, width: 300, height: 300 };
      borders.push(b);
      return b;
    },
    token_borders_remove: ({ id }: Record<string, unknown>) => {
      const i = borders.findIndex((b) => b.id === id && !b.builtin);
      if (i < 0) throw notFound(`user border ${id}`);
      borders.splice(i, 1);
    },
    image_source_from_path: ({ path }: Record<string, unknown>) => {
      const hash = `mock-src-${String(path).length}-${++seq}`;
      sources.set(hash, { width: 640, height: 400 });
      return { hash, width: 640, height: 400 };
    },
    image_source_from_clipboard: () => {
      throw invalid('the clipboard has no image');
    },
    image_source_info: ({ hash }: Record<string, unknown>) => ({ hash, ...(sources.get(hash as string) ?? { width: 512, height: 512 }) }),
    token_render: ({ params }: Record<string, unknown>) => {
      const p = params as TokenParams;
      if (!borders.some((b) => b.id === p.border)) throw notFound(`border ${p.border}`);
      return renderSvg(p);
    },
    token_save_png: ({ path }: Record<string, unknown>) => path,
    token_save_to_library: ({ source, params, name, size, update }: Record<string, unknown>) => {
      if (!(name as string).trim()) throw invalid('please enter a name');
      const s = checkSize(size as string);
      let a: Asset;
      if (update != null) {
        a = getAsset(update as number);
        a.name = (name as string).trim();
        a.size_label = s;
      } else {
        a = newAsset((name as string).trim(), 'token', s, []);
        assets.push(a);
      }
      a.original_blob = source as string;
      a.params_json = JSON.stringify(params);
      tokenParams.set(a.id, structuredClone(params as TokenParams));
      return a;
    },
    token_open_asset: ({ id }: Record<string, unknown>) => {
      const a = getAsset(id as number);
      const params = tokenParams.get(a.id) ?? { border: 'builtin:roll20', bg: null, ring_inset_pct: 0, pan: [0, 0], zoom: 1 };
      sources.set(a.original_blob, sources.get(a.original_blob) ?? { width: 512, height: 512 });
      return { source: { hash: a.original_blob, ...sources.get(a.original_blob)! }, params, name: a.name, size: a.size_label, asset_id: a.id };
    },

    token_sheet_get: () => getSheet(),
    token_sheet_add_assets: ({ ids }: Record<string, unknown>) => {
      for (const id of ids as number[]) {
        const a = getAsset(id);
        getSheet().items.push(sheetItem(a.name, a.size_label, a.rendered_blob, a.id));
      }
      return getSheet();
    },
    token_sheet_add_files: ({ paths, size }: Record<string, unknown>) => {
      const s = checkSize((size as string) ?? 'Medium');
      for (const p of paths as string[]) {
        getSheet().items.push(sheetItem(String(p).split(/[\\/]/).pop()!.replace(/\.[^.]+$/, ''), s, `mock-file-${++seq}`, null));
      }
      return getSheet();
    },
    token_sheet_add_composed: ({ label, size }: Record<string, unknown>) => {
      getSheet().items.push(sheetItem((label as string).trim() || 'Token', checkSize(size as string), `mock-composed-${++seq}`, null));
      return getSheet();
    },
    token_sheet_update: ({ id, patch }: Record<string, unknown>) => {
      const it = getSheet().items.find((i) => i.id === id);
      if (!it) throw notFound(`sheet item ${id}`);
      const p = patch as SheetItemPatch;
      if (p.label != null) it.label = p.label;
      if (p.size != null) it.size = checkSize(p.size);
      if (p.copies != null) {
        if (p.copies < 1 || p.copies > 99) throw invalid('copies must be 1–99');
        it.copies = p.copies;
      }
      return getSheet();
    },
    token_sheet_remove: ({ id }: Record<string, unknown>) => {
      const s = getSheet();
      const n = s.items.length;
      s.items = s.items.filter((i) => i.id !== id);
      if (s.items.length === n) throw notFound(`sheet item ${id}`);
      return s;
    },
    token_sheet_clear: () => {
      getSheet().items = [];
      return getSheet();
    },
    token_sheet_set_options: ({ format, cutBorder }: Record<string, unknown>) => {
      const s = getSheet();
      if (format != null) s.format = format as PageFormat;
      if (cutBorder != null) s.cut_border = cutBorder as boolean;
      return s;
    },
    token_sheet_layout: () => layoutSheet(),
    token_sheet_export_pdf: () => {
      if (!getSheet().items.length) throw invalid('the token sheet is empty');
      const id = `mock-job-${++seq}`;
      const job = { id, kind: 'token sheet export', state: 'done' as const, progress: 1, message: '', result: { pages: layoutSheet().pages.length }, error: null };
      setTimeout(() => listeners.forEach((l) => l({ type: 'job', ...job })), 10);
      return id;
    },
  });

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
