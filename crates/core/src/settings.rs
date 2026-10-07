//! Global settings: typed defaults, strict validation for edits (UI/MCP) and
//! lenient coercion for legacy `app_settings.json` files.
//! Spec: `docs/features/settings.md`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::units::PageFormat;

/// Navigable views. The string ids are stable and persisted (`start_view`).
pub const ROUTES: [&str; 10] = [
    "library",
    "tokens.cutter",
    "tokens.sheet",
    "cutouts.cutter",
    "cutouts.sheet",
    "cards",
    "maps.paginator",
    "maps.aoe",
    "campaigns",
    "settings",
];

/// Maps a Python sidebar label (the old `start_tab` value) to a route id.
pub fn route_for_legacy_tab(label: &str) -> Option<&'static str> {
    Some(match label {
        "Manage Campaigns" => "campaigns",
        "Tagged Assets" | "Assets" => "library",
        "Token Cutter" => "tokens.cutter",
        "Token Sheet" => "tokens.sheet",
        "Object Cutter" => "cutouts.cutter",
        "Object Sheet" => "cutouts.sheet",
        "Card Printer" => "cards",
        "Map Paginator" => "maps.paginator",
        "AoE Templates" => "maps.aoe",
        "⚙ Settings" | "Settings" => "settings",
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CutLineStyle {
    #[default]
    Dashed,
    Dotted,
    Solid,
}

impl CutLineStyle {
    /// PDF dash array in points (empty = solid). Matches `app_settings.CUT_LINE_STYLES`.
    pub fn pdf_dash(self) -> &'static [f64] {
        match self {
            CutLineStyle::Dashed => &[2.0, 3.0],
            CutLineStyle::Dotted => &[0.5, 2.0],
            CutLineStyle::Solid => &[],
        }
    }

    /// Dash pattern for on-screen previews (canvas `setLineDash`).
    pub fn preview_dash(self) -> &'static [f64] {
        match self {
            CutLineStyle::Dashed => &[3.0, 2.0],
            CutLineStyle::Dotted => &[1.0, 2.0],
            CutLineStyle::Solid => &[],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// An `#rrggbb` colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HexColor(pub [u8; 3]);

impl HexColor {
    pub fn parse(s: &str) -> Option<Self> {
        let h = s.trim().strip_prefix('#')?;
        if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
        Some(HexColor([byte(0)?, byte(2)?, byte(4)?]))
    }

    /// Components scaled to 0..=1 (PDF colour space).
    pub fn unit_rgb(self) -> [f64; 3] {
        self.0.map(|c| f64::from(c) / 255.0)
    }
}

impl TryFrom<String> for HexColor {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        HexColor::parse(&s).ok_or_else(|| format!("invalid colour {s:?}, expected #rrggbb"))
    }
}

impl From<HexColor> for String {
    fn from(c: HexColor) -> String {
        format!("#{:02x}{:02x}{:02x}", c.0[0], c.0[1], c.0[2])
    }
}

/// All persisted settings with their defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub start_view: String,
    pub sidebar_collapsed: bool,
    pub theme: Theme,
    pub token_page_format: PageFormat,
    pub object_page_format: PageFormat,
    pub card_page_format: PageFormat,
    pub map_page_format: PageFormat,
    pub token_cut_border: bool,
    pub map_cut_border: bool,
    pub cut_line_style: CutLineStyle,
    pub cut_line_color: HexColor,
    pub cut_line_width: f64,
    pub map_square_in: f64,
    pub map_calibration_mm: f64,
    pub map_margin_in: f64,
    pub map_overlap_in: f64,
    pub map_align_to_grid: bool,
    pub mcp_http_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            start_view: "tokens.sheet".into(),
            sidebar_collapsed: false,
            theme: Theme::System,
            token_page_format: PageFormat::Letter,
            object_page_format: PageFormat::Letter,
            card_page_format: PageFormat::A4,
            map_page_format: PageFormat::A4,
            token_cut_border: false,
            map_cut_border: false,
            cut_line_style: CutLineStyle::Dashed,
            cut_line_color: HexColor([0x80, 0x80, 0x80]),
            cut_line_width: 0.5,
            map_square_in: 1.0,
            map_calibration_mm: 25.4,
            map_margin_in: 0.0,
            map_overlap_in: 0.0,
            map_align_to_grid: false,
            mcp_http_enabled: false,
        }
    }
}

