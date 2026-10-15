//! Perspective de-warp (Python `map_grid.py` + `_dewarp_from_reference`).
//!
//! For maps photographed at an angle, the four corners of a known N×N block
//! fully determine the perspective, so rectifying that block flattens the whole
//! image.

use image::RgbImage;
use rayon::prelude::*;
use serde::Serialize;

use super::grid::dist;

pub type Pt = (f64, f64);

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WarpError {
    #[error("degenerate corner selection")]
    Degenerate,
}

/// Sorts four points into `[TL, TR, BR, BL]` by x+y and x−y, so they may be
/// clicked in any order.
pub fn order_quad_corners(pts: &[Pt; 4]) -> [Pt; 4] {
    let mut by_sum = pts.to_vec();
    by_sum.sort_by(|a, b| (a.0 + a.1).total_cmp(&(b.0 + b.1)));
    let mut by_diff = pts.to_vec();
    by_diff.sort_by(|a, b| (a.0 - a.1).total_cmp(&(b.0 - b.1)));
    [by_sum[0], by_diff[3], by_sum[3], by_diff[0]]
}

/// Dense Gaussian elimination with partial pivoting (Gauss–Jordan, as Python).
fn solve_linear(mut aug: Vec<[f64; 9]>) -> Result<[f64; 8], WarpError> {
    let n = 8;
    for col in 0..n {
        let mut pivot = col;
        for r in col + 1..n {
            if aug[r][col].abs() > aug[pivot][col].abs() {
                pivot = r;
            }
        }
        if aug[pivot][col].abs() < 1e-12 {
            return Err(WarpError::Degenerate);
        }
        aug.swap(col, pivot);
        let pv = aug[col][col];
        for r in 0..n {
            if r == col {
                continue;
            }
            let factor = aug[r][col] / pv;
            if factor != 0.0 {
                let prow = aug[col];
                for (v, p) in aug[r][col..].iter_mut().zip(&prow[col..]) {
                    *v -= factor * p;
                }
            }
        }
    }
    let mut x = [0.0; 8];
    for r in 0..n {
        x[r] = aug[r][n] / aug[r][r];
    }
    Ok(x)
}

fn system(from: &[Pt; 4], to: &[Pt; 4]) -> Vec<[f64; 9]> {
    let mut a = Vec::with_capacity(8);
    for (&(x, y), &(tx, ty)) in from.iter().zip(to) {
        a.push([x, y, 1.0, 0.0, 0.0, 0.0, -x * tx, -y * tx, tx]);
        a.push([0.0, 0.0, 0.0, x, y, 1.0, -x * ty, -y * ty, ty]);
    }
    a
}

/// The 8 coefficients PIL's `Image.transform(PERSPECTIVE)` takes: they map each
/// *output* point in `dst` to the matching *input* point in `src`.
pub fn perspective_coeffs(dst: &[Pt; 4], src: &[Pt; 4]) -> Result<[f64; 8], WarpError> {
    solve_linear(system(dst, src))
}

/// Row-major 3×3 homography mapping each point of `src` onto `dst`.
pub fn homography_matrix(src: &[Pt; 4], dst: &[Pt; 4]) -> Result<[[f64; 3]; 3], WarpError> {
    let h = solve_linear(system(src, dst))?;
    Ok([[h[0], h[1], h[2]], [h[3], h[4], h[5]], [h[6], h[7], 1.0]])
}

pub fn apply_homography(m: &[[f64; 3]; 3], x: f64, y: f64) -> Pt {
    let mut d = m[2][0] * x + m[2][1] * y + m[2][2];
    if d == 0.0 {
        d = 1e-9;
    }
    ((m[0][0] * x + m[0][1] * y + m[0][2]) / d, (m[1][0] * x + m[1][1] * y + m[1][2]) / d)
}

/// Output canvas size cap for a de-warp.
pub const DEWARP_CAP: i64 = 8000;

/// Everything needed to rectify a map from a reference block.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DewarpPlan {
    pub out_w: u32,
    pub out_h: u32,
    /// Output→input coefficients for [`warp_perspective`].
    pub coeffs: [f64; 8],
    /// One square in the rectified image, px.
    pub cell_px: u32,
    /// Grid phase in the rectified image, px (rounded to 2 decimals).
    pub offset_x: f64,
    pub offset_y: f64,
    /// Top-left of the rectified reference block in the output, px.
    pub block_x: f64,
    pub block_y: f64,
}

