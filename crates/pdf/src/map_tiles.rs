//! True-scale tiled map PDF. Python reference:
//! `map_pdf_generator.MapPDFGenerator._render_tiles`.
//!
//! Each tile's float crop box is resampled with Lanczos (no integer crop), so
//! adjacent pages share exact edges and the scale is uniform across seams.
//!
//! Cut lines run along all four edges of each page's image and on through the
//! white margin to the paper edge, so the margin a non-borderless printer
//! leaves can be trimmed with a ruler or a paper trimmer.

use image::RgbImage;
use krilla::Document;
use krilla::geom::PathBuilder;
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
                for path in trim_lines(x, y, w, h, pw as f32, ph as f32) {
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

/// The four cut lines of an image at `(x, y, w, h)` on a `pw × ph` page:
/// left and right edges top to bottom of the sheet, top and bottom edges
/// across its whole width.
fn trim_lines(x: f32, y: f32, w: f32, h: f32, pw: f32, ph: f32) -> Vec<krilla::geom::Path> {
    let line = |x0: f32, y0: f32, x1: f32, y1: f32| {
        let mut pb = PathBuilder::new();
        pb.move_to(x0, y0);
        pb.line_to(x1, y1);
        pb.finish()
    };
    [
        line(x, 0.0, x, ph),
        line(x + w, 0.0, x + w, ph),
        line(0.0, y, pw, y),
        line(0.0, y + h, pw, y + h),
    ]
    .into_iter()
    .flatten()
    .collect()
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
    fn cut_lines_run_along_every_image_edge_to_the_paper_edge() {
        let img = RgbImage::from_pixel(100, 100, image::Rgb([90, 90, 90]));
        let tile = Tile {
            col: 0,
            row: 0,
            crop: (0.0, 0.0, 100.0, 100.0),
            page_rect: (0.0, 0.0, 100.0, 100.0),
            page_x_pt: 0.0,
            page_y_pt: 0.0,
            w_pt: 300.0,
            h_pt: 200.0,
        };
        let cut = CutLine::from_settings(&sleuth_core::settings::Settings::default());
        let pdf = render(&img, &[tile], PageFormat::A4, 18.0, Some(&cut), &mut |_| true)
            .unwrap()
            .unwrap();
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        let p = *doc.get_pages().values().next().unwrap();
        let ops = lopdf::content::Content::decode(&doc.get_page_content(p)).unwrap();
        let strokes = ops.operations.iter().filter(|o| o.operator == "S").count();
        assert_eq!(strokes, 4, "four separate cut lines");
        // every line reaches the paper edge: a 0 coordinate appears in the
        // move/line operands of each one
        let ends: Vec<Vec<f32>> = ops
            .operations
            .iter()
            .filter(|o| o.operator == "m" || o.operator == "l")
            .map(|o| o.operands.iter().map(|v| v.as_float().unwrap()).collect())
            .collect();
        assert_eq!(ends.len(), 8);
        let (pw, ph) = PageFormat::A4.size_pt();
        let touches_edge = |v: &Vec<f32>| {
            v.iter().any(|c| c.abs() < 0.01)
                || (v[0] - pw as f32).abs() < 0.01
                || (v[1] - ph as f32).abs() < 0.01
        };
        assert!(ends.iter().all(touches_edge), "{ends:?}");
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
