//! Map filter pipeline (Python `map_filters.py`).
//!
//! Every filter is a port of the PIL operation the Python app used, including
//! PIL's integer/fixed-point rounding: Gaussian blur is PIL's 3-pass box
//! approximation, UnsharpMask/blend/autocontrast/colorize/dither/CONTOUR
//! follow `libImaging`. `canny` is new: a port of the unused OpenCV-based
//! `line_art_extractor.py`.

use std::collections::BTreeMap;

use image::{GrayImage, RgbImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// One step of a pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterStep {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub params: BTreeMap<String, f64>,
}

fn yes() -> bool {
    true
}

/// Slider description for a filter's single parameter.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ParamDef {
    pub key: &'static str,
    pub label: &'static str,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub step: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FilterDef {
    pub key: &'static str,
    pub label: &'static str,
    /// Zero, one (all Python filters) or several (canny) sliders.
    pub params: Vec<ParamDef>,
}

const fn p(
    key: &'static str,
    label: &'static str,
    min: f64,
    max: f64,
    default: f64,
    step: f64,
) -> ParamDef {
    ParamDef { key, label, min, max, default, step }
}

/// All filters in the default pipeline order.
pub fn filter_defs() -> Vec<FilterDef> {
    let d = |key, label, params: Vec<ParamDef>| FilterDef { key, label, params };
    vec![
        d("autocontrast", "Auto-contrast", vec![p("clip", "Clip %", 0.0, 20.0, 1.0, 1.0)]),
        d("contrast", "Contrast", vec![p("amount", "Amount %", 0.0, 300.0, 150.0, 5.0)]),
        d("lightness", "Lightness", vec![p("amount", "Amount %", 0.0, 200.0, 100.0, 5.0)]),
        d(
            "clarity",
            "Local contrast (clarity)",
            vec![p("amount", "Amount %", 0.0, 300.0, 100.0, 10.0)],
        ),
        d("sharpen", "Sharpen", vec![p("amount", "Amount %", 0.0, 500.0, 150.0, 10.0)]),
        d("sepia", "Sepia", vec![]),
        d("grayscale", "Grayscale", vec![]),
        d(
            "adaptive",
            "Adaptive B&W",
            vec![p("sensitivity", "Sensitivity %", 0.0, 100.0, 50.0, 1.0)],
        ),
        d("threshold", "Ink-saving B&W", vec![p("cutoff", "Cutoff %", 0.0, 100.0, 50.0, 1.0)]),
        d("posterize", "Posterize", vec![p("levels", "Levels", 2.0, 8.0, 4.0, 1.0)]),
        d("dither", "Dither B&W", vec![]),
        d("edge_boost", "Edge boost", vec![p("strength", "Strength %", 0.0, 100.0, 60.0, 1.0)]),
        d("line_art", "Line art only", vec![p("detail", "Detail %", 0.0, 100.0, 55.0, 1.0)]),
        d(
            "canny",
            "Canny edges",
            vec![
                p("low", "Low threshold", 0.0, 255.0, 50.0, 1.0),
                p("high", "High threshold", 0.0, 255.0, 150.0, 1.0),
                p("dilate", "Dilate", 0.0, 5.0, 0.0, 1.0),
            ],
        ),
    ]
}

/// Steps of a preset: `(filter, [(param, value)])`.
pub type PresetSteps = &'static [(&'static str, &'static [(&'static str, f64)])];

