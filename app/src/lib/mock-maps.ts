// Mock handlers for the Map Paginator and AoE templates (browser dev, unit
// tests, Playwright). Simplified ports; the Rust service is the reference.

import type {
  AoeConfig, AoeInfo, AoePreset, AoeSheet, AoeSheetLayout, FilterCatalog, FilterDef, FilterStep,
  FewestPages, GridModel, MapDoc, MapLayout, PageFormat, Tile,
} from './types';

type Handler = (args: Record<string, unknown>) => unknown;
type Emit = (e: Record<string, unknown>) => void;

const PAGE_PT: Record<PageFormat, [number, number]> = {
  Letter: [612, 792],
  A4: [(21 / 2.54) * 72, (29.7 / 2.54) * 72],
  A3: [(29.7 / 2.54) * 72, (42 / 2.54) * 72],
};
const WORK_MAX = 2000;

const FILTER_DEFS: FilterDef[] = [
  ['autocontrast', 'Auto-contrast', [['clip', 'Clip %', 0, 20, 1, 1]]],
  ['contrast', 'Contrast', [['amount', 'Amount %', 0, 300, 150, 5]]],
  ['lightness', 'Lightness', [['amount', 'Amount %', 0, 200, 100, 5]]],
  ['clarity', 'Local contrast (clarity)', [['amount', 'Amount %', 0, 300, 100, 10]]],
  ['sharpen', 'Sharpen', [['amount', 'Amount %', 0, 500, 150, 10]]],
  ['sepia', 'Sepia', []],
  ['grayscale', 'Grayscale', []],
  ['adaptive', 'Adaptive B&W', [['sensitivity', 'Sensitivity %', 0, 100, 50, 1]]],
  ['threshold', 'Ink-saving B&W', [['cutoff', 'Cutoff %', 0, 100, 50, 1]]],
  ['posterize', 'Posterize', [['levels', 'Levels', 2, 8, 4, 1]]],
  ['dither', 'Dither B&W', []],
  ['edge_boost', 'Edge boost', [['strength', 'Strength %', 0, 100, 60, 1]]],
  ['line_art', 'Line art only', [['detail', 'Detail %', 0, 100, 55, 1]]],
  ['canny', 'Canny edges', [['low', 'Low threshold', 0, 255, 50, 1], ['high', 'High threshold', 0, 255, 150, 1], ['dilate', 'Dilate', 0, 5, 0, 1]]],
].map(([key, label, ps]) => ({
  key: key as string,
  label: label as string,
  params: (ps as [string, string, number, number, number, number][]).map(([k, l, min, max, def, step]) => ({
    key: k, label: l, min, max, default: def, step,
  })),
}));

const PRESET_STEPS: [string, [string, Record<string, number>][]][] = [
  ['None (clear all)', []],
  ['Most legible (gray + edges)', [['autocontrast', {}], ['sharpen', { amount: 150 }], ['grayscale', {}], ['edge_boost', { strength: 70 }]]],
  ['Ink-saver (adaptive)', [['autocontrast', {}], ['sharpen', { amount: 150 }], ['adaptive', { sensitivity: 50 }]]],
  ['Detailed line maps', [['autocontrast', {}], ['sharpen', { amount: 200 }], ['adaptive', { sensitivity: 60 }]]],
  ['Max detail B&W', [['autocontrast', {}], ['clarity', { amount: 80 }], ['sharpen', { amount: 150 }], ['edge_boost', { strength: 30 }], ['adaptive', { sensitivity: 60 }]]],
  ['Atmospheric (dither)', [['autocontrast', {}], ['edge_boost', { strength: 50 }], ['dither', {}]]],
  ['Newsprint', [['autocontrast', {}], ['contrast', { amount: 140 }], ['dither', {}]]],
  ['Newsprint (print-friendly)', [['autocontrast', { clip: 2 }], ['contrast', { amount: 120 }], ['sharpen', { amount: 150 }], ['edge_boost', { strength: 55 }], ['dither', {}]]],
  ['High-contrast B&W', [['autocontrast', {}], ['contrast', { amount: 180 }], ['threshold', { cutoff: 55 }]]],
  ['Posterized (3 levels)', [['autocontrast', {}], ['clarity', { amount: 100 }], ['posterize', { levels: 3 }], ['edge_boost', { strength: 50 }]]],
  ['Posterized (4 levels)', [['autocontrast', {}], ['posterize', { levels: 4 }]]],
  ['Soft grayscale', [['autocontrast', {}], ['clarity', { amount: 80 }], ['grayscale', {}]]],
  ['Clean grayscale', [['autocontrast', {}], ['grayscale', {}]]],
  ['Pure line art', [['sharpen', { amount: 150 }], ['line_art', { detail: 55 }]]],
  ['Pencil sketch', [['clarity', { amount: 120 }], ['line_art', { detail: 68 }]]],
  ['Bold ink (thick lines)', [['autocontrast', {}], ['sharpen', { amount: 280 }], ['adaptive', { sensitivity: 40 }]]],
  ['Sepia print', [['autocontrast', {}], ['sepia', {}]]],
  ['Faded parchment', [['autocontrast', {}], ['clarity', { amount: 60 }], ['sepia', {}]]],
];

