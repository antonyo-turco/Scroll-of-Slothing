//! Token compositing: frame an image inside a border (round, square or hexagonal).
//! Python reference: `ui/token_compositing.py`. Spec: docs/features/token-cutter.md.

use fast_image_resize::{
    self as fr,
    images::{Image as FrImage, ImageRef},
};
use image::{RgbImage, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::maps::filters::{self, FilterStep};

/// Map filters a token can use (the colour/tone ones; the ink-saving B&W
/// filters are for printing maps). Other kinds in `filters` are ignored.
pub const TOKEN_FILTERS: &[&str] = &[
    "autocontrast",
    "contrast",
    "lightness",
    "clarity",
    "sharpen",
    "sepia",
    "grayscale",
    "posterize",
    "edge_boost",
];

/// Definitions of the token filters, in the map pipeline's default order.
pub fn token_filter_defs() -> Vec<filters::FilterDef> {
    filters::filter_defs().into_iter().filter(|d| TOKEN_FILTERS.contains(&d.key)).collect()
}

/// A map preset as a token pipeline (every token filter listed, the preset's
/// ones enabled). `None` for unknown presets and those using map-only filters.
pub fn token_preset_pipeline(name: &str) -> Option<Vec<FilterStep>> {
    let full = filters::preset_pipeline(name)?;
    if full.iter().any(|s| s.enabled && !TOKEN_FILTERS.contains(&s.kind.as_str())) {
        return None;
    }
    Some(full.into_iter().filter(|s| TOKEN_FILTERS.contains(&s.kind.as_str())).collect())
}

/// Names of the map presets that work on tokens, in catalog order.
pub fn token_preset_names() -> Vec<&'static str> {
    filters::PRESETS
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| token_preset_pipeline(n).is_some())
        .collect()
}

/// Outline of a token: what the border frames, what gets clipped, and the cut line.
/// The shape comes from the border (its list entry, or [`detect_shape`]), not the params.
///
/// Every shape is measured in "radius" units: the unit shape is scaled so its
/// larger half-extent is 1, so radius `min(w, h) / 2` fits a square canvas.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenShape {
    #[default]
    Circle,
    Square,
    /// Pointy-top regular hexagon (vertices at the top and bottom, flat sides
    /// left and right; √3/2 as wide as tall).
    Hex,
    /// Regular octagon with flat sides at the top, bottom, left and right.
    Octagon,
    /// Regular decagon with flat top and bottom (vertices left and right;
    /// cos 18° ≈ 0.951 as tall as wide).
    Decagon,
}

/// A regular polygon: side count, angle of the first side's outward normal
/// (degrees, y down), and circumradius in radius units.
struct Polygon {
    sides: u32,
    normal_deg: f64,
    circumradius: f64,
}

impl Polygon {
    fn apothem(&self) -> f64 {
        self.circumradius * (std::f64::consts::PI / f64::from(self.sides)).cos()
    }

    fn normals(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        (0..self.sides).map(|k| {
            let t = (self.normal_deg + 360.0 * f64::from(k) / f64::from(self.sides)).to_radians();
            (t.cos(), t.sin())
        })
    }

    /// Vertices (unit radius), starting between the first and second normals.
    fn vertices(&self) -> Vec<(f64, f64)> {
        let half = 180.0 / f64::from(self.sides);
        (0..self.sides)
            .map(|k| {
                let t = (self.normal_deg + half + 360.0 * f64::from(k) / f64::from(self.sides))
                    .to_radians();
                (self.circumradius * t.cos(), self.circumradius * t.sin())
            })
            .collect()
    }
}

