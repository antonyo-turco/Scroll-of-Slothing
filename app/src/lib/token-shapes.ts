// Token outlines, mirroring crates/core/src/token.rs `TokenShape`: every shape
// is measured in radius units, scaled so its larger half-extent is 1.
import type { TokenShape } from './types';

/** Regular polygons: side count, first outward normal (degrees, y down), circumradius. */
const POLYGONS: Record<Exclude<TokenShape, 'circle'>, [number, number, number]> = {
  square: [4, 0, Math.SQRT2],
  hex: [6, 0, 1],
  octagon: [8, 0, 1 / Math.cos(Math.PI / 8)],
  decagon: [10, 18, 1],
};

export const TOKEN_SHAPES: { id: TokenShape; label: string }[] = [
  { id: 'circle', label: 'Circle' },
  { id: 'square', label: 'Square' },
  { id: 'hex', label: 'Hexagon' },
  { id: 'octagon', label: 'Octagon' },
  { id: 'decagon', label: 'Decagon' },
];

/** Corners of `shape` with radius `r` around (cx, cy); empty for a circle. */
export function shapeVertices(shape: TokenShape, cx: number, cy: number, r: number): [number, number][] {
  if (shape === 'circle') return [];
  const [n, normal, rc] = POLYGONS[shape];
  return Array.from({ length: n }, (_, k) => {
    const t = ((normal + 180 / n + (360 * k) / n) * Math.PI) / 180;
    return [cx + rc * r * Math.cos(t), cy + rc * r * Math.sin(t)];
  });
}

/** Largest radius whose shape fits a w×h box. */
export function fitRadius(shape: TokenShape, w: number, h: number): number {
  const v = shapeVertices(shape, 0, 0, 1);
  const ex = v.length ? Math.max(...v.map(([x]) => Math.abs(x))) : 1;
  const ey = v.length ? Math.max(...v.map(([, y]) => Math.abs(y))) : 1;
  return Math.min(w / 2 / ex, h / 2 / ey);
}

/** SVG path of `shape` filling a w×h box at (x, y). */
export function shapePath(shape: TokenShape, x: number, y: number, w: number, h: number): string {
  const [cx, cy, r] = [x + w / 2, y + h / 2, fitRadius(shape, w, h)];
  if (shape === 'circle') return `M${cx - r} ${cy}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0z`;
  return 'M' + shapeVertices(shape, cx, cy, r).map(([px, py]) => `${+px.toFixed(3)} ${+py.toFixed(3)}`).join('L') + 'z';
}