const defaults = (key: string) =>
  Object.fromEntries((FILTER_DEFS.find((d) => d.key === key)?.params ?? []).map((p) => [p.key, p.default]));

function presetPipeline(name: string): FilterStep[] {
  const steps = PRESET_STEPS.find(([n]) => n === name)?.[1] ?? [];
  const out: FilterStep[] = steps.map(([type, over]) => ({ type, enabled: true, params: { ...defaults(type), ...over } }));
  for (const d of FILTER_DEFS) {
    if (!out.some((s) => s.type === d.key)) out.push({ type: d.key, enabled: false, params: defaults(d.key) });
  }
  return out;
}

export const MOCK_FILTER_CATALOG: FilterCatalog = {
  defs: FILTER_DEFS,
  presets: PRESET_STEPS.map(([name]) => ({ name, pipeline: presetPipeline(name) })),
};

// Token filters (sleuth_core::token::TOKEN_FILTERS): colour/tone only.
const TOKEN_FILTERS = ['autocontrast', 'contrast', 'lightness', 'clarity', 'sharpen', 'sepia', 'grayscale', 'posterize', 'edge_boost'];
const isToken = (k: string) => TOKEN_FILTERS.includes(k);

export const MOCK_TOKEN_FILTER_CATALOG: FilterCatalog = {
  defs: FILTER_DEFS.filter((d) => isToken(d.key)),
  presets: PRESET_STEPS.filter(([, steps]) => steps.every(([t]) => isToken(t))).map(([name]) => ({
    name,
    pipeline: presetPipeline(name).filter((s) => isToken(s.type)),
  })),
};

export function defaultMapDoc(): MapDoc {
  return {
    source: null, pre_warp: null, name: '', asset_id: null, detected_dpi: 70, dpi_detected: false,
    mode: 'ppi', rows: 35, cols: 55, square_cells: true, ref_n: 3, ref_quad: null, ppi: 70, side: 70,
    offset_x: 0, offset_y: 0, color: [40, 40, 40], thickness: 1, opacity: 160, visible: true, style: 'solid',
    remove_grid: false, filters: presetPipeline('None (clear all)'), format: 'A4', landscape: false, square_in: 1,
    calibration_mm: 25.4, margin_in: 0, overlap_in: 0, align_to_grid: false, cut_border: false,
    fit_w_ft: 3, fit_h_ft: 2, region: null, page_offset: [0, 0], excluded: [],
  };
}

const dist = (a: [number, number], b: [number, number]) => Math.hypot(a[0] - b[0], a[1] - b[1]);
const mod = (a: number, b: number) => ((a % b) + b) % b;

