//! Token compositing: frame an image inside a circular border ring.
//! Python reference: `ui/token_compositing.py`. Spec: docs/features/token-cutter.md.

use fast_image_resize::{
    self as fr,
    images::{Image as FrImage, ImageRef},
};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

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
}

impl Default for TokenParams {
    fn default() -> Self {
        TokenParams {
            border: String::new(),
            bg: None,
            ring_inset_pct: 0.0,
            pan: [0.0, 0.0],
            zoom: 1.0,
        }
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

/// Mask filled inside the border's outer circle (from its alpha bounding box).
fn outer_ring_mask(border: &RgbaImage) -> Option<Vec<u8>> {
    let (bx0, by0, bx1, by1) = alpha_bbox(border)?;
    let cx = f64::from(bx0 + bx1) / 2.0;
    let cy = f64::from(by0 + by1) / 2.0;
    let r = f64::from((bx1 - bx0).min(by1 - by0)) / 2.0;
    let (w, h) = border.dimensions();
    // PIL draws ellipse([cx-r, cy-r, cx+r, cy+r]) with an inclusive end coordinate.
    Some(ellipse_mask(w, h, cx - r, cy - r, cx + r - 1.0, cy + r - 1.0))
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

/// Composes the token. Output has the border's dimensions.
pub fn compose(base: &RgbaImage, border: &RgbaImage, params: &TokenParams) -> RgbaImage {
    let p = params.clone().clamped();
    let (w, h) = border.dimensions();
    let radius = f64::from(w.min(h)) / 2.0;
    let ei = (p.ring_inset_pct / 100.0 * radius).trunc();
    let content = ellipse_mask(w, h, ei, ei, f64::from(w) - ei - 1.0, f64::from(h) - ei - 1.0);
    let fitted = fit_with_transform(base, w, h, p.pan, p.zoom);
    let outer = outer_ring_mask(border);

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

    #[test]
    fn alpha_bbox_matches_pil_semantics() {
        let b = ring(50, 10.0, 20.0);
        let (x0, y0, x1, y1) = alpha_bbox(&b).unwrap();
        assert_eq!((x0, y0, x1, y1), (5, 5, 45, 45));
        assert_eq!(alpha_bbox(&RgbaImage::new(4, 4)), None);
    }
}
