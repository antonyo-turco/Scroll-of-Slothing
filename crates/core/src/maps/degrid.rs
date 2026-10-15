//! Embedded-grid removal by Fourier notch (Python `map_grid.remove_grid_lines`).
//!
//! A grid drawn into a map is periodic, so its 2-D spectrum collapses to a few
//! sharp spikes along the frequency axes. Notching those spikes erases the grid
//! everywhere while leaving the broadband artwork intact. An axis is notched
//! only when several harmonics clearly stand out, so maps without an embedded
//! grid (or with a mismatched cell size) come back unchanged.
//!
//! Differences from Python, none visible in the output: detection runs on the
//! spectrum of R+G+B (identical to the mean of the channel spectra, the FFT is
//! linear), channels are filtered one at a time to bound memory, and the maths
//! is f32 instead of f64.

use std::sync::Arc;

use image::RgbImage;
use rayon::prelude::*;
use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

/// Default detection strength (prominence ratio = 2 + 2·strength).
pub const DEFAULT_STRENGTH: f64 = 0.8;

struct Fft2 {
    w: usize,
    h: usize,
    row: Arc<dyn Fft<f32>>,
    col: Arc<dyn Fft<f32>>,
}

impl Fft2 {
    fn new(w: usize, h: usize, inverse: bool) -> Self {
        let mut p = FftPlanner::new();
        let (row, col) = if inverse {
            (p.plan_fft_inverse(w), p.plan_fft_inverse(h))
        } else {
            (p.plan_fft_forward(w), p.plan_fft_forward(h))
        };
        Self { w, h, row, col }
    }

    /// In-place 2-D transform of a row-major `h×w` buffer.
    fn run(&self, buf: &mut Vec<Complex32>) {
        let (w, h) = (self.w, self.h);
        buf.par_chunks_mut(w).for_each_init(
            || vec![Complex32::default(); self.row.get_inplace_scratch_len()],
            |scratch, r| self.row.process_with_scratch(r, scratch),
        );
        let mut t = transpose(buf, w, h);
        t.par_chunks_mut(h).for_each_init(
            || vec![Complex32::default(); self.col.get_inplace_scratch_len()],
            |scratch, c| self.col.process_with_scratch(c, scratch),
        );
        *buf = transpose(&t, h, w);
    }
}

fn transpose(src: &[Complex32], w: usize, h: usize) -> Vec<Complex32> {
    let mut out = vec![Complex32::default(); w * h];
    out.par_chunks_mut(h).enumerate().for_each(|(x, col)| {
        for (y, v) in col.iter_mut().enumerate() {
            *v = src[y * w + x];
        }
    });
    out
}

/// Magnitude spectrum in fftshift-ed coordinates (row-major `h×w`).
struct Spectrum {
    mag: Vec<f32>,
    w: usize,
    h: usize,
}

impl Spectrum {
    fn at(&self, y: usize, x: usize) -> f32 {
        self.mag[y * self.w + x]
    }

    /// Prominence of the spike at `(py, px)` over its neighbours along the
    /// harmonic axis: 3×3 max over the median of bins 3..12 away.
    fn peak_ratio(&self, py: usize, px: usize, vertical_axis: bool) -> f64 {
        let mut peak = f32::MIN;
        for y in py.saturating_sub(1)..(py + 2).min(self.h) {
            for x in px.saturating_sub(1)..(px + 2).min(self.w) {
                peak = peak.max(self.at(y, x));
            }
        }
        let mut nb = Vec::with_capacity(20);
        let (p, n) =
            if vertical_axis { (py as i64, self.h as i64) } else { (px as i64, self.w as i64) };
        // Python slices [p+3, p+13) and [p-12, p-2), clipped to the array.
        for range in [(p + 3, p + 13), (p - 12, p - 2)] {
            for i in range.0.max(0)..range.1.min(n) {
                let i = i as usize;
                nb.push(if vertical_axis { self.at(i, px) } else { self.at(py, i) });
            }
        }
        f64::from(peak) / (median(&mut nb) + 1e-6)
    }
}