function baseGrid(d: MapDoc): [number, number, number, number] {
  const [w, h] = d.source ? [d.source.width, d.source.height] : [0, 0];
  switch (d.mode) {
    case 'rows_cols': {
      let cw = w / Math.max(1, d.cols), ch = h / Math.max(1, d.rows);
      if (d.square_cells) cw = ch = Math.min(cw, ch);
      return [cw, ch, 0, 0];
    }
    case 'reference': {
      if (d.pre_warp) return [d.side, d.side, 0, 0];
      if (d.ref_quad) {
        const q = d.ref_quad;
        const bySum = [...q].sort((a, b) => a[0] + a[1] - (b[0] + b[1]));
        const byDiff = [...q].sort((a, b) => a[0] - a[1] - (b[0] - b[1]));
        const [tl, br, bl, tr] = [bySum[0], bySum[3], byDiff[0], byDiff[3]];
        const n = Math.max(1, d.ref_n);
        const cw = (dist(tl, tr) + dist(bl, br)) / 2 / n;
        const ch = (dist(tl, bl) + dist(tr, br)) / 2 / n;
        const left = Math.min(...q.map((p) => p[0])), top = Math.min(...q.map((p) => p[1]));
        return [cw, ch, cw > 0 ? mod(left, cw) : 0, ch > 0 ? mod(top, ch) : 0];
      }
      return [d.detected_dpi, d.detected_dpi, 0, 0];
    }
    case 'ppi':
      return [Math.max(0, d.ppi), Math.max(0, d.ppi), 0, 0];
    default:
      return [Math.max(0, d.side), Math.max(0, d.side), 0, 0];
  }
}

// Mirrors core::maps::tiling::axis_pages: a page must hold ≥ 1 px of the region.
function axisPages(origin: number, step: number, tile: number, lo: number, len: number): number[] {
  const out: number[] = [];
  const min = Math.min(1, len / 2);
  let i = Math.floor((lo - origin) / step) - 1;
  for (let g = 0; g < 20000; g++, i++) {
    const left = origin + i * step;
    if (left >= lo + len - min) break;
    if (left + tile > lo + min) out.push(left);
  }
  return out;
}