impl TokenShape {
    pub const ALL: [TokenShape; 5] = [
        TokenShape::Circle,
        TokenShape::Square,
        TokenShape::Hex,
        TokenShape::Octagon,
        TokenShape::Decagon,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TokenShape::Circle => "circle",
            TokenShape::Square => "square",
            TokenShape::Hex => "hex",
            TokenShape::Octagon => "octagon",
            TokenShape::Decagon => "decagon",
        }
    }

    fn polygon(self) -> Option<Polygon> {
        let p = |sides, normal_deg, circumradius| Some(Polygon { sides, normal_deg, circumradius });
        match self {
            TokenShape::Circle => None,
            TokenShape::Square => p(4, 0.0, std::f64::consts::SQRT_2),
            TokenShape::Hex => p(6, 0.0, 1.0),
            TokenShape::Octagon => p(8, 0.0, 1.0 / (std::f64::consts::PI / 8.0).cos()),
            TokenShape::Decagon => p(10, 18.0, 1.0),
        }
    }

    /// Half width and half height of the unit shape (the larger one is 1).
    pub fn extents(self) -> (f64, f64) {
        match self.polygon() {
            None => (1.0, 1.0),
            Some(p) => p
                .vertices()
                .iter()
                .fold((0.0f64, 0.0f64), |(ex, ey), &(x, y)| (ex.max(x.abs()), ey.max(y.abs()))),
        }
    }

    /// Whether the point `(x, y)`, relative to the centre and in radius units,
    /// is inside the shape.
    pub fn contains(self, x: f64, y: f64) -> bool {
        match self.polygon() {
            None => x * x + y * y <= 1.0,
            Some(p) => {
                let a = p.apothem() + 1e-12;
                p.normals().all(|(nx, ny)| x * nx + y * ny <= a)
            }
        }
    }

    /// The outline's corners for radius `r` around `(cx, cy)` (y down);
    /// empty for a circle.
    pub fn vertices(self, cx: f64, cy: f64, r: f64) -> Vec<(f64, f64)> {
        self.polygon()
            .map(|p| p.vertices().into_iter().map(|(x, y)| (cx + x * r, cy + y * r)).collect())
            .unwrap_or_default()
    }

    /// Largest radius whose shape fits a `w`×`h` box.
    pub fn fit_radius(self, w: f64, h: f64) -> f64 {
        let (ex, ey) = self.extents();
        (w / 2.0 / ex).min(h / 2.0 / ey)
    }
}

/// Guesses the outline of a border or a finished token from its alpha: the
/// opaque silhouette (each row filled between its first and last opaque
/// pixel) is compared with every shape fitted to the same box, and the
/// closest wins (ties go to the circle, the old default).
pub fn detect_shape(img: &RgbaImage) -> TokenShape {
    let Some((x0, y0, x1, y1)) = alpha_bbox(img) else { return TokenShape::Circle };
    let (bw, bh) = (f64::from(x1 - x0), f64::from(y1 - y0));
    let step = ((x1 - x0).max(y1 - y0) / 160).max(1) as usize;
    let spans: Vec<Option<(u32, u32)>> = (y0..y1)
        .step_by(step)
        .map(|y| {
            let opaque = |x: &u32| img.get_pixel(*x, y)[3] >= 128;
            let first = (x0..x1).find(opaque)?;
            let last = (x0..x1).rev().find(opaque)?;
            Some((first, last))
        })
        .collect();
    let (cx, cy) = (f64::from(x0 + x1) / 2.0, f64::from(y0 + y1) / 2.0);
    let mut best = (usize::MAX, TokenShape::Circle);
    for shape in TokenShape::ALL {
        let r = shape.fit_radius(bw, bh);
        let mut err = 0;
        for (row, y) in spans.iter().zip((y0..y1).step_by(step)) {
            let py = (f64::from(y) + 0.5 - cy) / r;
            for x in (x0..x1).step_by(step) {
                let filled = row.is_some_and(|(a, b)| (a..=b).contains(&x));
                if filled != shape.contains((f64::from(x) + 0.5 - cx) / r, py) {
                    err += 1;
                }
            }
        }
        if err < best.0 {
            best = (err, shape);
        }
    }
    best.1
}