fn median(v: &mut [f32]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f32::total_cmp);
    let m = v.len() / 2;
    if v.len() % 2 == 1 { f64::from(v[m]) } else { (f64::from(v[m - 1]) + f64::from(v[m])) / 2.0 }
}

/// Shifted-spectrum coordinates of one axis's harmonic spikes, or empty when no
/// convincing periodic grid is near the expected spacing. `vertical_axis` walks
/// the vertical frequency axis (horizontal grid lines, spacing `cell_h`).
fn grid_harmonics(s: &Spectrum, cell: f64, vertical_axis: bool, ratio: f64) -> Vec<(usize, usize)> {
    let (cy, cx) = (s.h / 2, s.w / 2);
    let span = if vertical_axis { s.h } else { s.w };
    let f = span as f64 / cell;
    let limit = (span / 2) as i64 - 4;
    if f < 6.0 || f > limit as f64 {
        return Vec::new();
    }
    let lo = ((f * 0.8).round_ties_even() as i64).max(6);
    let hi = ((f * 1.2).round_ties_even() as i64).min(limit);
    if hi <= lo {
        return Vec::new();
    }
    let at = |r: i64| if vertical_axis { (cy + r as usize, cx) } else { (cy, cx + r as usize) };
    let harmonic_ratios = |r: i64| {
        let mut out = Vec::new();
        let mut n = 1;
        while n * r <= limit && out.len() < 6 {
            let (py, px) = at(n * r);
            out.push(s.peak_ratio(py, px, vertical_axis));
            n += 1;
        }
        out
    };
    // Comb search: the fundamental is the bin whose whole harmonic series is
    // strongest; the window is narrow so a 2× harmonic can't masquerade as it.
    let (mut r0, mut ratios, mut best) = (lo, Vec::new(), -1.0);
    for r in lo..=hi {
        let rs = harmonic_ratios(r);
        let score = rs.iter().sum::<f64>() / rs.len().max(1) as f64;
        if score > best {
            best = score;
            r0 = r;
            ratios = rs;
        }
    }
    if ratios.iter().filter(|&&v| v >= ratio).count() < 2 {
        return Vec::new();
    }
    let mut pts = Vec::new();
    let mut n = 1;
    while n * r0 <= limit {
        let r = (n * r0) as usize;
        if vertical_axis {
            pts.push((cy + r, cx));
            pts.push((cy - r, cx));
        } else {
            pts.push((cy, cx + r));
            pts.push((cy, cx - r));
        }
        n += 1;
    }
    pts
}

