//! Area-of-effect templates (Python `aoe_templates.py`).
//!
//! Shapes are defined in feet and scaled by `px_per_square / feet_per_square`,
//! anchored so their origin lands on a grid intersection. Rendering draws hard
//! edges at 2× and downsamples with Lanczos, like the PIL original.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use image::{RgbImage, RgbaImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::fonts::FontStyle;
use crate::pack::maxrects::{self, PackItem, SheetLayout};
use crate::units::PageFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[default]
    Circle,
    Cone,
    Line,
    Square,
}

impl Shape {
    pub const ALL: [Shape; 4] = [Shape::Circle, Shape::Cone, Shape::Line, Shape::Square];

    pub fn label(self) -> &'static str {
        match self {
            Shape::Circle => "Circle / Sphere (radius)",
            Shape::Cone => "Cone",
            Shape::Line => "Line",
            Shape::Square => "Square / Cube",
        }
    }

    /// Only the line exposes a second (width) dimension.
    pub fn uses_width(self) -> bool {
        self == Shape::Line
    }

    /// `(primary, secondary)` input labels.
    pub fn dimension_labels(self) -> (&'static str, Option<&'static str>) {
        match self {
            Shape::Circle => ("Radius (ft)", None),
            Shape::Cone => ("Length (ft)", None),
            Shape::Line => ("Length (ft)", Some("Width (ft)")),
            Shape::Square => ("Side (ft)", None),
        }
    }
}

/// Built-in presets, verbatim: `(name, shape, length_ft, width_ft)`.
pub const PRESETS: [(&str, Shape, f64, f64); 11] = [
    ("Sphere — 10 ft radius", Shape::Circle, 10.0, 5.0),
    ("Sphere — 20 ft (Fireball)", Shape::Circle, 20.0, 5.0),
    ("Sphere — 30 ft radius", Shape::Circle, 30.0, 5.0),
    ("Cone — 15 ft (Burning Hands)", Shape::Cone, 15.0, 5.0),
    ("Cone — 30 ft", Shape::Cone, 30.0, 5.0),
    ("Cone — 60 ft (Cone of Cold)", Shape::Cone, 60.0, 5.0),
    ("Line — 30 × 5 ft", Shape::Line, 30.0, 5.0),
    ("Line — 60 × 5 ft", Shape::Line, 60.0, 5.0),
    ("Line — 100 × 5 ft (Lightning)", Shape::Line, 100.0, 5.0),
    ("Cube — 15 ft", Shape::Square, 15.0, 5.0),
    ("Cube — 20 ft", Shape::Square, 20.0, 5.0),
];

/// Python `f"{v:g}"`: 6 significant digits, trailing zeros trimmed.
pub fn fmt_g(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return if v.is_nan() {
            "nan".into()
        } else if v == 0.0 {
            "0".into()
        } else {
            format!("{v}")
        };
    }
    let exp = v.abs().log10().floor() as i32;
    let trim = |s: String| {
        if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
    };
    if !(-4..6).contains(&exp) {
        let m = trim(format!("{:.5}", v / 10f64.powi(exp)));
        return format!("{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    trim(format!("{:.*}", (5 - exp).max(0) as usize, v))
}

/// The caption drawn on a template, e.g. "20 ft radius".
pub fn default_label(shape: Shape, a_ft: f64, b_ft: f64) -> String {
    let a = fmt_g(a_ft);
    match shape {
        Shape::Circle => format!("{a} ft radius"),
        Shape::Cone => format!("{a} ft cone"),
        Shape::Line => format!("{a} x {} ft line", fmt_g(b_ft)),
        Shape::Square => format!("{a} ft cube"),
    }
}

/// Footprint `(w, h)` in feet.
pub fn bbox_ft(shape: Shape, a_ft: f64, b_ft: f64) -> (f64, f64) {
    let a = a_ft.max(0.0);
    let b = b_ft.max(0.0);
    match shape {
        Shape::Circle => (2.0 * a, 2.0 * a),
        Shape::Cone | Shape::Square => (a, a),
        Shape::Line => (a, b.max(0.01)),
    }
}

/// Geometry in px with the origin at `(ox, oy)`: the ellipse box for a circle,
/// otherwise polygon vertices.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Geometry {
    Ellipse([f64; 4]),
    Polygon(Vec<(f64, f64)>),
}

