//! Golden parity: embedded-grid removal, every map filter and preset, canny and
//! the perspective warp vs the Python/PIL/OpenCV output
//! (fixtures/parity/map_images.json + maps/*.png).
//!
//! The ports reproduce PIL's fixed-point maths, so every image is bit-exact
//! except grid removal (f32 FFT vs numpy's f64: a handful of pixels off by
//! one). The test allows only that: ≥ 99.9 % identical pixels and a mean
//! absolute error ≤ 0.05. (The documented floor, MAE ≤ 3 and ≥ 97 % for
//! black-and-white output, is far looser.)

use std::collections::BTreeMap;

use image::RgbImage;
use serde_json::Value;
use sleuth_core::maps::{degrid, filters, homography};

fn dir() -> String {
    format!("{}/../../fixtures/parity", env!("CARGO_MANIFEST_DIR"))
}

fn load(rel: &str) -> RgbImage {
    image::open(format!("{}/{rel}", dir())).unwrap().to_rgb8()
}

fn cases() -> Vec<Value> {
    let json: Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{}/map_images.json", dir())).unwrap(),
    )
    .unwrap();
    json["cases"].as_array().unwrap().clone()
}

/// (mean absolute error per channel, fraction of identical pixels)
fn compare(a: &RgbImage, b: &RgbImage) -> (f64, f64) {
    assert_eq!(a.dimensions(), b.dimensions());
    let mut sum = 0u64;
    let mut same = 0usize;
    for (pa, pb) in a.pixels().zip(b.pixels()) {
        let d: u64 = (0..3).map(|c| u64::from(pa.0[c].abs_diff(pb.0[c]))).sum();
        sum += d;
        same += usize::from(d == 0);
    }
    let n = a.pixels().len();
    (sum as f64 / (n * 3) as f64, same as f64 / n as f64)
}

fn check(label: &str, got: &RgbImage, want: &RgbImage, report: &mut Vec<String>) -> bool {
    let (mae, same) = compare(got, want);
    let ok = same >= 0.999 && mae <= 0.05;
    report.push(format!(
        "{} {label:<40} mae {mae:6.3}  same {:6.2}%",
        if ok { "ok " } else { "BAD" },
        same * 100.0
    ));
    ok
}

fn params(v: &Value) -> BTreeMap<String, f64> {
    v.as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.as_f64().unwrap())).collect()
}

#[test]
fn map_images_match_python() {
    let src = load("maps/source.png");
    let mut report = Vec::new();
    let mut all_ok = true;
    for c in cases() {
        let want = || load(c["output"].as_str().unwrap());
        match c["kind"].as_str().unwrap() {
            "degrid" => {
                let input = load(c["input"].as_str().unwrap());
                let (cw, ch) = (c["cell"][0].as_f64().unwrap(), c["cell"][1].as_f64().unwrap());
                let out = degrid::remove_grid_lines(&input, cw, ch, degrid::DEFAULT_STRENGTH);
                assert_eq!(out.is_some(), c["changed"].as_bool().unwrap(), "{}", c["input"]);
                let got = out.unwrap_or(input);
                all_ok &= check(c["input"].as_str().unwrap(), &got, &want(), &mut report);
            }
            "filter" => {
                let kind = c["type"].as_str().unwrap();
                let step = filters::FilterStep {
                    kind: kind.into(),
                    enabled: true,
                    params: params(&c["params"]),
                };
                let got = filters::apply_pipeline(&src, &[step]);
                let label = format!("filter {kind} {}", c["params"]);
                all_ok &= check(&label, &got, &want(), &mut report);
            }
            "preset" => {
                let name = c["name"].as_str().unwrap();
                let pipeline =
                    filters::preset_pipeline(name).unwrap_or_else(|| panic!("preset {name}"));
                // the Rust preset must enable the same steps, in order, with the same params
                let enabled: Vec<_> = pipeline.iter().filter(|s| s.enabled).collect();
                let py = c["pipeline"].as_array().unwrap();
                assert_eq!(enabled.len(), py.len(), "{name}");
                for (r, p) in enabled.iter().zip(py) {
                    assert_eq!(r.kind, p["type"].as_str().unwrap(), "{name}");
                    assert_eq!(r.params, params(&p["params"]), "{name}");
                }
                let got = filters::apply_pipeline(&src, &pipeline);
                all_ok &= check(&format!("preset {name}"), &got, &want(), &mut report);
            }
            "canny" => {
                let gray = filters::grayscale(&src);
                let g = filters::canny(
                    &gray,
                    c["low"].as_f64().unwrap(),
                    c["high"].as_f64().unwrap(),
                    c["dilate"].as_u64().unwrap() as u32,
                );
                let got = image::DynamicImage::ImageLuma8(g).to_rgb8();
                all_ok &= check(
                    &format!("canny {} {} {}", c["low"], c["high"], c["dilate"]),
                    &got,
                    &want(),
                    &mut report,
                );
            }
            "warp" => {
                let k: Vec<f64> =
                    c["coeffs"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
                let coeffs: [f64; 8] = k.try_into().unwrap();
                let (w, h) =
                    (c["size"][0].as_u64().unwrap() as u32, c["size"][1].as_u64().unwrap() as u32);
                let got = homography::warp_perspective(&src, w, h, &coeffs);
                all_ok &= check("warp", &got, &want(), &mut report);
            }
            "aoe" => {} // covered by parity_aoe
            k => panic!("unknown kind {k}"),
        }
    }
    println!("{}", report.join("\n"));
    assert!(all_ok, "some images are outside tolerance:\n{}", report.join("\n"));
}