/// Anti-aliased coverage (0–255) of `shape` centred on `(cx, cy)` with radius `r`
/// (see [`TokenShape::contains`]). Circles use [`ellipse_mask`]'s pixel convention.
pub fn shape_mask(w: u32, h: u32, shape: TokenShape, cx: f64, cy: f64, r: f64) -> Vec<u8> {
    if shape == TokenShape::Circle {
        return ellipse_mask(w, h, cx - r, cy - r, cx + r - 1.0, cy + r - 1.0);
    }
    let mut out = vec![0u8; (w * h) as usize];
    if r <= 0.0 {
        return out;
    }
    let sub = 1.0 / f64::from(AA);
    for y in 0..h {
        if ((f64::from(y) + 0.5 - cy) / r).abs() > 1.0 + 2.0 / r {
            continue;
        }
        for x in 0..w {
            let mut hits = 0u32;
            for sy in 0..AA {
                let py = (f64::from(y) + (f64::from(sy) + 0.5) * sub - cy) / r;
                for sx in 0..AA {
                    let px = (f64::from(x) + (f64::from(sx) + 0.5) * sub - cx) / r;
                    if shape.contains(px, py) {
                        hits += 1;
                    }
                }
            }
            out[(y * w + x) as usize] = ((hits * 255 + (AA * AA) / 2) / (AA * AA)) as u8;
        }
    }
    out
}

/// Editable token parameters (persisted as the asset's `params_json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TokenParams {
    /// Border id (e.g. `builtin:roll20`).
    pub border: String,
    /// Solid fill behind the image inside the ring; `None` = transparent.
    pub bg: Option<[u8; 3]>,
    /// Content circle inset as % of the radius (0–50).
    pub ring_inset_pct: f64,
    /// Pan in border pixels (positive moves the view right/down over the image).
    pub pan: [f64; 2],
    /// Zoom multiplier on top of cover-fit (0.1–10).
    pub zoom: f64,
    /// Filter pipeline applied to the framed image only (never the border).
    pub filters: Vec<FilterStep>,
}

impl Default for TokenParams {
    fn default() -> Self {
        TokenParams {
            border: String::new(),
            bg: None,
            ring_inset_pct: 0.0,
            pan: [0.0, 0.0],
            zoom: 1.0,
            filters: Vec::new(),
        }
    }
}

/// Runs the enabled token filters on the colour channels; alpha is kept.
fn apply_token_filters(img: &mut RgbaImage, pipeline: &[FilterStep]) {
    let steps: Vec<FilterStep> = pipeline
        .iter()
        .filter(|s| s.enabled && TOKEN_FILTERS.contains(&s.kind.as_str()))
        .cloned()
        .collect();
    if filters::is_identity(&steps) {
        return;
    }
    let (w, h) = img.dimensions();
    let rgb = RgbImage::from_fn(w, h, |x, y| {
        let p = img.get_pixel(x, y);
        image::Rgb([p[0], p[1], p[2]])
    });
    let out = filters::apply_pipeline(&rgb, &steps);
    for (px, f) in img.pixels_mut().zip(out.pixels()) {
        px[0] = f[0];
        px[1] = f[1];
        px[2] = f[2];
    }
}

impl TokenParams {
    /// Clamps values into their valid ranges.
    pub fn clamped(mut self) -> Self {
        self.ring_inset_pct = self.ring_inset_pct.clamp(0.0, 50.0);
        self.zoom = if self.zoom.is_finite() { self.zoom.clamp(0.1, 10.0) } else { 1.0 };
        for p in &mut self.pan {
            if !p.is_finite() {
                *p = 0.0;
            }
        }
        self
    }
}

/// Supersampling factor per axis for anti-aliased circle masks.
const AA: u32 = 4;

