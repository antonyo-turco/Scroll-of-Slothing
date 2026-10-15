//! Golden parity: token sheet packing (Legacy grid) vs Python
//! `PDFGenerator.generate_preview_pages` (fixtures/parity/token_pack.json).

use serde_json::Value;
use sleuth_core::pack::token_grid::{GridMode, TokenItem, pack};
use sleuth_core::units::{PageFormat, SizeLabel};

fn fixture(name: &str) -> Value {
    let path = format!("{}/../../fixtures/parity/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).expect(&path)).unwrap()
}

#[test]
fn legacy_packing_matches_python_exactly() {
    let cases = fixture("token_pack.json")["cases"].as_array().unwrap().clone();
    assert!(cases.len() >= 10);
    for case in cases {
        let fmt = PageFormat::parse(case["format"].as_str().unwrap()).unwrap();
        let items: Vec<TokenItem> = case["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| TokenItem {
                size_pt: SizeLabel::parse(i["size"].as_str().unwrap()).unwrap().points(),
                copies: i["copies"].as_u64().unwrap() as u32,
            })
            .collect();
        let got = pack(&items, fmt, GridMode::Legacy);
        let want = case["pages"].as_array().unwrap();
        assert_eq!(got.pages.len(), want.len(), "page count for {}", case["items"]);
        for (gp, wp) in got.pages.iter().zip(want) {
            let wp = wp.as_array().unwrap();
            assert_eq!(gp.len(), wp.len());
            for (g, w) in gp.iter().zip(wp) {
                assert_eq!(g.item as u64, w["item"].as_u64().unwrap());
                for (gv, key) in [(g.x, "x"), (g.y, "y"), (g.w, "w"), (g.h, "h")] {
                    let wv = w[key].as_f64().unwrap();
                    assert!((gv - wv).abs() < 1e-6, "{key}: {gv} vs {wv} in {}", case["items"]);
                }
            }
        }
    }
}
