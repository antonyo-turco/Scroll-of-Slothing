import { describe, expect, it } from 'vitest';
import { TOKEN_SHAPES, fitRadius, shapePath, shapeVertices } from './token-shapes';

describe('token shapes (mirror of sleuth_core::token::TokenShape)', () => {
  it('every shape touches a square canvas', () => {
    for (const { id } of TOKEN_SHAPES) {
      const r = fitRadius(id, 100, 100);
      const v = shapeVertices(id, 50, 50, r);
      if (id === 'circle') {
        expect(r).toBe(50);
        continue;
      }
      const xs = v.map(([x]) => x);
      const ys = v.map(([, y]) => y);
      const extent = Math.max(Math.max(...xs) - Math.min(...xs), Math.max(...ys) - Math.min(...ys));
      expect(extent).toBeCloseTo(100, 6);
    }
  });

  it('has the right corners and orientation', () => {
    expect(shapeVertices('octagon', 0, 0, 1)).toHaveLength(8);
    expect(shapeVertices('decagon', 0, 0, 1)).toHaveLength(10);
    // Pointy-top hexagon: a vertex straight up; decagon: flat top, vertex at the right.
    expect(shapeVertices('hex', 0, 0, 1).some(([x, y]) => Math.abs(x) < 1e-9 && Math.abs(y + 1) < 1e-9)).toBe(true);
    expect(shapeVertices('decagon', 0, 0, 1).some(([x, y]) => Math.abs(x - 1) < 1e-9 && Math.abs(y) < 1e-9)).toBe(true);
    expect(shapePath('square', 0, 0, 10, 10)).toBe('M10 10L0 10L0 0L10 0z');
  });
});