pub fn shape_points(shape: Shape, a_ft: f64, b_ft: f64, ppf: f64, ox: f64, oy: f64) -> Geometry {
    match shape {
        Shape::Circle => {
            let d = 2.0 * a_ft * ppf;
            Geometry::Ellipse([ox, oy, ox + d, oy + d])
        }
        Shape::Cone => {
            let l = a_ft * ppf;
            // 5e cone: the width at the end equals the length.
            Geometry::Polygon(vec![(ox, oy + l / 2.0), (ox + l, oy), (ox + l, oy + l)])
        }
        Shape::Line => {
            let l = a_ft * ppf;
            let w = b_ft.max(0.01) * ppf;
            Geometry::Polygon(vec![(ox, oy), (ox + l, oy), (ox + l, oy + w), (ox, oy + w)])
        }
        Shape::Square => {
            let s = a_ft * ppf;
            Geometry::Polygon(vec![(ox, oy), (ox + s, oy), (ox + s, oy + s), (ox, oy + s)])
        }
    }
}

/// Full template configuration (the keys of a saved preset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AoeConfig {
    pub shape: Shape,
    pub a_ft: f64,
    pub b_ft: f64,
    pub feet_per_square: f64,
    pub fill_rgb: [u8; 3],
    pub fill_alpha: u8,
    pub line_rgb: [u8; 3],
    pub line_width: u32,
    pub grid_on: bool,
    pub grid_rgb: [u8; 3],
    pub grid_alpha: u8,
    pub label_on: bool,
    pub label: String,
    /// Printed inches per grid square.
    pub inch: f64,
    pub page_format: PageFormat,
    pub name: String,
}

impl Default for AoeConfig {
    fn default() -> Self {
        Self {
            shape: Shape::Circle,
            a_ft: 20.0,
            b_ft: 5.0,
            feet_per_square: 5.0,
            fill_rgb: [220, 70, 40],
            fill_alpha: 80,
            line_rgb: [150, 25, 0],
            line_width: 4,
            grid_on: true,
            grid_rgb: [60, 60, 60],
            grid_alpha: 110,
            label_on: true,
            label: default_label(Shape::Circle, 20.0, 5.0),
            inch: 1.0,
            page_format: PageFormat::A4,
            name: String::new(),
        }
    }
}

impl AoeConfig {
    /// Clamps inputs the way the Python panel reads them.
    pub fn sanitized(&self) -> Self {
        let mut c = self.clone();
        c.a_ft = if c.a_ft.is_finite() { c.a_ft.max(0.0) } else { 0.0 };
        c.b_ft = if c.b_ft.is_finite() { c.b_ft.max(0.0) } else { 5.0 };
        c.feet_per_square =
            if c.feet_per_square.is_finite() { c.feet_per_square.max(0.1) } else { 5.0 };
        c.inch = if c.inch.is_finite() { c.inch.max(0.05) } else { 1.0 };
        c
    }

    /// Display name: the explicit name, else the label, else the default label.
    pub fn display_name(&self) -> String {
        [&self.name, &self.label]
            .into_iter()
            .find(|s| !s.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| default_label(self.shape, self.a_ft, self.b_ft))
    }

    /// Preview/print resolution: `clamp(150 × inch, 60, 300)` px per square.
    pub fn px_per_square(&self) -> f64 {
        (150.0 * self.inch).clamp(60.0, 300.0)
    }