export function mockMapLayout(d: MapDoc, suggest = true): MapLayout {
  const [cw, ch, bx, by] = baseGrid(d);
  const grid: GridModel = {
    cell_w: cw, cell_h: ch, offset_x: bx + d.offset_x, offset_y: by + d.offset_y,
    color: d.color, thickness: d.thickness, opacity: d.opacity, visible: d.visible, style: d.style,
  };
  const [w, h] = d.source ? [d.source.width, d.source.height] : [0, 0];
  const s = Math.min(1, WORK_MAX / Math.max(1, w, h));
  const [pw, ph] = d.landscape ? [PAGE_PT[d.format][1], PAGE_PT[d.format][0]] : PAGE_PT[d.format];
  const margin = Math.max(0, d.margin_in) * 72;
  const sqPt = d.square_in * 72 * (d.calibration_mm > 0 ? 25.4 / d.calibration_mm : 1);
  const tiles: Tile[] = [];
  let sx = 1, sy = 1;
  let [rx, ry, rw, rh] = d.region ?? [0, 0, w, h];
  rx = Math.max(0, rx); ry = Math.max(0, ry); rw = Math.min(rw, w - rx); rh = Math.min(rh, h - ry);
  let cols = 0, rows = 0;
  if (cw > 0 && ch > 0 && rw > 0 && rh > 0) {
    sx = sqPt / cw; sy = sqPt / ch;
    const uw = pw - 2 * margin, uh = ph - 2 * margin;
    const ov = Math.min(Math.max(0, d.overlap_in * 72), uw * 0.9, uh * 0.9);
    let tw: number, th: number, stw: number, sth: number, gx: number, gy: number;
    if (d.align_to_grid) {
      const cx = Math.max(1, Math.floor(uw / sqPt)), cy = Math.max(1, Math.floor(uh / sqPt));
      const ox = Math.min(cx - 1, Math.round(ov / sqPt)), oy = Math.min(cy - 1, Math.round(ov / sqPt));
      tw = cx * cw; th = cy * ch; stw = (cx - ox) * cw; sth = (cy - oy) * ch;
      const snap = (v: number, o: number, p: number) => o + Math.round((v - o) / p) * p;
      gx = snap(rx + d.page_offset[0], grid.offset_x, cw);
      gy = snap(ry + d.page_offset[1], grid.offset_y, ch);
    } else {
      tw = uw / sx; th = uh / sy; stw = (uw - ov) / sx; sth = (uh - ov) / sy;
      gx = rx + d.page_offset[0]; gy = ry + d.page_offset[1];
    }
    const cs = axisPages(gx, stw, tw, rx, rw);
    const rs = axisPages(gy, sth, th, ry, rh);
    cols = cs.length; rows = rs.length;
    rs.forEach((top, rj) =>
      cs.forEach((left, ci) => {
        const l = Math.max(rx, left), t = Math.max(ry, top);
        const r = Math.min(rx + rw, left + tw), b = Math.min(ry + rh, top + th);
        if (r - l <= 0 || b - t <= 0) return;
        tiles.push({
          col: ci, row: rj, crop: [l, t, r, b], page_rect: [left, top, left + tw, top + th],
          page_x_pt: (l - left) * sx, page_y_pt: (t - top) * sy, w_pt: (r - l) * sx, h_pt: (b - t) * sy,
        });
      }),
    );
  }
  const excluded = tiles.filter((t) => d.excluded.some(([c, r]) => c === t.col && r === t.row)).length;
  const total = tiles.length;
  const sq: [number, number] = [Math.max(0, Math.round(rw / (cw || 1))), Math.max(0, Math.round(rh / (ch || 1)))];
  const pin: [number, number] = [sq[0] * d.square_in, sq[1] * d.square_in];
  const [tW, tH] = [d.fit_w_ft * 12, d.fit_h_ft * 12];
  const fits = tW > 0 && tH > 0 && ((pin[0] <= tW && pin[1] <= tH) || (pin[0] <= tH && pin[1] <= tW));
  const mc = total ? Math.max(...tiles.map((t) => t.col)) + 1 : 0;
  const mr = total ? Math.max(...tiles.map((t) => t.row)) + 1 : 0;
  return {
    grid, base_offset: [bx, by], work_scale: s,
    proxy_w: Math.max(1, Math.floor(w * s)), proxy_h: Math.max(1, Math.floor(h * s)),
    scale_x: sx, scale_y: sy, tiles, cols, rows, selected: total - excluded,
    fit: cw > 0 && sq[0] && sq[1] ? { squares: sq, printed_in: pin, fits } : null,
    summary: total ? `${excluded ? `${total - excluded}/${total} selected · ` : ''}${total} page(s) · ${mc}×${mr} grid · 1 square = ${d.square_in}"` : '',
    page_w_pt: pw, page_h_pt: ph, margin_pt: margin,
    fewest: suggest && total > 1 ? mockFewest(d, total) : null,
  };
}

/** Simplified core::maps::tiling::fewest_pages: both orientations, no shift. */
function mockFewest(d: MapDoc, total: number): FewestPages | null {
  let best: FewestPages | null = null;
  for (const landscape of [false, true]) {
    const l = mockMapLayout({ ...d, landscape, page_offset: [0, 0] }, false);
    if (l.tiles.length && l.tiles.length < (best?.pages ?? total)) {
      best = { landscape, offset: [0, 0], pages: l.tiles.length, cols: l.cols, rows: l.rows };
    }
  }
  return best;
}

