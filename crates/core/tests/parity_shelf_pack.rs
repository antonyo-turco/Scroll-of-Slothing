//! Golden parity: object sheet shelf packing vs Python `CutoutSheetGenerator._layout`
//! (fixtures/parity/shelf_pack.json).

use serde_json::Value;
use sleuth_core::pack::shelf::{SheetMode, ShelfItem, pack};
use sleuth_core::units::{PageFormat, SizeLabel};

#[test]
fn shelf_packing_matches_python_exactly() {
    let path = format!("{}/../../fixtures/parity/shelf_pack.json", env!("CARGO_MANIFEST_DIR"));
    let json: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let cases = json["cases"].as_array().unwrap();
    assert!(cases.len() >= 20);
    for case in cases {
        let fmt = PageFormat::parse(case["format"].as_str().unwrap()).unwrap();
        let mode = if case["mode"] == "standee" { SheetMode::Standee } else { SheetMode::Flat };
        let items: Vec<ShelfItem> = case["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| ShelfItem {
                w_px: i["w_px"].as_u64().unwrap() as u32,
                h_px: i["h_px"].as_u64().unwrap() as u32,
                size_pt: SizeLabel::parse(i["size"].as_str().unwrap()).unwrap().points(),
                scale_pct: i["scale"].as_f64().unwrap(),
                copies: i["copies"].as_u64().unwrap() as u32,
            })
            .collect();
        let got = pack(&items, fmt, mode);
        let want = case["pages"].as_array().unwrap();
        // Python keeps a trailing empty page only when every item was oversized; none are here.
        assert_eq!(got.pages.len(), want.len(), "pages for {case}");
        for (gp, wp) in got.pages.iter().zip(want) {
            let wp = wp.as_array().unwrap();
            assert_eq!(gp.len(), wp.len());
            for (g, w) in gp.iter().zip(wp) {
                for (gv, key) in [(g.x, "x"), (g.y, "y"), (g.w, "w"), (g.h, "h")] {
                    let wv = w[key].as_f64().unwrap();
                    assert!((gv - wv).abs() < 1e-6, "{key}: {gv} vs {wv}");
                }
            }
        }
    }
}
