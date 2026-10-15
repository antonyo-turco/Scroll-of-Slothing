//! Page formats, physical units and the shared token/asset size labels.

use serde::{Deserialize, Serialize};

/// PostScript points per inch.
pub const PT_PER_INCH: f64 = 72.0;
/// Millimetres per inch.
pub const MM_PER_INCH: f64 = 25.4;

/// Supported paper formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PageFormat {
    #[default]
    Letter,
    A4,
}

impl PageFormat {
    pub const ALL: [PageFormat; 2] = [PageFormat::Letter, PageFormat::A4];

    /// Page size in points `(width, height)`, portrait. Values match ReportLab's
    /// `LETTER` and `A4` exactly so layouts are bit-compatible with the Python app.
    pub fn size_pt(self) -> (f64, f64) {
        match self {
            PageFormat::Letter => (612.0, 792.0),
            PageFormat::A4 => (21.0 / 2.54 * PT_PER_INCH, 29.7 / 2.54 * PT_PER_INCH),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PageFormat::Letter => "Letter",
            PageFormat::A4 => "A4",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            s if s.eq_ignore_ascii_case("letter") => Some(PageFormat::Letter),
            s if s.eq_ignore_ascii_case("a4") => Some(PageFormat::A4),
            _ => None,
        }
    }
}

/// Print size of a token / cut-out. Point values follow `utils.SIZE_OPTIONS`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, PartialOrd, Ord,
)]
pub enum SizeLabel {
    Tiny,
    Small,
    #[default]
    Medium,
    Large,
    Huge,
    Gargantuan,
}

impl SizeLabel {
    pub const ALL: [SizeLabel; 6] = [
        SizeLabel::Tiny,
        SizeLabel::Small,
        SizeLabel::Medium,
        SizeLabel::Large,
        SizeLabel::Huge,
        SizeLabel::Gargantuan,
    ];

    /// Printed size in points.
    pub fn points(self) -> f64 {
        match self {
            SizeLabel::Tiny => 36.0,
            SizeLabel::Small => 57.6,
            SizeLabel::Medium => 72.0,
            SizeLabel::Large => 144.0,
            SizeLabel::Huge => 216.0,
            SizeLabel::Gargantuan => 288.0,
        }
    }

    pub fn inches(self) -> f64 {
        self.points() / PT_PER_INCH
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SizeLabel::Tiny => "Tiny",
            SizeLabel::Small => "Small",
            SizeLabel::Medium => "Medium",
            SizeLabel::Large => "Large",
            SizeLabel::Huge => "Huge",
            SizeLabel::Gargantuan => "Gargantuan",
        }
    }

    /// Parses both the new labels (`"Large"`) and the Python labels
    /// (`"Large (2 inch)"`), case-insensitively.
    pub fn parse(s: &str) -> Option<Self> {
        let head = s.split('(').next().unwrap_or("").trim();
        Self::ALL.into_iter().find(|l| l.as_str().eq_ignore_ascii_case(head))
    }
}

/// Converts millimetres to points.
pub fn mm_to_pt(mm: f64) -> f64 {
    mm / MM_PER_INCH * PT_PER_INCH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_sizes_match_reportlab() {
        assert_eq!(PageFormat::Letter.size_pt(), (612.0, 792.0));
        let (w, h) = PageFormat::A4.size_pt();
        assert!((w - 595.275_590_551_181_2).abs() < 1e-9);
        assert!((h - 841.889_763_779_527_7).abs() < 1e-9);
    }

    #[test]
    fn size_labels_parse_python_and_new_forms() {
        assert_eq!(SizeLabel::parse("Medium (1 inch)"), Some(SizeLabel::Medium));
        assert_eq!(SizeLabel::parse("Gargantuan (4 inch)"), Some(SizeLabel::Gargantuan));
        assert_eq!(SizeLabel::parse("small"), Some(SizeLabel::Small));
        assert_eq!(SizeLabel::parse("Colossal"), None);
        for l in SizeLabel::ALL {
            assert_eq!(SizeLabel::parse(l.as_str()), Some(l));
        }
    }

    #[test]
    fn size_points_match_python_size_options() {
        let pts: Vec<f64> = SizeLabel::ALL.iter().map(|l| l.points()).collect();
        assert_eq!(pts, vec![36.0, 57.6, 72.0, 144.0, 216.0, 288.0]);
    }

    #[test]
    fn page_format_parse_roundtrip() {
        for f in PageFormat::ALL {
            assert_eq!(PageFormat::parse(f.as_str()), Some(f));
        }
        assert_eq!(PageFormat::parse("legal"), None);
    }
}
