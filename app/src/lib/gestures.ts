// Canvas gestures shared by the map paginator and the cutters (docs/ux.md,
// "Canvas gestures").
//
// Chromium / WebView2 delivers touchpad gestures as wheel events:
// - pinch      → wheel with ctrlKey, small fractional deltaY (zoom);
// - two-finger → wheel with deltaX/deltaY in pixels (pan);
// - a mouse wheel notch → wheel with wheelDeltaY a multiple of 120 (zoom).
// Touchscreens send pointer events; `PinchTracker` turns two fingers into
// zoom + pan.

export type WheelIntent =
  | { kind: 'zoom'; factor: number }
  | { kind: 'pan'; dx: number; dy: number };

const LINE_PX = 16;

/** Pixel deltas whatever the `deltaMode`. */
function pixels(e: WheelEvent): [number, number] {
  const k = e.deltaMode === 1 ? LINE_PX : e.deltaMode === 2 ? 800 : 1;
  return [e.deltaX * k, e.deltaY * k];
}

/** True for a mouse wheel notch, false for a touchpad scroll. */
export function isMouseWheel(e: WheelEvent): boolean {
  if (e.deltaMode !== 0) return true; // lines/pages: only wheels do that
  if (e.deltaX !== 0) return false; // wheels scroll one axis
  const legacy = (e as WheelEvent & { wheelDeltaY?: number }).wheelDeltaY;
  if (typeof legacy === 'number' && legacy !== 0) return legacy % 120 === 0;
  return Math.abs(e.deltaY) >= 50 && Number.isInteger(e.deltaY);
}

/**
 * What a wheel event means on a zoomable canvas: Ctrl/⌘ + wheel and touchpad
 * pinches zoom in proportion to the gesture; mouse wheel notches zoom one
 * step; touchpad scrolling pans (`dx`, `dy` are how far the content moves).
 */
export function wheelIntent(e: WheelEvent, step = 1.1): WheelIntent {
  const [dx, dy] = pixels(e);
  if (e.ctrlKey || e.metaKey) {
    // pinch deltas are small and continuous; a Ctrl+wheel notch is ~100
    const d = Math.max(-25, Math.min(25, dy));
    return { kind: 'zoom', factor: Math.exp(-d * 0.01) };
  }
  if (isMouseWheel(e)) {
    const notches = Math.max(1, Math.round(Math.abs(dy) / 100));
    return { kind: 'zoom', factor: dy < 0 ? step ** notches : step ** -notches };
  }
  // Shift + vertical scroll pans sideways, as in browsers
  return e.shiftKey && dx === 0 ? { kind: 'pan', dx: -dy, dy: 0 } : { kind: 'pan', dx: -dx, dy: -dy };
}

export interface PinchStep {
  /** Zoom factor since the last step. */
  factor: number;
  /** Midpoint of the two fingers (client px). */
  cx: number;
  cy: number;
  /** How far the midpoint moved since the last step (client px). */
  dx: number;
  dy: number;
}

/** Two-finger pinch/pan from touch pointer events. */
export class PinchTracker {
  #pts = new Map<number, { x: number; y: number }>();
  #last: { d: number; cx: number; cy: number } | null = null;

  /** Whether two touches are down (the caller should not start a tool drag). */
  get active(): boolean {
    return this.#pts.size >= 2;
  }

  down(e: PointerEvent): boolean {
    if (e.pointerType !== 'touch') return false;
    this.#pts.set(e.pointerId, { x: e.clientX, y: e.clientY });
    this.#last = this.#measure();
    return this.active;
  }

  /** A step while two fingers move, else `null`. */
  move(e: PointerEvent): PinchStep | null {
    if (!this.#pts.has(e.pointerId)) return null;
    this.#pts.set(e.pointerId, { x: e.clientX, y: e.clientY });
    const now = this.#measure();
    const last = this.#last;
    this.#last = now;
    if (!now || !last || last.d === 0) return null;
    return { factor: now.d / last.d, cx: now.cx, cy: now.cy, dx: now.cx - last.cx, dy: now.cy - last.cy };
  }

  up(e: PointerEvent): void {
    this.#pts.delete(e.pointerId);
    this.#last = this.#measure();
  }

  #measure() {
    if (this.#pts.size < 2) return null;
    const [a, b] = [...this.#pts.values()];
    return { d: Math.hypot(a.x - b.x, a.y - b.y), cx: (a.x + b.x) / 2, cy: (a.y + b.y) / 2 };
  }
}
