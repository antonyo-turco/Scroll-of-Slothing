//! Golden parity: AoE geometry, labels, presets, MaxRects sheet packing and the
//! rendered templates vs Python `aoe_templates.py`
//! (fixtures/parity/{aoe_geometry,maxrects,map_images}.json).

use serde_json::Value;
use sleuth_core::aoe::{self, Geometry, Shape, Style};
use sleuth_core::pack::maxrects::{PackItem, pack};

fn fixture(name: &str) -> Value {
    let path = format!("{}/../../fixtures/parity/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn shape(s: &str) -> Shape {
    match s {
        "circle" => Shape::Circle,
        "cone" => Shape::Cone,
        "line" => Shape::Line,
        _ => Shape::Square,
    }
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

#[test]
fn geometry_labels_and_presets_match_python() {
    let json = fixture("aoe_geometry.json");
    let mut n = 0;
    for c in json["cases"].as_array().unwrap() {
        if let Some(presets) = c.get("presets") {
            let presets = presets.as_array().unwrap();
            assert_eq!(presets.len(), aoe::PRESETS.len());
            for (p, (name, s, a, b)) in presets.iter().zip(aoe::PRESETS) {
                assert_eq!(p[0].as_str().unwrap(), name);
                assert_eq!(shape(p[1].as_str().unwrap()), s);
                assert_eq!((f(&p[2]), f(&p[3])), (a, b));
            }
            continue;
        }
        let s = shape(c["shape"].as_str().unwrap());
        let (a, b) = (f(&c["a_ft"]), f(&c["b_ft"]));
        assert_eq!(aoe::bbox_ft(s, a, b), (f(&c["bbox_ft"][0]), f(&c["bbox_ft"][1])), "{c}");
        assert_eq!(aoe::default_label(s, a, b), c["label"].as_str().unwrap());
        match aoe::shape_points(s, a, b, 30.0, 8.0, 8.0) {
            Geometry::Ellipse(bx) => {
                let want: Vec<f64> = c["points"].as_array().unwrap().iter().map(f).collect();
                assert_eq!(bx.to_vec(), want, "{c}");
            }
            Geometry::Polygon(pts) => {
                let want: Vec<(f64, f64)> =
                    c["points"].as_array().unwrap().iter().map(|p| (f(&p[0]), f(&p[1]))).collect();
                assert_eq!(pts, want, "{c}");
            }
        }
        n += 1;
    }
    assert_eq!(n, 24);
}

#[test]
fn maxrects_matches_python() {
    let json = fixture("maxrects.json");
    for c in json["cases"].as_array().unwrap() {
        let dpi = f(&c["dpi"]);
        let r = |v: f64| v.round_ties_even() as i64;
        let items: Vec<PackItem> = c["items_in"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| PackItem { w: r(f(&i[0]) * dpi), h: r(f(&i[1]) * dpi) })
            .collect();
        let (pw, ph) = (c["page_px"][0].as_i64().unwrap(), c["page_px"][1].as_i64().unwrap());
        let got = pack(&items, pw, ph, r(f(&c["margin_in"]) * dpi), r(f(&c["gap_in"]) * dpi));
        assert_eq!(got.pages as u64, c["pages"].as_u64().unwrap(), "{c}");
        let skipped: Vec<usize> =
            c["skipped"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect();
        assert_eq!(got.skipped, skipped);
        let want = c["placed"].as_array().unwrap();
        assert_eq!(got.placed.len(), want.len());
        for (g, w) in got.placed.iter().zip(want) {
            assert_eq!(
                (g.item as i64, g.page as i64, g.x, g.y, g.w, g.h),
                (
                    w["item"].as_i64().unwrap(),
                    w["page"].as_i64().unwrap(),
                    w["x"].as_i64().unwrap(),
                    w["y"].as_i64().unwrap(),
                    w["w"].as_i64().unwrap(),
                    w["h"].as_i64().unwrap()
                ),
                "{c}"
            );
        }
        // the full sheet_layout path agrees with the replayed loop
        if c["format"] == "A4" {
            let sizes = items.iter().map(|i| (i.w as u32, i.h as u32, dpi));
            let l =
                aoe::sheet_layout(sizes, sleuth_core::units::PageFormat::A4, 1.0, f(&c["gap_in"]));
            assert_eq!(l.placed, got.placed);
        }
    }
}

/// Rendered templates (no label) vs PIL's output. Fill, grid and straight
/// edges match; anti-aliased diagonal/elliptic outline edges differ slightly
/// (measured MAE 0.006–0.44), so the bound is 1.0 per channel.
#[test]
fn rendered_templates_match_python() {
    let json = fixture("map_images.json");
    let dir = format!("{}/../../fixtures/parity", env!("CARGO_MANIFEST_DIR"));
    let mut n = 0;
    for c in json["cases"].as_array().unwrap().iter().filter(|c| c["kind"] == "aoe") {
        let (img, cell) = aoe::render_template(
            shape(c["shape"].as_str().unwrap()),
            f(&c["a_ft"]),
            f(&c["b_ft"]),
            5.0,
            f(&c["px_per_square"]),
            &Style::default(),
            None,
            2,
        );
        assert_eq!(cell, f(&c["cell_px"]));
        let want =
            image::open(format!("{dir}/{}", c["output"].as_str().unwrap())).unwrap().to_rgb8();
        assert_eq!(img.dimensions(), want.dimensions(), "{c}");
        let sum: u64 =
            img.as_raw().iter().zip(want.as_raw()).map(|(a, b)| u64::from(a.abs_diff(*b))).sum();
        let mae = sum as f64 / img.as_raw().len() as f64;
        println!("{} mae {mae:.3}", c["shape"]);
        if let Ok(d) = std::env::var("SLEUTH_DUMP") {
            img.save(format!("{d}/aoe_{}.png", c["shape"].as_str().unwrap())).unwrap();
        }
        assert!(mae <= 1.0, "{}: mae {mae}", c["shape"]);
        n += 1;
    }
    assert_eq!(n, 4);
}
