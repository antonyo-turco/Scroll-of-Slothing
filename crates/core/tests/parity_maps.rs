//! Golden parity: map grid sizing, line positions, dash patterns, homography
//! and page tiling vs Python `map_grid.py` / `map_pdf_generator.py`
//! (fixtures/parity/{grid_modes,homography,tiling}.json).

use serde_json::Value;
use sleuth_core::maps::grid::{self, GridStyle};
use sleuth_core::maps::homography::{self, Pt, WarpError};
use sleuth_core::maps::tiling::{TileParams, plan};
use sleuth_core::units::PageFormat;

fn fixture(name: &str) -> Value {
    let path = format!("{}/../../fixtures/parity/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn close(a: f64, b: f64, ctx: &str) {
    assert!((a - b).abs() <= 1e-9 * b.abs().max(1.0), "{ctx}: {a} vs {b}");
}

fn tuple4(v: &Value) -> (f64, f64, f64, f64) {
    let a = v.as_array().unwrap();
    (f(&a[0]), f(&a[1]), f(&a[2]), f(&a[3]))
}

fn pt(v: &Value) -> Pt {
    (f(&v[0]), f(&v[1]))
}

fn quad(v: &Value) -> [Pt; 4] {
    let a = v.as_array().unwrap();
    [pt(&a[0]), pt(&a[1]), pt(&a[2]), pt(&a[3])]
}

#[test]
fn grid_modes_match_python() {
    let json = fixture("grid_modes.json");
    let cases = json["cases"].as_array().unwrap();
    assert!(cases.len() >= 25);
    for c in cases {
        let ctx = c.to_string();
        match c["mode"].as_str().unwrap() {
            "rows_cols" | "reference" | "ppi" | "cell_side" => {
                let got = match c["mode"].as_str().unwrap() {
                    "rows_cols" => grid::from_rows_cols(
                        (
                            c["size"][0].as_u64().unwrap() as u32,
                            c["size"][1].as_u64().unwrap() as u32,
                        ),
                        c["rows"].as_i64().unwrap(),
                        c["cols"].as_i64().unwrap(),
                    ),
                    "reference" => {
                        grid::from_reference(tuple4(&c["rect"]), c["n"].as_i64().unwrap())
                    }
                    "ppi" => grid::from_ppi(f(&c["ppi"]), f(&c["square_in"])),
                    _ => grid::from_cell_side(f(&c["side"])),
                };
                let want = tuple4(&c["out"]);
                close(got.0, want.0, &ctx);
                close(got.1, want.1, &ctx);
                close(got.2, want.2, &ctx);
                close(got.3, want.3, &ctx);
            }
            "lines" => {
                let got = grid::line_positions(f(&c["offset"]), f(&c["step"]), f(&c["extent"]));
                let want: Vec<f64> = c["out"].as_array().unwrap().iter().map(f).collect();
                assert_eq!(got.len(), want.len(), "{ctx}");
                for (g, w) in got.iter().zip(&want) {
                    close(*g, *w, &ctx);
                }
            }
            "style" => {
                let style = match c["style"].as_str().unwrap() {
                    "solid" => GridStyle::Solid,
                    "dashed" => GridStyle::Dashed,
                    "dotted" => GridStyle::Dotted,
                    _ => GridStyle::Broken,
                };
                let got = grid::style_pattern(
                    style,
                    f(&c["cell"]),
                    c["thickness"].as_u64().unwrap() as u32,
                );
                match got {
                    None => assert!(c["out"].is_null(), "{ctx}"),
                    Some((on, off)) => {
                        close(on, f(&c["out"][0]), &ctx);
                        close(off, f(&c["out"][1]), &ctx);
                    }
                }
            }
            m => panic!("unknown mode {m}"),
        }
    }
}

#[test]
fn homography_matches_python() {
    let json = fixture("homography.json");
    let mut checked = 0;
    for c in json["cases"].as_array().unwrap() {
        if let Some(msg) = c.get("degenerate") {
            let q = [(0.0, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0)];
            let sq = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
            let err = homography::homography_matrix(&q, &sq).unwrap_err();
            assert_eq!(err, WarpError::Degenerate);
            assert_eq!(err.to_string(), msg.as_str().unwrap());
            continue;
        }
        let ctx = c["quad"].to_string();
        let ordered = homography::order_quad_corners(&quad(&c["quad"]));
        assert_eq!(ordered, quad(&c["ordered"]), "{ctx}");
        let side = 200.0;
        let square = [(0.0, 0.0), (side, 0.0), (side, side), (0.0, side)];
        let coeffs = homography::perspective_coeffs(&square, &ordered).unwrap();
        for (g, w) in coeffs.iter().zip(c["perspective_coeffs"].as_array().unwrap()) {
            close(*g, f(w), &ctx);
        }
        let m = homography::homography_matrix(&ordered, &square).unwrap();
        for r in 0..3 {
            for k in 0..3 {
                close(m[r][k], f(&c["homography"][r][k]), &ctx);
            }
        }
        for ((x, y), w) in
            [(0.0, 0.0), (500.0, 0.0), (123.0, 456.0)].iter().zip(c["apply"].as_array().unwrap())
        {
            let (gx, gy) = homography::apply_homography(&m, *x, *y);
            close(gx, f(&w[0]), &ctx);
            close(gy, f(&w[1]), &ctx);
        }
        checked += 1;
    }
    assert_eq!(checked, 4);
}

#[test]
fn tiling_matches_python() {
    let json = fixture("tiling.json");
    let cases = json["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 28);
    for c in cases {
        let ctx = format!("{} margin {} {}", c["format"], c["margin_pt"], c["image_size"]);
        let p = TileParams {
            image_w: c["image_size"][0].as_u64().unwrap() as u32,
            image_h: c["image_size"][1].as_u64().unwrap() as u32,
            region: (!c["region"].is_null()).then(|| tuple4(&c["region"])),
            cell_w: f(&c["cell"][0]),
            cell_h: f(&c["cell"][1]),
            square_in: f(&c["sq"]),
            format: PageFormat::parse(c["format"].as_str().unwrap()).unwrap(),
            margin_pt: f(&c["margin_pt"]),
            offset: pt(&c["off"]),
            overlap_in: f(&c["ov"]),
            align_to_grid: c["align"].as_bool().unwrap(),
            grid_offset: pt(&c["goff"]),
            landscape: false,
        };
        let got = plan(&p);
        close(got.scale_x, f(&c["scale"][0]), &ctx);
        close(got.scale_y, f(&c["scale"][1]), &ctx);
        let want = c["tiles"].as_array().unwrap();
        assert_eq!(got.tiles.len(), want.len(), "{ctx}");
        for (g, w) in got.tiles.iter().zip(want) {
            assert_eq!(
                (g.col, g.row),
                (w["col"].as_u64().unwrap() as u32, w["row"].as_u64().unwrap() as u32),
                "{ctx}"
            );
            let (gc, wc) = (g.crop, tuple4(&w["crop"]));
            for (a, b) in [(gc.0, wc.0), (gc.1, wc.1), (gc.2, wc.2), (gc.3, wc.3)] {
                close(a, b, &ctx);
            }
            let (gp, wp) = (g.page_rect, tuple4(&w["page_rect"]));
            for (a, b) in [(gp.0, wp.0), (gp.1, wp.1), (gp.2, wp.2), (gp.3, wp.3)] {
                close(a, b, &ctx);
            }
            close(g.page_x_pt, f(&w["page_x_pt"]), &ctx);
            close(g.page_y_pt, f(&w["page_y_pt"]), &ctx);
            close(g.w_pt, f(&w["w_pt"]), &ctx);
            close(g.h_pt, f(&w["h_pt"]), &ctx);
        }
    }
}
