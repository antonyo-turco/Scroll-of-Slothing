import { describe, expect, it } from 'vitest';
import { moveQuadEdge, orderQuad, quadEdges, quadOrder } from './geometry';

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