/// Coverage (0–255) of an ellipse inscribed in the inclusive pixel box
/// `[x0, y0, x1, y1]` — the same box convention as PIL's `ImageDraw.ellipse`.
/// Edges are anti-aliased (PIL's are hard; see "Intentional changes" in the spec).
pub fn ellipse_mask(w: u32, h: u32, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<u8> {
    let cx = (x0 + x1 + 1.0) / 2.0;
    let cy = (y0 + y1 + 1.0) / 2.0;
    let rx = (x1 - x0 + 1.0) / 2.0;
    let ry = (y1 - y0 + 1.0) / 2.0;
    let mut out = vec![0u8; (w * h) as usize];
    if rx <= 0.0 || ry <= 0.0 {
        return out;
    }
    let sub = 1.0 / f64::from(AA);
    for y in 0..h {
        // Fast path per row: skip rows fully outside / inside.
        let dy_mid = (f64::from(y) + 0.5 - cy) / ry;
        if dy_mid.abs() > 1.0 + 2.0 / ry {
            continue;
        }
        for x in 0..w {
            let mut hits = 0u32;
            for sy in 0..AA {
                let py = (f64::from(y) + (f64::from(sy) + 0.5) * sub - cy) / ry;
                for sx in 0..AA {
                    let px = (f64::from(x) + (f64::from(sx) + 0.5) * sub - cx) / rx;
                    if px * px + py * py <= 1.0 {
                        hits += 1;
                    }
                }
            }
            out[(y * w + x) as usize] = ((hits * 255 + (AA * AA) / 2) / (AA * AA)) as u8;
        }
    }
    out
}

/// Bounding box `(x0, y0, x1_exclusive, y1_exclusive)` of non-zero alpha (PIL `getbbox`).
pub fn alpha_bbox(img: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = img.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] != 0 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
}

/// Mask filled inside the border's outer edge (from its alpha bounding box).
fn outer_ring_mask(border: &RgbaImage, shape: TokenShape) -> Option<Vec<u8>> {
    let (bx0, by0, bx1, by1) = alpha_bbox(border)?;
    let cx = f64::from(bx0 + bx1) / 2.0;
    let cy = f64::from(by0 + by1) / 2.0;
    let r = shape.fit_radius(f64::from(bx1 - bx0), f64::from(by1 - by0));
    let (w, h) = border.dimensions();
    // Circle: PIL draws ellipse([cx-r, cy-r, cx+r, cy+r]) with an inclusive end coordinate.
    Some(shape_mask(w, h, shape, cx, cy, r))
}

/// Cover-fits `img` into `(tw, th)`, applies zoom and pan, and returns the
/// visible window. Only the visible region of the source is resampled
/// (Python resized the whole image first, then cropped).
pub fn fit_with_transform(
    img: &RgbaImage,
    tw: u32,
    th: u32,
    pan: [f64; 2],
    zoom: f64,
) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    let mut out = RgbaImage::new(tw, th);
    if iw == 0 || ih == 0 {
        return out;
    }
    let scale = (f64::from(tw) / f64::from(iw)).max(f64::from(th) / f64::from(ih)) * zoom;
    // Python: int() truncation of the scaled size, then integer centring.
    let nw = ((f64::from(iw) * scale) as i64).max(1);
    let nh = ((f64::from(ih) * scale) as i64).max(1);
    let cx = nw / 2 + pan[0].trunc() as i64;
    let cy = nh / 2 + pan[1].trunc() as i64;
    let left = cx - i64::from(tw) / 2;
    let top = cy - i64::from(th) / 2;
    // Visible part of the (virtual) resized image, in resized coordinates.
    let (sl, st) = (left.max(0), top.max(0));
    let (sr, sb) = ((left + i64::from(tw)).min(nw), (top + i64::from(th)).min(nh));
    if sr <= sl || sb <= st {
        return out;
    }
    let (dw, dh) = ((sr - sl) as u32, (sb - st) as u32);
    // Map the visible window back to source pixels (exact float box).
    let sx = iw as f64 / nw as f64;
    let sy = ih as f64 / nh as f64;
    let src = ImageRef::new(iw, ih, img.as_raw(), fr::PixelType::U8x4).expect("valid buffer");
    let mut dst = FrImage::new(dw, dh, fr::PixelType::U8x4);
    let opts = fr::ResizeOptions::new()
        .resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3))
        .use_alpha(true)
        .crop(
            sl as f64 * sx,
            st as f64 * sy,
            // Clamp: float rounding must not push the box past the image edge.
            (f64::from(dw) * sx).min(f64::from(iw) - sl as f64 * sx),
            (f64::from(dh) * sy).min(f64::from(ih) - st as f64 * sy),
        );
    fr::Resizer::new().resize(&src, &mut dst, &opts).expect("same pixel type");
    let window = RgbaImage::from_raw(dw, dh, dst.into_vec()).expect("valid buffer");
    image::imageops::replace(&mut out, &window, sl - left, st - top);
    out
}

