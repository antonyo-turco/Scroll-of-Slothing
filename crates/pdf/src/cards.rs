//! Card deck PDF: renders `sleuth_core::cards::DeckLayout` ops.
//! Text is drawn glyph-by-glyph with Helvetica-exact advances (no kerning),
//! so the PDF matches the layout measurements and the UI preview.

use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke, StrokeDash};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph};
use sleuth_core::cards::{DeckLayout, Op, Rgb};
use sleuth_core::fonts::{FontStyle, glyphs};

use crate::{PdfError, Progress, Result};

fn color(c: Rgb) -> rgb::Color {
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    rgb::Color::new(b(c[0]), b(c[1]), b(c[2]))
}

fn fill(c: Rgb) -> Fill {
    Fill { paint: color(c).into(), opacity: NormalizedF32::ONE, rule: Default::default() }
}

fn stroke(c: Rgb, width: f64, dash: Option<[f64; 2]>) -> Stroke {
    Stroke {
        paint: color(c).into(),
        width: width as f32,
        opacity: NormalizedF32::ONE,
        dash: dash.map(|d| StrokeDash { array: vec![d[0] as f32, d[1] as f32], offset: 0.0 }),
        ..Default::default()
    }
}

struct Fonts([Font; 4]);

impl Fonts {
    fn new() -> Result<Self> {
        let load = |s: FontStyle| {
            Font::new(s.data().to_vec().into(), 0)
                .ok_or_else(|| PdfError::Write("bundled font failed to load".into()))
        };
        Ok(Fonts([
            load(FontStyle::Regular)?,
            load(FontStyle::Bold)?,
            load(FontStyle::Italic)?,
            load(FontStyle::BoldItalic)?,
        ]))
    }
    fn get(&self, s: FontStyle) -> Font {
        self.0[s as usize].clone()
    }
}

fn draw_op(surface: &mut Surface<'_>, fonts: &Fonts, ph: f64, op: &Op) {
    // Ops use a bottom-left origin; krilla draws top-left.
    let flip = |y: f64| (ph - y) as f32;
    match op {
        Op::Rect { x, y, w, h, width, rgb } => {
            if let Some(r) = Rect::from_xywh(*x as f32, flip(y + h), *w as f32, *h as f32) {
                let mut pb = PathBuilder::new();
                pb.push_rect(r);
                if let Some(p) = pb.finish() {
                    surface.set_fill(None);
                    surface.set_stroke(Some(stroke(*rgb, *width, None)));
                    surface.draw_path(&p);
                    surface.set_stroke(None);
                }
            }
        }
        Op::Fill { x, y, w, h, rgb } => {
            if let Some(r) = Rect::from_xywh(*x as f32, flip(y + h), *w as f32, *h as f32) {
                let mut pb = PathBuilder::new();
                pb.push_rect(r);
                if let Some(p) = pb.finish() {
                    surface.set_stroke(None);
                    surface.set_fill(Some(fill(*rgb)));
                    surface.draw_path(&p);
                }
            }
        }
        Op::Line { x1, y1, x2, y2, width, rgb, dash } => {
            let mut pb = PathBuilder::new();
            pb.move_to(*x1 as f32, flip(*y1));
            pb.line_to(*x2 as f32, flip(*y2));
            if let Some(p) = pb.finish() {
                surface.set_fill(None);
                surface.set_stroke(Some(stroke(*rgb, *width, *dash)));
                surface.draw_path(&p);
                surface.set_stroke(None);
            }
        }
        Op::Text { x, y, text, style, size, rgb } => {
            if text.is_empty() {
                return;
            }
            let gl: Vec<KrillaGlyph> = glyphs(text, *style)
                .into_iter()
                .map(|g| {
                    KrillaGlyph::new(
                        GlyphId::new(u32::from(g.glyph_id)),
                        g.advance_em,
                        0.0,
                        0.0,
                        0.0,
                        g.range,
                        None,
                    )
                })
                .collect();
            surface.set_stroke(None);
            surface.set_fill(Some(fill(*rgb)));
            surface.draw_glyphs(
                Point::from_xy(*x as f32, flip(*y)),
                &gl,
                fonts.get(*style),
                text,
                *size as f32,
                false,
            );
        }
    }
}

/// Renders every page of the deck layout.
pub fn render(layout: &DeckLayout, progress: Progress<'_>) -> Result<Vec<u8>> {
    let fonts = Fonts::new()?;
    let mut doc = Document::new();
    let n = layout.pages.len().max(1);
    for (i, page) in layout.pages.iter().enumerate() {
        let settings = PageSettings::from_wh(layout.page_w as f32, layout.page_h as f32)
            .ok_or_else(|| PdfError::Write("invalid page size".into()))?;
        let mut p = doc.start_page_with(settings);
        let mut surface = p.surface();
        for op in &page.ops {
            draw_op(&mut surface, &fonts, layout.page_h, op);
        }
        surface.finish();
        p.finish();
        if !progress((i + 1) as f32 / n as f32) {
            return Err(PdfError::Cancelled);
        }
    }
    doc.finish().map_err(|e| PdfError::Write(format!("{e:?}")))
}

#[cfg(test)]
mod tests {
    use sleuth_core::cards::{Flip, layout, parse};
    use sleuth_core::units::PageFormat;

    use super::*;

    fn doc(bytes: &[u8]) -> lopdf::Document {
        lopdf::Document::load_mem(bytes).expect("valid pdf")
    }

    #[test]
    fn deck_renders_with_backs_and_embedded_font() {
        let md = format!(
            "# Long\n**Type:** Spell\n\n{}\n---\n# Lucky\n**Type:** Feat\n**Copies:** 9\n\n*luck*",
            "word ".repeat(400)
        );
        let l = layout(&parse(&md), PageFormat::Letter, true, Flip::Long);
        assert_eq!(l.pages.len(), 4, "2 fronts + 2 backs (10 cards)");
        let bytes = render(&l, &mut |_| true).unwrap();
        let d = doc(&bytes);
        assert_eq!(d.get_pages().len(), 4);
        let fonts = d
            .objects
            .values()
            .filter(|o| {
                o.as_dict()
                    .ok()
                    .and_then(|dict| dict.get(b"Type").ok())
                    .and_then(|t| t.as_name().ok())
                    == Some(b"FontDescriptor")
            })
            .count();
        assert!(fonts >= 2, "regular/bold/italic subsets embedded, got {fonts}");
    }

    #[test]
    fn cancellation_is_honoured() {
        let l = layout(&parse("# A\n**Copies:** 40\n\nx"), PageFormat::A4, false, Flip::Long);
        assert!(matches!(render(&l, &mut |p| p < 0.3), Err(PdfError::Cancelled)));
    }
}
