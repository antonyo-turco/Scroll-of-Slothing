//! Tokens and cut-outs packed together on the same pages (the Token Sheet once
//! it holds both). No Python counterpart: the old app printed them apart.
//!
//! Every copy is a box of its printed size; the boxes go through MaxRects
//! (largest first, leftover space reused), so small tokens fill the gaps
//! beside tall standees.

use serde::{Deserialize, Serialize};

use super::maxrects::{self, PackItem};
use super::{Layout, Placement};
use crate::units::PageFormat;

/// Page margin, like the cut-out sheet.
pub const MARGIN_PT: f64 = 18.0;
/// Space between boxes, to cut along.
pub const GAP_PT: f64 = 6.0;
/// MaxRects works in integers: hundredths of a point.
const UNIT: f64 = 100.0;

/// One item of the sheet: its printed box and how many copies.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MixedItem {
    pub w: f64,
    pub h: f64,
    pub copies: u32,
}

pub fn pack(items: &[MixedItem], format: PageFormat) -> Layout {
    let (pw, ph) = format.size_pt();
    let to = |v: f64| (v * UNIT).round() as i64;
    let mut boxes = Vec::new();
    let mut owner = Vec::new();
    for (i, it) in items.iter().enumerate() {
        for _ in 0..it.copies {
            boxes.push(PackItem { w: to(it.w), h: to(it.h) });
            owner.push(i);
        }
    }
    let l = maxrects::pack(&boxes, to(pw), to(ph), to(MARGIN_PT), to(GAP_PT));
    let mut pages: Vec<Vec<Placement>> = vec![Vec::new(); l.pages];
    for p in &l.placed {
        let (w, h) = (items[owner[p.item]].w, items[owner[p.item]].h);
        let (x, y_top) = (p.x as f64 / UNIT, p.y as f64 / UNIT);
        // MaxRects is top-left; layouts are bottom-left (like ReportLab).
        pages[p.page].push(Placement { item: owner[p.item], x, y: ph - y_top - h, w, h });
    }
    let mut skipped: Vec<usize> = l.skipped.iter().map(|&b| owner[b]).collect();
    skipped.dedup();
    Layout { page_w: pw, page_h: ph, pages, skipped }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlap(a: &Placement, b: &Placement) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn tokens_and_cutouts_share_a_page_without_overlap() {
        let items = [
            MixedItem { w: 72.0, h: 72.0, copies: 6 },
            MixedItem { w: 100.0, h: 300.0, copies: 2 },
        ];
        let l = pack(&items, PageFormat::Letter);
        assert_eq!(l.pages.len(), 1);
        let page = &l.pages[0];
        assert_eq!(page.len(), 8);
        assert!(page.iter().any(|p| p.item == 0) && page.iter().any(|p| p.item == 1));
        for (i, a) in page.iter().enumerate() {
            assert!(a.x >= MARGIN_PT - 1e-6 && a.x + a.w <= l.page_w - MARGIN_PT + 1e-6);
            assert!(a.y >= MARGIN_PT - 1e-6 && a.y + a.h <= l.page_h - MARGIN_PT + 1e-6);
            for b in &page[i + 1..] {
                assert!(!overlap(a, b), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn oversized_items_are_reported_once() {
        let items = [
            MixedItem { w: 2000.0, h: 10.0, copies: 3 },
            MixedItem { w: 36.0, h: 36.0, copies: 1 },
        ];
        let l = pack(&items, PageFormat::A4);
        assert_eq!(l.skipped, vec![0]);
        assert_eq!(l.pages.iter().map(Vec::len).sum::<usize>(), 1);
    }
}