/// The 18 quick presets, verbatim: `(name, steps)`.
pub const PRESETS: &[(&str, PresetSteps)] = &[
    ("None (clear all)", &[]),
    (
        "Most legible (gray + edges)",
        &[
            ("autocontrast", &[]),
            ("sharpen", &[("amount", 150.0)]),
            ("grayscale", &[]),
            ("edge_boost", &[("strength", 70.0)]),
        ],
    ),
    (
        "Ink-saver (adaptive)",
        &[
            ("autocontrast", &[]),
            ("sharpen", &[("amount", 150.0)]),
            ("adaptive", &[("sensitivity", 50.0)]),
        ],
    ),
    (
        "Detailed line maps",
        &[
            ("autocontrast", &[]),
            ("sharpen", &[("amount", 200.0)]),
            ("adaptive", &[("sensitivity", 60.0)]),
        ],
    ),
    (
        "Max detail B&W",
        &[
            ("autocontrast", &[]),
            ("clarity", &[("amount", 120.0)]),
            ("sharpen", &[("amount", 150.0)]),
            ("adaptive", &[("sensitivity", 65.0)]),
            ("edge_boost", &[("strength", 40.0)]),
        ],
    ),
    (
        "Atmospheric (dither)",
        &[("autocontrast", &[]), ("dither", &[]), ("edge_boost", &[("strength", 50.0)])],
    ),
    ("Newsprint", &[("autocontrast", &[]), ("contrast", &[("amount", 140.0)]), ("dither", &[])]),
    (
        "Newsprint (print-friendly)",
        &[
            ("autocontrast", &[("clip", 2.0)]),
            ("contrast", &[("amount", 120.0)]),
            ("sharpen", &[("amount", 150.0)]),
            ("dither", &[]),
            ("edge_boost", &[("strength", 55.0)]),
        ],
    ),
    (
        "High-contrast B&W",
        &[
            ("autocontrast", &[]),
            ("contrast", &[("amount", 180.0)]),
            ("threshold", &[("cutoff", 55.0)]),
        ],
    ),
    (
        "Posterized (3 levels)",
        &[
            ("autocontrast", &[]),
            ("clarity", &[("amount", 100.0)]),
            ("posterize", &[("levels", 3.0)]),
            ("edge_boost", &[("strength", 50.0)]),
        ],
    ),
    ("Posterized (4 levels)", &[("autocontrast", &[]), ("posterize", &[("levels", 4.0)])]),
    (
        "Soft grayscale",
        &[("autocontrast", &[]), ("clarity", &[("amount", 80.0)]), ("grayscale", &[])],
    ),
    ("Clean grayscale", &[("autocontrast", &[]), ("grayscale", &[])]),
    ("Pure line art", &[("sharpen", &[("amount", 150.0)]), ("line_art", &[("detail", 55.0)])]),
    ("Pencil sketch", &[("clarity", &[("amount", 120.0)]), ("line_art", &[("detail", 68.0)])]),
    (
        "Bold ink (thick lines)",
        &[
            ("autocontrast", &[]),
            ("sharpen", &[("amount", 280.0)]),
            ("adaptive", &[("sensitivity", 40.0)]),
        ],
    ),
    ("Sepia print", &[("autocontrast", &[]), ("sepia", &[])]),
    ("Faded parchment", &[("autocontrast", &[]), ("clarity", &[("amount", 60.0)]), ("sepia", &[])]),
];

/// The full pipeline for a preset: the listed filters enabled in the listed
/// order with defaults plus overrides, followed by every other filter disabled
/// (so the UI keeps a complete, reorderable list). `None` for an unknown name.
pub fn preset_pipeline(name: &str) -> Option<Vec<FilterStep>> {
    let (_, steps) = PRESETS.iter().find(|(n, _)| *n == name)?;
    let defs = filter_defs();
    let defaults = |key: &str| -> BTreeMap<String, f64> {
        defs.iter()
            .find(|d| d.key == key)
            .map(|d| d.params.iter().map(|p| (p.key.to_string(), p.default)).collect())
            .unwrap_or_default()
    };
    let mut out: Vec<FilterStep> = steps
        .iter()
        .map(|(kind, over)| {
            let mut params = defaults(kind);
            params.extend(over.iter().map(|(k, v)| (k.to_string(), *v)));
            FilterStep { kind: kind.to_string(), enabled: true, params }
        })
        .collect();
    for d in &defs {
        if !out.iter().any(|s| s.kind == d.key) {
            out.push(FilterStep {
                kind: d.key.to_string(),
                enabled: false,
                params: defaults(d.key),
            });
        }
    }
    Some(out)
}

/// Every filter, disabled, at its default (the "None" state).
pub fn default_pipeline() -> Vec<FilterStep> {
    preset_pipeline("None (clear all)").expect("built-in preset")
}

/// Applies the enabled steps in order. Unknown filter types are skipped.
pub fn apply_pipeline(img: &RgbImage, pipeline: &[FilterStep]) -> RgbImage {
    let mut cur = img.clone();
    for step in pipeline.iter().filter(|s| s.enabled) {
        if let Some(next) = apply_one(&cur, step) {
            cur = next;
        }
    }
    cur
}

/// True when the pipeline would change nothing (no enabled known filter).
pub fn is_identity(pipeline: &[FilterStep]) -> bool {
    let defs = filter_defs();
    !pipeline.iter().any(|s| s.enabled && defs.iter().any(|d| d.key == s.kind))
}

