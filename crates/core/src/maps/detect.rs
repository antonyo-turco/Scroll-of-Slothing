//! Automatic map analysis for the Map Paginator (new; Python had neither):
//! - [`detect_grid`] finds the embedded grid's square size and line offset;
//! - [`content_bounds`] finds the artwork inside uniform borders (scanner
//!   bars, blank paper).
//!
//! Both run on the working proxy; callers scale the results to full size.
//!
//! **Grid.** Grid lines are edges that repeat at a fixed spacing. For each axis
//! the image is reduced to an edge profile (summed |brightness step| per
//! column, or per row), high-passed so only sharp peaks remain, and probed
//! with a single-frequency Fourier sum at every candidate period:
//! `S(P) = Σ p[x]·e^(−2πi·x/P)`. A comb of lines `P` apart scores high at `P`
//! and at its harmonics `P/2, P/3…`, but near zero at `2P, 3P…`, so the
//! largest period that scores close to the best is the square size. The
//! phase of `S` gives where the lines sit. That estimate is then refined by
//! locating each individual line near its predicted position and fitting a
//! straight line through them (sub-pixel period and offset). An axis has no
//! grid unless its best score clearly beats the typical score *and* real,
//! evenly spaced lines are found at most predicted positions.

use image::RgbImage;
use serde::{Deserialize, Serialize};

/// A detected grid, in the analysed image's pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GridGuess {
    pub cell_w: f64,
    pub cell_h: f64,
    /// Position of one vertical / horizontal grid line, in `[0, cell)`.
    pub offset_x: f64,
    pub offset_y: f64,
    /// How far the best score stands out from the typical one (≥ `MIN_CONFIDENCE`).
    pub confidence: f64,
}

/// Smallest square the detector looks for, image px. Below this, texture and
/// JPEG's 8 px compression blocks win. Squares are also never smaller than
/// 1/80 of the image (no map is 80+ squares across at preview size).
pub const MIN_CELL_PX: f64 = 12.0;
const MAX_SQUARES_ACROSS: f64 = 80.0;
/// The best period must score this many times the median score.
const MIN_CONFIDENCE: f64 = 4.0;
/// A sub-period counts as the fundamental's harmonic if it scores this close.
const HARMONIC_RATIO: f64 = 0.8;
/// Edge energy on the lines vs between them.
const MIN_CONTRAST: f64 = 2.5;
/// One of k interleaved sub-combs must carry this much more energy than the
/// others to be taken as the grid on top of finer texture.
const SUBCOMB_DOMINANCE: f64 = 1.35;
/// Readings with more squares than this across an axis are suspected texture.
const PLAUSIBLE_SQUARES: f64 = 60.0;

fn luma(img: &RgbImage) -> Vec<f32> {
    img.pixels()
        .map(|p| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
        .collect()
}

/// Summed absolute brightness step between neighbouring columns (`vertical`
/// lines) or rows; entry `i` is the boundary at position `i + 1`.
fn edge_profile(g: &[f32], w: usize, h: usize, columns: bool) -> Vec<f64> {
    if columns {
        let mut p = vec![0.0f64; w.saturating_sub(1)];
        for y in 0..h {
            let row = &g[y * w..(y + 1) * w];
            for x in 0..w - 1 {
                p[x] += f64::from((row[x + 1] - row[x]).abs());
            }
        }
        p
    } else {
        let mut p = vec![0.0f64; h.saturating_sub(1)];
        for (y, v) in p.iter_mut().enumerate() {
            let (a, b) = (&g[y * w..(y + 1) * w], &g[(y + 1) * w..(y + 2) * w]);
            *v = a.iter().zip(b).map(|(a, b)| f64::from((b - a).abs())).sum();
        }
        p
    }
}

/// Keeps only what rises above a local moving average (sharp line peaks).
fn high_pass(p: &[f64], radius: usize) -> Vec<f64> {
    let n = p.len();
    let mut prefix = vec![0.0; n + 1];
    for (i, v) in p.iter().enumerate() {
        prefix[i + 1] = prefix[i] + v;
    }
    (0..n)
        .map(|i| {
            let (a, b) = (i.saturating_sub(radius), (i + radius + 1).min(n));
            let mean = (prefix[b] - prefix[a]) / (b - a) as f64;
            (p[i] - mean).max(0.0)
        })
        .collect()
}

/// `|S(P)| / Σp` and the phase-derived line position for one period.
fn probe(p: &[f64], total: f64, period: f64) -> (f64, f64) {
    let w = std::f64::consts::TAU / period;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, v) in p.iter().enumerate() {
        if *v == 0.0 {
            continue;
        }
        // entry i is the boundary at position i + 1
        let a = w * (i as f64 + 1.0);
        re += v * a.cos();
        im -= v * a.sin();
    }
    let score = (re * re + im * im).sqrt() / total;
    let offset = (-im.atan2(re) / w).rem_euclid(period);
    (score, offset)
}

