//! Object sheet shelf packer. Python reference: `cutout_sheet_generator.py`
//! (`_resolve_item`, `_layout`).

use serde::{Deserialize, Serialize};

use super::{Layout, Placement};
use crate::units::PageFormat;

pub const MARGIN_PT: f64 = 18.0;
pub const GAP_PT: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SheetMode {
    #[default]
    Flat,
    Standee,
}

/// A cut-out to place: its pixel size and print settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShelfItem {
    pub w_px: u32,
    pub h_px: u32,
    /// Longest side in points before scaling (size label points).
    pub size_pt: f64,
    /// Percentage (10–400); 0 is treated as 100 like Python.
    pub scale_pct: f64,
    pub copies: u32,
}

impl ShelfItem {
    /// Printed `(w, h)` in points. Standees are twice as tall (mirrored copy).
    pub fn printed(&self, mode: SheetMode) -> (f64, f64) {
        let scale = if self.scale_pct == 0.0 { 100.0 } else { self.scale_pct };
        let longest = self.size_pt * scale / 100.0;
        let s = longest / f64::from(self.w_px.max(self.h_px).max(1));
        let w = f64::from(self.w_px) * s;
        let h = match mode {
            SheetMode::Flat => f64::from(self.h_px) * s,
            SheetMode::Standee => f64::from(2 * self.h_px) * s,
        };
        (w, h)
    }
}

pub fn pack(items: &[ShelfItem], format: PageFormat, mode: SheetMode) -> Layout {
    let (pw, ph) = format.size_pt();
    let mut rects: Vec<(usize, f64, f64)> = Vec::new();
    for (i, it) in items.iter().enumerate() {
        let (w, h) = it.printed(mode);
        for _ in 0..it.copies {
            rects.push((i, w, h));
        }
    }
    // Tallest first (stable, like Python `sort(key=h, reverse=True)`).
    rects.sort_by(|a, b| b.2.total_cmp(&a.2));

    let (left, right) = (MARGIN_PT, pw - MARGIN_PT);
    let (top0, bottom) = (MARGIN_PT, ph - MARGIN_PT);
    let (avail_w, avail_h) = (right - left, bottom - top0);

    let mut layout = Layout { page_w: pw, page_h: ph, pages: vec![], skipped: vec![] };
    if rects.is_empty() {
        return layout;
    }
    layout.pages.push(vec![]);
    let (mut x, mut top, mut shelf_h) = (left, top0, 0.0f64);
    for (idx, w, h) in rects {
        if w > avail_w || h > avail_h {
            if !layout.skipped.contains(&idx) {
                layout.skipped.push(idx);
            }
            continue;
        }
        if x + w > right {
            x = left;
            top += shelf_h + GAP_PT;
            shelf_h = 0.0;
        }
        if top + h > bottom {
            layout.pages.push(vec![]);
            x = left;
            top = top0;
            shelf_h = 0.0;
        }
        layout.pages.last_mut().expect("page").push(Placement {
            item: idx,
            x,
            y: ph - top - h,
            w,
            h,
        });
        x += w + GAP_PT;
        shelf_h = shelf_h.max(h);
    }
    if layout.pages.iter().all(Vec::is_empty) {
        layout.pages.clear();
    }
    layout
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standee_doubles_height_and_scale_applies_to_longest_side() {
        let it = ShelfItem { w_px: 100, h_px: 200, size_pt: 72.0, scale_pct: 150.0, copies: 1 };
        assert_eq!(it.printed(SheetMode::Flat), (54.0, 108.0));
        assert_eq!(it.printed(SheetMode::Standee), (54.0, 216.0));
        let zero = ShelfItem { scale_pct: 0.0, ..it };
        assert_eq!(zero.printed(SheetMode::Flat), (36.0, 72.0), "0% behaves like 100%");
    }

    #[test]
    fn oversized_reported_and_shelves_wrap() {
        let big = ShelfItem { w_px: 10, h_px: 10, size_pt: 2000.0, scale_pct: 100.0, copies: 3 };
        let small = ShelfItem { w_px: 10, h_px: 10, size_pt: 144.0, scale_pct: 100.0, copies: 5 };
        let l = pack(&[big, small], PageFormat::Letter, SheetMode::Flat);
        assert_eq!(l.skipped, vec![0]);
        // 576pt usable: 3 per shelf (144+8+144+8+144=448 → 4th at 608 > 594? 18+4*152=626 > 594).
        let ys: Vec<f64> = l.pages[0].iter().map(|p| p.y).collect();
        assert_eq!(ys.iter().filter(|&&y| y == ys[0]).count(), 3);
        assert_eq!(l.pages[0].len(), 5);
    }
}
