//! Token sheet PDF. Python reference: `pdf_generator.PDFGenerator.export_pdf`.
//! Images are embedded as PNG with alpha (Python flattened to a temp JPEG).

use krilla::Document;
use krilla::page::PageSettings;
use sleuth_core::pack::Layout;

use crate::{CutLine, PdfError, Progress, Result, circle, draw_image, png_image, stroke_path, top};

/// Renders a packed token layout. `images[i]` is the PNG for layout item `i`.
/// With `cut`, a circle of radius `min(w, h) / 2` is stroked around each token.
pub fn render(
    layout: &Layout,
    images: &[Vec<u8>],
    cut: Option<&CutLine>,
    progress: Progress<'_>,
) -> Result<Vec<u8>> {
    // One embedded image per item, reused for every copy.
    let embedded =
        images.iter().enumerate().map(|(i, png)| png_image(i, png)).collect::<Result<Vec<_>>>()?;
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
            draw_image(&mut surface, img, pl.x as f32, y, pl.w as f32, pl.h as f32);
            if let Some(cut) = cut {
                let r = (pl.w.min(pl.h) / 2.0) as f32;
                if let Some(path) =
                    circle(pl.x as f32 + pl.w as f32 / 2.0, y + pl.h as f32 / 2.0, r)
                {
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
        let with_cut = render(&layout, &[png([0, 0, 0, 255])], Some(&cut), &mut |_| true).unwrap();
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
