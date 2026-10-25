//! Token sheet PDF. Python reference: `pdf_generator.PDFGenerator.export_pdf`.
//! Images are embedded as PNG with alpha (Python flattened to a temp JPEG).

use krilla::Document;
use krilla::geom::{Path, PathBuilder};
use krilla::page::PageSettings;
use sleuth_core::pack::Layout;
use sleuth_core::token::TokenShape;

use crate::{CutLine, PdfError, Progress, Result, circle, draw_image, png_image, stroke_path, top};

/// Renders a packed token layout. `images[i]` is the PNG for layout item `i`.
/// With `cut`, each token's outline is stroked: `shapes[i]` (circle when
/// missing; `None` for items without a cut line, like cut-outs). A circle has radius `min(w, h) / 2`; a polygon is the largest one
/// that fits the drawn image's box.
pub fn render(
    layout: &Layout,
    images: &[Vec<u8>],
    cut: Option<(&CutLine, &[Option<TokenShape>])>,
    progress: Progress<'_>,
) -> Result<Vec<u8>> {
    // One embedded image per item, reused for every copy.
    let embedded =
        images.iter().enumerate().map(|(i, png)| png_image(i, png)).collect::<Result<Vec<_>>>()?;
    // Aspect ratio per item: non-square images are fitted (centred), not stretched.
    let aspects: Vec<f64> = images
        .iter()
        .map(|png| {
            sleuth_core::imaging::dimensions(png)
                .map_or(1.0, |(w, h)| f64::from(w) / f64::from(h.max(1)))
        })
        .collect();
    let mut doc = Document::new();
    let pages = layout.pages.len().max(1);
    for (pi, page) in layout.pages.iter().enumerate() {
        let settings = PageSettings::from_wh(layout.page_w as f32, layout.page_h as f32)
            .ok_or_else(|| PdfError::Write("invalid page size".into()))?;
        let mut p = doc.start_page_with(settings);
        let mut surface = p.surface();
        for pl in page {
            let img = embedded
                .get(pl.item)
                .ok_or(PdfError::Image { item: pl.item, msg: "missing".into() })?;
            let y = top(layout.page_h, pl.y, pl.h);
            let (fw, fh) = fit(pl.w, pl.h, aspects[pl.item]);
            draw_image(
                &mut surface,
                img,
                (pl.x + (pl.w - fw) / 2.0) as f32,
                y + ((pl.h - fh) / 2.0) as f32,
                fw as f32,
                fh as f32,
            );
            if let Some((cut, shapes)) = cut {
                let shape = shapes.get(pl.item).copied().unwrap_or(Some(TokenShape::default()));
                let Some(shape) = shape else { continue };
                let (cx, cy) = (pl.x as f32 + pl.w as f32 / 2.0, y + pl.h as f32 / 2.0);
                if let Some(path) = outline(shape, cx, cy, pl.w.min(pl.h), fw, fh) {
                    stroke_path(&mut surface, &path, cut.stroke());
                }
            }
        }
        surface.finish();
        p.finish();
        if !progress((pi + 1) as f32 / pages as f32) {
            return Err(PdfError::Cancelled);
        }
    }
    doc.finish().map_err(|e| PdfError::Write(format!("{e:?}")))
}

/// Cut path around a token centred on `(cx, cy)` in a `cell`-sized slot whose
/// image is drawn `fw`×`fh` (top-left origin).
fn outline(shape: TokenShape, cx: f32, cy: f32, cell: f64, fw: f64, fh: f64) -> Option<Path> {
    if shape == TokenShape::Circle {
        return circle(cx, cy, (cell / 2.0) as f32);
    }
    let pts = shape.vertices(f64::from(cx), f64::from(cy), shape.fit_radius(fw, fh));
    polygon(&pts.into_iter().map(|(x, y)| (x as f32, y as f32)).collect::<Vec<_>>())
}

fn polygon(pts: &[(f32, f32)]) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let (&(x0, y0), rest) = pts.split_first()?;
    pb.move_to(x0, y0);
    for &(x, y) in rest {
        pb.line_to(x, y);
    }
    pb.close();
    pb.finish()
}

