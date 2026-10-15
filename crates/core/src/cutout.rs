//! Cut-outs: lasso masking, sticker-style contour outline, fold-over standees.
//! Python reference: `ui/cutout_compositing.py`. Spec: docs/features/object-cutter.md.

use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::token::alpha_bbox;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LassoMode {
    #[default]
    Polygon,
    Freehand,
}

/// Editable cut-out parameters (persisted as the asset's `params_json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CutoutParams {
    pub mode: LassoMode,
    /// Selection polygon in source-image pixels.
    pub points: Vec<[f64; 2]>,
    pub outline_rgb: [u8; 3],
    /// Outline thickness in pixels (0–40).
    pub outline_px: u32,
}

impl Default for CutoutParams {
    fn default() -> Self {
        CutoutParams {
            mode: LassoMode::Polygon,
            points: vec![],
            outline_rgb: [255, 255, 255],
            outline_px: 6,
        }
    }
}

/// Supersampling per axis for anti-aliased polygon edges.
const AA: usize = 4;

/// Coverage mask (0–255) of a polygon (even-odd rule), anti-aliased.
///
/// Like PIL, pixel `(x, y)` is treated as the point at integer coordinates
/// (not the pixel centre), so a polygon from 20 to 300 covers columns 20..=300.
/// Sub-samples are spread ±0.5 px around that point, so boundary pixels get
/// partial coverage instead of PIL's hard edge.
pub fn polygon_mask(w: u32, h: u32, points: &[[f64; 2]]) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let mut out = vec![0u8; w * h];
    if points.len() < 3 || w == 0 || h == 0 {
        return out;
    }
    let mut cov = vec![0u16; w];
    let mut xs: Vec<f64> = Vec::new();
    let min_y =
        (points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min) - 0.5).floor().max(0.0) as usize;
    let max_y =
        ((points.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max) + 0.5).ceil() as usize + 1)
            .min(h);
    for y in min_y..max_y {
        cov.iter_mut().for_each(|c| *c = 0);
        for sy in 0..AA {
            let py = y as f64 - 0.5 + (sy as f64 + 0.5) / AA as f64;
            xs.clear();
            for i in 0..points.len() {
                let (a, b) = (points[i], points[(i + 1) % points.len()]);
                if (a[1] <= py) != (b[1] <= py) {
                    xs.push(a[0] + (py - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
                }
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.as_chunks::<2>().0 {
                // Sub-sample s sits at x = (s + 0.5) / AA - 0.5; keep those in [x0, x1].
                let to_sub =
                    |x: f64| (((x + 0.5) * AA as f64 - 0.5).ceil().max(0.0) as usize).min(w * AA);
                for s in to_sub(pair[0])..to_sub(pair[1]) {
                    cov[s / AA] += 1;
                }
            }
        }
        for x in 0..w {
            out[y * w + x] =
                ((u32::from(cov[x]) * 255 + (AA * AA) as u32 / 2) / (AA * AA) as u32) as u8;
        }
    }
    out
}

fn crop_to_alpha(img: RgbaImage) -> RgbaImage {
    match alpha_bbox(&img) {
        Some((x0, y0, x1, y1)) => {
            image::imageops::crop_imm(&img, x0, y0, x1 - x0, y1 - y0).to_image()
        }
        None => img,
    }
}

/// Masks `img` to the selection and crops tight (Python `apply_lasso_mask`).
/// With fewer than 3 points everything becomes transparent and nothing is cropped.
pub fn apply_lasso(img: &RgbaImage, points: &[[f64; 2]]) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mask = polygon_mask(w, h, points);
    let mut out = img.clone();
    for (p, m) in out.pixels_mut().zip(mask) {
        p[3] = ((u16::from(p[3]) * u16::from(m) + 127) / 255) as u8;
    }
    crop_to_alpha(out)
}

/// 1-D binary dilation by `r` in O(n): `out[i] = any(src[i-r ..= i+r])`.
fn dilate_line(src: &[bool], r: usize, out: &mut [bool]) {
    let n = src.len();
    let mut last: Option<usize> = None; // most recent set index to the left (inclusive)
    let mut left = vec![usize::MAX; n];
    for i in 0..n {
        if src[i] {
            last = Some(i);
        }
        left[i] = last.map_or(usize::MAX, |j| i - j);
    }
    let mut next: Option<usize> = None;
    for i in (0..n).rev() {
        if src[i] {
            next = Some(i);
        }
        let right = next.map_or(usize::MAX, |j| j - i);
        out[i] = left[i].min(right) <= r;
    }
}

/// Square (Chebyshev) dilation by `r`, identical to applying a 3×3 max filter `r` times.
pub fn dilate_square(mask: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    let mut rows = vec![false; w * h];
    for y in 0..h {
        dilate_line(&mask[y * w..(y + 1) * w], r, &mut rows[y * w..(y + 1) * w]);
    }
    let mut out = vec![false; w * h];
    let (mut col, mut res) = (vec![false; h], vec![false; h]);
    for x in 0..w {
        for y in 0..h {
            col[y] = rows[y * w + x];
        }
        dilate_line(&col, r, &mut res);
        for y in 0..h {
            out[y * w + x] = res[y];
        }
    }
    out
}

/// Straight-alpha "over".
fn over(dst: &mut Rgba<u8>, src: &Rgba<u8>) {
    let sa = f32::from(src[3]) / 255.0;
    if sa <= 0.0 {
        return;
    }
    let da = f32::from(dst[3]) / 255.0;
    let oa = sa + da * (1.0 - sa);
    for c in 0..3 {
        dst[c] =
            ((f32::from(src[c]) * sa + f32::from(dst[c]) * da * (1.0 - sa)) / oa).round() as u8;
    }
    dst[3] = (oa * 255.0).round() as u8;
}

/// Solid outline hugging the silhouette (Python `add_contour_outline`).
pub fn add_outline(img: &RgbaImage, thickness: u32, rgb: [u8; 3]) -> RgbaImage {
    if thickness == 0 {
        return img.clone();
    }
    let pad = thickness + 2;
    let (w, h) = (img.width() + 2 * pad, img.height() + 2 * pad);
    let mut padded = RgbaImage::new(w, h);
    image::imageops::replace(&mut padded, img, i64::from(pad), i64::from(pad));
    // Binarise alpha (> 16) so faint edges don't bleed into the stroke.
    let binary: Vec<bool> = padded.pixels().map(|p| p[3] > 16).collect();
    let dilated = dilate_square(&binary, w as usize, h as usize, thickness as usize);
    let mut out = RgbaImage::new(w, h);
    for (i, (p, src)) in out.pixels_mut().zip(padded.pixels()).enumerate() {
        if dilated[i] {
            *p = Rgba([rgb[0], rgb[1], rgb[2], 255]);
        }
        over(p, src);
    }
    crop_to_alpha(out)
}

/// Full cut-out pipeline: lasso, then outline.
pub fn make_cutout(img: &RgbaImage, params: &CutoutParams) -> RgbaImage {
    let cut = apply_lasso(img, &params.points);
    add_outline(&cut, params.outline_px.min(40), params.outline_rgb)
}

/// Fold-over standee: mirrored copy on top, upright below, dashed fold line on the
/// seam (Python `make_standee_block`, gap 0).
pub fn standee_block(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut block = RgbaImage::new(w, 2 * h);
    // Alpha-composite (not copy) onto the transparent canvas, like PIL: fully
    // transparent source pixels stay (0, 0, 0, 0).
    let flipped = image::imageops::flip_vertical(img);
    for (x, y, p) in flipped.enumerate_pixels() {
        over(block.get_pixel_mut(x, y), p);
    }
    for (x, y, p) in img.enumerate_pixels() {
        over(block.get_pixel_mut(x, y + h), p);
    }
    if h > 0 {
        // PIL draws 1px segments [x, x+6] every 12px, overwriting pixels.
        let mut x = 0;
        while x < w {
            for xx in x..=(x + 6).min(w - 1) {
                block.put_pixel(xx, h, Rgba([120, 120, 120, 220]));
            }
            x += 12;
        }
    }
    block
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn solid(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([200, 50, 50, 255]))
    }

    #[test]
    fn lasso_crops_to_the_selection() {
        let img = solid(100, 80);
        let out = apply_lasso(&img, &[[10.0, 10.0], [60.0, 10.0], [60.0, 50.0], [10.0, 50.0]]);
        assert_eq!(out.dimensions(), (51, 41), "PIL includes the boundary row/column");
        assert_eq!(out.get_pixel(25, 20)[3], 255, "interior fully opaque");
        assert!(
            out.get_pixel(0, 20)[3] > 100 && out.get_pixel(0, 20)[3] < 200,
            "edge partially covered"
        );
    }

    #[test]
    fn lasso_outside_polygon_is_transparent() {
        let img = solid(100, 100);
        let tri = [[0.0, 0.0], [100.0, 0.0], [0.0, 100.0]];
        let out = apply_lasso(&img, &tri);
        assert_eq!(out.get_pixel(95, 95)[3], 0, "below the diagonal");
        assert_eq!(out.get_pixel(5, 5)[3], 255);
    }

    #[test]
    fn fewer_than_three_points_selects_nothing() {
        let img = solid(10, 10);
        let out = apply_lasso(&img, &[[0.0, 0.0], [5.0, 5.0]]);
        assert!(out.pixels().all(|p| p[3] == 0));
    }

    #[test]
    fn outline_grows_by_exactly_thickness() {
        let img = solid(20, 10);
        let out = add_outline(&img, 6, [255, 255, 255]);
        assert_eq!(out.dimensions(), (32, 22));
        assert_eq!(
            *out.get_pixel(0, 0),
            Rgba([255, 255, 255, 255]),
            "square dilation fills corners"
        );
        assert_eq!(*out.get_pixel(16, 11), Rgba([200, 50, 50, 255]), "original on top");
        assert_eq!(add_outline(&img, 0, [0, 0, 0]), img);
    }

    #[test]
    fn faint_alpha_does_not_seed_the_outline() {
        let mut img = RgbaImage::from_pixel(9, 9, Rgba([0, 0, 0, 0]));
        img.put_pixel(4, 4, Rgba([0, 0, 0, 255]));
        img.put_pixel(0, 0, Rgba([0, 0, 0, 16]));
        let out = add_outline(&img, 2, [1, 2, 3]);
        // The outline (5×5 around the centre) is seeded only by the opaque pixel;
        // the faint pixel still counts for the crop box (PIL getbbox), so the
        // result spans from it: 7×7, with the faint pixel left un-outlined.
        assert_eq!(out.dimensions(), (7, 7));
        assert_eq!(out.get_pixel(0, 0)[3], 16, "faint pixel kept, no outline under it");
        assert_eq!(*out.get_pixel(2, 2), Rgba([1, 2, 3, 255]), "outline around the opaque pixel");
    }

    #[test]
    fn standee_mirrors_and_draws_fold_line() {
        let img = RgbaImage::from_fn(30, 4, |_, y| Rgba([y as u8 * 50, 0, 0, 255]));
        let s = standee_block(&img);
        assert_eq!(s.dimensions(), (30, 8));
        assert_eq!(s.get_pixel(0, 0)[0], 150, "top half is flipped");
        assert_eq!(s.get_pixel(0, 7)[0], 150, "bottom half upright");
        assert_eq!(*s.get_pixel(0, 4), Rgba([120, 120, 120, 220]), "dash at the seam");
        assert_eq!(*s.get_pixel(6, 4), Rgba([120, 120, 120, 220]), "dash covers x..=x+6");
        assert_ne!(s.get_pixel(8, 4)[3], 220, "gap between dashes");
        assert_eq!(*s.get_pixel(12, 4), Rgba([120, 120, 120, 220]));
    }

    fn naive_dilate(mask: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
        let mut cur = mask.to_vec();
        for _ in 0..r {
            let prev = cur.clone();
            for y in 0..h {
                for x in 0..w {
                    let mut any = false;
                    for dy in -1i64..=1 {
                        for dx in -1i64..=1 {
                            let (yy, xx) = (y as i64 + dy, x as i64 + dx);
                            if yy >= 0 && xx >= 0 && (yy as usize) < h && (xx as usize) < w {
                                any |= prev[yy as usize * w + xx as usize];
                            }
                        }
                    }
                    cur[y * w + x] = any;
                }
            }
        }
        cur
    }

    proptest! {
        #[test]
        fn separable_dilation_equals_repeated_max_filter(
            w in 1usize..14, h in 1usize..14, r in 0usize..5,
            bits in prop::collection::vec(prop::bool::weighted(0.1), 196)
        ) {
            let mask: Vec<bool> = bits[..w * h].to_vec();
            prop_assert_eq!(dilate_square(&mask, w, h, r), naive_dilate(&mask, w, h, r));
        }
    }
}