/// Per-key validation errors (`key -> message`).
pub type SettingsErrors = BTreeMap<String, String>;

impl Settings {
    /// Line width actually used when stroking cut guides.
    pub fn effective_cut_line_width(&self) -> f64 {
        self.cut_line_width.max(0.1)
    }

    /// Scale factor that corrects printer scaling (`25.4 / measured mm`).
    pub fn map_calibration_factor(&self) -> f64 {
        25.4 / self.map_calibration_mm
    }

    /// Flattens to `key -> json` rows for storage.
    pub fn to_map(&self) -> Map<String, Value> {
        match serde_json::to_value(self) {
            Ok(Value::Object(m)) => m,
            _ => unreachable!("Settings always serializes to an object"),
        }
    }

    /// Builds settings from stored `key -> json` rows. Unknown keys are ignored;
    /// invalid values fall back to defaults (storage is trusted but may be older).
    pub fn from_map(map: &Map<String, Value>) -> Self {
        let mut s = Settings::default();
        let mut errors = SettingsErrors::new();
        for (k, v) in map {
            s.apply_one(k, v, &mut errors);
        }
        s
    }

    /// Strictly applies a patch (UI / MCP edits). Valid keys are applied, invalid
    /// ones are reported and left unchanged. Unknown keys are reported too.
    pub fn apply_patch(&mut self, patch: &Map<String, Value>) -> SettingsErrors {
        let mut errors = SettingsErrors::new();
        for (k, v) in patch {
            self.apply_one(k, v, &mut errors);
        }
        errors
    }

    fn apply_one(&mut self, key: &str, v: &Value, errors: &mut SettingsErrors) {
        let mut err = |msg: String| {
            errors.insert(key.to_string(), msg);
        };
        let positive = |x: f64| x.is_finite() && x > 0.0;
        let non_negative = |x: f64| x.is_finite() && x >= 0.0;
        macro_rules! set {
            ($field:ident, $ty:ty) => {
                match serde_json::from_value::<$ty>(v.clone()) {
                    Ok(x) => self.$field = x,
                    Err(e) => err(e.to_string()),
                }
            };
            ($field:ident, f64, $check:expr, $msg:literal) => {
                match v.as_f64() {
                    Some(x) if $check(x) => self.$field = x,
                    _ => err(format!($msg)),
                }
            };
        }
        match key {
            "start_view" => match v.as_str() {
                Some(r) if ROUTES.contains(&r) => self.start_view = r.to_string(),
                _ => err(format!("unknown view; expected one of {ROUTES:?}")),
            },
            "sidebar_collapsed" => set!(sidebar_collapsed, bool),
            "theme" => set!(theme, Theme),
            "token_page_format" => set!(token_page_format, PageFormat),
            "object_page_format" => set!(object_page_format, PageFormat),
            "card_page_format" => set!(card_page_format, PageFormat),
            "map_page_format" => set!(map_page_format, PageFormat),
            "token_cut_border" => set!(token_cut_border, bool),
            "map_cut_border" => set!(map_cut_border, bool),
            "cut_line_style" => set!(cut_line_style, CutLineStyle),
            "cut_line_color" => set!(cut_line_color, HexColor),
            "cut_line_width" => set!(cut_line_width, f64, positive, "must be > 0"),
            "map_square_in" => set!(map_square_in, f64, positive, "must be > 0"),
            "map_calibration_mm" => set!(map_calibration_mm, f64, positive, "must be > 0"),
            "map_margin_in" => set!(map_margin_in, f64, non_negative, "must be ≥ 0"),
            "map_overlap_in" => set!(map_overlap_in, f64, non_negative, "must be ≥ 0"),
            "map_align_to_grid" => set!(map_align_to_grid, bool),
            "mcp_http_enabled" => set!(mcp_http_enabled, bool),
            _ => err("unknown setting".into()),
        }
    }