/// Largest `aspect`-shaped box inside `w`×`h`.
fn fit(w: f64, h: f64, aspect: f64) -> (f64, f64) {
    if aspect >= w / h { (w, w / aspect) } else { (h * aspect, h) }
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};
    use sleuth_core::pack::token_grid::{GridMode, TokenItem, pack};
    use sleuth_core::settings::Settings;
    use sleuth_core::units::{PageFormat, SizeLabel};

    use super::*;

    fn png(c: [u8; 4]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        RgbaImage::from_pixel(32, 32, Rgba(c)).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn doc(bytes: &[u8]) -> lopdf::Document {
        lopdf::Document::load_mem(bytes).expect("valid pdf")
    }

    fn media_box(d: &lopdf::Document, page: lopdf::ObjectId) -> Vec<f64> {
        let dict = d.get_dictionary(page).unwrap();
        dict.get(b"MediaBox")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o.as_float().unwrap() as f64)
            .collect()
    }

    #[test]
    fn pages_sizes_and_images_match_the_layout() {
        let items = [
            TokenItem { size_pt: SizeLabel::Large.points(), copies: 30 },
            TokenItem { size_pt: SizeLabel::Medium.points(), copies: 5 },
        ];
        let layout = pack(&items, PageFormat::A4, GridMode::HalfInch);
        let mut calls = 0;
        let bytes =
            render(&layout, &[png([255, 0, 0, 255]), png([0, 0, 255, 128])], None, &mut |_| {
                calls += 1;
                true
            })
            .unwrap();
        let d = doc(&bytes);
        let pages = d.get_pages();
        assert_eq!(pages.len(), layout.pages.len());
        assert_eq!(calls, layout.pages.len());
        let mb = media_box(&d, *pages.values().next().unwrap());
        assert!((mb[2] - 595.2756).abs() < 0.01 && (mb[3] - 841.8898).abs() < 0.01, "{mb:?}");
        // Two distinct images embedded once each (copies reuse the XObject).
        let xobjects = d.objects.values().filter(|o| {
            o.as_stream()
                .ok()
                .and_then(|s| s.dict.get(b"Subtype").ok())
                .and_then(|v| v.as_name().ok())
                == Some(b"Image")
        });
        let n = xobjects.count();
        assert!((2..=4).contains(&n), "2 images (+ up to 2 soft masks), got {n}");
    }

    #[test]
    fn cut_border_adds_stroked_paths() {
        let layout =
            pack(&[TokenItem { size_pt: 72.0, copies: 3 }], PageFormat::Letter, GridMode::HalfInch);
        let plain = render(&layout, &[png([0, 0, 0, 255])], None, &mut |_| true).unwrap();
        let cut = CutLine::from_settings(&Settings::default());
        let with_cut =
            render(&layout, &[png([0, 0, 0, 255])], Some((&cut, &[])), &mut |_| true).unwrap();
        let content = |b: &[u8]| {
            let d = doc(b);
            let p = *d.get_pages().values().next().unwrap();
            String::from_utf8_lossy(&d.get_page_content(p)).into_owned()
        };
        // Count PDF stroke operators ("S") in the content stream.
        let strokes = |c: &str| c.split_whitespace().filter(|op| *op == "S").count();
        let (a, b) = (content(&plain), content(&with_cut));
        assert_eq!(strokes(&a), 0);
        assert_eq!(strokes(&b), 3, "one stroke per token:\n{b}");
        assert!(b.contains("[2 3]"), "dashed per settings:\n{b}");
        assert!(b.matches(" c").count() >= 12, "circles are curves:\n{b}");
        // Square and hexagon tokens get straight-sided cut lines.
        for (shape, lines) in [
            (TokenShape::Square, 3),
            (TokenShape::Hex, 5),
            (TokenShape::Octagon, 7),
            (TokenShape::Decagon, 9),
        ] {
            let bytes =
                render(&layout, &[png([0, 0, 0, 255])], Some((&cut, &[Some(shape)])), &mut |_| {
                    true
                })
                .unwrap();
            let c = content(&bytes);
            assert_eq!(strokes(&c), 3);
            assert_eq!(c.matches(" l\n").count() + c.matches(" l ").count(), 3 * lines, "{c}");
            assert!(!c.contains(" c\n") && !c.contains(" c "), "no curves:\n{c}");
        }
    }

    #[test]
    fn non_square_images_are_fitted_not_stretched() {
        assert_eq!(fit(72.0, 72.0, 1.0), (72.0, 72.0));
        assert_eq!(fit(72.0, 72.0, 2.0), (72.0, 36.0), "wide image: full width, half height");
        assert_eq!(fit(72.0, 72.0, 0.5), (36.0, 72.0), "tall image: half width, full height");
    }

    #[test]
    fn cancellation_stops_rendering() {
        let layout =
            pack(&[TokenItem { size_pt: 72.0, copies: 200 }], PageFormat::Letter, GridMode::Legacy);
        let r = render(&layout, &[png([0, 0, 0, 255])], None, &mut |p| p < 0.5);
        assert!(matches!(r, Err(PdfError::Cancelled)));
    }

    #[test]
    fn bad_image_is_reported_with_its_item() {
        let layout =
            pack(&[TokenItem { size_pt: 72.0, copies: 1 }], PageFormat::Letter, GridMode::Legacy);
        let r = render(&layout, &[b"nope".to_vec()], None, &mut |_| true);
        assert!(matches!(r, Err(PdfError::Image { item: 0, .. })));
    }
}
