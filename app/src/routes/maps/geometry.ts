// Pure helpers for the Map Paginator canvas (all coordinates in full-res image px).
// Mirrors sleuth_core::maps::grid (line positions, dash patterns).

export type GridStyle = 'solid' | 'dashed' | 'dotted' | 'broken';
export type Rect = [number, number, number, number]; // x, y, w, h

/** Grid-line coordinates in 0..=extent (Python `_line_positions`). */
export function linePositions(offset: number, step: number, extent: number): number[] {
  const out: number[] = [];
  if (!(step >= 0.5)) return out;
  let x = offset - Math.floor(offset / step) * step;
  for (; x <= extent + 0.001; x += step) if (x >= -0.001) out.push(x);
  return out;
}

/** (on, off) dash pattern in px, or null for solid (Python `_style_pattern`). */
export function stylePattern(style: GridStyle, cell: number, thickness: number): [number, number] | null {
  const t = Math.max(1, thickness);
  switch (style) {
    case 'dashed':
      return [Math.max(2, cell * 0.18), Math.max(2, cell * 0.12)];
    case 'dotted':
      return [t, Math.max(2, cell * 0.1)];
    case 'broken':
      return [Math.max(4, cell * 0.45), Math.max(2, cell * 0.12)];
    default:
      return null;
  }
}

/** Snaps v onto the nearest grid line off + k·step. */
export function snapLine(v: number, off: number, step: number): number {
  if (!(step > 0)) return v;
  return off + Math.round((v - off) / step) * step;
}

/** Positive modulo (Python float %). */
export const pmod = (a: number, b: number): number => ((a % b) + b) % b;

/** Normalised rect from two corners. */
export function rectFrom(x0: number, y0: number, x1: number, y1: number): Rect {
  return [Math.min(x0, x1), Math.min(y0, y1), Math.abs(x1 - x0), Math.abs(y1 - y0)];
}

/** Square from an anchor towards a point (side = max of |dx|, |dy|). */
export function squareFrom(x0: number, y0: number, x1: number, y1: number): [number, number] {
  const s = Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0));
  return [x0 + Math.sign(x1 - x0 || 1) * s, y0 + Math.sign(y1 - y0 || 1) * s];
}

/** Handle ids of a rect: corners and edge midpoints. */
export const HANDLES = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'] as const;
export type Handle = (typeof HANDLES)[number];

export function handlePoint([x, y, w, h]: Rect, hd: Handle): [number, number] {
  const cx = hd.includes('w') ? x : hd.includes('e') ? x + w : x + w / 2;
  const cy = hd.includes('n') ? y : hd.includes('s') ? y + h : y + h / 2;
  return [cx, cy];
}

/** Resizes `r` by dragging handle `hd` to (px, py). */
export function resizeRect(r: Rect, hd: Handle, px: number, py: number): Rect {
  let [x0, y0, x1, y1] = [r[0], r[1], r[0] + r[2], r[1] + r[3]];
  if (hd.includes('w')) x0 = px;
  if (hd.includes('e')) x1 = px;
  if (hd.includes('n')) y0 = py;
  if (hd.includes('s')) y1 = py;
  return rectFrom(x0, y0, x1, y1);
}

/** Snaps a page-grid offset to 0, ¼, ½, ¾ or 1 page when within `tol` (image px). */
export function snapQuarter(v: number, page: number, tol: number): number {
  if (!(page > 0)) return v;
  for (let k = -4; k <= 4; k++) {
    const c = (k * page) / 4;
    if (Math.abs(v - c) <= tol) return c;
  }
  return v;
}

/** Python-style ordering of quad corners: TL, TR, BR, BL. */
export function orderQuad(q: [number, number][]): [number, number][] {
  const bySum = [...q].sort((a, b) => a[0] + a[1] - (b[0] + b[1]));
  const byDiff = [...q].sort((a, b) => a[0] - a[1] - (b[0] - b[1]));
  return [bySum[0], byDiff[3], bySum[3], byDiff[0]];
}