/// Best period and line offset of one axis' profile, if a grid stands out.
fn axis_grid(profile: &[f64]) -> Option<(f64, f64, f64)> {
    let n = profile.len();
    let max_p = n as f64 / 4.0; // at least four squares across
    let min_p = MIN_CELL_PX.max(n as f64 / MAX_SQUARES_ACROSS);
    if max_p <= min_p {
        return None;
    }
    let p = high_pass(profile, 4);
    let total: f64 = p.iter().sum();
    if total <= 0.0 {
        return None;
    }
    // coarse scan: 0.25 % steps in period
    let mut scores = Vec::new();
    let mut period = min_p;
    while period <= max_p {
        scores.push((period, probe(&p, total, period).0));
        period *= 1.0025;
    }
    let best = scores.iter().map(|s| s.1).fold(0.0, f64::max);
    let mut sorted: Vec<f64> = scores.iter().map(|s| s.1).collect();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2].max(1e-12);
    if best / median < MIN_CONFIDENCE {
        return None;
    }
    // the fundamental: the largest period scoring close to the best
    // (harmonics P/2, P/3… score as well as P; multiples 2P… do not)
    let (coarse, _) = scores.iter().rev().find(|(_, s)| *s >= HARMONIC_RATIO * best).copied()?;
    // Strong texture (bricks, tiles) can outscore a grid that is a few of
    // them wide, so multiples 2P…4P that still score well are candidates
    // too; the sharpest lines (on/between contrast) win.
    let mut found: Option<(f64, f64, f64)> = None; // (period, offset, contrast)
    for k in 1..=4 {
        let target = coarse * k as f64;
        if target > max_p {
            break;
        }
        let near = scores
            .iter()
            .filter(|(q, _)| (q / target - 1.0).abs() < 0.02)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let Some(&(near, near_score)) = near else { continue };
        if k > 1 && near_score < 0.35 * best {
            continue;
        }
        let (mut bp, mut bs) = (near, 0.0);
        let mut q = near * 0.995;
        while q <= near * 1.005 {
            let s = probe(&p, total, q).0;
            if s > bs {
                (bp, bs) = (q, s);
            }
            q += near * 0.0002;
        }
        let (_, offset) = probe(&p, total, bp);
        if let Some(r) = refine(&p, bp, offset, MIN_CONTRAST)
            && found.is_none_or(|f| r.2 > f.2 * 1.15)
        {
            found = Some(r);
        }
    }
    let (mut period, mut offset, _) = found?;
    // A grid drawn over fine regular texture (stone blocks a third of a
    // square wide) shows as the texture's period, with every k-th line
    // stronger: the grid lines. Of the k interleaved sub-combs, one then
    // clearly dominates; with texture alone they are all alike. Only tried
    // when the reading implies implausibly many squares: real grids whose
    // artwork repeats every few squares must not be promoted.
    let half = (period * 0.12).clamp(1.0, 6.0);
    let mut best_ratio = SUBCOMB_DOMINANCE;
    let max_k = if n as f64 / period > PLAUSIBLE_SQUARES { 4 } else { 1 };
    for k in 2..=max_k {
        let big = period * k as f64;
        if big > max_p {
            break;
        }
        let e: Vec<f64> =
            (0..k).map(|j| energy_at(&p, big, offset + j as f64 * period, half)).collect();
        let (jmax, emax) = e
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(j, v)| (j, *v))
            .unwrap_or((0, 0.0));
        let rest = (e.iter().sum::<f64>() - emax) / (k - 1) as f64;
        let ratio = emax / rest.max(1e-9);
        if ratio > best_ratio
            && let Some(r) = refine(&p, big, offset + jmax as f64 * period, 1.0)
        {
            best_ratio = ratio;
            (period, offset) = (r.0, r.1);
        }
    }
    Some((period, offset, best / median))
}

