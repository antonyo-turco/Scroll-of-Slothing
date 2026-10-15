//! PDF writers for the printable sheets (krilla). Layout coordinates from
//! `sleuth_core::pack` use a bottom-left origin (ReportLab convention); krilla
//! draws top-left, so every writer converts with `top()`.

pub mod cards;
pub mod image_pages;
pub mod map_tiles;
pub mod token_sheet;

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use krilla::color::rgb;
use krilla::geom::{Path, PathBuilder, Size, Transform};
use krilla::image::{BitsPerComponent, CustomImage, Image, ImageColorspace};
use krilla::num::NormalizedF32;
use krilla::paint::{Stroke, StrokeDash};
use krilla::surface::Surface;
use sleuth_core::settings::{CutLineStyle, HexColor, Settings};

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("invalid image for item {item}: {msg}")]
    Image { item: usize, msg: String },
    #[error("pdf generation failed: {0}")]
    Write(String),
    #[error("cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, PdfError>;

/// Progress callback: receives 0..=1, returns `false` to cancel.
pub type Progress<'a> = &'a mut dyn FnMut(f32) -> bool;

/// Cut-guide stroke style (from the global settings).
#[derive(Debug, Clone, PartialEq)]
pub struct CutLine {
    pub style: CutLineStyle,
    pub color: HexColor,
    pub width: f64,
}

impl CutLine {
    pub fn from_settings(s: &Settings) -> Self {
        CutLine {
            style: s.cut_line_style,
            color: s.cut_line_color,
            width: s.effective_cut_line_width(),
        }
    }

    pub(crate) fn stroke(&self) -> Stroke {
        let [r, g, b] = self.color.0;
        let dash = self.style.pdf_dash();
        Stroke {
            paint: rgb::Color::new(r, g, b).into(),
            width: self.width as f32,
            opacity: NormalizedF32::ONE,
            dash: (!dash.is_empty()).then(|| StrokeDash {
                array: dash.iter().map(|&d| d as f32).collect(),
                offset: 0.0,
            }),
            ..Default::default()
        }
    }
}

/// Converts a bottom-left-origin rect top edge to krilla's top-left origin.
pub(crate) fn top(page_h: f64, y: f64, h: f64) -> f32 {
    (page_h - y - h) as f32
}

/// A circle path (4 cubic Béziers).
pub(crate) fn circle(cx: f32, cy: f32, r: f32) -> Option<Path> {
    const K: f32 = 0.552_284_8;
    let k = K * r;
    let mut pb = PathBuilder::new();
    pb.move_to(cx + r, cy);
    pb.cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
    pb.cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
    pb.cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
    pb.cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
    pb.close();
    pb.finish()
}

/// Draws `image` into the rect (top-left origin, points).
pub(crate) fn draw_image(surface: &mut Surface<'_>, image: &Image, x: f32, y: f32, w: f32, h: f32) {
    let Some(size) = Size::from_wh(w, h) else { return };
    surface.push_transform(&Transform::from_translate(x, y));
    surface.draw_image(image.clone(), size);
    surface.pop();
}

pub(crate) fn png_image(item: usize, png: &[u8]) -> Result<Image> {
    Image::from_png(png.to_vec().into(), true).map_err(|msg| PdfError::Image { item, msg })
}

pub(crate) fn stroke_path(surface: &mut Surface<'_>, path: &Path, stroke: Stroke) {
    surface.set_fill(None);
    surface.set_stroke(Some(stroke));
    surface.draw_path(path);
    surface.set_stroke(None);
}

/// Raw 8-bit RGB pixels handed to krilla without a PNG/temp-file round-trip.
/// Hashing uses `id` (unique per image within one document) so krilla does not
/// hash megabytes of pixels.
#[derive(Clone)]
struct RawRgb {
    data: Arc<Vec<u8>>,
    w: u32,
    h: u32,
    id: u64,
}

impl Hash for RawRgb {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.id, self.w, self.h, self.data.len()).hash(state);
    }
}

impl CustomImage for RawRgb {
    fn color_channel(&self) -> &[u8] {
        &self.data
    }
    fn alpha_channel(&self) -> Option<&[u8]> {
        None
    }
    fn bits_per_component(&self) -> BitsPerComponent {
        BitsPerComponent::Eight
    }
    fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }
    fn icc_profile(&self) -> Option<&[u8]> {
        None
    }
    fn color_space(&self) -> ImageColorspace {
        ImageColorspace::Rgb
    }
}

/// Wraps an RGB image for embedding; `id` must be unique within the document.
pub(crate) fn raw_rgb_image(img: image::RgbImage, id: u64) -> Image {
    let (w, h) = img.dimensions();
    Image::from_custom(RawRgb { data: Arc::new(img.into_raw()), w, h, id }, true)
        .expect("valid raw image")
}
