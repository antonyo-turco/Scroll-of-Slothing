//! Reads an image from the system clipboard in every form Python's
//! `ImageGrab.grabclipboard()` accepted (`ui/map_paginator_panel.py:867`):
//! image data, or a copied image file (Explorer "Copy"), plus a file path
//! copied as text. Returns encoded image bytes ready for `store_source`.

use std::path::Path;

use sleuth_core::imaging;

use crate::{Result, ServiceError};

/// Encoded bytes of the clipboard image, trying the best quality first:
/// PNG (lossless, keeps alpha), the bitmap, copied files, a copied path.
pub(crate) fn read_image() -> Result<Vec<u8>> {
    #[cfg(windows)]
    if let Some(png) = win::png() {
        return Ok(png);
    }

    let mut cb = arboard::Clipboard::new().map_err(|e| ServiceError::Io(e.to_string()))?;
    let arboard_err = match cb.get_image() {
        Ok(img) => {
            let rgba = image::RgbaImage::from_raw(
                img.width as u32,
                img.height as u32,
                img.bytes.into_owned(),
            )
            .ok_or_else(|| {
                ServiceError::InvalidInput("clipboard image has an odd layout".into())
            })?;
            return imaging::encode_png(&rgba).map_err(|e| ServiceError::Internal(e.to_string()));
        }
        Err(e) => e.to_string(),
    };

    #[cfg(windows)]
    if let Some(bytes) = win::bitmap().or_else(win::first_image_file) {
        return Ok(bytes);
    }

    // Python also accepted a copied file path.
    if let Ok(text) = cb.get_text()
        && let Some(bytes) = image_file(Path::new(text.trim().trim_matches('"')))
    {
        return Ok(bytes);
    }

    #[cfg(windows)]
    let found = win::format_names().join(", ");
    #[cfg(not(windows))]
    let found = String::from("unknown");
    tracing::info!(formats = %found, arboard = %arboard_err, "no usable clipboard image");
    Err(ServiceError::InvalidInput(format!("the clipboard has no image (it holds: {found})")))
}

/// The file's bytes if it is an image we can decode.
fn image_file(path: &Path) -> Option<Vec<u8>> {
    if !path.is_file() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    imaging::sniff_mime(&bytes)?;
    Some(bytes)
}

#[cfg(windows)]
mod win {
    use std::path::PathBuf;

    use clipboard_win::{Clipboard, formats, get_clipboard, raw};
    use sleuth_core::imaging;

    /// The registered "PNG" format (browsers, Snipping Tool, Office…).
    pub fn png() -> Option<Vec<u8>> {
        let fmt = raw::register_format("PNG")?.get();
        let bytes: Vec<u8> = get_clipboard(formats::RawData(fmt)).ok()?;
        imaging::dimensions(&bytes).map(|_| bytes)
    }

    /// `CF_BITMAP` as a BMP file, for producers arboard cannot parse.
    pub fn bitmap() -> Option<Vec<u8>> {
        let bytes: Vec<u8> = get_clipboard(formats::Bitmap).ok()?;
        imaging::dimensions(&bytes).map(|_| bytes)
    }

    /// The first copied file (`CF_HDROP`) that is a decodable image.
    pub fn first_image_file() -> Option<Vec<u8>> {
        let files: Vec<PathBuf> = get_clipboard(formats::FileList).ok()?;
        files.iter().find_map(|p| super::image_file(p))
    }

    /// Names of the formats on the clipboard, for the error message.
    pub fn format_names() -> Vec<String> {
        let Ok(_clip) = Clipboard::new_attempts(10) else {
            return vec!["clipboard busy".into()];
        };
        let names: Vec<String> = raw::EnumFormats::new()
            .map(|f| raw::format_name_big(f).unwrap_or_else(|| standard_name(f)))
            .collect();
        if names.is_empty() { vec!["nothing".into()] } else { names }
    }

    fn standard_name(f: u32) -> String {
        match f {
            1 => "text".into(),
            2 => "CF_BITMAP".into(),
            8 => "CF_DIB".into(),
            13 => "unicode text".into(),
            15 => "files".into(),
            17 => "CF_DIBV5".into(),
            other => format!("format {other}"),
        }
    }
}