fn param(step: &FilterStep, key: &str, fallback: f64) -> f64 {
    step.params.get(key).copied().filter(|v| v.is_finite()).unwrap_or(fallback)
}

fn apply_one(img: &RgbImage, s: &FilterStep) -> Option<RgbImage> {
    let w = f64::from(img.width());
    Some(match s.kind.as_str() {
        "autocontrast" => autocontrast(img, param(s, "clip", 1.0).clamp(0.0, 20.0)),
        "contrast" => contrast_rgb(img, (param(s, "amount", 100.0).max(0.0) / 100.0) as f32),
        "lightness" => brightness(img, (param(s, "amount", 100.0).max(0.0) / 100.0) as f32),
        "sharpen" => unsharp_rgb(img, 2.0, param(s, "amount", 150.0).max(0.0) as i32, 2),
        "clarity" => unsharp_rgb(
            img,
            (w / 40.0).max(5.0) as f32,
            param(s, "amount", 100.0).max(0.0) as i32,
            0,
        ),
        "sepia" => sepia(img),
        "grayscale" => gray_to_rgb(&grayscale(img)),
        "adaptive" => adaptive(img, param(s, "sensitivity", 50.0).clamp(0.0, 100.0)),
        "threshold" => threshold(img, param(s, "cutoff", 50.0)),
        "posterize" => posterize(img, (param(s, "levels", 4.0) as i64).clamp(2, 8)),
        "dither" => gray_to_rgb(&dither(&grayscale(img))),
        "edge_boost" => edge_boost(img, param(s, "strength", 60.0).clamp(0.0, 100.0)),
        "line_art" => line_art(img, param(s, "detail", 55.0).clamp(0.0, 100.0)),
        "canny" => gray_to_rgb(&canny(
            &grayscale(img),
            param(s, "low", 50.0).clamp(0.0, 255.0),
            param(s, "high", 150.0).clamp(0.0, 255.0),
            (param(s, "dilate", 0.0) as i64).clamp(0, 5) as u32,
        )),
        _ => return None,
    })
}

// ── PIL primitives ───────────────────────────────────────────────────────

/// PIL `convert("L")`: ITU-R 601-2 luma in 16.16 fixed point, rounded.
pub fn grayscale(img: &RgbImage) -> GrayImage {
    let data = img.as_raw().par_chunks(3).map(|p| luma(p[0], p[1], p[2])).collect();
    GrayImage::from_raw(img.width(), img.height(), data).expect("size")
}

fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((u32::from(r) * 19595 + u32::from(g) * 38470 + u32::from(b) * 7471 + 0x8000) >> 16) as u8
}

fn gray_to_rgb(g: &GrayImage) -> RgbImage {
    let data = g.as_raw().iter().flat_map(|&v| [v, v, v]).collect();
    RgbImage::from_raw(g.width(), g.height(), data).expect("size")
}

fn map_gray(g: &GrayImage, lut: &[u8; 256]) -> GrayImage {
    let data = g.as_raw().par_iter().map(|&v| lut[v as usize]).collect();
    GrayImage::from_raw(g.width(), g.height(), data).expect("size")
}

fn lut_from(f: impl Fn(u8) -> u8) -> [u8; 256] {
    std::array::from_fn(|i| f(i as u8))
}

/// PIL `_gaussian_blur_radius`: box radius whose 3-pass blur approximates a
/// Gaussian of `radius` (float maths as in BoxBlur.c).
fn box_radius(radius: f32, passes: u32) -> f32 {
    let sigma2 = radius * radius / passes as f32;
    let big_l = (12.0 * f64::from(sigma2) + 1.0).sqrt() as f32;
    let l = ((f64::from(big_l) - 1.0) / 2.0).floor() as f32;
    let mut a = (2.0 * l + 1.0) * (l * (l + 1.0) - 3.0 * sigma2);
    a /= 6.0 * (sigma2 - (l + 1.0) * (l + 1.0));
    l + a
}

