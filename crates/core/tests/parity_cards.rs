//! Golden parity: cards vs Python `card_generator.py`
//! (fixtures/parity/cards_parse.json, cards_layout.json).

use serde_json::Value;
use sleuth_core::cards::{
    BODY_FONT_SIZES, CARD_W, body_height, find_overflow, inline_tokens, parse, sim_body_start_y,
    token_lines,
};
use sleuth_core::units::PageFormat;

fn fixture(name: &str) -> Vec<Value> {
    let path = format!("{}/../../fixtures/parity/{name}", env!("CARGO_MANIFEST_DIR"));
    let j: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    j["cases"].as_array().unwrap().clone()
}

#[test]
fn parser_matches_python() {
    let mut n = 0;
    for case in fixture("cards_parse.json") {
        if case.get("inline").is_some() {
            for t in case["texts"].as_array().unwrap() {
                let got: Vec<Value> = inline_tokens(t["text"].as_str().unwrap())
                    .into_iter()
                    .map(|t| serde_json::json!([t.text, t.bold, t.italic, t.underline]))
                    .collect();
                assert_eq!(Value::Array(got), t["tokens"], "inline {}", t["text"]);
            }
            continue;
        }
        let md = case["markdown"].as_str().unwrap();
        let got = parse(md);
        let want = case["cards"].as_array().unwrap();
        assert_eq!(got.len(), want.len(), "card count for {md:?}");
        for (g, w) in got.iter().zip(want) {
            assert_eq!(g.title, w["title"].as_str().unwrap(), "title in {md:?}");
            assert_eq!(g.kind, w["type"].as_str().unwrap());
            assert_eq!(g.body, w["body"].as_str().unwrap());
            assert_eq!(u64::from(g.copies), w["copies"].as_u64().unwrap());
            let stats: Vec<Value> =
                g.stats.iter().map(|(k, v)| serde_json::json!([k, v])).collect();
            assert_eq!(Value::Array(stats), w["stats"]);
            for (i, c) in g.color.iter().enumerate() {
                assert!((c - w["color"][i].as_f64().unwrap()).abs() < 1e-12);
            }
            n += 1;
        }
    }
    assert!(n >= 10);
}

#[test]
fn layout_measurements_and_overflow_match_python() {
    let line_w = CARD_W - 12.0;
    for case in fixture("cards_layout.json") {
        if let Some(fmt) = case.get("page_layout").and_then(Value::as_str) {
            let (pw, ph) = PageFormat::parse(fmt).unwrap().size_pt();
            assert!((pw - case["page_w"].as_f64().unwrap()).abs() < 1e-9);
            assert!(((pw - 540.0) / 2.0 - case["margin_x"].as_f64().unwrap()).abs() < 1e-9);
            assert!(((ph - 756.0) / 2.0 - case["margin_y"].as_f64().unwrap()).abs() < 1e-9);
            continue;
        }
        let md = case["markdown"].as_str().unwrap();
        let cards = parse(md);
        let overflow: Vec<Value> = find_overflow(&cards).into_iter().map(Value::from).collect();
        assert_eq!(Value::Array(overflow), case["overflow"], "overflow set for {md:?}");
        for (card, want) in cards.iter().zip(case["cards"].as_array().unwrap()) {
            let start = sim_body_start_y(card);
            assert!(
                (start - want["sim_body_start_y"].as_f64().unwrap()).abs() < 1e-9,
                "{}: start {start}",
                card.title
            );
            if let Some(heights) = want["body_heights"].as_object() {
                for fs in BODY_FONT_SIZES {
                    let key = format!("{fs:.1}");
                    if let Some(h) = heights.get(&key) {
                        let got = body_height(&card.body, fs, line_w);
                        assert!(
                            (got - h.as_f64().unwrap()).abs() < 1e-9,
                            "{} @ {fs}: {got} vs {h}",
                            card.title
                        );
                    }
                }
            }
            if let Some(lines) = want["lines_at"].as_object() {
                for (fs, want_lines) in lines {
                    let fs: f64 = fs.parse().unwrap();
                    let first_para = card.body.split('\n').next().unwrap();
                    let got: Vec<Vec<String>> = token_lines(&inline_tokens(first_para), fs, line_w)
                        .into_iter()
                        .map(|l| l.into_iter().map(|t| t.text).collect())
                        .collect();
                    assert_eq!(
                        serde_json::to_value(&got).unwrap(),
                        *want_lines,
                        "{} line breaks @ {fs}",
                        card.title
                    );
                }
            }
        }
    }
}
