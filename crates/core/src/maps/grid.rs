//! Grid model and sizing maths for the Map Paginator (Python `map_grid.py`).
//!
//! All four sizing modes funnel into the same [`GridModel`]: cell width/height in
//! *image pixels* plus an origin offset. Square grids only (hex is not in the
//! reference app either).

use serde::{Deserialize, Serialize};

/// Fallback DPI when the source image carries no resolution metadata.
pub const DEFAULT_DPI: f64 = 70.0;

/// Line style of the printed grid overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GridStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
    Broken,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GridModel {
    pub cell_w: f64,
    pub cell_h: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub color: [u8; 3],
    pub thickness: u32,
    /// 0..=255.
    pub opacity: u8,
    pub visible: bool,
    pub style: GridStyle,
}

impl Default for GridModel {
    fn default() -> Self {
        Self {
            cell_w: DEFAULT_DPI,
            cell_h: DEFAULT_DPI,
            offset_x: 0.0,
            offset_y: 0.0,
            color: [40, 40, 40],
            thickness: 1,
            opacity: 160,
            visible: true,
            style: GridStyle::Solid,
        }
    }
}

impl GridModel {
    pub fn vertical_xs(&self, width: f64) -> Vec<f64> {
        line_positions(self.offset_x, self.cell_w, width)
    }

    pub fn horizontal_ys(&self, height: f64) -> Vec<f64> {
        line_positions(self.offset_y, self.cell_h, height)
    }
}

/// Grid-line coordinates in `0..=extent` for a given offset and cell step.
pub fn line_positions(offset: f64, step: f64, extent: f64) -> Vec<f64> {
    let mut xs = Vec::new();
    if step <= 0.0 || step < 0.5 {
        return xs;
    }
    // Normalise the offset into [0, step) so the first line is the closest to 0.
    let mut x = offset - (offset / step).floor() * step;
    while x <= extent + 0.001 {
        if x >= -0.001 {
            xs.push(x);
        }
        x += step;
    }
    xs
}

/// `(cell_w, cell_h, offset_x, offset_y)` in image pixels.
pub type GridFit = (f64, f64, f64, f64);

/// Mode A: exact grid dimensions spanning the whole image.
pub fn from_rows_cols(size: (u32, u32), rows: i64, cols: i64) -> GridFit {
    let cols = cols.max(1) as f64;
    let rows = rows.max(1) as f64;
    (f64::from(size.0) / cols, f64::from(size.1) / rows, 0.0, 0.0)
}

/// Mode B: one cell inferred from a selected N×N block `(x0, y0, x1, y1)`. The
/// offset aligns grid lines with the block's edges.
pub fn from_reference(rect: (f64, f64, f64, f64), n_blocks: i64) -> GridFit {
    let (x0, y0, x1, y1) = rect;
    let n = n_blocks.max(1) as f64;
    let cw = (x1 - x0).abs() / n;
    let ch = (y1 - y0).abs() / n;
    let left = x0.min(x1);
    let top = y0.min(y1);
    let ox = if cw > 0.0 { left.rem_euclid(cw) } else { 0.0 };
    let oy = if ch > 0.0 { top.rem_euclid(ch) } else { 0.0 };
    (cw, ch, ox, oy)
}

/// Mode C: cell = PPI × real-world square size.
pub fn from_ppi(ppi: f64, square_in: f64) -> GridFit {
    let cell = ppi.max(0.0) * square_in;
    (cell, cell, 0.0, 0.0)
}

/// Mode D: the pixel length of one cell side, used directly.
pub fn from_cell_side(side: f64) -> GridFit {
    let s = side.max(0.0);
    (s, s, 0.0, 0.0)
}