    /// Lenient import of the Python `app_settings.json`, mirroring
    /// `app_settings._coerce`: wrong types fall back to defaults, bools must be
    /// real bools, floats accept numbers, numeric strings and bools.
    /// Afterwards, values the new app would reject (e.g. a zero square size,
    /// which Python accepted and then divided by) are reset to their defaults.
    pub fn from_legacy_json(v: &Value) -> Self {
        let mut s = Settings::default();
        let Some(obj) = v.as_object() else { return s };
        let fmt = |v: &Value, d: PageFormat| v.as_str().and_then(PageFormat::parse).unwrap_or(d);
        for (k, v) in obj {
            match k.as_str() {
                "start_tab" => {
                    if let Some(r) = v.as_str().and_then(route_for_legacy_tab) {
                        s.start_view = r.into();
                    }
                }
                "sidebar_collapsed" => s.sidebar_collapsed = v.as_bool().unwrap_or(false),
                "token_page_format" => s.token_page_format = fmt(v, s.token_page_format),
                "map_page_format" => s.map_page_format = fmt(v, s.map_page_format),
                "token_cut_border" => s.token_cut_border = v.as_bool().unwrap_or(false),
                "map_cut_border" => s.map_cut_border = v.as_bool().unwrap_or(false),
                "map_align_to_grid" => s.map_align_to_grid = v.as_bool().unwrap_or(false),
                "cut_line_style" => {
                    s.cut_line_style = match v.as_str() {
                        Some("Dashed") => CutLineStyle::Dashed,
                        Some("Dotted") => CutLineStyle::Dotted,
                        Some("Solid") => CutLineStyle::Solid,
                        _ => CutLineStyle::Dashed,
                    }
                }
                "cut_line_color" => {
                    if let Some(c) = v.as_str().and_then(HexColor::parse) {
                        s.cut_line_color = c;
                    }
                }
                "cut_line_width" => s.cut_line_width = legacy_float(v).unwrap_or(0.5),
                "map_square_in" => s.map_square_in = legacy_float(v).unwrap_or(1.0),
                "map_calibration_mm" => s.map_calibration_mm = legacy_float(v).unwrap_or(25.4),
                "map_margin_in" => s.map_margin_in = legacy_float(v).unwrap_or(0.0),
                "map_overlap_in" => s.map_overlap_in = legacy_float(v).unwrap_or(0.0),
                _ => {}
            }
        }
        s.sanitize();
        s
    }

    /// Resets numeric values that fail strict validation to their defaults.
    fn sanitize(&mut self) {
        let d = Settings::default();
        let ok_pos = |x: f64| x.is_finite() && x > 0.0;
        let ok_nonneg = |x: f64| x.is_finite() && x >= 0.0;
        if !ok_pos(self.cut_line_width) {
            self.cut_line_width = d.cut_line_width;
        }
        if !ok_pos(self.map_square_in) {
            self.map_square_in = d.map_square_in;
        }
        if !ok_pos(self.map_calibration_mm) {
            self.map_calibration_mm = d.map_calibration_mm;
        }
        if !ok_nonneg(self.map_margin_in) {
            self.map_margin_in = d.map_margin_in;
        }
        if !ok_nonneg(self.map_overlap_in) {
            self.map_overlap_in = d.map_overlap_in;
        }
    }
}