function mapSvg(w: number, h: number, label: string): string {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">
    <rect width="${w}" height="${h}" fill="#c9b48a"/>
    <rect x="${w * 0.1}" y="${h * 0.15}" width="${w * 0.35}" height="${h * 0.45}" fill="#a58d62" stroke="#3b2f20" stroke-width="6"/>
    <rect x="${w * 0.55}" y="${h * 0.3}" width="${w * 0.35}" height="${h * 0.55}" fill="#d8ccb0" stroke="#3b2f20" stroke-width="4"/>
    <circle cx="${w * 0.3}" cy="${h * 0.78}" r="${Math.min(w, h) * 0.1}" fill="#5f8a5a"/>
    <text x="10" y="24" font-size="18" fill="#222">${label}</text></svg>`;
}

// ── AoE ──

const AOE_PRESETS: [string, AoeConfig['shape'], number, number][] = [
  ['Sphere — 10 ft radius', 'circle', 10, 5],
  ['Sphere — 20 ft (Fireball)', 'circle', 20, 5],
  ['Sphere — 30 ft radius', 'circle', 30, 5],
  ['Cone — 15 ft (Burning Hands)', 'cone', 15, 5],
  ['Cone — 30 ft', 'cone', 30, 5],
  ['Cone — 60 ft (Cone of Cold)', 'cone', 60, 5],
  ['Line — 30 × 5 ft', 'line', 30, 5],
  ['Line — 60 × 5 ft', 'line', 60, 5],
  ['Line — 100 × 5 ft (Lightning)', 'line', 100, 5],
  ['Cube — 15 ft', 'square', 15, 5],
  ['Cube — 20 ft', 'square', 20, 5],
];

export function aoeLabel(shape: AoeConfig['shape'], a: number, b: number): string {
  const g = (v: number) => String(+v.toPrecision(6));
  if (shape === 'circle') return `${g(a)} ft radius`;
  if (shape === 'cone') return `${g(a)} ft cone`;
  if (shape === 'line') return `${g(a)} x ${g(b)} ft line`;
  return `${g(a)} ft cube`;
}

export function defaultAoeConfig(): AoeConfig {
  return {
    shape: 'circle', a_ft: 20, b_ft: 5, feet_per_square: 5, fill_rgb: [220, 70, 40], fill_alpha: 80,
    line_rgb: [150, 25, 0], line_width: 4, grid_on: true, grid_rgb: [60, 60, 60], grid_alpha: 110,
    label_on: true, label: '20 ft radius', inch: 1, page_format: 'A4', name: '',
  };
}

const bboxFt = (c: AoeConfig): [number, number] => {
  const a = Math.max(0, c.a_ft), b = Math.max(0, c.b_ft);
  if (c.shape === 'circle') return [2 * a, 2 * a];
  if (c.shape === 'line') return [a, Math.max(b, 0.01)];
  return [a, a];
};

function aoeSize(c: AoeConfig): [number, number, number] {
  let pps = Math.min(300, Math.max(60, 150 * c.inch));
  const fps = Math.max(0.1, c.feet_per_square), pad = c.line_width + 4;
  const [bw, bh] = bboxFt(c);
  for (;;) {
    const w = Math.max(1, Math.round((bw * pps) / fps + 2 * pad));
    const h = Math.max(1, Math.round((bh * pps) / fps + 2 * pad));
    if (Math.max(w, h) <= 6000) return [w, h, pps];
    pps *= 6000 / Math.max(w, h);
  }
}

function aoeInfo(c: AoeConfig): AoeInfo {
  const [bw, bh] = bboxFt(c);
  const inches: [number, number] = [(bw / c.feet_per_square) * c.inch, (bh / c.feet_per_square) * c.inch];
  const [w, h, cell] = aoeSize(c);
  const [pw, ph] = PAGE_PT[c.page_format];
  const tw = pw / ((c.inch * 72) / cell), th = ph / ((c.inch * 72) / cell);
  const cols = c.a_ft > 0 ? Math.ceil(w / tw) : 0, rows = c.a_ft > 0 ? Math.ceil(h / th) : 0;
  const rects: [number, number, number, number][] = [];
  for (let r = 0; r < rows; r++) for (let q = 0; q < cols; q++) rects.push([q * tw, r * th, (q + 1) * tw, (r + 1) * th]);
  return {
    footprint_ft: [bw, bh], inches, image_w: w, image_h: h, cell_px: cell, pages: rects.length, cols, rows,
    page_rects: rects, label: aoeLabel(c.shape, c.a_ft, c.b_ft),
    summary: c.a_ft > 0
      ? `Footprint: ${bw} × ${bh} ft  ·  ${inches[0].toFixed(1)}" × ${inches[1].toFixed(1)}" on paper  ·  ${rects.length} page(s) (${cols}×${rows})`
      : 'Set a size greater than 0.',
  };
}

function aoeSvg(c: AoeConfig): string {
  const [w, h, pps] = aoeSize(c);
  const ppf = pps / c.feet_per_square, o = c.line_width + 4;
  const rgba = (rgb: number[], a: number) => `rgba(${rgb.join(',')},${a / 255})`;
  const fill = rgba(c.fill_rgb, c.fill_alpha), line = `rgb(${c.line_rgb.join(',')})`;
  let shape: string;
  if (c.shape === 'circle') {
    const r = c.a_ft * ppf;
    shape = `<circle cx="${o + r}" cy="${o + r}" r="${r}" fill="${fill}" stroke="${line}" stroke-width="${c.line_width}"/>`;
  } else {
    const L = c.a_ft * ppf, W = c.shape === 'line' ? Math.max(c.b_ft, 0.01) * ppf : L;
    const pts = c.shape === 'cone'
      ? [[o, o + L / 2], [o + L, o], [o + L, o + L]]
      : [[o, o], [o + L, o], [o + L, o + W], [o, o + W]];
    shape = `<polygon points="${pts.map((p) => p.join(',')).join(' ')}" fill="${fill}" stroke="${line}" stroke-width="${c.line_width}"/>`;
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">
    <rect width="${w}" height="${h}" fill="#fff"/>${shape}
    ${c.label_on && c.label ? `<text x="${o}" y="${o + 20}" font-size="${Math.max(12, 0.26 * pps)}" fill="${line}">${c.label}</text>` : ''}</svg>`;
}