/// PIL `ImagingLineBoxBlur` for one line of one channel.
fn box_blur_line(out: &mut [u8], line: &[u8], radius: i64, ww: u64, fw: u64) {
    let n = line.len() as i64;
    let last = n - 1;
    let edge_a = (radius + 1).min(n);
    let edge_b = (n - radius - 1).max(0);
    let at = |i: i64| i64::from(line[i as usize]);
    let mut acc: i64 = at(0) * (radius + 1);
    for x in 0..edge_a - 1 {
        acc += at(x);
    }
    acc += at(last) * (radius - edge_a + 1);
    let mut save = |x: i64, acc: i64, l: i64, r: i64| {
        let bulk = acc as u64 * ww + (at(l) + at(r)) as u64 * fw;
        out[x as usize] = ((bulk + (1 << 23)) >> 24) as u8;
    };
    if edge_a <= edge_b {
        for x in 0..edge_a {
            acc += at(x + radius) - at(0);
            save(x, acc, 0, x + radius + 1);
        }
        for x in edge_a..edge_b {
            acc += at(x + radius) - at(x - radius - 1);
            save(x, acc, x - radius - 1, x + radius + 1);
        }
        for x in edge_b..=last {
            acc += at(last) - at(x - radius - 1);
            save(x, acc, x - radius - 1, last);
        }
    } else {
        for x in 0..edge_b {
            acc += at(x + radius) - at(0);
            save(x, acc, 0, x + radius + 1);
        }
        for x in edge_b..edge_a {
            acc += at(last) - at(0);
            save(x, acc, 0, last);
        }
        for x in edge_a..=last {
            acc += at(last) - at(x - radius - 1);
            save(x, acc, x - radius - 1, last);
        }
    }
}

/// One horizontal box-blur pass over a row-major plane.
fn hblur(plane: &mut [u8], w: usize, fradius: f32) {
    let radius = fradius as i64;
    let ww = (16_777_216.0f32 / (fradius * 2.0 + 1.0)) as u64;
    let fw = ((1u64 << 24) - (radius as u64 * 2 + 1) * ww) / 2;
    plane.par_chunks_mut(w).for_each(|row| {
        let src = row.to_vec();
        box_blur_line(row, &src, radius, ww, fw);
    });
}

fn transpose_u8(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h];
    out.par_chunks_mut(h).enumerate().for_each(|(x, col)| {
        for (y, v) in col.iter_mut().enumerate() {
            *v = src[y * w + x];
        }
    });
    out
}

/// PIL `GaussianBlur(radius)` on one plane: 3 horizontal box passes, then 3
/// vertical (via transpose), each rounded to 8 bits.
fn gaussian_plane(plane: &[u8], w: usize, h: usize, radius: f32) -> Vec<u8> {
    let r = box_radius(radius, 3);
    let mut out = plane.to_vec();
    if r == 0.0 || w == 0 || h == 0 {
        return out;
    }
    for _ in 0..3 {
        hblur(&mut out, w, r);
    }
    let mut t = transpose_u8(&out, w, h);
    for _ in 0..3 {
        hblur(&mut t, h, r);
    }
    transpose_u8(&t, h, w)
}

pub fn gaussian_gray(g: &GrayImage, radius: f32) -> GrayImage {
    let (w, h) = (g.width() as usize, g.height() as usize);
    GrayImage::from_raw(g.width(), g.height(), gaussian_plane(g.as_raw(), w, h, radius))
        .expect("size")
}

fn split(img: &RgbImage) -> [Vec<u8>; 3] {
    let raw = img.as_raw();
    std::array::from_fn(|c| raw.iter().skip(c).step_by(3).copied().collect())
}

fn merge(w: u32, h: u32, planes: &[Vec<u8>; 3]) -> RgbImage {
    let mut data = vec![0u8; planes[0].len() * 3];
    data.par_chunks_mut(3).enumerate().for_each(|(i, px)| {
        px[0] = planes[0][i];
        px[1] = planes[1][i];
        px[2] = planes[2][i];
    });
    RgbImage::from_raw(w, h, data).expect("size")
}

/// PIL `UnsharpMask(radius, percent, threshold)` (UnsharpMask.c).
fn unsharp_rgb(img: &RgbImage, radius: f32, percent: i32, threshold: i32) -> RgbImage {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let planes = split(img);
    let out: Vec<Vec<u8>> = planes
        .par_iter()
        .map(|p| {
            let blur = gaussian_plane(p, w, h, radius);
            p.iter()
                .zip(&blur)
                .map(|(&i, &b)| {
                    let diff = i32::from(i) - i32::from(b);
                    if diff.abs() > threshold {
                        (i32::from(i) + diff * percent / 100).clamp(0, 255) as u8
                    } else {
                        i
                    }
                })
                .collect()
        })
        .collect();
    merge(img.width(), img.height(), &[out[0].clone(), out[1].clone(), out[2].clone()])
}

