//! True-physical-scale page tiling for the Map Paginator (Python
//! `map_pdf_generator.MapPDFGenerator._tiles`).
//!
//! Each grid square prints at a fixed real-world size, so a large map is tiled
//! across several pages that assemble into a life-size battlemap. The x and y
//! axes scale independently so rectangular source cells still print square.

use serde::{Deserialize, Serialize};

use crate::units::{PT_PER_INCH, PageFormat};

/// Inputs to the tiler. All pixel values are in full-resolution image space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileParams {
    pub image_w: u32,
    pub image_h: u32,
    /// `(x, y, w, h)` print region, or the whole image.
    pub region: Option<(f64, f64, f64, f64)>,
    pub cell_w: f64,
    pub cell_h: f64,
    /// Printed size of one square, inches (already calibration-corrected).
    pub square_in: f64,
    pub format: PageFormat,
    /// Page margin, points.
    pub margin_pt: f64,
    /// Page grid shift relative to the region's top-left, image px.
    pub offset: (f64, f64),
    pub overlap_in: f64,
    pub align_to_grid: bool,
    /// Grid origin in image px (used when aligning to the grid).
    pub grid_offset: (f64, f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    pub col: u32,
    pub row: u32,
    /// `(l, t, r, b)` crop of the map, clipped to the region, image px.
    pub crop: (f64, f64, f64, f64),
    /// The full page footprint, image px.
    pub page_rect: (f64, f64, f64, f64),
    /// Where the crop sits inside the page's printable area, points.
    pub page_x_pt: f64,
    pub page_y_pt: f64,
    pub w_pt: f64,
    pub h_pt: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TilePlan {
    pub scale_x: f64,
    pub scale_y: f64,
    pub tiles: Vec<Tile>,
    pub cols: u32,
    pub rows: u32,
}

/// Snaps `v` onto the nearest grid line `origin + k·period`.
fn snap_to_line(v: f64, origin: f64, period: f64) -> f64 {
    if period <= 0.0 {
        return v;
    }
    origin + ((v - origin) / period).round_ties_even() * period
}

/// Page positions `(index, left)` along one axis whose span overlaps `[lo, lo+len]`.
fn axis_pages(origin: f64, step: f64, tile: f64, lo: f64, len: f64) -> Vec<f64> {
    let hi = lo + len;
    let mut i = ((lo - origin) / step).floor() - 1.0;
    let mut out = Vec::new();
    for _ in 0..20000 {
        let left = origin + i * step;
        if left >= hi {
            break;
        }
        if left + tile > lo {
            out.push(left);
        }
        i += 1.0;
    }
    out
}

/// Computes the tiling without rendering (used by the live preview and export).
pub fn plan(p: &TileParams) -> TilePlan {
    let empty = |sx, sy| TilePlan { scale_x: sx, scale_y: sy, tiles: Vec::new(), cols: 0, rows: 0 };
    let (w, h) = (f64::from(p.image_w), f64::from(p.image_h));
    let (rx, ry, rw, rh) = p.region.unwrap_or((0.0, 0.0, w, h));
    let rx = rx.max(0.0);
    let ry = ry.max(0.0);
    let rw = rw.min(w - rx);
    let rh = rh.min(h - ry);
    if p.cell_w <= 0.0 || p.cell_h <= 0.0 || rw <= 0.0 || rh <= 0.0 {
        return empty(1.0, 1.0);
    }
    let sq_pt = p.square_in * PT_PER_INCH;
    let scale_x = sq_pt / p.cell_w;
    let scale_y = sq_pt / p.cell_h;
    let (pw, ph) = p.format.size_pt();
    let usable_w = pw - 2.0 * p.margin_pt;
    let usable_h = ph - 2.0 * p.margin_pt;
    let overlap = (p.overlap_in * PT_PER_INCH).max(0.0).min(usable_w * 0.9).min(usable_h * 0.9);

    let (tile_w, tile_h, step_w, step_h, gx, gy);
    if p.align_to_grid {
        // Pages hold whole squares so every seam falls on a grid line; the
        // overlap is rounded to whole squares too.
        let cells_w = (usable_w / sq_pt).floor().max(1.0);
        let cells_h = (usable_h / sq_pt).floor().max(1.0);
        let ov_w = (cells_w - 1.0).min((overlap / sq_pt).round_ties_even());
        let ov_h = (cells_h - 1.0).min((overlap / sq_pt).round_ties_even());
        tile_w = cells_w * p.cell_w;
        tile_h = cells_h * p.cell_h;
        step_w = (cells_w - ov_w) * p.cell_w;
        step_h = (cells_h - ov_h) * p.cell_h;
        gx = snap_to_line(rx + p.offset.0, p.grid_offset.0, p.cell_w);
        gy = snap_to_line(ry + p.offset.1, p.grid_offset.1, p.cell_h);
    } else {
        tile_w = usable_w / scale_x;
        tile_h = usable_h / scale_y;
        step_w = (usable_w - overlap) / scale_x;
        step_h = (usable_h - overlap) / scale_y;
        gx = rx + p.offset.0;
        gy = ry + p.offset.1;
    }
    if step_w <= 0.0 || step_h <= 0.0 {
        return empty(scale_x, scale_y);
    }
    let cols = axis_pages(gx, step_w, tile_w, rx, rw);
    let rows = axis_pages(gy, step_h, tile_h, ry, rh);
    let mut tiles = Vec::new();
    for (rj, &top) in rows.iter().enumerate() {
        for (ci, &left) in cols.iter().enumerate() {
            let l = rx.max(left);
            let t = ry.max(top);
            let r = (rx + rw).min(left + tile_w);
            let b = (ry + rh).min(top + tile_h);
            if r - l <= 0.0 || b - t <= 0.0 {
                continue;
            }
            tiles.push(Tile {
                col: ci as u32,
                row: rj as u32,
                crop: (l, t, r, b),
                page_rect: (left, top, left + tile_w, top + tile_h),
                page_x_pt: (l - left) * scale_x,
                page_y_pt: (t - top) * scale_y,
                w_pt: (r - l) * scale_x,
                h_pt: (b - t) * scale_y,
            });
        }
    }
    TilePlan { scale_x, scale_y, tiles, cols: cols.len() as u32, rows: rows.len() as u32 }
}

/// Calibration factor: a printed 1 in measured as `mm` millimetres.
pub fn calibration_factor(mm: f64) -> f64 {
    if mm > 0.0 { crate::units::MM_PER_INCH / mm } else { 1.0 }
}

/// Fit check: whole squares in the region (`round(region/cell)`) printed at
/// `square_in` (uncalibrated, as Python), compared with a target play area in
/// feet in either orientation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FitCheck {
    pub squares: (i64, i64),
    pub printed_in: (f64, f64),
    pub fits: bool,
}