/// Edge energy within `half` of every `offset + k·period` position.
fn energy_at(p: &[f64], period: f64, offset: f64, half: f64) -> f64 {
    let n = p.len() as f64;
    let mut sum = 0.0;
    let mut x = offset.rem_euclid(period);
    while x <= n {
        let lo = ((x - half - 1.0).floor().max(0.0)) as usize;
        let hi = ((x + half - 1.0).ceil().max(0.0) as usize).min(p.len() - 1);
        sum += p[lo..=hi].iter().sum::<f64>();
        x += period;
    }
    sum
}

/// Locates each line near `offset + k·period` (weighted centroid of the
/// profile peak) and least-squares fits `position = a + k·P`. Returns the
/// refined `(P, a mod P)`, or `None` when it is not a grid: the edge energy
/// on the predicted lines must be ≥ 3× that halfway between them (random
/// texture gives about 1×), most predicted positions must hold a line, and
/// the lines must be evenly spaced. "Between" is sampled at 30 % and 70 % of
/// a square, not the midpoint, which some maps decorate (half-square tiles).
fn refine(p: &[f64], period: f64, offset: f64, min_contrast: f64) -> Option<(f64, f64, f64)> {
    let n = p.len() as f64;
    let half = (period * 0.12).clamp(1.0, 6.0);
    let on = energy_at(p, period, offset, half);
    let between = (energy_at(p, period, offset + 0.3 * period, half)
        + energy_at(p, period, offset + 0.7 * period, half))
        / 2.0;
    let contrast = on / between.max(1e-9);
    if contrast < min_contrast {
        return None;
    }
    let mut peaks: Vec<(f64, f64, f64)> = Vec::new(); // (k, position, weight)
    let mut k = 0.0;
    let mut expected = 0;
    loop {
        let x = offset + k * period;
        if x > n - 1.0 {
            break;
        }
        // entry i is the boundary at position i + 1
        let (lo, hi) = (
            ((x - half - 1.0).floor().max(0.0)) as usize,
            ((x + half - 1.0).ceil() as usize).min(p.len() - 1),
        );
        let (mut sw, mut sx) = (0.0, 0.0);
        for (i, v) in p.iter().enumerate().take(hi + 1).skip(lo) {
            sw += v;
            sx += v * (i as f64 + 1.0);
        }
        expected += 1;
        if sw > 0.0 {
            peaks.push((k, sx / sw, sw));
        }
        k += 1.0;
    }
    if expected < 4 {
        return None;
    }
    // a real line is a strong peak: keep those above a third of the median strength
    let mut ws: Vec<f64> = peaks.iter().map(|p| p.2).collect();
    ws.sort_by(f64::total_cmp);
    let strong = ws.get(ws.len() / 2).copied().unwrap_or(0.0) / 3.0;
    let pts: Vec<(f64, f64)> = peaks.iter().filter(|p| p.2 >= strong).map(|p| (p.0, p.1)).collect();
    if (pts.len() as f64) < 0.6 * expected as f64 || pts.len() < 4 {
        return None;
    }
    let m = pts.len() as f64;
    let (sk, sx) = pts.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
    let (mk, mx) = (sk / m, sx / m);
    let (mut num, mut den) = (0.0, 0.0);
    for (k, x) in &pts {
        num += (k - mk) * (x - mx);
        den += (k - mk) * (k - mk);
    }
    if den == 0.0 {
        return None;
    }
    let fitted = num / den;
    let a = mx - fitted * mk;
    let rms = (pts.iter().map(|(k, x)| (x - (a + fitted * k)).powi(2)).sum::<f64>() / m).sqrt();
    if rms > 0.1 * fitted || (fitted / period - 1.0).abs() > 0.02 {
        return None; // irregular: texture, not a grid
    }
    Some((fitted, a.rem_euclid(fitted), contrast))
}

