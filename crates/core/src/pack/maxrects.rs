//! MaxRects bin packer, best-short-side-fit (Python `aoe_templates._maxrects_place`
//! and the packing loop of `compose_sheets`).
//!
//! Unlike shelf packing, leftover space next to a placed box stays available
//! to later, smaller boxes on any already-opened page.

use serde::Serialize;

/// `(x, y, w, h)` in integer pixels.
pub type Rect = (i64, i64, i64, i64);

fn overlaps(a: Rect, b: Rect) -> bool {
    !(b.0 >= a.0 + a.2 || b.0 + b.2 <= a.0 || b.1 >= a.1 + a.3 || b.1 + b.3 <= a.1)
}

fn contains(outer: Rect, inner: Rect) -> bool {
    inner.0 >= outer.0
        && inner.1 >= outer.1
        && inner.0 + inner.2 <= outer.0 + outer.2
        && inner.1 + inner.3 <= outer.1 + outer.3
}

/// Slices `free` around `placed` into up to 4 maximal leftover rects.
fn split(placed: Rect, free: Rect) -> Vec<Rect> {
    if !overlaps(placed, free) {
        return vec![free];
    }
    let (px, py, pw, ph) = placed;
    let (fx, fy, fw, fh) = free;
    let mut out = Vec::with_capacity(4);
    if px > fx {
        out.push((fx, fy, px - fx, fh));
    }
    if px + pw < fx + fw {
        out.push((px + pw, fy, (fx + fw) - (px + pw), fh));
    }
    if py > fy {
        out.push((fx, fy, fw, py - fy));
    }
    if py + ph < fy + fh {
        out.push((fx, py + ph, fw, (fy + fh) - (py + ph)));
    }
    out.retain(|r| r.2 > 0 && r.3 > 0);
    out
}

/// Drops duplicates (keeping first occurrence order) and rects contained in another.
fn prune(rects: Vec<Rect>) -> Vec<Rect> {
    let mut uniq: Vec<Rect> = Vec::with_capacity(rects.len());
    for r in rects {
        if !uniq.contains(&r) {
            uniq.push(r);
        }
    }
    uniq.iter()
        .enumerate()
        .filter(|&(i, a)| !uniq.iter().enumerate().any(|(j, b)| j != i && contains(*b, *a)))
        .map(|(_, a)| *a)
        .collect()
}

/// Best-short-side-fit placement of a `w`×`h` box. Returns the position and
/// the new free list, or `None` when it fits nowhere.
pub fn place(free: &[Rect], w: i64, h: i64) -> Option<((i64, i64), Vec<Rect>)> {
    let mut best: Option<(usize, i64, i64)> = None;
    for (i, &(_, _, fw, fh)) in free.iter().enumerate() {
        if fw >= w && fh >= h {
            let short = (fw - w).min(fh - h);
            let long = (fw - w).max(fh - h);
            if best.is_none_or(|(_, bs, bl)| short < bs || (short == bs && long < bl)) {
                best = Some((i, short, long));
            }
        }
    }
    let (i, _, _) = best?;
    let pos = (free[i].0, free[i].1);
    let placed = (pos.0, pos.1, w, h);
    let new_free = free.iter().flat_map(|&f| split(placed, f)).collect();
    Some((pos, prune(new_free)))
}

/// A box to pack: its real size and its gap-padded footprint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackItem {
    pub w: i64,
    pub h: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Placed {
    /// Index into the input list.
    pub item: usize,
    pub page: usize,
    /// Top-left on the page (margin included), px.
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SheetLayout {
    pub page_w: i64,
    pub page_h: i64,
    pub pages: usize,
    pub placed: Vec<Placed>,
    /// Items larger than one page's usable area (input indices).
    pub skipped: Vec<usize>,
}

/// Packs items onto pages of `page_w`×`page_h` with `margin` and `gap` px:
/// oversized items are skipped, the rest go largest-area first, trying every
/// open page before opening a new one.
pub fn pack(items: &[PackItem], page_w: i64, page_h: i64, margin: i64, gap: i64) -> SheetLayout {
    let (uw, uh) = (page_w - 2 * margin, page_h - 2 * margin);
    let mut boxes = Vec::new();
    let mut skipped = Vec::new();
    for (i, it) in items.iter().enumerate() {
        if it.w > uw || it.h > uh {
            skipped.push(i);
            continue;
        }
        boxes.push((i, it.w, it.h, (it.w + gap).min(uw), (it.h + gap).min(uh)));
    }
    // stable, largest area first (Python sorted(..., reverse=True) keeps ties in order)
    boxes.sort_by_key(|b| std::cmp::Reverse(b.1 * b.2));
    let mut pages: Vec<Vec<Rect>> = Vec::new();
    let mut placed = Vec::new();
    for (i, w, h, pw, ph) in boxes {
        let mut spot =
            pages.iter().enumerate().find_map(|(pi, free)| place(free, pw, ph).map(|r| (pi, r)));
        if spot.is_none() {
            pages.push(vec![(0, 0, uw, uh)]);
            let pi = pages.len() - 1;
            spot = place(&pages[pi], pw, ph).map(|r| (pi, r));
        }
        let Some((pi, (pos, free))) = spot else { continue };
        pages[pi] = free;
        placed.push(Placed { item: i, page: pi, x: margin + pos.0, y: margin + pos.1, w, h });
    }
    SheetLayout { page_w, page_h, pages: pages.len(), placed, skipped }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leftover_space_is_reused() {
        let l = pack(&[PackItem { w: 600, h: 600 }, PackItem { w: 100, h: 100 }], 1000, 700, 0, 0);
        assert_eq!(l.pages, 1);
        // best short side: the 1000×100 strip below (slack 0) beats the 400×700 one
        assert_eq!((l.placed[1].x, l.placed[1].y), (0, 600));
    }

    #[test]
    fn oversized_items_are_reported() {
        let l = pack(&[PackItem { w: 2000, h: 10 }], 1000, 1000, 10, 5);
        assert_eq!(l.skipped, vec![0]);
        assert_eq!(l.pages, 0);
    }
}