/// PIL `Image.blend(a, b, alpha)` per byte (float32, truncating, clipped
/// when extrapolating).
fn blend_byte(a: u8, b: u8, alpha: f32) -> u8 {
    let v = f32::from(a) + alpha * (f32::from(b) - f32::from(a));
    if (0.0..=1.0).contains(&alpha) {
        v as u8
    } else if v <= 0.0 {
        0
    } else if v >= 255.0 {
        255
    } else {
        v as u8
    }
}

fn mean_gray(g: &[u8]) -> u8 {
    if g.is_empty() {
        return 0;
    }
    let sum: u64 = g.par_iter().map(|&v| u64::from(v)).sum();
    (sum as f64 / g.len() as f64 + 0.5) as u8
}

/// `ImageEnhance.Contrast`: blend with the mean grey of the L conversion.
fn contrast_rgb(img: &RgbImage, factor: f32) -> RgbImage {
    let m = mean_gray(grayscale(img).as_raw());
    let data = img.as_raw().par_iter().map(|&v| blend_byte(m, v, factor)).collect();
    RgbImage::from_raw(img.width(), img.height(), data).expect("size")
}

fn contrast_gray(g: &GrayImage, factor: f32) -> GrayImage {
    let m = mean_gray(g.as_raw());
    map_gray(g, &lut_from(|v| blend_byte(m, v, factor)))
}

/// `ImageEnhance.Brightness`: blend with black.
fn brightness(img: &RgbImage, factor: f32) -> RgbImage {
    let lut = lut_from(|v| blend_byte(0, v, factor));
    let data = img.as_raw().par_iter().map(|&v| lut[v as usize]).collect();
    RgbImage::from_raw(img.width(), img.height(), data).expect("size")
}

/// `ImageOps.autocontrast(cutoff)`, per channel.
fn autocontrast(img: &RgbImage, cutoff: f64) -> RgbImage {
    let mut hist = [[0u64; 256]; 3];
    for px in img.as_raw().as_chunks::<3>().0 {
        for c in 0..3 {
            hist[c][px[c] as usize] += 1;
        }
    }
    let luts: Vec<[u8; 256]> = hist
        .iter()
        .map(|h| {
            let mut h = *h;
            if cutoff != 0.0 {
                let n: u64 = h.iter().sum();
                for order in [false, true] {
                    let mut cut = (n as f64 * cutoff / 100.0).floor() as u64;
                    for k in 0..256 {
                        let i = if order { 255 - k } else { k };
                        if cut > h[i] {
                            cut -= h[i];
                            h[i] = 0;
                        } else {
                            h[i] -= cut;
                            cut = 0;
                        }
                        if cut == 0 {
                            break;
                        }
                    }
                }
            }
            let lo = (0..256).find(|&i| h[i] != 0).unwrap_or(255);
            let hi = (0..256).rev().find(|&i| h[i] != 0).unwrap_or(0);
            if hi <= lo {
                return lut_from(|v| v);
            }
            let scale = 255.0 / (hi - lo) as f64;
            let offset = -(lo as f64) * scale;
            lut_from(|v| (f64::from(v) * scale + offset).trunc().clamp(0.0, 255.0) as u8)
        })
        .collect();
    let mut out = img.clone();
    for px in out.pixels_mut() {
        for c in 0..3 {
            px.0[c] = luts[c][px.0[c] as usize];
        }
    }
    out
}

/// `ImageOps.colorize(gray, black, white, mid)` with the default 0/127/255 points.
fn colorize_lut(black: [i32; 3], mid: [i32; 3], white: [i32; 3]) -> [[u8; 256]; 3] {
    let mut luts = [[0u8; 256]; 3];
    for (c, lut) in luts.iter_mut().enumerate() {
        for (i, v) in lut.iter_mut().enumerate() {
            let i = i as i32;
            *v = match i {
                0..127 => black[c] + (i * (mid[c] - black[c])).div_euclid(127),
                127..255 => mid[c] + ((i - 127) * (white[c] - mid[c])).div_euclid(128),
                _ => white[c],
            } as u8;
        }
    }
    luts
}

fn sepia(img: &RgbImage) -> RgbImage {
    let g = grayscale(img);
    let luts = colorize_lut([34, 20, 8], [150, 110, 60], [255, 240, 200]);
    let data = g
        .as_raw()
        .iter()
        .flat_map(|&v| [luts[0][v as usize], luts[1][v as usize], luts[2][v as usize]])
        .collect();
    RgbImage::from_raw(img.width(), img.height(), data).expect("size")
}