/// Python `float(value)` semantics used by `app_settings._coerce` for float
/// settings: numbers, bools (`True` → 1.0) and numeric strings convert;
/// anything else is `None` (→ default).
pub fn legacy_float(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(t) => t.trim().parse::<f64>().ok().filter(|x| x.is_finite()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn defaults_match_python_app_settings() {
        let s = Settings::default();
        assert_eq!(s.token_page_format, PageFormat::Letter);
        assert_eq!(s.map_page_format, PageFormat::A4);
        assert!(!s.token_cut_border && !s.map_cut_border && !s.map_align_to_grid);
        assert_eq!(s.cut_line_style, CutLineStyle::Dashed);
        assert_eq!(String::from(s.cut_line_color), "#808080");
        assert_eq!(s.cut_line_width, 0.5);
        assert_eq!(s.map_square_in, 1.0);
        assert_eq!(s.map_calibration_mm, 25.4);
        assert_eq!((s.map_margin_in, s.map_overlap_in), (0.0, 0.0));
        assert_eq!(s.start_view, "tokens.sheet");
    }

    #[test]
    fn map_roundtrip_is_lossless() {
        let s = Settings {
            cut_line_style: CutLineStyle::Dotted,
            map_overlap_in: 0.25,
            theme: Theme::Dark,
            ..Settings::default()
        };
        assert_eq!(Settings::from_map(&s.to_map()), s);
    }

    #[test]
    fn patch_applies_valid_and_reports_invalid_keys() {
        let mut s = Settings::default();
        let errors = s.apply_patch(&obj(json!({
            "map_margin_in": 0.5,
            "map_square_in": 0,
            "cut_line_color": "red",
            "start_view": "music",
            "token_page_format": "A4",
            "bogus": 1
        })));
        assert_eq!(s.map_margin_in, 0.5);
        assert_eq!(s.token_page_format, PageFormat::A4);
        assert_eq!(s.map_square_in, 1.0, "invalid value must not be applied");
        let keys: Vec<_> = errors.keys().map(String::as_str).collect();
        assert_eq!(keys, ["bogus", "cut_line_color", "map_square_in", "start_view"]);
    }

    #[test]
    fn negative_margin_rejected_zero_allowed() {
        let mut s = Settings::default();
        assert!(s.apply_patch(&obj(json!({"map_margin_in": 0.0}))).is_empty());
        assert!(
            s.apply_patch(&obj(json!({"map_overlap_in": -1.0}))).contains_key("map_overlap_in")
        );
    }

    #[test]
    fn legacy_coercion_mirrors_python() {
        let s = Settings::from_legacy_json(&json!({
            "start_tab": "Map Paginator",
            "sidebar_collapsed": "yes",       // not a real bool -> default
            "token_cut_border": true,
            "cut_line_width": "1.5",          // numeric string -> float
            "map_square_in": true,            // float(True) == 1.0 in Python
            "map_calibration_mm": "abc",      // invalid -> default
            "cut_line_style": "Wavy",         // invalid -> Dashed
            "map_page_format": "Letter",
            "unknown": 3
        }));
        assert_eq!(s.start_view, "maps.paginator");
        assert!(!s.sidebar_collapsed);
        assert!(s.token_cut_border);
        assert_eq!(s.cut_line_width, 1.5);
        assert_eq!(s.map_square_in, 1.0);
        assert_eq!(s.map_calibration_mm, 25.4);
        assert_eq!(s.cut_line_style, CutLineStyle::Dashed);
        assert_eq!(s.map_page_format, PageFormat::Letter);
    }

    #[test]
    fn every_legacy_tab_label_maps_to_a_known_route() {
        for label in [
            "Manage Campaigns",
            "Tagged Assets",
            "Assets",
            "Token Cutter",
            "Token Sheet",
            "Object Cutter",
            "Object Sheet",
            "Card Printer",
            "Map Paginator",
            "AoE Templates",
        ] {
            let r = route_for_legacy_tab(label).unwrap();
            assert!(ROUTES.contains(&r), "{label} -> {r}");
        }
        assert_eq!(route_for_legacy_tab("Mixer"), None, "music tabs fall back to default");
    }

    #[test]
    fn dash_patterns_match_python() {
        assert_eq!(CutLineStyle::Dashed.pdf_dash(), &[2.0, 3.0]);
        assert_eq!(CutLineStyle::Dotted.pdf_dash(), &[0.5, 2.0]);
        assert!(CutLineStyle::Solid.pdf_dash().is_empty());
    }

    #[test]
    fn effective_width_has_floor_and_calibration_factor() {
        let mut s = Settings { cut_line_width: 0.01, ..Settings::default() };
        assert_eq!(s.effective_cut_line_width(), 0.1);
        s.map_calibration_mm = 25.0;
        assert!((s.map_calibration_factor() - 1.016).abs() < 1e-12);
    }
}
