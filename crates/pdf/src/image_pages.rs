//! One full-page raster per PDF page (Python `aoe_templates.export_sheet_pdf`).

use image::RgbImage;
use krilla::Document;
use krilla::page::PageSettings;
use sleuth_core::units::PageFormat;

use crate::{PdfError, Progress, Result, draw_image, raw_rgb_image};

/// Each image is stretched over a whole `format` page.
pub fn render(pages: Vec<RgbImage>, format: PageFormat, progress: Progress<'_>) -> Result<Vec<u8>> {
    let (pw, ph) = format.size_pt();
    let n = pages.len().max(1);
    let mut doc = Document::new();
    for (i, img) in pages.into_iter().enumerate() {
        let settings = PageSettings::from_wh(pw as f32, ph as f32)
            .ok_or_else(|| PdfError::Write("invalid page size".into()))?;
        let mut page = doc.start_page_with(settings);
        let mut surface = page.surface();
        draw_image(&mut surface, &raw_rgb_image(img, i as u64), 0.0, 0.0, pw as f32, ph as f32);
        surface.finish();
        page.finish();
        if !progress((i + 1) as f32 / n as f32) {
            return Err(PdfError::Cancelled);
        }
    }
    doc.finish().map_err(|e| PdfError::Write(format!("{e:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_one_page_per_image() {
        let pages = vec![RgbImage::new(20, 30), RgbImage::new(20, 30)];
        let pdf = render(pages, PageFormat::A4, &mut |_| true).unwrap();
        assert_eq!(lopdf::Document::load_mem(&pdf).unwrap().get_pages().len(), 2);
    }
}