/// Straight-alpha "over" (PIL `alpha_composite`).
fn over(dst: &mut Rgba<u8>, src: &Rgba<u8>) {
    let sa = f32::from(src[3]) / 255.0;
    if sa <= 0.0 {
        return;
    }
    let da = f32::from(dst[3]) / 255.0;
    let oa = sa + da * (1.0 - sa);
    for c in 0..3 {
        let v = (f32::from(src[c]) * sa + f32::from(dst[c]) * da * (1.0 - sa)) / oa;
        dst[c] = v.round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (oa * 255.0).round() as u8;
}

fn mul(a: u8, b: u8) -> u8 {
    ((u16::from(a) * u16::from(b) + 127) / 255) as u8
}

/// Composes the token. Output has the border's dimensions; its outline
/// (circle, square or hexagon) follows the border ([`detect_shape`]).
pub fn compose(base: &RgbaImage, border: &RgbaImage, params: &TokenParams) -> RgbaImage {
    compose_shaped(base, border, detect_shape(border), params)
}

/// [`compose`] with a known outline. A fully transparent border gives a
/// borderless token: the image clipped to `shape` (inset by the ring thickness).
pub fn compose_shaped(
    base: &RgbaImage,
    border: &RgbaImage,
    shape: TokenShape,
    params: &TokenParams,
) -> RgbaImage {
    let p = params.clone().clamped();
    let (w, h) = border.dimensions();
    let radius = f64::from(w.min(h)) / 2.0;
    let ei = (p.ring_inset_pct / 100.0 * radius).trunc();
    let content = match shape {
        TokenShape::Circle => {
            ellipse_mask(w, h, ei, ei, f64::from(w) - ei - 1.0, f64::from(h) - ei - 1.0)
        }
        _ => shape_mask(w, h, shape, f64::from(w) / 2.0, f64::from(h) / 2.0, radius - ei),
    };
    let mut fitted = fit_with_transform(base, w, h, p.pan, p.zoom);
    // On the border-sized window: preview and export match, and the ring is untouched.
    apply_token_filters(&mut fitted, &p.filters);
    let outer = outer_ring_mask(border, shape);

    let mut out = RgbaImage::new(w, h);
    for (i, (x, y, px)) in out.enumerate_pixels_mut().enumerate() {
        let m = content[i];
        if let Some([r, g, b]) = p.bg {
            *px = Rgba([r, g, b, m]);
        }
        let mut f = *fitted.get_pixel(x, y);
        f[3] = mul(f[3], m);
        over(px, &f);
        over(px, border.get_pixel(x, y));
        if let Some(o) = &outer {
            px[3] = mul(px[3], o[i]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ring border: opaque annulus between r_in and r_out, transparent elsewhere.
    pub(crate) fn ring(size: u32, r_in: f64, r_out: f64) -> RgbaImage {
        let c = f64::from(size) / 2.0;
        RgbaImage::from_fn(size, size, |x, y| {
            let d = ((f64::from(x) + 0.5 - c).powi(2) + (f64::from(y) + 0.5 - c).powi(2)).sqrt();
            if d >= r_in && d <= r_out { Rgba([200, 150, 0, 255]) } else { Rgba([0, 0, 0, 0]) }
        })
    }

    fn solid(w: u32, h: u32, c: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba(c))
    }

    #[test]
    fn output_has_border_size_and_is_clipped_to_outer_circle() {
        let border = ring(100, 40.0, 50.0);
        let t = compose(&solid(300, 200, [10, 200, 30, 255]), &border, &TokenParams::default());
        assert_eq!(t.dimensions(), (100, 100));
        assert_eq!(t.get_pixel(0, 0)[3], 0, "corner outside the circle is transparent");
        assert_eq!(*t.get_pixel(50, 50), Rgba([10, 200, 30, 255]), "centre shows the image");
        assert_eq!(t.get_pixel(50, 5)[0], 200, "ring is drawn over the image");
    }

    #[test]
    fn ring_inset_reveals_background_or_transparency() {
        let border = ring(100, 46.0, 50.0);
        let params = TokenParams { ring_inset_pct: 30.0, ..Default::default() };
        let img = solid(100, 100, [255, 0, 0, 255]);
        let t = compose(&img, &border, &params);
        // Between the inset circle (r=35) and the ring (r=46): transparent.
        assert_eq!(t.get_pixel(50, 10)[3], 0);
        let with_bg = compose(&img, &border, &TokenParams { bg: Some([0, 0, 255]), ..params });
        assert_eq!(with_bg.get_pixel(50, 10)[3], 0, "bg is also limited to the inset circle");
        assert_eq!(*with_bg.get_pixel(50, 50), Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn background_shows_through_transparent_source() {
        let border = ring(64, 30.0, 32.0);
        let t = compose(
            &solid(10, 10, [0, 0, 0, 0]),
            &border,
            &TokenParams { bg: Some([1, 2, 3]), ..Default::default() },
        );
        assert_eq!(*t.get_pixel(32, 32), Rgba([1, 2, 3, 255]));
    }

    #[test]
    fn pan_moves_the_view_like_python() {
        // Left half red, right half blue; cover-fit 200x100 into 100x100 => scale 1.
        let img = RgbaImage::from_fn(200, 100, |x, _| {
            if x < 100 { Rgba([255, 0, 0, 255]) } else { Rgba([0, 0, 255, 255]) }
        });
        let border = ring(100, 49.0, 50.0);
        let centre = |pan: [f64; 2]| {
            *compose(&img, &border, &TokenParams { pan, ..Default::default() }).get_pixel(50, 50)
        };
        assert_eq!(centre([-40.0, 0.0]), Rgba([255, 0, 0, 255]), "pan left shows the left half");
        assert_eq!(centre([40.0, 0.0]), Rgba([0, 0, 255, 255]));
        let far = compose(&img, &border, &TokenParams { pan: [500.0, 0.0], ..Default::default() });
        assert_eq!(far.get_pixel(50, 50)[3], 0, "panned past the image edge is empty");
    }

    #[test]
    fn zoom_scales_around_centre() {
        // A small centred white square on black: zooming in makes it cover more.
        let img = RgbaImage::from_fn(100, 100, |x, y| {
            if (40..60).contains(&x) && (40..60).contains(&y) {
                Rgba([255; 4])
            } else {
                Rgba([0, 0, 0, 255])
            }
        });
        let border = ring(100, 49.0, 50.0);
        let white = |zoom: f64| {
            compose(&img, &border, &TokenParams { zoom, ..Default::default() })
                .pixels()
                .filter(|p| p[0] > 200)
                .count()
        };
        let (one, two) = (white(1.0), white(2.0));
        assert!((350..=450).contains(&one), "{one}");
        assert!((two as f64 / one as f64 - 4.0).abs() < 0.4, "2x zoom ≈ 4x area: {one} -> {two}");
    }

    #[test]
    fn extreme_zoom_and_pan_never_panic() {
        let img = RgbaImage::from_pixel(333, 217, Rgba([9, 9, 9, 255]));
        let border = ring(101, 45.0, 50.0);
        for zoom in [0.1, 0.37, 0.6, 1.0, 3.3, 10.0] {
            for pan in [[0.0, 0.0], [-77.0, 13.0], [500.0, -500.0], [0.5, 0.5]] {
                let t = compose(&img, &border, &TokenParams { zoom, pan, ..Default::default() });
                assert_eq!(t.dimensions(), (101, 101));
            }
        }
    }

    #[test]
    fn filters_change_the_image_but_not_the_ring() {
        let border = ring(100, 40.0, 50.0);
        let img = solid(100, 100, [200, 40, 40, 255]);
        let step = |kind: &str| FilterStep {
            kind: kind.into(),
            enabled: true,
            params: Default::default(),
        };
        let plain = compose(&img, &border, &TokenParams::default());
        let grey = compose(
            &img,
            &border,
            &TokenParams { filters: vec![step("grayscale")], ..Default::default() },
        );
        let c = grey.get_pixel(50, 50);
        assert!(c[0] == c[1] && c[1] == c[2], "centre is grey: {c:?}");
        assert_eq!(grey.get_pixel(50, 5), plain.get_pixel(50, 5), "ring untouched");
        // map-only filters (ink-saving B&W) are ignored on tokens
        let dither = compose(
            &img,
            &border,
            &TokenParams { filters: vec![step("dither")], ..Default::default() },
        );
        assert_eq!(dither, plain);
        // old params_json without the field still loads
        let old: TokenParams = serde_json::from_str(r#"{"border":"b","zoom":2.0}"#).unwrap();
        assert!(old.filters.is_empty());
    }

    #[test]
    fn token_filters_are_known_filters() {
        let defs = filters::filter_defs();
        assert!(TOKEN_FILTERS.iter().all(|k| defs.iter().any(|d| d.key == *k)));
        assert_eq!(token_filter_defs().len(), TOKEN_FILTERS.len());
        let names = token_preset_names();
        assert!(names.contains(&"Sepia print") && names.contains(&"Clean grayscale"));
        assert!(!names.contains(&"Newsprint"), "dither is map-only");
        let p = token_preset_pipeline("Faded parchment").unwrap();
        assert_eq!(p.len(), TOKEN_FILTERS.len());
        assert_eq!(p.iter().filter(|s| s.enabled).count(), 3);
    }

    #[test]
    fn params_are_clamped() {
        let p = TokenParams {
            ring_inset_pct: 80.0,
            zoom: 0.0,
            pan: [f64::NAN, 3.0],
            ..Default::default()
        }
        .clamped();
        assert_eq!((p.ring_inset_pct, p.zoom, p.pan), (50.0, 0.1, [0.0, 3.0]));
    }

    #[test]
    fn ellipse_mask_is_symmetric_and_antialiased() {
        let m = ellipse_mask(21, 21, 0.0, 0.0, 20.0, 20.0);
        assert_eq!(m[10 * 21 + 10], 255);
        assert_eq!(m[0], 0);
        for y in 0..21 {
            for x in 0..21 {
                assert_eq!(m[y * 21 + x], m[y * 21 + (20 - x)], "mirror symmetry");
            }
        }
        assert!(m.iter().any(|&v| v > 0 && v < 255), "edges are anti-aliased");
    }

    /// A frame of `shape`: opaque between radius `r_in` and `r_out` (shape units).
    fn frame(size: u32, shape: TokenShape, r_in: f64, r_out: f64) -> RgbaImage {
        let c = f64::from(size) / 2.0;
        RgbaImage::from_fn(size, size, |x, y| {
            let (px, py) = (f64::from(x) + 0.5 - c, f64::from(y) + 0.5 - c);
            let inside = |r: f64| shape.contains(px / r, py / r);
            if inside(r_out) && !inside(r_in) { Rgba([200, 150, 0, 255]) } else { Rgba([0; 4]) }
        })
    }

    #[test]
    fn shapes_are_detected_from_alpha() {
        assert_eq!(detect_shape(&ring(100, 40.0, 50.0)), TokenShape::Circle);
        assert_eq!(detect_shape(&frame(100, TokenShape::Square, 40.0, 50.0)), TokenShape::Square);
        for shape in TokenShape::ALL {
            assert_eq!(detect_shape(&frame(100, shape, 40.0, 50.0)), shape, "{shape:?} frame");
            assert_eq!(detect_shape(&frame(442, shape, 45.0, 221.0)), shape, "{shape:?} big");
        }
        assert_eq!(detect_shape(&solid(30, 20, [1, 2, 3, 255])), TokenShape::Square, "opaque");
        assert_eq!(detect_shape(&RgbaImage::new(8, 8)), TokenShape::Circle);
        assert_eq!(serde_json::to_string(&TokenShape::Hex).unwrap(), r#""hex""#);
    }

    #[test]
    fn square_tokens_fill_the_corners() {
        let border = frame(100, TokenShape::Square, 42.0, 50.0);
        let img = solid(300, 200, [10, 200, 30, 255]);
        let t = compose(&img, &border, &TokenParams::default());
        assert_eq!(t.get_pixel(1, 1)[3], 255, "the frame's corner is kept");
        assert_eq!(*t.get_pixel(12, 12), Rgba([10, 200, 30, 255]), "image reaches the corner");
        let inset =
            compose(&img, &border, &TokenParams { ring_inset_pct: 30.0, ..Default::default() });
        assert_eq!(inset.get_pixel(12, 12)[3], 0, "the inset is square too");
        assert_eq!(*inset.get_pixel(50, 50), Rgba([10, 200, 30, 255]));
    }

    #[test]
    fn hex_tokens_are_clipped_to_the_hexagon() {
        let border = frame(100, TokenShape::Hex, 42.0, 50.0);
        let t = compose(&solid(100, 100, [255, 0, 0, 255]), &border, &TokenParams::default());
        assert_eq!(t.get_pixel(50, 1)[3], 255, "top vertex");
        assert_eq!(t.get_pixel(3, 50)[3], 0, "outside the left side");
        assert_eq!(t.get_pixel(15, 15)[3], 0, "corner is cut");
        assert_eq!(*t.get_pixel(50, 50), Rgba([255, 0, 0, 255]));
        // Borderless: a blank border, the image clipped to the hexagon.
        let bare = compose_shaped(
            &solid(100, 100, [255, 0, 0, 255]),
            &RgbaImage::new(100, 100),
            TokenShape::Hex,
            &TokenParams::default(),
        );
        assert_eq!(*bare.get_pixel(50, 1), Rgba([255, 0, 0, 255]), "image up to the vertex");
        assert_eq!(bare.get_pixel(3, 50)[3], 0);
        let pts = TokenShape::Hex.vertices(50.0, 50.0, 50.0);
        assert_eq!(pts.len(), 6);
        assert!(pts.iter().any(|&(x, y)| (x - 50.0).abs() < 1e-9 && y.abs() < 1e-9), "top vertex");
        assert!(pts.iter().any(|&(x, y)| (x - 93.30127).abs() < 1e-4 && (y - 25.0).abs() < 1e-9));
    }

    #[test]
    fn shapes_fit_the_canvas_and_match_their_vertices() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        for shape in TokenShape::ALL {
            let (ex, ey) = shape.extents();
            assert!(close(ex.max(ey), 1.0), "{shape:?} touches the canvas");
            for (x, y) in shape.vertices(0.0, 0.0, 1.0) {
                assert!(
                    shape.contains(x * 0.999, y * 0.999) && !shape.contains(x * 1.01, y * 1.01)
                );
            }
        }
        assert_eq!(TokenShape::Octagon.vertices(0.0, 0.0, 1.0).len(), 8);
        assert!(close(TokenShape::Decagon.extents().1, 18f64.to_radians().cos()));
        assert!(close(TokenShape::Hex.extents().0, 3f64.sqrt() / 2.0));
        assert!(TokenShape::Octagon.contains(0.0, 1.0) && TokenShape::Octagon.contains(1.0, 0.0));
        assert!(!TokenShape::Octagon.contains(0.8, 0.8), "the corners are cut");
    }

    #[test]
    fn alpha_bbox_matches_pil_semantics() {
        let b = ring(50, 10.0, 20.0);
        let (x0, y0, x1, y1) = alpha_bbox(&b).unwrap();
        assert_eq!((x0, y0, x1, y1), (5, 5, 45, 45));
        assert_eq!(alpha_bbox(&RgbaImage::new(4, 4)), None);
    }
}