/// Detects an embedded square grid. `None` when no regular grid stands out
/// on either axis. With only one axis detected, the cells are taken square.
pub fn detect_grid(img: &RgbImage) -> Option<GridGuess> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w < 16 || h < 16 {
        return None;
    }
    let g = luma(img);
    let (px, py) = (edge_profile(&g, w, h, true), edge_profile(&g, w, h, false));
    let mut x = axis_grid(&px);
    let mut y = axis_grid(&py);
    // Squares are square. When one axis reads about 2× or 3× the other, it
    // saw extra lines (e.g. running-bond floor tiles offset by half a tile):
    // use the larger period for both, re-picking the smaller axis' line.
    if let (Some(a), Some(b)) = (x, y) {
        let (big, small_is_x) = if a.0 > b.0 { (a.0, false) } else { (b.0, true) };
        let small = if small_is_x { a } else { b };
        let k = (big / small.0).round();
        if k >= 2.0 && (big / small.0 / k - 1.0).abs() < 0.03 {
            let prof = high_pass(if small_is_x { &px } else { &py }, 4);
            let off = (0..k as usize)
                .map(|j| small.1 + j as f64 * small.0)
                .max_by(|o1, o2| {
                    let half = (big * 0.12).clamp(1.0, 6.0);
                    energy_at(&prof, big, *o1, half).total_cmp(&energy_at(&prof, big, *o2, half))
                })
                .unwrap_or(small.1)
                .rem_euclid(big);
            let fixed = Some((big, off, small.2));
            if small_is_x {
                x = fixed;
            } else {
                y = fixed;
            }
        }
    }
    let (cw, ox, cx, ch, oy, cy) = match (x, y) {
        (Some((cw, ox, cx)), Some((ch, oy, cy))) => (cw, ox, cx, ch, oy, cy),
        (Some((c, ox, cx)), None) => (c, ox, cx, c, 0.0, 0.0),
        (None, Some((c, oy, cy))) => (c, 0.0, 0.0, c, oy, cy),
        (None, None) => return None,
    };
    Some(GridGuess { cell_w: cw, cell_h: ch, offset_x: ox, offset_y: oy, confidence: cx.max(cy) })
}

