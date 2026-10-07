//! Image decoding, fast downscaling and encoding helpers shared by all features.
//! Pure functions over byte buffers (no filesystem access).

use std::io::Cursor;

use fast_image_resize::{self as fr, images::Image as FrImage};
use image::{DynamicImage, ImageFormat, RgbaImage};

/// Max side of the small library thumbnail derivative.
pub const THUMB_MAX: u32 = 256;
/// Max side of the editor/hover preview derivative.
pub const PREVIEW_MAX: u32 = 2048;

#[derive(Debug, thiserror::Error)]
pub enum ImagingError {
    #[error("unsupported or corrupt image: {0}")]
    Decode(String),
    #[error("encode failed: {0}")]
    Encode(String),
}

pub type Result<T> = std::result::Result<T, ImagingError>;

/// MIME type sniffed from the bytes, if it is an image format we accept.
pub fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    match image::guess_format(bytes).ok()? {
        ImageFormat::Png => Some("image/png"),
        ImageFormat::Jpeg => Some("image/jpeg"),
        ImageFormat::WebP => Some("image/webp"),
        ImageFormat::Gif => Some("image/gif"),
        ImageFormat::Bmp => Some("image/bmp"),
        _ => None,
    }
}

pub fn decode(bytes: &[u8]) -> Result<DynamicImage> {
    image::load_from_memory(bytes).map_err(|e| ImagingError::Decode(e.to_string()))
}

/// Scale factor that fits `(w, h)` inside a `max`×`max` box without upscaling.
pub fn fit_scale(w: u32, h: u32, max: u32) -> f64 {
    let longest = w.max(h).max(1);
    if longest <= max { 1.0 } else { f64::from(max) / f64::from(longest) }
}

/// Downscales to fit within `max`×`max` (Lanczos3, SIMD). Never upscales.
pub fn downscale(img: &DynamicImage, max: u32) -> RgbaImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let s = fit_scale(w, h, max);
    if s >= 1.0 {
        return rgba;
    }
    let tw = ((f64::from(w) * s).round() as u32).max(1);
    let th = ((f64::from(h) * s).round() as u32).max(1);
    resize_rgba(&rgba, tw, th)
}

/// Resizes an RGBA image to exactly `tw`×`th` with Lanczos3 (alpha-aware).
pub fn resize_rgba(src: &RgbaImage, tw: u32, th: u32) -> RgbaImage {
    let (w, h) = src.dimensions();
    if (w, h) == (tw, th) {
        return src.clone();
    }
    let src_img = FrImage::from_vec_u8(w, h, src.as_raw().clone(), fr::PixelType::U8x4)
        .expect("buffer size matches dimensions");
    let mut dst = FrImage::new(tw, th, fr::PixelType::U8x4);
    let mut resizer = fr::Resizer::new();
    let opts = fr::ResizeOptions::new()
        .resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3))
        .use_alpha(true);
    resizer.resize(&src_img, &mut dst, &opts).expect("same pixel type");
    RgbaImage::from_raw(tw, th, dst.into_vec()).expect("buffer size matches dimensions")
}

/// Lossless WebP (used for derivatives: small, exact, fast to decode in WebView2).
pub fn encode_webp_lossless(img: &RgbaImage) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut out)
        .encode(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8)
        .map_err(|e| ImagingError::Encode(e.to_string()))?;
    Ok(out)
}

pub fn encode_png(img: &RgbaImage) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).map_err(|e| ImagingError::Encode(e.to_string()))?;
    Ok(out.into_inner())
}

/// The two cached derivatives generated for every stored image.
pub struct Derivatives {
    pub thumb: RgbaImage,
    pub preview: RgbaImage,
}

pub fn derivatives(img: &DynamicImage) -> Derivatives {
    let preview = downscale(img, PREVIEW_MAX);
    // Derive the thumb from the preview: same quality, much less work for huge maps.
    let thumb = downscale(&DynamicImage::ImageRgba8(preview.clone()), THUMB_MAX);
    Derivatives { thumb, preview }
}

#[cfg(test)]
mod tests {
    use image::Rgba;

    use super::*;

    fn checker(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_fn(w, h, |x, y| {
            if (x / 8 + y / 8) % 2 == 0 { Rgba([255, 0, 0, 255]) } else { Rgba([0, 0, 255, 128]) }
        }))
    }

    #[test]
    fn downscale_preserves_aspect_and_never_upscales() {
        let img = checker(4000, 1000);
        let d = downscale(&img, 2048);
        assert_eq!(d.dimensions(), (2048, 512));
        let small = checker(100, 50);
        assert_eq!(downscale(&small, 2048).dimensions(), (100, 50));
    }

    #[test]
    fn derivative_sizes() {
        let d = derivatives(&checker(3000, 6000));
        assert_eq!(d.preview.dimensions(), (1024, 2048));
        assert_eq!(d.thumb.dimensions(), (128, 256));
    }

    #[test]
    fn webp_lossless_roundtrip_is_exact() {
        let img = checker(64, 48).to_rgba8();
        let bytes = encode_webp_lossless(&img).unwrap();
        assert_eq!(sniff_mime(&bytes), Some("image/webp"));
        assert_eq!(decode(&bytes).unwrap().to_rgba8(), img);
    }

    #[test]
    fn png_roundtrip_and_sniffing() {
        let img = checker(10, 10).to_rgba8();
        let png = encode_png(&img).unwrap();
        assert_eq!(sniff_mime(&png), Some("image/png"));
        assert_eq!(decode(&png).unwrap().to_rgba8(), img);
        assert_eq!(sniff_mime(b"not an image"), None);
        assert!(decode(b"garbage").is_err());
    }
}