fn posterize(img: &RgbImage, levels: i64) -> RgbImage {
    let step = 255.0 / (levels - 1) as f64;
    let lut =
        lut_from(|i| ((f64::from(i) / step).round_ties_even() * step).round_ties_even() as u8);
    gray_to_rgb(&map_gray(&grayscale(img), &lut))
}

fn threshold(img: &RgbImage, cutoff_pct: f64) -> RgbImage {
    let cutoff = (cutoff_pct / 100.0 * 255.0).round_ties_even();
    gray_to_rgb(&map_gray(
        &grayscale(img),
        &lut_from(|p| if f64::from(p) >= cutoff { 255 } else { 0 }),
    ))
}

fn adaptive(img: &RgbImage, sensitivity: f64) -> RgbImage {
    let gray = grayscale(img);
    let radius = (f64::from(gray.width()) / 120.0).max(2.0) as f32;
    let local = gaussian_gray(&gray, radius);
    let offset = (2.0 + (100.0 - sensitivity) / 100.0 * 28.0).round_ties_even() as i32;
    let data = local
        .as_raw()
        .iter()
        .zip(gray.as_raw())
        .map(|(&l, &g)| {
            let diff = (i32::from(l) - i32::from(g)).clamp(0, 255);
            if diff > offset { 0 } else { 255 }
        })
        .collect();
    gray_to_rgb(&GrayImage::from_raw(gray.width(), gray.height(), data).expect("size"))
}

/// PIL `convert("1")`: Floyd–Steinberg error diffusion (Convert.c `tobilevel`).
pub fn dither(g: &GrayImage) -> GrayImage {
    let (w, h) = (g.width() as usize, g.height() as usize);
    let src = g.as_raw();
    let mut out = vec![0u8; w * h];
    let mut errors = vec![0i32; w + 1];
    for y in 0..h {
        let (mut l, mut l0, mut l1) = (0i32, 0i32, 0i32);
        for x in 0..w {
            // C integer division truncates toward zero
            l = (i32::from(src[y * w + x]) + (l + errors[x + 1]) / 16).clamp(0, 255);
            let o = if l > 128 { 255 } else { 0 };
            out[y * w + x] = o as u8;
            // propagate 3/16, 5/16, 1/16 below and 7/16 right
            l -= o;
            let l2 = l;
            let d2 = l + l;
            l += d2;
            errors[x] = l + l0;
            l += d2;
            l0 = l + l1;
            l1 = l2;
            l += d2;
        }
        errors[w] = l0;
    }
    GrayImage::from_raw(g.width(), g.height(), out).expect("size")
}

fn abs_diff(a: &GrayImage, b: &GrayImage) -> GrayImage {
    let data = a.as_raw().iter().zip(b.as_raw()).map(|(&x, &y)| x.abs_diff(y)).collect();
    GrayImage::from_raw(a.width(), a.height(), data).expect("size")
}

fn edge_boost(img: &RgbImage, strength: f64) -> RgbImage {
    let gray = grayscale(img);
    let radius = (f64::from(gray.width()) / 700.0).max(1.0) as f32;
    let hp = contrast_gray(&abs_diff(&gray, &gaussian_gray(&gray, radius)), 2.0);
    let cutoff = (110.0 - strength / 100.0 * 100.0).round_ties_even();
    let mut out = img.clone();
    for (px, &m) in out.pixels_mut().zip(hp.as_raw()) {
        if f64::from(m) >= cutoff {
            px.0 = [0, 0, 0];
        }
    }
    out
}

/// PIL `ImageFilter.CONTOUR` (3×3, offset 255) on L; border pixels copied.
fn contour(g: &GrayImage) -> GrayImage {
    let (w, h) = (g.width() as usize, g.height() as usize);
    let src = g.as_raw();
    let mut out = src.clone();
    if w >= 3 && h >= 3 {
        out.par_chunks_mut(w).enumerate().skip(1).take(h - 2).for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate().skip(1).take(w - 2) {
                let mut ss = 255.0f32 + 0.5;
                let mut acc = 0.0f32;
                for dy in [1i64, 0, -1] {
                    let r = (y as i64 + dy) as usize * w;
                    for dx in [-1i64, 0, 1] {
                        let k = if dx == 0 && dy == 0 { 8.0 } else { -1.0 };
                        acc += f32::from(src[r + (x as i64 + dx) as usize]) * k;
                    }
                }
                ss += acc;
                *o = if ss <= 0.0 {
                    0
                } else if ss >= 255.0 {
                    255
                } else {
                    ss as u8
                };
            }
        });
    }
    GrayImage::from_raw(g.width(), g.height(), out).expect("size")
}