/// Content bounds `(x0, y0, x1, y1)` (exclusive end) inside uniform borders.
/// Each side is trimmed while its outermost remaining line is one colour
/// (≥ 98 % of its pixels within `tolerance` of the line's median colour).
/// `None` when nothing is trimmed or the result would be implausibly small.
pub fn content_bounds(img: &RgbImage, tolerance: u8) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = (img.width(), img.height());
    if w < 8 || h < 8 {
        return None;
    }
    let uniform = |px: &mut dyn Iterator<Item = [u8; 3]>| -> bool {
        let v: Vec<[u8; 3]> = px.collect();
        let med = |c: usize| {
            let mut ch: Vec<u8> = v.iter().map(|p| p[c]).collect();
            ch.sort_unstable();
            ch[ch.len() / 2]
        };
        let m = [med(0), med(1), med(2)];
        let close = v.iter().filter(|p| (0..3).all(|c| p[c].abs_diff(m[c]) <= tolerance)).count();
        close * 100 >= v.len() * 98
    };
    let col = |x: u32, y0: u32, y1: u32| (y0..y1).map(move |y| img.get_pixel(x, y).0);
    let row = |y: u32, x0: u32, x1: u32| (x0..x1).map(move |x| img.get_pixel(x, y).0);
    let (mut x0, mut y0, mut x1, mut y1) = (0, 0, w, h);
    // alternate sides so corners do not bias the colour estimate
    loop {
        let mut moved = false;
        if x1 - x0 > 2 && uniform(&mut col(x0, y0, y1)) {
            x0 += 1;
            moved = true;
        }
        if x1 - x0 > 2 && uniform(&mut col(x1 - 1, y0, y1)) {
            x1 -= 1;
            moved = true;
        }
        if y1 - y0 > 2 && uniform(&mut row(y0, x0, x1)) {
            y0 += 1;
            moved = true;
        }
        if y1 - y0 > 2 && uniform(&mut row(y1 - 1, x0, x1)) {
            y1 -= 1;
            moved = true;
        }
        if !moved {
            break;
        }
    }
    let trimmed = (x0, y0, x1, y1) != (0, 0, w, h);
    let big_enough = (x1 - x0) * 4 >= w && (y1 - y0) * 4 >= h;
    (trimmed && big_enough).then_some((x0, y0, x1, y1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic texture noise (splitmix64: no lattice structure, unlike
    /// a bare power-of-two LCG, whose stride correlations look like a grid).
    fn noise(seed: &mut u64) -> u8 {
        *seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 56) as u8
    }

    /// A textured map with grid lines every `cell` px starting at `off`,
    /// `t` px thick, inside `bar`-px grey bars on the left and right.
    fn map(w: u32, h: u32, cell: f64, off: (f64, f64), t: f64, bar: u32) -> RgbImage {
        let mut seed = 7;
        RgbImage::from_fn(w, h, |x, y| {
            if x < bar || x >= w - bar {
                return image::Rgb([128, 128, 128]);
            }
            let on = |v: f64, o: f64| ((v - o).rem_euclid(cell)) < t;
            let n = noise(&mut seed) / 6;
            if on(f64::from(x), off.0) || on(f64::from(y), off.1) {
                image::Rgb([40 + n / 2, 35, 30])
            } else {
                // parchment with blotches so the profile is not flat
                let b = ((f64::from(x) / 37.0).sin() * (f64::from(y) / 53.0).cos() * 30.0) as i32;
                let v = (200 + b) as u8;
                image::Rgb([v.saturating_sub(n), v.saturating_sub(n / 2), 150])
            }
        })
    }

    fn circ_dist(a: f64, b: f64, p: f64) -> f64 {
        let d = (a - b).rem_euclid(p);
        d.min(p - d)
    }

    #[test]
    fn finds_square_size_and_offset() {
        for (cell, off) in [(40.31, (5.9, 2.3)), (17.25, (3.0, 11.0)), (70.0, (0.0, 33.5))] {
            let img = map(1060, 890, cell, off, 2.0, 0);
            let g = detect_grid(&img).expect("grid found");
            assert!((g.cell_w - cell).abs() < 0.05, "{cell}: {g:?}");
            assert!((g.cell_h - cell).abs() < 0.05, "{cell}: {g:?}");
            // the detected line position is the line's centre (off + t/2)
            assert!(circ_dist(g.offset_x, off.0 + 1.0, cell) < 0.75, "{cell}: {g:?}");
            assert!(circ_dist(g.offset_y, off.1 + 1.0, cell) < 0.75, "{cell}: {g:?}");
        }
    }

    #[test]
    fn grey_bars_do_not_confuse_the_grid() {
        let img = map(1058, 889, 40.31, (5.94, 0.95), 2.0, 45);
        let g = detect_grid(&img).unwrap();
        assert!((g.cell_w - 40.31).abs() < 0.05, "{g:?}");
    }

    #[test]
    fn no_grid_means_none() {
        let mut seed = 3;
        let img = RgbImage::from_fn(800, 600, |x, y| {
            let b = ((f64::from(x) / 23.0).sin() * (f64::from(y) / 41.0).cos() * 40.0) as i32;
            let n = noise(&mut seed) / 3;
            image::Rgb([(150 + b) as u8 ^ n, 120, 90])
        });
        assert_eq!(detect_grid(&img), None);
        assert_eq!(detect_grid(&RgbImage::from_pixel(400, 300, image::Rgb([9, 9, 9]))), None);
    }

    #[test]
    fn content_inside_bars_and_blank_margins() {
        let img = map(1058, 889, 40.31, (5.94, 0.95), 2.0, 45);
        // the last row is a grid line here, so it may go too (one pixel)
        let (x0, y0, x1, y1) = content_bounds(&img, 24).unwrap();
        assert_eq!((x0, y0, x1), (45, 0, 1013));
        assert!(y1 >= 888, "{y1}");
        // a white page margin all round, with JPEG-like speckle in it
        let mut seed = 11;
        let page = RgbImage::from_fn(400, 300, |x, y| {
            if (30..370).contains(&x) && (20..280).contains(&y) {
                // artwork has texture (a solid block would itself be "uniform")
                let n = noise(&mut seed) / 2;
                image::Rgb([60 + n, 90, 120 - n / 2])
            } else {
                let n = noise(&mut seed) % 9;
                image::Rgb([250 - n, 250, 248])
            }
        });
        assert_eq!(content_bounds(&page, 24), Some((30, 20, 370, 280)));
        // nothing to trim, or nothing left
        let full = map(300, 200, 30.0, (5.0, 5.0), 2.0, 0);
        assert_eq!(content_bounds(&full, 24), None);
        assert_eq!(
            content_bounds(&RgbImage::from_pixel(100, 100, image::Rgb([1, 2, 3])), 24),
            None
        );
    }
}
