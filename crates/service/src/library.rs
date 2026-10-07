//! Asset library service: import, search, edit, bulk actions, export.
//! Spec: docs/features/library.md.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use image::DynamicImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sleuth_core::imaging;
use sleuth_core::units::SizeLabel;
use sleuth_store::{Asset, AssetImages, AssetPage, AssetPatch, AssetQuery, LinkTarget, NewAsset, Store};

use crate::{Result, Service, ServiceError};

/// Asset kinds known to this version. The DB column is open-ended for the future.
pub const ASSET_KINDS: [&str; 3] = ["token", "cutout", "map"];

pub(crate) fn check_kind(kind: &str) -> Result<()> {
    if ASSET_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(ServiceError::InvalidInput(format!("unknown kind {kind:?}; expected one of {ASSET_KINDS:?}")))
    }
}

pub(crate) fn check_size(size: &str) -> Result<String> {
    SizeLabel::parse(size)
        .map(|s| s.as_str().to_string())
        .ok_or_else(|| ServiceError::InvalidInput(format!("unknown size {size:?}")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportFile {
    pub path: PathBuf,
    /// Defaults to the file stem.
    pub name: Option<String>,
    pub kind: String,
    /// Defaults to Medium; ignored for maps.
    pub size: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportFailure {
    pub path: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImportReport {
    pub imported: Vec<i64>,
    pub failed: Vec<ImportFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportWhat {
    Original,
    Rendered,
    Pdf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BulkAction {
    Pin,
    Unpin,
}

/// Image bytes stored as an asset's original + rendered pair, with derivatives.
pub(crate) struct StoredImages {
    pub original_blob: String,
    pub rendered_blob: String,
    pub thumb_blob: String,
    pub preview_blob: String,
}

/// Stores `original` (encoded bytes, kept as-is) and `rendered` (decoded image)
/// plus thumb/preview derivatives of the rendered image.
pub(crate) fn store_images(
    store: &Store,
    original: &[u8],
    original_img: &DynamicImage,
    rendered: Option<&DynamicImage>,
) -> Result<StoredImages> {
    let mime = imaging::sniff_mime(original)
        .ok_or_else(|| ServiceError::InvalidInput("unsupported image format".into()))?;
    let orig = store.put_blob(original, mime, Some((original_img.width(), original_img.height())))?;
    let rendered_hash = match rendered {
        None => orig.hash.clone(),
        Some(r) => {
            let png = imaging::encode_png(&r.to_rgba8()).map_err(|e| ServiceError::Internal(e.to_string()))?;
            store.put_blob(&png, "image/png", Some((r.width(), r.height())))?.hash
        }
    };
    let deriv = imaging::derivatives(rendered.unwrap_or(original_img));
    let put_webp = |img: &image::RgbaImage| -> Result<String> {
        let bytes = imaging::encode_webp_lossless(img).map_err(|e| ServiceError::Internal(e.to_string()))?;
        Ok(store.put_blob(&bytes, "image/webp", Some(img.dimensions()))?.hash)
    };
    Ok(StoredImages {
        original_blob: orig.hash,
        rendered_blob: rendered_hash,
        thumb_blob: put_webp(&deriv.thumb)?,
        preview_blob: put_webp(&deriv.preview)?,
    })
}

/// Stores already-encoded `original` and `rendered` images verbatim (no
/// re-encoding) and generates derivatives from the rendered one.
pub(crate) fn store_encoded(store: &Store, original: &[u8], rendered: &[u8]) -> Result<StoredImages> {
    let put_raw = |bytes: &[u8]| -> Result<(String, Option<DynamicImage>)> {
        let mime = imaging::sniff_mime(bytes)
            .ok_or_else(|| ServiceError::InvalidInput("unsupported image format".into()))?;
        let dims = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .ok()
            .and_then(|r| r.into_dimensions().ok());
        Ok((store.put_blob(bytes, mime, dims)?.hash, None))
    };
    let (original_blob, _) = put_raw(original)?;
    let (rendered_blob, _) = if rendered == original { (original_blob.clone(), None) } else { put_raw(rendered)? };
    let img = imaging::decode(rendered).map_err(|e| ServiceError::InvalidInput(e.to_string()))?;
    let deriv = imaging::derivatives(&img);
    drop(img);
    let put_webp = |img: &image::RgbaImage| -> Result<String> {
        let bytes = imaging::encode_webp_lossless(img).map_err(|e| ServiceError::Internal(e.to_string()))?;
        Ok(store.put_blob(&bytes, "image/webp", Some(img.dimensions()))?.hash)
    };
    Ok(StoredImages {
        original_blob,
        rendered_blob,
        thumb_blob: put_webp(&deriv.thumb)?,
        preview_blob: put_webp(&deriv.preview)?,
    })
}

/// A filesystem-safe file stem.
pub(crate) fn safe_file_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') { c } else { '_' })
        .collect();
    let s = s.trim().trim_matches('.').to_string();
    if s.is_empty() { "asset".into() } else { s }
}

fn ext_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "application/pdf" => "pdf",
        _ => "bin",
    }
}

impl Service {
    pub async fn library_search(&self, query: AssetQuery) -> Result<AssetPage> {
        self.blocking(move |s| Ok(s.search_assets(&query)?)).await
    }

    pub async fn library_get(&self, id: i64) -> Result<Asset> {
        self.blocking(move |s| Ok(s.get_asset(id)?)).await
    }

    /// Imports image files unprocessed (original == rendered). Files are decoded
    /// in parallel; failures are reported per file and never abort the batch.
    pub async fn library_import(&self, files: Vec<ImportFile>) -> Result<ImportReport> {
        for f in &files {
            check_kind(&f.kind)?;
            if let Some(sz) = &f.size {
                check_size(sz)?;
            }
        }
        self.blocking(move |s| {
            let results: Vec<std::result::Result<NewAsset, ImportFailure>> = files
                .par_iter()
                .map(|f| {
                    let fail = |e: String| ImportFailure { path: f.path.clone(), error: e };
                    let bytes = std::fs::read(&f.path).map_err(|e| fail(e.to_string()))?;
                    let img = imaging::decode(&bytes).map_err(|e| fail(e.to_string()))?;
                    let stored = store_images(s, &bytes, &img, None).map_err(|e| fail(e.to_string()))?;
                    let name = f
                        .name
                        .clone()
                        .filter(|n| !n.trim().is_empty())
                        .or_else(|| f.path.file_stem().map(|s| s.to_string_lossy().into_owned()))
                        .unwrap_or_else(|| "Untitled".into());
                    let size = f.size.as_deref().and_then(SizeLabel::parse).unwrap_or_default();
                    Ok(NewAsset {
                        kind: f.kind.clone(),
                        name,
                        aliases: String::new(),
                        size_label: size.as_str().into(),
                        original_blob: stored.original_blob,
                        rendered_blob: stored.rendered_blob,
                        pdf_blob: None,
                        thumb_blob: Some(stored.thumb_blob),
                        preview_blob: Some(stored.preview_blob),
                        params_json: None,
                        legacy_id: None,
                        pinned: false,
                    })
                })
                .collect();
            let mut report = ImportReport::default();
            for r in results {
                match r {
                    Ok(a) => report.imported.push(s.insert_asset(&a)?),
                    Err(f) => report.failed.push(f),
                }
            }
            Ok(report)
        })
        .await
    }

    pub async fn library_update(&self, id: i64, mut patch: AssetPatch) -> Result<Asset> {
        if let Some(sz) = &patch.size_label {
            patch.size_label = Some(check_size(sz)?);
        }
        self.blocking(move |s| {
            s.update_asset_meta(id, &patch)?;
            Ok(s.get_asset(id)?)
        })
        .await
    }

    pub async fn library_set_tags(&self, id: i64, tags: Vec<String>) -> Result<Asset> {
        self.blocking(move |s| {
            s.set_asset_tags(id, &tags)?;
            Ok(s.get_asset(id)?)
        })
        .await
    }

    pub async fn library_set_campaigns(&self, id: i64, campaign_ids: Vec<String>) -> Result<Asset> {
        self.blocking(move |s| {
            s.set_asset_campaigns(id, &campaign_ids)?;
            Ok(s.get_asset(id)?)
        })
        .await
    }

    pub async fn library_set_sessions(&self, id: i64, session_ids: Vec<String>) -> Result<Asset> {
        self.blocking(move |s| {
            s.set_asset_sessions(id, &session_ids)?;
            Ok(s.get_asset(id)?)
        })
        .await
    }

    /// Deletes assets; returns how many existed.
    pub async fn library_delete(&self, ids: Vec<i64>) -> Result<usize> {
        self.blocking(move |s| Ok(s.delete_assets(&ids)?)).await
    }

    pub async fn library_bulk_tag(&self, ids: Vec<i64>, tag: String) -> Result<()> {
        self.blocking(move |s| Ok(s.add_tag_to_assets(&ids, &tag)?)).await
    }

    pub async fn library_bulk_pin(&self, ids: Vec<i64>, pinned: bool) -> Result<()> {
        self.blocking(move |s| {
            for id in ids {
                s.update_asset_meta(id, &AssetPatch { pinned: Some(pinned), ..Default::default() })?;
            }
            Ok(())
        })
        .await
    }

    pub async fn library_bulk_link_session(&self, ids: Vec<i64>, session_id: String) -> Result<()> {
        self.blocking(move |s| Ok(s.link_assets(&ids, LinkTarget::Session(&session_id))?)).await
    }

    pub async fn library_bulk_link_campaign(&self, ids: Vec<i64>, campaign_id: String) -> Result<()> {
        self.blocking(move |s| Ok(s.link_assets(&ids, LinkTarget::Campaign(&campaign_id))?)).await
    }

    /// Replaces an asset's images (re-edit "update existing").
    pub async fn library_replace_images(&self, id: i64, images: AssetImages) -> Result<Asset> {
        self.blocking(move |s| {
            s.update_asset_images(id, &images)?;
            Ok(s.get_asset(id)?)
        })
        .await
    }

    /// Writes one of the asset's files to `path`. If `path` is a directory, a
    /// file name is derived from the asset name.
    pub async fn library_export(&self, id: i64, what: ExportWhat, path: PathBuf) -> Result<PathBuf> {
        self.blocking(move |s| {
            let a = s.get_asset(id)?;
            let hash = match what {
                ExportWhat::Original => a.original_blob.clone(),
                ExportWhat::Rendered => a.rendered_blob.clone(),
                ExportWhat::Pdf => a
                    .pdf_blob
                    .clone()
                    .ok_or_else(|| ServiceError::InvalidInput(format!("asset {id} has no PDF")))?,
            };
            let meta = s.blob_meta(&hash)?;
            let target = if path.is_dir() {
                path.join(format!("{}.{}", safe_file_stem(&a.name), ext_for_mime(&meta.mime)))
            } else {
                path
            };
            std::fs::copy(s.blob_file(&hash)?, &target)?;
            Ok(target)
        })
        .await
    }

    /// Bulk export: writes `<name>_<kind>.png` (rendered image, as PNG) for each asset into `dir`.
    /// Name clashes get a numeric suffix.
    pub async fn library_export_many(&self, ids: Vec<i64>, dir: PathBuf) -> Result<Vec<PathBuf>> {
        self.blocking(move |s| {
            std::fs::create_dir_all(&dir)?;
            let mut used = HashSet::new();
            let mut out = Vec::new();
            for id in ids {
                let a = s.get_asset(id)?;
                let base = format!("{}_{}", safe_file_stem(&a.name), a.kind);
                let mut name = format!("{base}.png");
                let mut n = 2;
                while used.contains(&name) || dir.join(&name).exists() {
                    name = format!("{base} ({n}).png");
                    n += 1;
                }
                used.insert(name.clone());
                let target = dir.join(&name);
                write_png(s, &a.rendered_blob, &target)?;
                out.push(target);
            }
            Ok(out)
        })
        .await
    }

    /// Copies the asset's rendered image to the system clipboard.
    pub async fn library_copy_to_clipboard(&self, id: i64) -> Result<()> {
        self.blocking(move |s| {
            let a = s.get_asset(id)?;
            let img = imaging::decode(&s.read_blob(&a.rendered_blob)?)
                .map_err(|e| ServiceError::Internal(e.to_string()))?
                .to_rgba8();
            let mut cb = arboard::Clipboard::new().map_err(|e| ServiceError::Io(e.to_string()))?;
            cb.set_image(arboard::ImageData {
                width: img.width() as usize,
                height: img.height() as usize,
                bytes: img.into_raw().into(),
            })
            .map_err(|e| ServiceError::Io(e.to_string()))?;
            Ok(())
        })
        .await
    }
}

/// Writes a blob to `target` as PNG (re-encoding if stored in another format).
fn write_png(s: &Store, hash: &str, target: &Path) -> Result<()> {
    let meta = s.blob_meta(hash)?;
    if meta.mime == "image/png" {
        std::fs::copy(s.blob_file(hash)?, target)?;
    } else {
        let img = imaging::decode(&s.read_blob(hash)?).map_err(|e| ServiceError::Internal(e.to_string()))?;
        let png = imaging::encode_png(&img.to_rgba8()).map_err(|e| ServiceError::Internal(e.to_string()))?;
        std::fs::write(target, png)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use image::{Rgba, RgbaImage};

    use super::*;
    use crate::temp_service;

    pub(crate) fn write_test_png(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
        let img = RgbaImage::from_fn(w, h, |x, y| Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]));
        let p = dir.join(name);
        img.save(&p).unwrap();
        p
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn import_creates_assets_with_derivatives_and_reports_failures() {
        let (d, svc) = temp_service();
        let good = write_test_png(d.path(), "Goblin King.png", 3000, 1500);
        let bad = d.path().join("broken.png");
        std::fs::write(&bad, b"nope").unwrap();
        let report = svc
            .library_import(vec![
                ImportFile { path: good, name: None, kind: "token".into(), size: Some("Large (2 inch)".into()) },
                ImportFile { path: bad.clone(), name: None, kind: "map".into(), size: None },
            ])
            .await
            .unwrap();
        assert_eq!(report.imported.len(), 1);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].path, bad);

        let a = svc.library_get(report.imported[0]).await.unwrap();
        assert_eq!((a.name.as_str(), a.kind.as_str(), a.size_label.as_str()), ("Goblin King", "token", "Large"));
        assert_eq!(a.original_blob, a.rendered_blob, "imports are unprocessed");
        let store = svc.store();
        let thumb = store.blob_meta(a.thumb_blob.as_ref().unwrap()).unwrap();
        let preview = store.blob_meta(a.preview_blob.as_ref().unwrap()).unwrap();
        assert_eq!((thumb.mime.as_str(), thumb.width, thumb.height), ("image/webp", Some(256), Some(128)));
        assert_eq!((preview.width, preview.height), (Some(2048), Some(1024)));
    }

    #[tokio::test]
    async fn validation_of_kind_and_size() {
        let (_d, svc) = temp_service();
        let err = svc
            .library_import(vec![ImportFile { path: "x.png".into(), name: None, kind: "spell".into(), size: None }])
            .await
            .unwrap_err();
        assert_eq!(err.code(), crate::ErrorCode::InvalidInput);
        assert!(check_size("Huge (3 inch)").is_ok());
        assert!(check_size("Colossal").is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn edit_bulk_and_export() {
        let (d, svc) = temp_service();
        let p = write_test_png(d.path(), "a.png", 40, 40);
        let ids = svc
            .library_import(vec![
                ImportFile { path: p.clone(), name: Some("Orc/Chief?".into()), kind: "token".into(), size: None },
                ImportFile { path: p, name: Some("Orc/Chief?".into()), kind: "cutout".into(), size: None },
            ])
            .await
            .unwrap()
            .imported;
        assert_eq!(ids.len(), 2, "identical files import twice");
        let a = svc
            .library_update(ids[0], AssetPatch { size_label: Some("huge".into()), ..Default::default() })
            .await
            .unwrap();
        assert_eq!(a.size_label, "Huge", "size labels normalized");
        svc.library_bulk_tag(ids.clone(), "orc".into()).await.unwrap();
        svc.library_bulk_pin(ids.clone(), true).await.unwrap();
        let pinned = svc.library_search(AssetQuery { pinned_only: true, ..Default::default() }).await.unwrap();
        assert_eq!(pinned.total, 2);
        assert!(pinned.items.iter().all(|a| a.tags == ["orc"]));

        let out = d.path().join("out");
        let files = svc.library_export_many(ids.clone(), out.clone()).await.unwrap();
        let names: Vec<_> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["Orc_Chief__token.png", "Orc_Chief__cutout.png"]);
        let single = svc.library_export(ids[0], ExportWhat::Original, out.clone()).await.unwrap();
        assert!(single.exists());
        let err = svc.library_export(ids[0], ExportWhat::Pdf, out).await.unwrap_err();
        assert_eq!(err.code(), crate::ErrorCode::InvalidInput);

        assert_eq!(svc.library_delete(ids).await.unwrap(), 2);
    }

    #[test]
    fn safe_stems() {
        assert_eq!(safe_file_stem("a/b:c*?"), "a_b_c__");
        assert_eq!(safe_file_stem("..."), "asset");
        assert_eq!(safe_file_stem(" Ogre "), "Ogre");
    }
}
