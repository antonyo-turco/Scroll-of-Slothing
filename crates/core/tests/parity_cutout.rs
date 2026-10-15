//! Golden parity: cut-out pipeline vs Python `ui/cutout_compositing.py`
//! (fixtures/parity/cutout.json + cutout/*.png).
//!
//! - Outline and standee are fed Python's own lasso result, so they must match
//!   **exactly** (no anti-aliasing involved).
//! - The lasso itself is anti-aliased here (PIL's edge is hard): same crop box,
//!   interior identical, edge pixels within tolerance.

use image::RgbaImage;
use serde_json::Value;
use sleuth_core::cutout::{add_outline, apply_lasso, standee_block};

fn dir() -> String {
    format!("{}/../../fixtures/parity", env!("CARGO_MANIFEST_DIR"))
}

fn load(rel: &str) -> RgbaImage {
    image::open(format!("{}/{rel}", dir())).unwrap_or_else(|e| panic!("{rel}: {e}")).to_rgba8()
}

fn cases() -> Vec<Value> {
    let j: Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{}/cutout.json", dir())).unwrap())
            .unwrap();
    j["cases"].as_array().unwrap().clone()
}

#[test]
fn outline_matches_python_exactly() {
    let mut n = 0;
    for c in cases().iter().filter(|c| c.get("thickness").is_some()) {
        let cut = load(c["cut"].as_str().unwrap());
        let rgb = [0, 1, 2].map(|i| c["color"][i].as_u64().unwrap() as u8);
        let got = add_outline(&cut, c["thickness"].as_u64().unwrap() as u32, rgb);
        let want = load(c["output"].as_str().unwrap());
        assert_eq!(got.dimensions(), want.dimensions(), "{}", c["output"]);
        let diff = got.pixels().zip(want.pixels()).filter(|(a, b)| a != b).count();
        assert_eq!(diff, 0, "{}: {diff} pixels differ", c["output"]);
        n += 1;
    }
    assert_eq!(n, 6);
}

#[test]
fn standee_matches_python_exactly() {
    for c in cases().iter().filter(|c| c.get("standee_of").is_some()) {
        let got = standee_block(&load(c["standee_of"].as_str().unwrap()));
        let want = load(c["output"].as_str().unwrap());
        assert_eq!(got, want, "{}", c["output"]);
    }
}

#[test]
fn lasso_matches_python_crop_and_interior() {
    let source = load("cutout/source.png");
    let mut seen = std::collections::HashSet::new();
    for c in cases().iter().filter(|c| c.get("thickness").is_some()) {
        let name = c["polygon"].as_str().unwrap();
        if !seen.insert(name.to_string()) {
            continue;
        }
        let pts: Vec<[f64; 2]> = c["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
            .collect();
        let got = apply_lasso(&source, &pts);
        let want = load(c["cut"].as_str().unwrap());
        assert_eq!(got.dimensions(), want.dimensions(), "{name}: crop box");
        let n = f64::from(got.width() * got.height());
        let (mut colour_diff, mut alpha_flip) = (0u32, 0u32);
        for (g, w) in got.pixels().zip(want.pixels()) {
            if g[3] == 255 && w[3] == 255 && g != w {
                colour_diff += 1;
            }
            if (i32::from(g[3]) - i32::from(w[3])).abs() > 128 {
                alpha_flip += 1;
            }
        }
        let flip_pct = 100.0 * f64::from(alpha_flip) / n;
        println!("{name}: alpha flips {flip_pct:.3}%");
        assert_eq!(colour_diff, 0, "{name}: interior colours must be untouched");
        assert!(flip_pct <= 1.5, "{name}: {flip_pct:.2}% of pixels differ in coverage");
    }
}