    /// Renders at the configured inch (Python `_image_from_config`).
    pub fn render(&self) -> (RgbImage, f64) {
        let c = self.sanitized();
        let label = (c.label_on && !c.label.is_empty()).then_some(c.label.as_str());
        render_template(
            c.shape,
            c.a_ft,
            c.b_ft,
            c.feet_per_square,
            c.px_per_square(),
            &Style::from(&c),
            label,
            2,
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub fill_rgb: [u8; 3],
    pub fill_alpha: u8,
    pub line_rgb: [u8; 3],
    pub line_width: u32,
    pub show_grid: bool,
    pub grid_rgb: [u8; 3],
    pub grid_alpha: u8,
}

impl Default for Style {
    fn default() -> Self {
        Self::from(&AoeConfig::default())
    }
}

impl From<&AoeConfig> for Style {
    fn from(c: &AoeConfig) -> Self {
        Self {
            fill_rgb: c.fill_rgb,
            fill_alpha: c.fill_alpha,
            line_rgb: c.line_rgb,
            line_width: c.line_width,
            show_grid: c.grid_on,
            grid_rgb: c.grid_rgb,
            grid_alpha: c.grid_alpha,
        }
    }
}

/// Longest side cap for a rendered template, px.
pub const MAX_DIM: i64 = 6000;

/// Output size `(w, h, px_per_square)` of [`render_template`] without
/// rendering: the footprint plus `line_width + 4` padding, with
/// `px_per_square` reduced until the longest side is within 6000 px.
pub fn template_size(
    shape: Shape,
    a_ft: f64,
    b_ft: f64,
    feet_per_square: f64,
    px_per_square: f64,
    line_width: u32,
) -> (i64, i64, f64) {
    let fps = feet_per_square.max(0.1);
    let mut pps = px_per_square.max(8.0);
    let pad = f64::from(line_width) + 4.0;
    let (bw, bh) = bbox_ft(shape, a_ft.max(0.0), b_ft.max(0.0));
    loop {
        let ppf = pps / fps;
        let w = ((bw * ppf + 2.0 * pad).round_ties_even() as i64).max(1);
        let h = ((bh * ppf + 2.0 * pad).round_ties_even() as i64).max(1);
        if w.max(h) <= MAX_DIM {
            return (w, h, pps);
        }
        pps *= MAX_DIM as f64 / w.max(h) as f64;
    }
}

/// Renders a template on white. Returns `(image, cell_px)`, where `cell_px`
/// is the px-per-square to hand to the tiler (reduced when the 6000 px cap
/// kicks in).
#[allow(clippy::too_many_arguments)]
pub fn render_template(
    shape: Shape,
    a_ft: f64,
    b_ft: f64,
    feet_per_square: f64,
    px_per_square: f64,
    style: &Style,
    label: Option<&str>,
    supersample: u32,
) -> (RgbImage, f64) {
    let a_ft = a_ft.max(0.0);
    let b_ft = b_ft.max(0.0);
    let fps = feet_per_square.max(0.1);
    let ss = supersample.max(1);
    let pad = f64::from(style.line_width) + 4.0;
    let (w, h, pps) = template_size(shape, a_ft, b_ft, fps, px_per_square, style.line_width);
    let ssf = f64::from(ss);
    let (sw, sh) = (w as usize * ss as usize, h as usize * ss as usize);
    let o = pad * ssf;
    let lw = ((f64::from(style.line_width) * ssf).round_ties_even() as i64).max(1);
    let cell = pps * ssf;
    let mut buf = Canvas { w: sw, h: sh, px: vec![[255u8; 3]; sw * sh] };

    if style.show_grid {
        let gt = i64::from(ss);
        let mut cols = Vec::new();
        let mut k = 0.0;
        while o + k * cell <= sw as f64 {
            cols.push(o + k * cell);
            k += 1.0;
        }
        k = 1.0;
        while o - k * cell >= 0.0 {
            cols.push(o - k * cell);
            k += 1.0;
        }
        let mut rows = Vec::new();
        k = 0.0;
        while o + k * cell <= sh as f64 {
            rows.push(o + k * cell);
            k += 1.0;
        }
        k = 1.0;
        while o - k * cell >= 0.0 {
            rows.push(o - k * cell);
            k += 1.0;
        }
        // PIL's wide line of width t covers [c − (t−1)/2, …] rounded up
        let band = |c: f64| {
            let s = (c - (gt as f64 - 1.0) / 2.0).ceil() as i64;
            (s, s + gt)
        };
        let mask = |lines: Vec<f64>, n: usize| {
            let mut m = vec![false; n];
            for (a, b) in lines.into_iter().map(band) {
                for i in a.max(0)..b.min(n as i64) {
                    m[i as usize] = true;
                }
            }
            m
        };
        let (cm, rm) = (mask(cols, sw), mask(rows, sh));
        buf.composite(style.grid_rgb, style.grid_alpha, |x, y| cm[x as usize] || rm[y as usize]);
    }

    let geom = shape_points(shape, a_ft, b_ft, pps / fps * ssf, o, o);
    match &geom {
        Geometry::Ellipse(bx) => {
            let e = Ellipse::new(bx);
            buf.composite(style.fill_rgb, style.fill_alpha, |x, y| e.inside(x, y, 0.0));
            buf.composite(style.line_rgb, 255, |x, y| {
                e.inside(x, y, 0.0) && !e.inside(x, y, lw as f64)
            });
        }
        Geometry::Polygon(pts) => {
            // PIL rounds polygon vertices to integers.
            let p: Vec<(f64, f64)> = pts.iter().map(|&(x, y)| (x.round(), y.round())).collect();
            buf.composite(style.fill_rgb, style.fill_alpha, |x, y| {
                in_polygon(&p, x as f64, y as f64)
            });
            // Wide outlines sit inside the polygon (lines of width 2w−1 masked by the fill).
            let half = (2 * lw - 1) as f64 / 2.0;
            buf.composite(style.line_rgb, 255, |x, y| {
                let (fx, fy) = (x as f64, y as f64);
                in_polygon(&p, fx, fy) && edge_distance(&p, fx, fy) <= half
            });
        }
    }

    if let Some(text) = label.filter(|t| !t.is_empty()) {
        let size = (0.26 * pps * ssf).round_ties_even().max(12.0) as f32;
        let at = (lw + 2).max(2) as f32;
        for dx in -2i32..=2 {
            for dy in -2i32..=2 {
                if dx != 0 || dy != 0 {
                    draw_text(
                        &mut buf,
                        text,
                        size,
                        at + dx as f32,
                        at + dy as f32,
                        [255, 255, 255],
                    );
                }
            }
        }
        draw_text(&mut buf, text, size, at, at, style.line_rgb);
    }

    let rgba = RgbaImage::from_raw(
        sw as u32,
        sh as u32,
        buf.px.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
    )
    .expect("size");
    let out = if ss > 1 { crate::imaging::resize_rgba(&rgba, w as u32, h as u32) } else { rgba };
    (image::DynamicImage::ImageRgba8(out).to_rgb8(), pps)
}

struct Canvas {
    w: usize,
    h: usize,
    px: Vec<[u8; 3]>,
}

impl Canvas {
    /// Composites `rgb` at `alpha` wherever `hit(x, y)` holds.
    fn composite(&mut self, rgb: [u8; 3], alpha: u8, hit: impl Fn(i64, i64) -> bool + Sync) {
        if alpha == 0 {
            return;
        }
        let a = u32::from(alpha);
        self.px.par_chunks_mut(self.w).enumerate().for_each(|(y, row)| {
            for (x, p) in row.iter_mut().enumerate() {
                if hit(x as i64, y as i64) {
                    for c in 0..3 {
                        p[c] = ((u32::from(rgb[c]) * a + u32::from(p[c]) * (255 - a) + 127) / 255)
                            as u8;
                    }
                }
            }
        });
    }

    fn blend(&mut self, x: i64, y: i64, rgb: [u8; 3], cov: f32) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 || cov <= 0.0 {
            return;
        }
        let p = &mut self.px[y as usize * self.w + x as usize];
        let m = cov.min(1.0);
        for c in 0..3 {
            p[c] = (f32::from(p[c]) + (f32::from(rgb[c]) - f32::from(p[c])) * m).round() as u8;
        }
    }
}

struct Ellipse {
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
}

impl Ellipse {
    /// PIL truncates the box to integers and covers pixels x0..=x1.
    fn new(b: &[f64; 4]) -> Self {
        let (x0, y0, x1, y1) = (b[0].trunc(), b[1].trunc(), b[2].trunc(), b[3].trunc());
        Self {
            cx: (x0 + x1 + 1.0) / 2.0,
            cy: (y0 + y1 + 1.0) / 2.0,
            rx: (x1 - x0 + 1.0) / 2.0,
            ry: (y1 - y0 + 1.0) / 2.0,
        }
    }

