//! Token sheet packing: square tokens on a centred cell grid, largest first,
//! first-fit within the current page. Python reference: `pdf_generator.py`
//! (`generate_preview_pages`) and `utils.find_space_for_token`.

use serde::{Deserialize, Serialize};

use super::{Layout, Placement};
use crate::units::PageFormat;

/// Page margin used to size the grid (Python `PDFGenerator.margin`).
pub const MARGIN_PT: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GridMode {
    /// Exact Python behaviour: 72 pt cells; sub-inch tokens print at 1 inch.
    Legacy,
    /// 36 pt cells; every token prints at its true size, centred in its cell block.
    HalfInch,
}

impl GridMode {
    pub fn cell_pt(self) -> f64 {
        match self {
            GridMode::Legacy => 72.0,
            GridMode::HalfInch => 36.0,
        }
    }

    /// Cells per side for a token of `size_pt`.
    fn cells(self, size_pt: f64) -> usize {
        let ratio = size_pt / self.cell_pt();
        match self {
            // Python: max(1, round(size / cell)) with round-half-to-even.
            GridMode::Legacy => (ratio.round_ties_even() as usize).max(1),
            GridMode::HalfInch => ((ratio - 1e-9).ceil() as usize).max(1),
        }
    }

    /// Drawn size of a token occupying `cells` cells.
    fn drawn(self, size_pt: f64, cells: usize) -> f64 {
        match self {
            GridMode::Legacy => cells as f64 * self.cell_pt(),
            GridMode::HalfInch => size_pt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TokenItem {
    /// Printed size in points (see `units::SizeLabel::points`).
    pub size_pt: f64,
    pub copies: u32,
}

fn find_space(grid: &[Vec<bool>], n: usize, rows: usize, cols: usize) -> Option<(usize, usize)> {
    if n > rows || n > cols {
        return None;
    }
    for r in 0..=rows - n {
        'col: for c in 0..=cols - n {
            for row in grid.iter().skip(r).take(n) {
                if row[c..c + n].iter().any(|&used| used) {
                    continue 'col;
                }
            }
            return Some((r, c));
        }
    }
    None
}

pub fn pack(items: &[TokenItem], format: PageFormat, mode: GridMode) -> Layout {
    let (pw, ph) = format.size_pt();
    let cell = mode.cell_pt();
    let cols = ((pw - 2.0 * MARGIN_PT) / cell).floor() as usize;
    let rows = ((ph - 2.0 * MARGIN_PT) / cell).floor() as usize;
    let off_x = (pw - cols as f64 * cell) / 2.0;
    let off_y = (ph - rows as f64 * cell) / 2.0;

    // Largest first; stable so equal sizes keep input order (Python `sorted`).
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| items[b].size_pt.total_cmp(&items[a].size_pt));

    let empty = || vec![vec![false; cols]; rows];
    let mut layout = Layout { page_w: pw, page_h: ph, pages: vec![], skipped: vec![] };
    if items.iter().all(|i| i.copies == 0) {
        return layout;
    }
    let mut grid = empty();
    layout.pages.push(vec![]);

    for idx in order {
        let it = items[idx];
        let n = mode.cells(it.size_pt);
        let drawn = mode.drawn(it.size_pt, n);
        let inset = (n as f64 * cell - drawn) / 2.0;
        if it.copies > 0 && (n > rows || n > cols) {
            // Can never fit: report it instead of opening empty pages (Python did).
            layout.skipped.push(idx);
            continue;
        }
        for _ in 0..it.copies {
            let spot = match find_space(&grid, n, rows, cols) {
                Some(s) => Some(s),
                None => {
                    // Python opens a new page and only ever fills the current one.
                    grid = empty();
                    layout.pages.push(vec![]);
                    find_space(&grid, n, rows, cols)
                }
            };
            let Some((r, c)) = spot else {
                if !layout.skipped.contains(&idx) {
                    layout.skipped.push(idx);
                }
                continue;
            };
            for row in grid.iter_mut().skip(r).take(n) {
                row[c..c + n].iter_mut().for_each(|u| *u = true);
            }
            layout.pages.last_mut().expect("a page exists").push(Placement {
                item: idx,
                x: off_x + c as f64 * cell + inset,
                y: ph - off_y - (r + n) as f64 * cell + inset,
                w: drawn,
                h: drawn,
            });
        }
    }
    if layout.pages.iter().all(Vec::is_empty) {
        layout.pages.clear();
    }
    layout
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::units::SizeLabel;

    fn item(l: SizeLabel, copies: u32) -> TokenItem {
        TokenItem { size_pt: l.points(), copies }
    }

    #[test]
    fn legacy_grid_dimensions_match_python() {
        // Letter: 8x10 cells of 72pt, centred → offsets 18 / 36.
        let l = pack(&[item(SizeLabel::Medium, 1)], PageFormat::Letter, GridMode::Legacy);
        let p = l.pages[0][0];
        assert_eq!((p.x, p.y, p.w), (18.0, 792.0 - 36.0 - 72.0, 72.0));
    }

    #[test]
    fn overflow_opens_new_pages() {
        let l = pack(&[item(SizeLabel::Medium, 81)], PageFormat::Letter, GridMode::Legacy);
        assert_eq!(l.pages.iter().map(Vec::len).collect::<Vec<_>>(), vec![80, 1]);
    }

    #[test]
    fn half_inch_mode_prints_true_sizes() {
        let l = pack(
            &[item(SizeLabel::Tiny, 1), item(SizeLabel::Small, 1)],
            PageFormat::A4,
            GridMode::HalfInch,
        );
        let mut sizes: Vec<f64> = l.pages[0].iter().map(|p| p.w).collect();
        sizes.sort_by(f64::total_cmp);
        assert_eq!(sizes, vec![36.0, 57.6]);
        // Small occupies a 2x2 block (72pt) and is centred in it.
        let small = l.pages[0].iter().find(|p| p.w == 57.6).unwrap();
        let (pw, _) = PageFormat::A4.size_pt();
        let off_x = (pw - ((pw - 2.0 * MARGIN_PT) / 36.0).floor() * 36.0) / 2.0;
        let cells_from_origin = (small.x - 7.2 - off_x) / 36.0;
        assert!(
            (cells_from_origin - cells_from_origin.round()).abs() < 1e-9,
            "inset (72-57.6)/2 from a cell line"
        );
    }

    #[test]
    fn half_inch_fits_more_small_tokens_than_legacy() {
        let items = [item(SizeLabel::Tiny, 300)];
        let legacy = pack(&items, PageFormat::Letter, GridMode::Legacy);
        let half = pack(&items, PageFormat::Letter, GridMode::HalfInch);
        assert!(half.pages.len() < legacy.pages.len());
    }

    #[test]
    fn oversized_items_are_reported_not_dropped() {
        let huge = TokenItem { size_pt: 72.0 * 20.0, copies: 2 };
        let l = pack(&[huge, item(SizeLabel::Medium, 1)], PageFormat::Letter, GridMode::Legacy);
        assert_eq!(l.skipped, vec![0]);
        assert_eq!(l.pages.len(), 1, "no empty pages are created for it");
        assert_eq!(l.pages[0].len(), 1);
        let only = pack(&[huge], PageFormat::Letter, GridMode::Legacy);
        assert!(only.pages.is_empty());
        assert_eq!(only.skipped, vec![0]);
    }

    #[test]
    fn empty_input_has_no_pages() {
        assert!(pack(&[], PageFormat::A4, GridMode::HalfInch).pages.is_empty());
        assert!(
            pack(&[item(SizeLabel::Huge, 0)], PageFormat::A4, GridMode::HalfInch).pages.is_empty()
        );
    }

    proptest! {
        #[test]
        fn placements_never_overlap_and_stay_on_the_page(
            spec in prop::collection::vec((0usize..6, 0u32..12), 0..8),
            a4 in any::<bool>(),
            legacy in any::<bool>(),
        ) {
            let items: Vec<TokenItem> = spec.iter().map(|&(s, c)| item(SizeLabel::ALL[s], c)).collect();
            let fmt = if a4 { PageFormat::A4 } else { PageFormat::Letter };
            let mode = if legacy { GridMode::Legacy } else { GridMode::HalfInch };
            let l = pack(&items, fmt, mode);
            let (pw, ph) = fmt.size_pt();
            let placed: usize = l.pages.iter().map(Vec::len).sum();
            let wanted: u32 = items.iter().map(|i| i.copies).sum();
            prop_assert_eq!(placed as u32, wanted, "every copy placed (all sizes fit a page)");
            prop_assert!(l.skipped.is_empty());
            for page in &l.pages {
                for (i, a) in page.iter().enumerate() {
                    prop_assert!(a.x >= MARGIN_PT - 1e-9 && a.y >= MARGIN_PT - 1e-9);
                    prop_assert!(a.x + a.w <= pw - MARGIN_PT + 1e-9 && a.y + a.h <= ph - MARGIN_PT + 1e-9);
                    for b in &page[i + 1..] {
                        let overlap = a.x < b.x + b.w - 1e-9 && b.x < a.x + a.w - 1e-9
                            && a.y < b.y + b.h - 1e-9 && b.y < a.y + a.h - 1e-9;
                        prop_assert!(!overlap, "{a:?} overlaps {b:?}");
                    }
                }
            }
        }
    }
}
