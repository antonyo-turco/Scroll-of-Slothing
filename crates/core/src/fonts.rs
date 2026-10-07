//! Bundled Liberation Sans (SIL OFL, metric-compatible with Helvetica/Arial)
//! and Helvetica-exact text measurement.
//!
//! Glyph advances are rounded to 1/1000 em, the unit of Adobe's Helvetica AFM
//! metrics that ReportLab uses, which makes `width()` identical to ReportLab's
//! `stringWidth` (checked for every printable ASCII glyph in all four styles).
//! Like ReportLab, no kerning is applied.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FontStyle {
    pub fn from_flags(bold: bool, italic: bool) -> Self {
        match (bold, italic) {
            (false, false) => FontStyle::Regular,
            (true, false) => FontStyle::Bold,
            (false, true) => FontStyle::Italic,
            (true, true) => FontStyle::BoldItalic,
        }
    }

    pub const ALL: [FontStyle; 4] =
        [FontStyle::Regular, FontStyle::Bold, FontStyle::Italic, FontStyle::BoldItalic];

    /// TTF bytes for this style.
    pub fn data(self) -> &'static [u8] {
        match self {
            FontStyle::Regular => {
                include_bytes!("../../../assets/fonts/LiberationSans-Regular.ttf")
            }
            FontStyle::Bold => include_bytes!("../../../assets/fonts/LiberationSans-Bold.ttf"),
            FontStyle::Italic => include_bytes!("../../../assets/fonts/LiberationSans-Italic.ttf"),
            FontStyle::BoldItalic => {
                include_bytes!("../../../assets/fonts/LiberationSans-BoldItalic.ttf")
            }
        }
    }

    /// CSS font description (family + weight + style) for UI previews.
    pub fn css(self) -> (&'static str, &'static str) {
        match self {
            FontStyle::Regular => ("400", "normal"),
            FontStyle::Bold => ("700", "normal"),
            FontStyle::Italic => ("400", "italic"),
            FontStyle::BoldItalic => ("700", "italic"),
        }
    }
}

struct Metrics {
    /// Advance per char in 1/1000 em.
    widths: HashMap<char, u16>,
    fallback: u16,
}

fn metrics(style: FontStyle) -> &'static Metrics {
    static CACHE: OnceLock<[Metrics; 4]> = OnceLock::new();
    let all = CACHE.get_or_init(|| {
        FontStyle::ALL.map(|s| {
            let face = ttf_parser::Face::parse(s.data(), 0).expect("bundled font parses");
            let upem = f64::from(face.units_per_em());
            let mut widths = HashMap::new();
            if let Some(cmap) = face.tables().cmap {
                for sub in cmap.subtables {
                    if !sub.is_unicode() {
                        continue;
                    }
                    sub.codepoints(|cp| {
                        if let (Some(ch), Some(gid)) = (char::from_u32(cp), sub.glyph_index(cp))
                            && let Some(adv) = face.glyph_hor_advance(gid)
                        {
                            widths
                                .entry(ch)
                                .or_insert((f64::from(adv) * 1000.0 / upem).round() as u16);
                        }
                    });
                }
            }
            let fallback = face
                .glyph_hor_advance(ttf_parser::GlyphId(0))
                .map_or(500, |a| (f64::from(a) * 1000.0 / upem).round() as u16);
            Metrics { widths, fallback }
        })
    });
    &all[style as usize]
}

/// Width of `text` in points at `size` (ReportLab `stringWidth` semantics).
pub fn width(text: &str, style: FontStyle, size: f64) -> f64 {
    let m = metrics(style);
    let units: u32 =
        text.chars().map(|c| u32::from(*m.widths.get(&c).unwrap_or(&m.fallback))).sum();
    f64::from(units) * size / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_helvetica_afm_widths() {
        // Adobe Helvetica AFM values (1/1000 em).
        let regular = [
            (' ', 278.0),
            ('A', 667.0),
            ('a', 556.0),
            ('m', 833.0),
            ('i', 222.0),
            ('W', 944.0),
            ('1', 556.0),
        ];
        for (c, w) in regular {
            assert_eq!(width(&c.to_string(), FontStyle::Regular, 1000.0), w, "{c:?}");
        }
        let bold = [('A', 722.0), ('i', 278.0), ('m', 889.0), (':', 333.0)];
        for (c, w) in bold {
            assert_eq!(width(&c.to_string(), FontStyle::Bold, 1000.0), w, "bold {c:?}");
        }
        assert_eq!(
            width("Casting Time: ", FontStyle::Bold, 8.5),
            width("Casting Time: ", FontStyle::BoldItalic, 8.5)
        );
    }

    #[test]
    fn widths_scale_linearly_and_sum() {
        let a = width("ab", FontStyle::Regular, 10.0);
        assert!(
            (a - (width("a", FontStyle::Regular, 10.0) + width("b", FontStyle::Regular, 10.0)))
                .abs()
                < 1e-12
        );
        assert!((width("ab", FontStyle::Regular, 20.0) - 2.0 * a).abs() < 1e-12);
        assert_eq!(width("", FontStyle::Italic, 12.0), 0.0);
    }
}