/// Removes a grid drawn into `img` whose spacing is `cell_w`×`cell_h` px.
/// Returns `None` when there is nothing to remove (cells ≤ 3 px, or no
/// convincing periodic grid), so callers can keep the original buffer.
pub fn remove_grid_lines(
    img: &RgbImage,
    cell_w: f64,
    cell_h: f64,
    strength: f64,
) -> Option<RgbImage> {
    if cell_w <= 3.0 || cell_h <= 3.0 {
        return None;
    }
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w < 2 || h < 2 {
        return None;
    }
    let raw = img.as_raw();
    let fwd = Fft2::new(w, h, false);

    // Detection spectrum: |FFT(R+G+B)| / 3, fftshift-ed.
    let mut buf: Vec<Complex32> = raw
        .par_chunks(3)
        .map(|p| Complex32::new((f32::from(p[0]) + f32::from(p[1]) + f32::from(p[2])) / 3.0, 0.0))
        .collect();
    fwd.run(&mut buf);
    let mut mag = vec![0f32; w * h];
    mag.par_chunks_mut(w).enumerate().for_each(|(sy, row)| {
        let oy = (sy + h - h / 2) % h;
        for (sx, m) in row.iter_mut().enumerate() {
            let ox = (sx + w - w / 2) % w;
            *m = buf[oy * w + ox].norm();
        }
    });
    let spec = Spectrum { mag, w, h };
    let ratio = 2.0 + 2.0 * strength;
    let mut pts = grid_harmonics(&spec, cell_w, false, ratio);
    pts.extend(grid_harmonics(&spec, cell_h, true, ratio));
    drop(spec);
    if pts.is_empty() {
        return None;
    }

    // Zero a 5×5 square around each spike, then restore the 13×13 low-frequency
    // core so overall content and lighting are preserved. Mask in shifted coords.
    let rad = 2usize;
    let mut mask = vec![1f32; w * h];
    for (py, px) in pts {
        for y in py.saturating_sub(rad)..(py + rad + 1).min(h) {
            for x in px.saturating_sub(rad)..(px + rad + 1).min(w) {
                mask[y * w + x] = 0.0;
            }
        }
    }
    let (cy, cx) = (h / 2, w / 2);
    for y in cy.saturating_sub(6)..(cy + 7).min(h) {
        for x in cx.saturating_sub(6)..(cx + 7).min(w) {
            mask[y * w + x] = 1.0;
        }
    }
    // Same mask in unshifted coordinates.
    let mut umask = vec![1f32; w * h];
    umask.par_chunks_mut(w).enumerate().for_each(|(oy, row)| {
        let sy = (oy + h / 2) % h;
        for (ox, m) in row.iter_mut().enumerate() {
            *m = mask[sy * w + (ox + w / 2) % w];
        }
    });
    drop(mask);

    let inv = Fft2::new(w, h, true);
    let norm = 1.0 / (w * h) as f32;
    let mut out = vec![0u8; w * h * 3];
    for c in 0..3 {
        let mut b: Vec<Complex32> =
            raw.par_chunks(3).map(|p| Complex32::new(f32::from(p[c]), 0.0)).collect();
        fwd.run(&mut b);
        b.par_iter_mut().zip(umask.par_iter()).for_each(|(v, &m)| *v *= m);
        inv.run(&mut b);
        out.par_chunks_mut(3).zip(b.par_iter()).for_each(|(o, v)| {
            // numpy: clip then astype(uint8) truncates
            o[c] = (v.re * norm).clamp(0.0, 255.0) as u8;
        });
    }
    Some(RgbImage::from_raw(w as u32, h as u32, out).expect("buffer size matches dimensions"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smooth "artwork" plus a dark grid every `cell` px.
    fn gridded(w: u32, h: u32, cell: u32, with_grid: bool) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            let base = 120.0 + 60.0 * ((x as f64) / 37.0).sin() * ((y as f64) / 53.0).cos();
            let v = if with_grid && (x % cell == 0 || y % cell == 0) { base - 90.0 } else { base };
            let v = v.clamp(0.0, 255.0) as u8;
            image::Rgb([v, v / 2 + 40, 255 - v])
        })
    }

    // Removal quality vs Python is covered by the golden test in
    // tests/parity_map_images.rs; these pin the gating behaviour.

    #[test]
    fn tolerates_an_inexact_cell() {
        let dirty = gridded(400, 300, 25, true);
        assert!(remove_grid_lines(&dirty, 27.0, 23.5, DEFAULT_STRENGTH).is_some());
    }

    #[test]
    fn tiny_cells_and_tiny_images_are_skipped() {
        let dirty = gridded(400, 300, 25, true);
        assert!(remove_grid_lines(&dirty, 3.0, 25.0, DEFAULT_STRENGTH).is_none());
        assert!(
            remove_grid_lines(&gridded(1, 1, 25, true), 25.0, 25.0, DEFAULT_STRENGTH).is_none()
        );
        // a cell so large that fewer than 6 periods fit is not trusted
        assert!(remove_grid_lines(&dirty, 100.0, 100.0, DEFAULT_STRENGTH).is_none());
    }
}