fn line_art(img: &RgbImage, detail: f64) -> RgbImage {
    let gray = grayscale(img);
    let (w, h) = (gray.width(), gray.height());
    let radius = (f64::from(w) / 600.0).max(1.0) as f32;
    let highpass = abs_diff(&gray, &gaussian_gray(&gray, radius));
    let cont = contour(&gaussian_gray(&gray, 0.6));
    let lighter: Vec<u8> =
        highpass.as_raw().iter().zip(cont.as_raw()).map(|(&a, &c)| a.max(255 - c)).collect();
    let mut edges = contrast_gray(&GrayImage::from_raw(w, h, lighter).expect("size"), 1.8);
    // CONTOUR leaves a bright frame: clear a 2 px border.
    for (x, y, px) in edges.enumerate_pixels_mut() {
        if x < 2 || y < 2 || x + 2 >= w || y + 2 >= h {
            px.0 = [0];
        }
    }
    let cutoff = ((0.55 - 0.45 * (detail / 100.0)) * 255.0).round_ties_even();
    gray_to_rgb(&map_gray(&edges, &lut_from(|p| if f64::from(p) >= cutoff { 0 } else { 255 })))
}

// ── canny (OpenCV port) ──────────────────────────────────────────────────

fn reflect101(i: i64, n: i64) -> usize {
    if n == 1 {
        return 0;
    }
    let mut i = i;
    while i < 0 || i >= n {
        i = if i < 0 { -i } else { 2 * n - 2 - i };
    }
    i as usize
}

/// OpenCV `GaussianBlur(ksize=5, sigma=0)`: the fixed [1 4 6 4 1]/16 kernel,
/// BORDER_REFLECT_101, rounded once at the end.
fn gauss5(g: &GrayImage) -> GrayImage {
    let (w, h) = (g.width() as i64, g.height() as i64);
    let src = g.as_raw();
    const K: [u32; 5] = [1, 4, 6, 4, 1];
    let mut tmp = vec![0u32; (w * h) as usize];
    tmp.par_chunks_mut(w as usize).enumerate().for_each(|(y, row)| {
        for (x, t) in row.iter_mut().enumerate() {
            *t = (0..5)
                .map(|k| {
                    K[k] * u32::from(src[y * w as usize + reflect101(x as i64 + k as i64 - 2, w)])
                })
                .sum();
        }
    });
    let mut out = vec![0u8; (w * h) as usize];
    out.par_chunks_mut(w as usize).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let s: u32 = (0..5)
                .map(|k| K[k] * tmp[reflect101(y as i64 + k as i64 - 2, h) * w as usize + x])
                .sum();
            *o = ((s + 128) >> 8) as u8;
        }
    });
    GrayImage::from_raw(g.width(), g.height(), out).expect("size")
}