pub fn fit_check(
    region_wh: (f64, f64),
    cell: (f64, f64),
    square_in: f64,
    target_ft: (f64, f64),
) -> Option<FitCheck> {
    if cell.0 <= 0.0 || cell.1 <= 0.0 {
        return None;
    }
    let cols = ((region_wh.0 / cell.0).round_ties_even() as i64).max(0);
    let rows = ((region_wh.1 / cell.1).round_ties_even() as i64).max(0);
    if cols == 0 || rows == 0 {
        return None;
    }
    let (w, h) = (cols as f64 * square_in, rows as f64 * square_in);
    let (tw, th) = (target_ft.0 * 12.0, target_ft.1 * 12.0);
    let tol = 1e-6;
    let fits = tw > 0.0
        && th > 0.0
        && ((w <= tw + tol && h <= th + tol) || (w <= th + tol && h <= tw + tol));
    Some(FitCheck { squares: (cols, rows), printed_in: (w, h), fits })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> TileParams {
        TileParams {
            image_w: 1000,
            image_h: 1000,
            region: None,
            cell_w: 72.0,
            cell_h: 72.0,
            square_in: 1.0,
            format: PageFormat::Letter,
            margin_pt: 0.0,
            offset: (0.0, 0.0),
            overlap_in: 0.0,
            align_to_grid: false,
            grid_offset: (0.0, 0.0),
        }
    }

    #[test]
    fn tiles_cover_region_without_gaps() {
        let plan = plan(&base());
        // 1 px == 1 pt here, so pages are 612×792 px.
        assert_eq!((plan.cols, plan.rows), (2, 2));
        let area: f64 =
            plan.tiles.iter().map(|t| (t.crop.2 - t.crop.0) * (t.crop.3 - t.crop.1)).sum();
        assert!((area - 1_000_000.0).abs() < 1e-6);
    }

    #[test]
    fn fit_check_either_orientation() {
        let f = fit_check((2400.0, 1680.0), (70.0, 70.0), 1.0, (2.0, 3.0)).unwrap();
        assert_eq!(f.squares, (34, 24));
        assert!(f.fits);
        assert!(!fit_check((2600.0, 1680.0), (70.0, 70.0), 1.0, (2.0, 3.0)).unwrap().fits);
        assert!(fit_check((10.0, 10.0), (70.0, 70.0), 1.0, (2.0, 3.0)).is_none());
    }

    #[test]
    fn aligned_pages_end_on_grid_lines() {
        let p = TileParams { align_to_grid: true, grid_offset: (10.0, 0.0), ..base() };
        let plan = plan(&p);
        for t in &plan.tiles {
            let k = (t.page_rect.0 - 10.0) / 72.0;
            assert!((k - k.round()).abs() < 1e-9);
            assert_eq!(t.page_rect.2 - t.page_rect.0, 8.0 * 72.0);
        }
    }
}