export function createMapsMockHandlers(emit: Emit): Record<string, Handler> {
  let seq = 0;
  let doc: MapDoc | null = null;
  const presets: AoePreset[] = [];
  const sheet: AoeSheet = { items: [], gap_in: 0.2 };
  const job = (kind: string, result: unknown) => {
    const id = `mock-job-maps-${++seq}`;
    setTimeout(() => emit({ type: 'job', id, kind, state: 'done', progress: 1, message: '', result, error: null }), 10);
    return id;
  };
  const openSource = (hash: string, width: number, height: number, name: string) => {
    doc = { ...defaultMapDoc(), source: { hash, width, height }, name };
    return doc;
  };
  const sheetLayout = (inch: number, format: PageFormat): AoeSheetLayout => {
    const dpi = 150;
    const [pw, ph] = format === 'A3' ? [1754, 2480] : format === 'A4' ? [1240, 1754] : [1275, 1650];
    const m = 30, gap = Math.round(sheet.gap_in * dpi);
    const placed: AoeSheetLayout['placed'] = [];
    const skipped: string[] = [];
    let x = m, y = m, rowH = 0, page = 0;
    for (const it of sheet.items) {
      const c = { ...it.config, inch, page_format: format };
      const [w0, h0, cell] = aoeSize(c);
      const w = Math.round((w0 / cell) * inch * dpi), h = Math.round((h0 / cell) * inch * dpi);
      if (w > pw - 2 * m || h > ph - 2 * m) { skipped.push(it.config.name || it.config.label); continue; }
      if (x + w > pw - m) { x = m; y += rowH + gap; rowH = 0; }
      if (y + h > ph - m) { page++; x = m; y = m; rowH = 0; }
      placed.push({ id: it.id, page, x, y, w, h });
      x += w + gap; rowH = Math.max(rowH, h);
    }
    const pages = placed.length ? page + 1 : 0;
    const summary = !sheet.items.length
      ? "Sheet is empty — design a template and click 'Add current to sheet'."
      : !pages
        ? `None of these templates fit on a single sheet: ${skipped.join(', ')}`
        : `Sheet: ${sheet.items.length} template(s) · ${pages} page(s)${skipped.length ? `  ·  too big for one sheet: ${skipped.join(', ')}` : ''}`;
    return { page_w: pw, page_h: ph, pages, placed, skipped, summary };
  };

  return {
    map_get: () => doc,
    map_set: ({ doc: d }) => (doc = d as MapDoc),
    map_open_file: ({ path }) => {
      const name = String(path).split(/[\\/]/).pop()?.replace(/\.[^.]+$/, '') ?? 'Map';
      return openSource(`mock-map-${++seq}`, 2100, 1400, name);
    },
    map_open_source: ({ hash, name }) => openSource(String(hash), 2100, 1400, (name as string) ?? ''),
    map_open_asset: ({ id }) => ({ ...openSource(`mock-map-${id}`, 2100, 1400, `Map ${id}`), asset_id: id as number }),
    map_preview: ({ doc: d }) => {
      const l = mockMapLayout(d as MapDoc);
      return mapSvg(l.proxy_w, l.proxy_h, (d as MapDoc).name || 'Map');
    },
    map_layout: ({ doc: d }) => mockMapLayout(d as MapDoc),
    map_preview_page: ({ doc: d, index, pxPerPt }) => {
      const l = mockMapLayout(d as MapDoc);
      const k = (pxPerPt as number) ?? 1;
      if (!l.tiles[index as number]) throw { code: 'invalid_input', message: 'invalid input: no such page' };
      return mapSvg(Math.round(l.page_w_pt * k), Math.round(l.page_h_pt * k), `Page ${(index as number) + 1}`);
    },
    map_dewarp: ({ doc: d }) => {
      const m = d as MapDoc;
      if (!m.ref_quad) throw { code: 'invalid_input', message: 'invalid input: draw a reference square over a known block first' };
      const [cw] = baseGrid(m);
      doc = { ...m, pre_warp: m.source, source: { ...m.source!, hash: `mock-warp-${++seq}` }, side: Math.max(2, Math.round(cw)), ref_quad: null, region: null, excluded: [], page_offset: [0, 0] };
      return doc;
    },
    // simplified: the mock maps carry a 70 px grid and a 5 % border
    map_detect_grid: ({ doc: d }) =>
      (doc = { ...(d as MapDoc), mode: 'cell_side', side: 70, offset_x: 0, offset_y: 0, excluded: [] }),
    map_fit_region: ({ doc: d }) => {
      const cur = d as MapDoc;
      const [w, h] = cur.source ? [cur.source.width, cur.source.height] : [0, 0];
      const c = cur.side || 70;
      const x0 = Math.round((w * 0.05) / c) * c, y0 = Math.round((h * 0.05) / c) * c;
      const x1 = Math.round((w * 0.95) / c) * c, y1 = Math.round((h * 0.95) / c) * c;
      return (doc = { ...cur, region: [x0, y0, x1 - x0, y1 - y0], excluded: [] });
    },
    map_fewest_pages: ({ doc: d }) => {
      const cur = d as MapDoc;
      const f = mockMapLayout(cur).fewest;
      return (doc = f ? { ...cur, landscape: f.landscape, page_offset: f.offset, excluded: [] } : cur);
    },
    map_undo_dewarp: ({ doc: d }) => {
      const m = d as MapDoc;
      if (!m.pre_warp) throw { code: 'invalid_input', message: 'invalid input: nothing to undo' };
      doc = { ...m, source: m.pre_warp, pre_warp: null, ref_quad: null, region: null, excluded: [], page_offset: [0, 0], offset_x: 0, offset_y: 0 };
      return doc;
    },
    map_export_pdf: ({ doc: d }) => {
      const l = mockMapLayout(d as MapDoc);
      if (!l.selected) throw { code: 'invalid_input', message: 'invalid input: no pages selected — include at least one page' };
      return job('map export', { pages: l.selected });
    },
    map_save_asset: ({ name }) => job('map save', { id: 9000 + seq, name, kind: 'map' }),
    map_filter_catalog: () => MOCK_FILTER_CATALOG,

    aoe_presets_list: () => [
      ...AOE_PRESETS.map(([name, shape, a, b]) => ({
        name, builtin: true, config: { ...defaultAoeConfig(), shape, a_ft: a, b_ft: b, label: aoeLabel(shape, a, b) },
      })),
      ...[...presets].sort((x, y) => x.name.localeCompare(y.name)),
    ],
    aoe_presets_save: ({ name, config, overwrite }) => {
      const n = String(name).trim();
      if (!n) throw { code: 'invalid_input', message: 'invalid input: preset name must not be empty' };
      const i = presets.findIndex((p) => p.name === n);
      if (i >= 0 && !overwrite) throw { code: 'invalid_input', message: `invalid input: a preset named '${n}' exists; overwrite it?` };
      const p = { name: n, builtin: false, config: { ...(config as AoeConfig), name: n } };
      if (i >= 0) presets[i] = p; else presets.push(p);
      return p;
    },
    aoe_presets_delete: ({ name }) => {
      const i = presets.findIndex((p) => p.name === name);
      if (i < 0) throw { code: 'not_found', message: `not found: preset ${name}` };
      presets.splice(i, 1);
    },
    aoe_render: ({ config }) => {
      const c = config as AoeConfig;
      if (!(c.a_ft > 0)) throw { code: 'invalid_input', message: 'invalid input: set a template size first' };
      return aoeSvg(c);
    },
    aoe_info: ({ config }) => aoeInfo(config as AoeConfig),
    aoe_export_png: ({ path }) => path,
    aoe_export_template_pdf: ({ config }) => job('aoe template export', { pages: aoeInfo(config as AoeConfig).pages }),
    aoe_sheet_get: () => sheet,
    aoe_sheet_add: ({ config }) => {
      const c = config as AoeConfig;
      if (!(c.a_ft > 0)) throw { code: 'invalid_input', message: 'invalid input: set a template size first' };
      sheet.items.push({ id: `aoe-${++seq}`, config: { ...c, name: c.name || c.label || aoeLabel(c.shape, c.a_ft, c.b_ft) } });
      return sheet;
    },
    aoe_sheet_remove: ({ ids }) => {
      sheet.items = sheet.items.filter((i) => !(ids as string[]).includes(i.id));
      return sheet;
    },
    aoe_sheet_clear: () => {
      sheet.items = [];
      return sheet;
    },
    aoe_sheet_set_gap: ({ gapIn }) => {
      sheet.gap_in = gapIn as number;
      return sheet;
    },
    aoe_sheet_layout: ({ inch, format }) => sheetLayout(inch as number, format as PageFormat),
    aoe_sheet_preview: ({ inch, format, page }) => {
      const l = sheetLayout(inch as number, format as PageFormat);
      const items = l.placed.filter((p) => p.page === page)
        .map((p) => `<rect x="${p.x}" y="${p.y}" width="${p.w}" height="${p.h}" fill="rgba(220,70,40,.3)" stroke="#962"/>`).join('');
      return `<svg xmlns="http://www.w3.org/2000/svg" width="${l.page_w}" height="${l.page_h}"><rect width="${l.page_w}" height="${l.page_h}" fill="#fff"/>${items}</svg>`;
    },
    aoe_sheet_export_pdf: ({ inch, format }) => {
      if (!sheet.items.length) throw { code: 'invalid_input', message: 'invalid input: add at least one template to the sheet' };
      const l = sheetLayout(inch as number, format as PageFormat);
      return job('aoe sheet export', { pages: l.pages, skipped: l.skipped });
    },
  };
}