/// OpenCV `Canny(low, high)` (Sobel 3×3 with replicated borders, L1 magnitude,
/// 4-direction non-maximum suppression, 8-connected hysteresis), inverted to
/// black lines on white, then `dilate(3×3)` `iterations` times like the Python
/// extractor (which grows the white background).
pub fn canny(gray: &GrayImage, low: f64, high: f64, iterations: u32) -> GrayImage {
    let blurred = gauss5(gray);
    let (w, h) = (gray.width() as i64, gray.height() as i64);
    let n = (w * h) as usize;
    let src = blurred.as_raw();
    let px = |x: i64, y: i64| i32::from(src[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize]);
    let mut dx = vec![0i32; n];
    let mut dy = vec![0i32; n];
    let mut mag = vec![0i32; n];
    for y in 0..h {
        for x in 0..w {
            let gx = px(x + 1, y - 1) + 2 * px(x + 1, y) + px(x + 1, y + 1)
                - px(x - 1, y - 1)
                - 2 * px(x - 1, y)
                - px(x - 1, y + 1);
            let gy = px(x - 1, y + 1) + 2 * px(x, y + 1) + px(x + 1, y + 1)
                - px(x - 1, y - 1)
                - 2 * px(x, y - 1)
                - px(x + 1, y - 1);
            let i = (y * w + x) as usize;
            dx[i] = gx;
            dy[i] = gy;
            mag[i] = gx.abs() + gy.abs();
        }
    }
    let (low, high) = (low.floor() as i32, high.floor() as i32);
    let (low, high) = if low > high { (high, low) } else { (low, high) };
    let m_at = |x: i64, y: i64| {
        if x < 0 || y < 0 || x >= w || y >= h { 0 } else { mag[(y * w + x) as usize] }
    };
    const SHIFT: i32 = 15;
    let tg22: i64 = (0.414_213_562_373_095_05_f64 * f64::from(1 << SHIFT) + 0.5) as i64;
    // 0 = not an edge, 1 = weak, 2 = strong
    let mut map = vec![0u8; n];
    let mut stack = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let m = mag[i];
            if m <= low {
                continue;
            }
            let (xs, ys) = (dx[i], dy[i]);
            let ax = i64::from(xs.abs());
            let ay = i64::from(ys.abs()) << SHIFT;
            let tg22x = ax * tg22;
            let is_max = if ay < tg22x {
                m > m_at(x - 1, y) && m >= m_at(x + 1, y)
            } else {
                let tg67x = tg22x + (ax << (SHIFT + 1));
                if ay > tg67x {
                    m > m_at(x, y - 1) && m >= m_at(x, y + 1)
                } else {
                    let s = if (xs ^ ys) < 0 { -1 } else { 1 };
                    m > m_at(x - s, y - 1) && m > m_at(x + s, y + 1)
                }
            };
            if !is_max {
                continue;
            }
            if m > high {
                map[i] = 2;
                stack.push((x, y));
            } else {
                map[i] = 1;
            }
        }
    }
    while let Some((x, y)) = stack.pop() {
        for ny in y - 1..=y + 1 {
            for nx in x - 1..=x + 1 {
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let j = (ny * w + nx) as usize;
                if map[j] == 1 {
                    map[j] = 2;
                    stack.push((nx, ny));
                }
            }
        }
    }
    let mut out: Vec<u8> = map.iter().map(|&v| if v == 2 { 0 } else { 255 }).collect();
    for _ in 0..iterations {
        let prev = out.clone();
        out.par_chunks_mut(w as usize).enumerate().for_each(|(y, row)| {
            let y = y as i64;
            for (x, o) in row.iter_mut().enumerate() {
                let x = x as i64;
                let mut v = 0u8;
                for ny in (y - 1).max(0)..=(y + 1).min(h - 1) {
                    for nx in (x - 1).max(0)..=(x + 1).min(w - 1) {
                        v = v.max(prev[(ny * w + nx) as usize]);
                    }
                }
                *o = v;
            }
        });
    }
    GrayImage::from_raw(gray.width(), gray.height(), out).expect("size")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_verbatim_and_complete() {
        assert_eq!(PRESETS.len(), 18);
        let p = preset_pipeline("Newsprint (print-friendly)").unwrap();
        let enabled: Vec<_> = p.iter().filter(|s| s.enabled).map(|s| s.kind.as_str()).collect();
        assert_eq!(enabled, ["autocontrast", "contrast", "sharpen", "dither", "edge_boost"]);
        assert_eq!(p[0].params["clip"], 2.0);
        assert_eq!(p.len(), filter_defs().len());
        assert!(is_identity(&default_pipeline()));
        assert!(preset_pipeline("nope").is_none());
    }

    #[test]
    fn box_radius_matches_pil() {
        // sigma2 = 4/3, L = sqrt(17), l = 1, a = 3·(2 − 4) / (6·(4/3 − 4)) = 0.375
        assert!((box_radius(2.0, 3) - 1.375).abs() < 1e-6);
    }

    #[test]
    fn dither_keeps_mean_tone() {
        let g = GrayImage::from_pixel(64, 64, image::Luma([128]));
        let d = dither(&g);
        let white = d.as_raw().iter().filter(|&&v| v == 255).count();
        assert!((white as f64 / 4096.0 - 0.5).abs() < 0.05);
    }

    #[test]
    fn unknown_filters_are_skipped() {
        let img = RgbImage::from_pixel(4, 4, image::Rgb([10, 20, 30]));
        let out = apply_pipeline(
            &img,
            &[FilterStep { kind: "nope".into(), enabled: true, params: BTreeMap::new() }],
        );
        assert_eq!(out, img);
    }
}