/// Rough uniform grid from a (possibly skewed) reference quad: cell = average
/// edge / N, origin at the quad's bounding top-left. A preview only; a skewed
/// quad lines up once the map is de-warped.
pub fn from_quad(quad: &[(f64, f64); 4], n_blocks: i64) -> GridFit {
    let n = n_blocks.max(1) as f64;
    let [tl, tr, br, bl] = super::homography::order_quad_corners(quad);
    let cw = (dist(tl, tr) + dist(bl, br)) / 2.0 / n;
    let ch = (dist(tl, bl) + dist(tr, br)) / 2.0 / n;
    let left = quad.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let top = quad.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let ox = if cw > 0.0 { left.rem_euclid(cw) } else { 0.0 };
    let oy = if ch > 0.0 { top.rem_euclid(ch) } else { 0.0 };
    (cw, ch, ox, oy)
}

pub(crate) fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// `(on, off)` dash pattern in px, or `None` for solid. Lengths scale with the
/// cell so the look is consistent at any grid size.
pub fn style_pattern(style: GridStyle, cell: f64, thickness: u32) -> Option<(f64, f64)> {
    let t = f64::from(thickness.max(1));
    match style {
        GridStyle::Solid => None,
        GridStyle::Dashed => Some(((cell * 0.18).max(2.0), (cell * 0.12).max(2.0))),
        GridStyle::Dotted => Some((t, (cell * 0.10).max(2.0))),
        GridStyle::Broken => Some(((cell * 0.45).max(4.0), (cell * 0.12).max(2.0))),
    }
}

/// One straight overlay segment in image px.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Segment {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// The overlay as line segments (dashes already expanded), shared by the raster
/// renderer and anything that wants vectors. Vertical lines dash along `cell_h`,
/// horizontal along `cell_w`, as in Python.
pub fn overlay_segments(grid: &GridModel, w: f64, h: f64) -> Vec<Segment> {
    let mut out = Vec::new();
    if !grid.visible || grid.cell_w <= 0.0 || grid.cell_h <= 0.0 {
        return out;
    }
    let t = grid.thickness.max(1);
    let pat_v = style_pattern(grid.style, grid.cell_h, t);
    let pat_h = style_pattern(grid.style, grid.cell_w, t);
    for x in grid.vertical_xs(w) {
        push_line(&mut out, (x, 0.0), (x, h), pat_v);
    }
    for y in grid.horizontal_ys(h) {
        push_line(&mut out, (0.0, y), (w, y), pat_h);
    }
    out
}

fn push_line(out: &mut Vec<Segment>, a: (f64, f64), b: (f64, f64), pat: Option<(f64, f64)>) {
    let Some((on, off)) = pat else {
        out.push(Segment { x0: a.0, y0: a.1, x1: b.0, y1: b.1 });
        return;
    };
    let len = dist(a, b);
    if len == 0.0 || on <= 0.0 {
        return;
    }
    let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
    let period = on + off.max(0.0);
    let mut pos = 0.0;
    while pos < len {
        let seg = on.min(len - pos);
        out.push(Segment {
            x0: a.0 + ux * pos,
            y0: a.1 + uy * pos,
            x1: a.0 + ux * (pos + seg),
            y1: a.1 + uy * (pos + seg),
        });
        pos += period;
    }
}

