import { describe, expect, it } from 'vitest';
import {
  HANDLES,
  handlePoint,
  moveQuadEdge,
  orderQuad,
  quadEdges,
  quadOrder,
  resizeRect,
  snapLine,
  snapQuarter,
  type Rect,
} from './geometry';

type Pt = [number, number];
// Created clockwise from top-left, like the Reference tool does.
const rect: Pt[] = [[10, 10], [110, 10], [110, 60], [10, 60]];

describe('reference quad', () => {
  it('orders corners TL, TR, BR, BL whatever the stored order', () => {
    const shuffled: Pt[] = [rect[2], rect[0], rect[3], rect[1]];
    expect(orderQuad(shuffled)).toEqual(rect);
    expect(quadOrder(shuffled)).toEqual([1, 3, 0, 2]);
  });

  it('lists the sides top, right, bottom, left', () => {
    expect(quadEdges(rect)).toEqual([[0, 1], [1, 2], [2, 3], [3, 0]]);
  });

  it('moving a side keeps a rectangle a rectangle', () => {
    // Drag the right side 20 px right; the vertical part of the drag is ignored.
    const moved = moveQuadEdge(rect, 1, 2, 20, 35);
    expect(moved).toEqual([[10, 10], [130, 10], [130, 60], [10, 60]]);
    // Drag the top side up by 5.
    expect(moveQuadEdge(rect, 0, 1, 7, -5)).toEqual([[10, 5], [110, 5], [110, 60], [10, 60]]);
  });

  it('moving a side of a skewed quad keeps that side parallel', () => {
    const skew: Pt[] = [[0, 0], [100, 10], [100, 60], [0, 50]];
    const [a, b] = [0, 1];
    const m = moveQuadEdge(skew, a, b, 0, -20);
    const dir = (q: Pt[]) => Math.atan2(q[b][1] - q[a][1], q[b][0] - q[a][0]);
    expect(dir(m)).toBeCloseTo(dir(skew), 10);
    expect(m[2]).toEqual(skew[2]);
    expect(m[3]).toEqual(skew[3]);
  });
});

describe('print region', () => {
  const r: Rect = [100, 50, 200, 80];

  it('has 8 handles on the corners and edge midpoints', () => {
    expect(HANDLES).toHaveLength(8);
    expect(HANDLES.map((h) => handlePoint(r, h))).toEqual([
      [100, 50], [200, 50], [300, 50], [300, 90], [300, 130], [200, 130], [100, 130], [100, 90],
    ]);
  });

  it('a corner handle moves two sides, an edge handle one', () => {
    expect(resizeRect(r, 'se', 350, 200)).toEqual([100, 50, 250, 150]);
    expect(resizeRect(r, 'n', 999, 20)).toEqual([100, 20, 200, 110]);
    expect(resizeRect(r, 'w', 120, 999)).toEqual([120, 50, 180, 80]);
  });

  it('dragging a handle past the opposite side flips the rect instead of going negative', () => {
    expect(resizeRect(r, 'e', 40, 0)).toEqual([40, 50, 60, 80]);
  });

  it('snaps edges to the nearest grid line', () => {
    expect(snapLine(37, 5, 20)).toBe(45);
    expect(snapLine(34, 5, 20)).toBe(25);
    expect(snapLine(-12, 5, 20)).toBe(-15);
    expect(snapLine(37, 5, 0)).toBe(37); // no grid: no snap
  });
});

describe('layout drag', () => {
  it('snaps the page-grid offset to quarter pages within the tolerance', () => {
    expect(snapQuarter(52, 200, 5)).toBe(50);
    expect(snapQuarter(103, 200, 5)).toBe(100);
    expect(snapQuarter(-148, 200, 5)).toBe(-150);
    expect(snapQuarter(197, 200, 5)).toBe(200);
    expect(snapQuarter(70, 200, 5)).toBe(70); // between quarters: free
    expect(snapQuarter(52, 0, 5)).toBe(52);
  });
});
