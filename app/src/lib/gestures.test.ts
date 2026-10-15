import { describe, expect, it } from 'vitest';
import { PinchTracker, isMouseWheel, wheelIntent } from './gestures';

const wheel = (init: WheelEventInit & { wheelDeltaY?: number }) => {
  const e = new WheelEvent('wheel', init);
  if (init.wheelDeltaY !== undefined) Object.defineProperty(e, 'wheelDeltaY', { value: init.wheelDeltaY });
  return e;
};

describe('wheelIntent', () => {
  it('touchpad pinch (ctrl + small deltas) zooms smoothly', () => {
    const g = wheelIntent(wheel({ deltaY: -3.5, ctrlKey: true, wheelDeltaY: 4 }));
    expect(g.kind).toBe('zoom');
    if (g.kind === 'zoom') expect(g.factor).toBeCloseTo(Math.exp(0.035), 6);
    const out = wheelIntent(wheel({ deltaY: 2, ctrlKey: true }));
    expect(out.kind === 'zoom' && out.factor < 1).toBe(true);
  });

  it('ctrl + mouse wheel zooms a bounded step', () => {
    const g = wheelIntent(wheel({ deltaY: -100, ctrlKey: true, wheelDeltaY: 120 }));
    expect(g.kind === 'zoom' && g.factor).toBeCloseTo(Math.exp(0.25), 6);
  });

  it('a mouse wheel notch zooms one step, two notches two steps', () => {
    expect(wheelIntent(wheel({ deltaY: -100, wheelDeltaY: 120 }))).toEqual({ kind: 'zoom', factor: 1.1 });
    const g = wheelIntent(wheel({ deltaY: 200, wheelDeltaY: -240 }));
    expect(g.kind === 'zoom' && g.factor).toBeCloseTo(1.1 ** -2, 9);
    // Firefox-style line deltas are wheels too
    expect(isMouseWheel(wheel({ deltaY: 3, deltaMode: 1 }))).toBe(true);
  });

  it('two-finger touchpad scroll pans the content with the fingers', () => {
    expect(wheelIntent(wheel({ deltaX: 4, deltaY: -7, wheelDeltaY: 8 }))).toEqual({ kind: 'pan', dx: -4, dy: 7 });
    // vertical-only touchpad scroll is still a pan (not a 120 multiple)
    expect(wheelIntent(wheel({ deltaY: 12, wheelDeltaY: -14 })).kind).toBe('pan');
    // shift + scroll pans sideways
    expect(wheelIntent(wheel({ deltaY: 10, shiftKey: true, wheelDeltaY: -12 }))).toEqual({ kind: 'pan', dx: -10, dy: 0 });
  });
});

describe('PinchTracker', () => {
  const touch = (id: number, x: number, y: number) =>
    ({ pointerId: id, pointerType: 'touch', clientX: x, clientY: y }) as PointerEvent;

  it('two fingers spreading zoom in around their midpoint', () => {
    const p = new PinchTracker();
    expect(p.down(touch(1, 100, 100))).toBe(false);
    expect(p.down(touch(2, 200, 100))).toBe(true);
    const st = p.move(touch(2, 300, 100))!;
    expect(st.factor).toBeCloseTo(2);
    expect([st.cx, st.cy, st.dx, st.dy]).toEqual([200, 100, 50, 0]);
    p.up(touch(2, 300, 100));
    expect(p.active).toBe(false);
    expect(p.move(touch(1, 120, 100))).toBeNull();
  });

  it('ignores mouse and pen pointers', () => {
    const p = new PinchTracker();
    expect(p.down({ pointerId: 1, pointerType: 'mouse', clientX: 0, clientY: 0 } as PointerEvent)).toBe(false);
    expect(p.active).toBe(false);
  });
});