/// Composites the grid overlay onto `img` (RGBA, in place) honouring colour,
/// opacity, thickness and style. Lines are axis-aligned, so each segment is a
/// rectangle filled the way PIL's `ImageDraw.line(width=t)` fills it.
pub fn render_overlay(img: &mut image::RgbaImage, grid: &GridModel) {
    let (w, h) = img.dimensions();
    let segs = overlay_segments(grid, f64::from(w), f64::from(h));
    if segs.is_empty() {
        return;
    }
    // Rasterise into a coverage mask first so overlapping dashes/crossings are
    // not composited twice (PIL draws into a separate overlay layer).
    let mut mask = vec![false; (w as usize) * (h as usize)];
    let t = i64::from(grid.thickness.max(1));
    for s in &segs {
        let (rx0, rx1, ry0, ry1);
        if s.x0 == s.x1 {
            // vertical: width t centred on the rounded coordinate
            let c = py_round(s.x0);
            rx0 = c - (t - 1) / 2;
            rx1 = rx0 + t - 1;
            ry0 = py_round(s.y0.min(s.y1));
            ry1 = py_round(s.y0.max(s.y1));
        } else {
            let c = py_round(s.y0);
            ry0 = c - (t - 1) / 2;
            ry1 = ry0 + t - 1;
            rx0 = py_round(s.x0.min(s.x1));
            rx1 = py_round(s.x0.max(s.x1));
        }
        for y in ry0.max(0)..=ry1.min(i64::from(h) - 1) {
            let row = y as usize * w as usize;
            for x in rx0.max(0)..=rx1.min(i64::from(w) - 1) {
                mask[row + x as usize] = true;
            }
        }
    }
    let a = u32::from(grid.opacity);
    let [cr, cg, cb] = grid.color.map(u32::from);
    for (px, m) in img.pixels_mut().zip(mask) {
        if m {
            px.0 = over(px.0, [cr, cg, cb], a);
        }
    }
}

/// `src` (straight alpha `a`) over `dst`, PIL `alpha_composite` rounding.
fn over(dst: [u8; 4], src: [u32; 3], a: u32) -> [u8; 4] {
    if a == 0 {
        return dst;
    }
    let da = u32::from(dst[3]);
    let out_a = a * 255 + da * (255 - a); // scaled by 255
    if out_a == 0 {
        return [0, 0, 0, 0];
    }
    let mut o = [0u8; 4];
    for i in 0..3 {
        let v = (src[i] * a * 255 + u32::from(dst[i]) * da * (255 - a) + out_a / 2) / out_a;
        o[i] = v.min(255) as u8;
    }
    o[3] = ((out_a + 127) / 255).min(255) as u8;
    o
}

fn py_round(v: f64) -> i64 {
    v.round_ties_even() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_draws_lines_with_opacity() {
        let mut img = image::RgbaImage::from_pixel(100, 50, image::Rgba([255, 255, 255, 255]));
        let g = GridModel {
            cell_w: 25.0,
            cell_h: 25.0,
            opacity: 255,
            color: [0, 0, 0],
            ..Default::default()
        };
        render_overlay(&mut img, &g);
        assert_eq!(img.get_pixel(25, 10).0, [0, 0, 0, 255]);
        assert_eq!(img.get_pixel(10, 25).0, [0, 0, 0, 255]);
        assert_eq!(img.get_pixel(10, 10).0, [255, 255, 255, 255]);
        let mut img2 = image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255]));
        let g2 = GridModel {
            cell_w: 5.0,
            cell_h: 5.0,
            opacity: 128,
            color: [0, 0, 0],
            ..Default::default()
        };
        render_overlay(&mut img2, &g2);
        // crossing pixel composited once, not twice
        assert_eq!(img2.get_pixel(5, 5).0, img2.get_pixel(5, 2).0);
        assert_eq!(img2.get_pixel(5, 2).0[0], 127);
    }

    #[test]
    fn hidden_or_degenerate_grid_is_noop() {
        let g = GridModel { visible: false, ..Default::default() };
        assert!(overlay_segments(&g, 100.0, 100.0).is_empty());
        let g = GridModel { cell_w: 0.0, ..Default::default() };
        assert!(overlay_segments(&g, 100.0, 100.0).is_empty());
    }

    #[test]
    fn dashed_segments_follow_pattern() {
        let g = GridModel {
            cell_w: 100.0,
            cell_h: 100.0,
            style: GridStyle::Dashed,
            ..Default::default()
        };
        let segs = overlay_segments(&g, 100.0, 100.0);
        // first vertical line at x=0: dashes of 18 every 30
        let v: Vec<_> = segs.iter().filter(|s| s.x0 == 0.0 && s.x1 == 0.0).collect();
        assert_eq!(v[0].y1 - v[0].y0, 18.0);
        assert_eq!(v[1].y0, 30.0);
        assert_eq!(v.last().unwrap().y1, 100.0);
    }
}