    fn inside(&self, x: i64, y: i64, inset: f64) -> bool {
        let (rx, ry) = (self.rx - inset, self.ry - inset);
        if rx <= 0.0 || ry <= 0.0 {
            return false;
        }
        let dx = (x as f64 + 0.5 - self.cx) / rx;
        let dy = (y as f64 + 0.5 - self.cy) / ry;
        dx * dx + dy * dy <= 1.0
    }
}

/// Point-in-polygon, boundary inclusive.
fn in_polygon(p: &[(f64, f64)], x: f64, y: f64) -> bool {
    if edge_distance(p, x, y) < 1e-9 {
        return true;
    }
    let mut inside = false;
    let mut j = p.len() - 1;
    for i in 0..p.len() {
        let (xi, yi) = p[i];
        let (xj, yj) = p[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn edge_distance(p: &[(f64, f64)], x: f64, y: f64) -> f64 {
    let mut best = f64::INFINITY;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        let (vx, vy) = (b.0 - a.0, b.1 - a.1);
        let len2 = vx * vx + vy * vy;
        let t = if len2 > 0.0 {
            (((x - a.0) * vx + (y - a.1) * vy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (dx, dy) = (x - (a.0 + t * vx), y - (a.1 + t * vy));
        best = best.min((dx * dx + dy * dy).sqrt());
    }
    best
}

/// Anti-aliased text with its top-left at the ascender line `(x, y)` (PIL's
/// default "la" anchor), using the bundled Liberation Sans.
fn draw_text(buf: &mut Canvas, text: &str, size_px: f32, x: f32, y: f32, rgb: [u8; 3]) {
    let Ok(font) = FontRef::try_from_slice(FontStyle::Regular.data()) else { return };
    let upem = font.units_per_em().unwrap_or(2048.0);
    let scale = PxScale::from(size_px * font.height_unscaled() / upem);
    let sf = font.as_scaled(scale);
    let baseline = y + sf.ascent();
    let mut pen = x;
    for ch in text.chars() {
        let id = font.glyph_id(ch);
        let g = id.with_scale_and_position(scale, point(pen, baseline));
        pen += sf.h_advance(id);
        if let Some(og) = font.outline_glyph(g) {
            let bb = og.px_bounds();
            og.draw(|gx, gy, cov| {
                buf.blend(
                    bb.min.x as i64 + i64::from(gx),
                    bb.min.y as i64 + i64::from(gy),
                    rgb,
                    cov,
                )
            });
        }
    }
}

/// Page size in inches used for raster sheet packing (Python `PAGE_INCHES`).
pub fn page_inches(format: PageFormat) -> (f64, f64) {
    match format {
        PageFormat::A4 => (8.2677, 11.6929),
        PageFormat::Letter => (8.5, 11.0),
        PageFormat::A3 => (11.6929, 16.5354),
    }
}

/// Sheet raster resolution, gap and margin defaults.
pub const SHEET_DPI: f64 = 150.0;
pub const SHEET_MARGIN_IN: f64 = 0.2;
pub const SHEET_GAP_IN: f64 = 0.2;

/// Packs rendered templates `(image, cell_px)` onto page rasters at true scale
/// (Python `compose_sheets`). Items too big for a page are listed in
/// `layout.skipped`.
pub fn compose_sheets(
    items: &[(RgbImage, f64)],
    format: PageFormat,
    inch: f64,
    gap_in: f64,
) -> (Vec<RgbImage>, SheetLayout) {
    let layout = sheet_layout(
        items.iter().map(|(img, cell)| (img.width(), img.height(), *cell)),
        format,
        inch,
        gap_in,
    );
    let mut pages = vec![
        RgbImage::from_pixel(
            layout.page_w as u32,
            layout.page_h as u32,
            image::Rgb([255, 255, 255])
        );
        layout.pages
    ];
    for p in &layout.placed {
        let (img, _) = &items[p.item];
        let rgba = image::DynamicImage::ImageRgb8(img.clone()).to_rgba8();
        let scaled = image::DynamicImage::ImageRgba8(crate::imaging::resize_rgba(
            &rgba, p.w as u32, p.h as u32,
        ))
        .to_rgb8();
        image::imageops::replace(&mut pages[p.page], &scaled, p.x, p.y);
    }
    (pages, layout)
}

/// The packing part of [`compose_sheets`] from sizes only `(w, h, cell_px)`.
pub fn sheet_layout(
    sizes: impl Iterator<Item = (u32, u32, f64)>,
    format: PageFormat,
    inch: f64,
    gap_in: f64,
) -> SheetLayout {
    let (pw, ph) = page_inches(format);
    let r = |v: f64| v.round_ties_even() as i64;
    let page_w = r(pw * SHEET_DPI);
    let page_h = r(ph * SHEET_DPI);
    let margin = r(SHEET_MARGIN_IN * SHEET_DPI).max(0);
    let gap = r(gap_in * SHEET_DPI).max(0);
    let items: Vec<PackItem> = sizes
        .map(|(w, h, cell)| {
            let cell = cell.max(1.0);
            PackItem {
                w: r(f64::from(w) / cell * inch * SHEET_DPI).max(1),
                h: r(f64::from(h) / cell * inch * SHEET_DPI).max(1),
            }
        })
        .collect();
    maxrects::pack(&items, page_w, page_h, margin, gap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_g_matches_python() {
        for (v, s) in [
            (20.0, "20"),
            (7.5, "7.5"),
            (0.0, "0"),
            (100.0, "100"),
            (2.5, "2.5"),
            (0.01, "0.01"),
            (1e6, "1e+06"),
            (123456.7, "123457"),
        ] {
            assert_eq!(fmt_g(v), s, "{v}");
        }
        assert_eq!(default_label(Shape::Line, 30.0, 5.0), "30 x 5 ft line");
    }

    #[test]
    fn render_size_and_cap() {
        let (img, cell) = render_template(
            Shape::Circle,
            20.0,
            5.0,
            5.0,
            150.0,
            &Style::default(),
            Some("20 ft radius"),
            2,
        );
        assert_eq!(img.dimensions(), (1216, 1216));
        assert_eq!(cell, 150.0);
        let (big, cell) =
            render_template(Shape::Line, 1000.0, 5.0, 5.0, 150.0, &Style::default(), None, 1);
        assert!(big.width() <= 6000);
        assert!(cell < 150.0);
    }

    #[test]
    fn label_is_drawn_in_the_padding() {
        let style = Style { show_grid: false, fill_alpha: 0, ..Style::default() };
        let (with, _) =
            render_template(Shape::Square, 15.0, 5.0, 5.0, 60.0, &style, Some("15 ft cube"), 2);
        let (without, _) = render_template(Shape::Square, 15.0, 5.0, 5.0, 60.0, &style, None, 2);
        assert_ne!(with, without);
    }

    #[test]
    fn label_halo_clears_the_grid_behind_the_text() {
        let style = Style { fill_alpha: 0, ..Style::default() };
        let (with, _) =
            render_template(Shape::Square, 15.0, 5.0, 5.0, 60.0, &style, Some("15 ft cube"), 1);
        let (without, _) = render_template(Shape::Square, 15.0, 5.0, 5.0, 60.0, &style, None, 1);
        let white = image::Rgb([255, 255, 255]);
        let line = image::Rgb(style.line_rgb);
        let haloed =
            with.pixels().zip(without.pixels()).filter(|(a, b)| **a == white && **b != white);
        assert!(haloed.count() > 0, "white halo erases grid lines around the label");
        let inked = with.pixels().zip(without.pixels()).filter(|(a, b)| **a == line && **b != line);
        assert!(inked.count() > 0, "the text itself is in the line colour");
    }
}