/// Plans the de-warp: maps the block to an axis-aligned N×N square anchored at
/// its own top-left, projects the image corners to size the output, and
/// shrinks `cell_px` (twice at most) to keep the output within 8000 px.
pub fn plan_dewarp(
    quad: &[Pt; 4],
    n_blocks: i64,
    img_w: u32,
    img_h: u32,
) -> Result<DewarpPlan, WarpError> {
    let n = n_blocks.max(1);
    let src = order_quad_corners(quad);
    let [tl, tr, br, bl] = src;
    let (top, bottom) = (dist(tl, tr), dist(bl, br));
    let (left, right) = (dist(tl, bl), dist(tr, br));
    let nf = n as f64;
    let cell = ((top + bottom) / 2.0 / nf).max((left + right) / 2.0 / nf);
    let mut cell_px = (cell.round_ties_even() as i64).max(2);
    let (w, h) = (f64::from(img_w), f64::from(img_h));
    let (bx, by) = tl;
    let (mut out_w, mut out_h, mut minx, mut miny, mut dst_block);
    let mut attempt = 0;
    loop {
        let side = (n * cell_px) as f64;
        dst_block = [(bx, by), (bx + side, by), (bx + side, by + side), (bx, by + side)];
        let fwd = homography_matrix(&src, &dst_block)?;
        let mapped =
            [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(|(x, y)| apply_homography(&fwd, x, y));
        let xs = mapped.map(|p| p.0);
        let ys = mapped.map(|p| p.1);
        let fmin = |v: [f64; 4]| v.into_iter().fold(f64::INFINITY, f64::min);
        let fmax = |v: [f64; 4]| v.into_iter().fold(f64::NEG_INFINITY, f64::max);
        minx = fmin(xs).floor();
        miny = fmin(ys).floor();
        out_w = fmax(xs).ceil() as i64 - minx as i64;
        out_h = fmax(ys).ceil() as i64 - miny as i64;
        attempt += 1;
        if out_w.max(out_h) <= DEWARP_CAP || attempt == 2 {
            break;
        }
        cell_px = ((cell_px as f64 * DEWARP_CAP as f64 / out_w.max(out_h) as f64) as i64).max(2);
    }
    let out_w = out_w.clamp(1, DEWARP_CAP) as u32;
    let out_h = out_h.clamp(1, DEWARP_CAP) as u32;
    let shifted = dst_block.map(|(x, y)| (x - minx, y - miny));
    let coeffs = perspective_coeffs(&shifted, &src)?;
    let c = cell_px as f64;
    let r2 = |v: f64| (v * 100.0).round_ties_even() / 100.0;
    Ok(DewarpPlan {
        out_w,
        out_h,
        coeffs,
        cell_px: cell_px as u32,
        offset_x: r2((bx - minx).rem_euclid(c)),
        offset_y: r2((by - miny).rem_euclid(c)),
        block_x: bx - minx,
        block_y: by - miny,
    })
}

/// PIL-style perspective transform with bicubic sampling and a white fill:
/// output pixel centre (x+.5, y+.5) samples the input at the projected point.
pub fn warp_perspective(src: &RgbImage, out_w: u32, out_h: u32, c: &[f64; 8]) -> RgbImage {
    let (iw, ih) = (src.width() as i64, src.height() as i64);
    let raw = src.as_raw();
    let mut out = vec![255u8; out_w as usize * out_h as usize * 3];
    out.par_chunks_mut(out_w as usize * 3).enumerate().for_each(|(y, row)| {
        let yo = y as f64 + 0.5;
        for x in 0..out_w as usize {
            let xo = x as f64 + 0.5;
            let d = c[6] * xo + c[7] * yo + 1.0;
            let xin = (c[0] * xo + c[1] * yo + c[2]) / d;
            let yin = (c[3] * xo + c[4] * yo + c[5]) / d;
            if !(xin >= 0.0 && yin >= 0.0 && xin < iw as f64 && yin < ih as f64) {
                continue;
            }
            let (fx, fy) = (xin - 0.5, yin - 0.5);
            let (x0, y0) = (fx.floor() as i64, fy.floor() as i64);
            let (dx, dy) = (fx - x0 as f64, fy - y0 as f64);
            for ch in 0..3 {
                let mut col = [0.0; 4];
                for (j, cv) in col.iter_mut().enumerate() {
                    let sy = (y0 - 1 + j as i64).clamp(0, ih - 1) as usize;
                    let px = |k: i64| {
                        let sx = (x0 - 1 + k).clamp(0, iw - 1) as usize;
                        f64::from(raw[(sy * iw as usize + sx) * 3 + ch])
                    };
                    *cv = bicubic(px(0), px(1), px(2), px(3), dx);
                }
                let v = bicubic(col[0], col[1], col[2], col[3], dy);
                // PIL truncates (bicubic_filter32RGB)
                row[x * 3 + ch] = v.clamp(0.0, 255.0) as u8;
            }
        }
    });
    RgbImage::from_raw(out_w, out_h, out).expect("buffer size matches dimensions")
}

/// PIL Geometry.c `BICUBIC` (a = −1 cubic convolution).
fn bicubic(v1: f64, v2: f64, v3: f64, v4: f64, d: f64) -> f64 {
    let p1 = v2;
    let p2 = -v1 + v3;
    let p3 = 2.0 * (v1 - v2) + v3 - v4;
    let p4 = -v1 + v2 - v3 + v4;
    p1 + d * (p2 + d * (p3 + d * p4))
}

/// Convenience: plan + warp. Returns the rectified image and the plan.
pub fn dewarp(
    img: &RgbImage,
    quad: &[Pt; 4],
    n_blocks: i64,
) -> Result<(RgbImage, DewarpPlan), WarpError> {
    let plan = plan_dewarp(quad, n_blocks, img.width(), img.height())?;
    let out = warp_perspective(img, plan.out_w, plan.out_h, &plan.coeffs);
    Ok((out, plan))
}

#[cfg(test)]
mod tests {
    use super::*;

    use image::Rgb;

    #[test]
    fn identity_quad_keeps_image() {
        let mut img = RgbImage::from_pixel(60, 40, Rgb([10, 20, 30]));
        img.put_pixel(30, 20, Rgb([200, 0, 0]));
        let quad = [(10.0, 10.0), (40.0, 10.0), (40.0, 40.0), (10.0, 40.0)];
        let (out, plan) = dewarp(&img, &quad, 3).unwrap();
        assert_eq!((plan.out_w, plan.out_h, plan.cell_px), (60, 40, 10));
        assert_eq!((plan.offset_x, plan.offset_y), (0.0, 0.0));
        assert_eq!(out.get_pixel(5, 5).0, [10, 20, 30]);
        assert_eq!(out.get_pixel(30, 20).0, [200, 0, 0]);
    }

    #[test]
    fn skewed_quad_becomes_square_block() {
        let quad = [(300.0, 120.0), (100.0, 100.0), (90.0, 300.0), (290.0, 320.0)];
        let plan = plan_dewarp(&quad, 4, 400, 400).unwrap();
        let side = f64::from(plan.cell_px * 4);
        let c = plan.coeffs;
        let map = |x: f64, y: f64| {
            let d = c[6] * x + c[7] * y + 1.0;
            ((c[0] * x + c[1] * y + c[2]) / d, (c[3] * x + c[4] * y + c[5]) / d)
        };
        let [tl, tr, br, bl] = order_quad_corners(&quad);
        let (x, y) = (plan.block_x, plan.block_y);
        for (out, want) in
            [((x, y), tl), ((x + side, y), tr), ((x + side, y + side), br), ((x, y + side), bl)]
        {
            let got = map(out.0, out.1);
            assert!(
                (got.0 - want.0).abs() < 1e-6 && (got.1 - want.1).abs() < 1e-6,
                "{got:?} vs {want:?}"
            );
        }
        assert_eq!(
            plan.offset_x,
            (x.rem_euclid(f64::from(plan.cell_px)) * 100.0).round_ties_even() / 100.0
        );
    }

    #[test]
    fn cap_shrinks_cell() {
        // Extreme perspective blows the output up; the cap keeps it bounded.
        let quad = [(0.0, 0.0), (100.0, 40.0), (100.0, 60.0), (0.0, 100.0)];
        let plan = plan_dewarp(&quad, 1, 5000, 5000).unwrap();
        assert!(plan.out_w <= 8000 && plan.out_h <= 8000);
    }

    #[test]
    fn degenerate_is_an_error() {
        let q = [(0.0, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0)];
        assert_eq!(
            homography_matrix(&q, &[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]),
            Err(WarpError::Degenerate)
        );
    }
}
