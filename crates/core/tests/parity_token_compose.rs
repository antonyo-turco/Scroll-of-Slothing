//! Golden parity: token compositing vs Python `compose_token`
//! (fixtures/parity/token_compose.json + token/*.png).
//!
//! Differences allowed: our circle masks are anti-aliased (PIL's are hard) and
//! the resampler is a different Lanczos3 implementation. So images are compared
//! as printed (over white) with a mean-error bound, and alpha must agree except
//! on a thin edge band.

use image::RgbaImage;
use serde_json::Value;
use sleuth_core::token::{TokenParams, compose};

fn dir() -> String {
    format!("{}/../../fixtures/parity", env!("CARGO_MANIFEST_DIR"))
}

fn load(rel: &str) -> RgbaImage {
    image::open(format!("{}/{rel}", dir())).unwrap_or_else(|e| panic!("{rel}: {e}")).to_rgba8()
}

fn on_white(p: &image::Rgba<u8>) -> [f64; 3] {
    let a = f64::from(p[3]) / 255.0;
    [0, 1, 2].map(|c| f64::from(p[c]) * a + 255.0 * (1.0 - a))
}

#[test]
fn compose_matches_python_within_tolerance() {
    let json: Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{}/token_compose.json", dir())).unwrap()).unwrap();
    let source = load("token/source.png");
    let borders = format!("{}/../../assets/token_borders", env!("CARGO_MANIFEST_DIR"));
    for case in json["cases"].as_array().unwrap() {
        let border = image::open(format!("{borders}/{}", case["border"].as_str().unwrap())).unwrap().to_rgba8();
        let params = TokenParams {
            border: String::new(),
            bg: case["bg_color"].as_array().map(|a| [0, 1, 2].map(|i| a[i].as_u64().unwrap() as u8)),
            ring_inset_pct: case["ring_inset_pct"].as_f64().unwrap(),
            pan: [case["pan_x"].as_f64().unwrap(), case["pan_y"].as_f64().unwrap()],
            zoom: case["zoom"].as_f64().unwrap(),
        };
        let got = compose(&source, &border, &params);
        let want = load(case["output"].as_str().unwrap());
        assert_eq!(got.dimensions(), want.dimensions());

        let n = f64::from(got.width() * got.height());
        let mut sum = 0.0;
        let mut alpha_mismatch = 0u32;
        for (g, w) in got.pixels().zip(want.pixels()) {
            let (a, b) = (on_white(g), on_white(w));
            sum += (0..3).map(|c| (a[c] - b[c]).abs()).sum::<f64>() / 3.0;
            if (i32::from(g[3]) - i32::from(w[3])).abs() > 128 {
                alpha_mismatch += 1;
            }
        }
        let mean = sum / n;
        let mismatch_pct = 100.0 * f64::from(alpha_mismatch) / n;
        println!("{}: mean |Δ| = {mean:.2}, alpha mismatches = {mismatch_pct:.3}%", case["output"]);
        assert!(mean <= 3.0, "{}: mean error {mean:.2} too high", case["output"]);
        assert!(mismatch_pct <= 1.0, "{}: alpha differs on {mismatch_pct:.2}% of pixels", case["output"]);
    }
}
