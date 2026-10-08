//! True-scale tiled map PDF. Python reference:
//! `map_pdf_generator.MapPDFGenerator._render_tiles`.
//!
//! Each tile's float crop box is resampled with Lanczos (no integer crop), so
//! adjacent pages share exact edges and the scale is uniform across seams.

use image::RgbImage;
use krilla::Document;
use krilla::geom::{PathBuilder, Rect};
use krilla::page::PageSettings;
use sleuth_core::maps::tiling::Tile;
use sleuth_core::units::PageFormat;

use crate::{CutLine, PdfError, Progress, Result, draw_image, raw_rgb_image, stroke_path};

/// Renders `tiles` of `image` (full resolution), one page each, positioned
/// inside a `margin_pt` page margin. Tiles whose clamped crop is under 1 px
/// are skipped like in Python. Returns `None` when no page was drawn.
pub fn render(
    image: &RgbImage,
    tiles: &[Tile],
    format: PageFormat,
    margin_pt: f64,
    cut: Option<&CutLine>,
    progress: Progress<'_>,
) -> Result<Option<Vec<u8>>> {
    let (pw, ph) = format.size_pt();
    let (iw, ih) = (f64::from(image.width()), f64::from(image.height()));
    let mut doc = Document::new();
    let mut drawn = 0;
    let n = tiles.len().max(1);
    for (i, tile) in tiles.iter().enumerate() {
        let (l, t, r, b) = tile.crop;
        let (l, r) = (l.clamp(0.0, iw), r.clamp(0.0, iw));
        let (t, b) = (t.clamp(0.0, ih), b.clamp(0.0, ih));
        if r - l >= 1.0 && b - t >= 1.0 {
            let out_w = ((r - l).round_ties_even() as u32).max(1);
            let out_h = ((b - t).round_ties_even() as u32).max(1);
            let sub = sleuth_core::imaging::resize_box_rgb(image, (l, t, r, b), out_w, out_h);
            let settings = PageSettings::from_wh(pw as f32, ph as f32)
                .ok_or_else(|| PdfError::Write("invalid page size".into()))?;
            let mut page = doc.start_page_with(settings);
            let mut surface = page.surface();
            let x = (margin_pt + tile.page_x_pt) as f32;
            let y = (margin_pt + tile.page_y_pt) as f32;
            let (w, h) = (tile.w_pt as f32, tile.h_pt as f32);
            draw_image(&mut surface, &raw_rgb_image(sub, i as u64), x, y, w, h);
            if let Some(cut) = cut {
                let mut pb = PathBuilder::new();
                if let Some(rect) = Rect::from_xywh(x, y, w, h) {
                    pb.push_rect(rect);
                }
                if let Some(path) = pb.finish() {
                    stroke_path(&mut surface, &path, cut.stroke());
                }
            }
            surface.finish();
            page.finish();
            drawn += 1;
        }
        if !progress((i + 1) as f32 / n as f32) {
            return Err(PdfError::Cancelled);
        }
    }
    if drawn == 0 {
        return Ok(None);
    }
    doc.finish().map(Some).map_err(|e| PdfError::Write(format!("{e:?}")))
}

#[cfg(test)]
mod tests {
    use sleuth_core::maps::tiling::{TileParams, plan};

    use super::*;

    #[test]
    fn one_page_per_tile_with_page_size() {
        let img =
            RgbImage::from_fn(700, 500, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 90]));
        let p = TileParams {
            image_w: 700,
            image_h: 500,
            region: None,
            cell_w: 70.0,
            cell_h: 70.0,
            square_in: 1.0,
            format: PageFormat::Letter,
            margin_pt: 18.0,
            offset: (0.0, 0.0),
            overlap_in: 0.0,
            align_to_grid: false,
            grid_offset: (0.0, 0.0),
        };
        let tp = plan(&p);
        let pdf = render(&img, &tp.tiles, PageFormat::Letter, 18.0, None, &mut |_| true)
            .unwrap()
            .unwrap();
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        assert_eq!(doc.get_pages().len(), tp.tiles.len());
        assert_eq!(tp.tiles.len(), 2);
    }

    #[test]
    fn nothing_drawn_is_none_and_cancel_stops() {
        assert!(
            render(&RgbImage::new(10, 10), &[], PageFormat::A4, 0.0, None, &mut |_| true)
                .unwrap()
                .is_none()
        );
        let tile = Tile {
            col: 0,
            row: 0,
            crop: (0.0, 0.0, 10.0, 10.0),
            page_rect: (0.0, 0.0, 10.0, 10.0),
            page_x_pt: 0.0,
            page_y_pt: 0.0,
            w_pt: 10.0,
            h_pt: 10.0,
        };
        let r = render(&RgbImage::new(10, 10), &[tile], PageFormat::A4, 0.0, None, &mut |_| false);
        assert!(matches!(r, Err(PdfError::Cancelled)));
    }
}
