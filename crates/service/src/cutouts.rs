//! Object Cutter service. Spec: docs/features/object-cutter.md.
//! Same model as tokens: a stored source blob + `CutoutParams` (lasso points
//! in source pixels, outline) → rendered cut-out; params are saved for re-edit.

use std::path::PathBuf;

use image::RgbaImage;
use serde::{Deserialize, Serialize};
use sleuth_core::cutout::{CutoutParams, make_cutout};
use sleuth_core::imaging;
use sleuth_core::units::SizeLabel;
use sleuth_store::{AssetImages, AssetPatch, NewAsset};

use crate::library::{check_size, store_encoded};
use crate::tokens::{SourceImage, source_info};
use crate::{Asset, Result, Service, ServiceError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutoutDraft {
    pub source: SourceImage,
    pub params: CutoutParams,
    pub name: String,
    pub size: String,
    pub asset_id: Option<i64>,
}

fn check_points(params: &CutoutParams) -> Result<()> {
    if params.points.len() < 3 {
        return Err(ServiceError::InvalidInput("trace at least 3 points around the object".into()));
    }
    Ok(())
}

impl Service {
    pub(crate) async fn compose_cutout(
        &self,
        source: &str,
        params: &CutoutParams,
    ) -> Result<RgbaImage> {
        check_points(params)?;
        let svc = self.clone();
        let (source, params) = (source.to_string(), params.clone());
        self.blocking(move |s| {
            let img = svc.inner.images.get(s, &source)?;
            let out = make_cutout(&img, &params);
            if out.pixels().all(|p| p[3] == 0) {
                return Err(ServiceError::InvalidInput("the selection is empty".into()));
            }
            Ok(out)
        })
        .await
    }

    /// Renders the cut-out (lasso + outline) to PNG bytes.
    pub async fn cutout_render(
        &self,
        source: String,
        params: CutoutParams,
        fast: bool,
    ) -> Result<Vec<u8>> {
        let img = self.compose_cutout(&source, &params).await?;
        tokio::task::spawn_blocking(move || {
            if fast { imaging::encode_png_fast(&img) } else { imaging::encode_png(&img) }
                .map_err(|e| ServiceError::Internal(e.to_string()))
        })
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?
    }

    pub async fn cutout_save_png(
        &self,
        source: String,
        params: CutoutParams,
        path: PathBuf,
    ) -> Result<PathBuf> {
        let png = self.cutout_render(source, params, false).await?;
        self.blocking(move |_| {
            std::fs::write(&path, png)?;
            Ok(path)
        })
        .await
    }

    /// Saves as a library asset of kind `cutout` (new, or replacing `update`).
    pub async fn cutout_save_to_library(
        &self,
        source: String,
        params: CutoutParams,
        name: String,
        size: String,
        update: Option<i64>,
    ) -> Result<Asset> {
        if name.trim().is_empty() {
            return Err(ServiceError::InvalidInput("please enter a name".into()));
        }
        let size = check_size(&size)?;
        let png = self.cutout_render(source.clone(), params.clone(), false).await?;
        let params_json =
            serde_json::to_string(&params).map_err(|e| ServiceError::Internal(e.to_string()))?;
        self.blocking(move |s| {
            let original = s.read_blob(&source)?;
            let stored = store_encoded(s, &original, &png)?;
            let id = match update {
                Some(id) => {
                    s.update_asset_images(
                        id,
                        &AssetImages {
                            original_blob: stored.original_blob,
                            rendered_blob: stored.rendered_blob,
                            pdf_blob: None,
                            thumb_blob: Some(stored.thumb_blob),
                            preview_blob: Some(stored.preview_blob),
                            params_json: Some(params_json),
                        },
                    )?;
                    s.update_asset_meta(
                        id,
                        &AssetPatch {
                            name: Some(name),
                            size_label: Some(size),
                            ..Default::default()
                        },
                    )?;
                    id
                }
                None => s.insert_asset(&NewAsset {
                    kind: "cutout".into(),
                    name: name.trim().into(),
                    aliases: String::new(),
                    size_label: size,
                    original_blob: stored.original_blob,
                    rendered_blob: stored.rendered_blob,
                    pdf_blob: None,
                    thumb_blob: Some(stored.thumb_blob),
                    preview_blob: Some(stored.preview_blob),
                    params_json: Some(params_json),
                    legacy_id: None,
                    pinned: false,
                })?,
            };
            Ok(s.get_asset(id)?)
        })
        .await
    }

    /// Opens an asset in the Object Cutter. Without stored params (imports,
    /// legacy) the trace starts empty, as in Python.
    pub async fn cutout_open_asset(&self, id: i64) -> Result<CutoutDraft> {
        self.blocking(move |s| {
            let a = s.get_asset(id)?;
            let params = a
                .params_json
                .as_deref()
                .and_then(|j| serde_json::from_str::<CutoutParams>(j).ok())
                .unwrap_or_default();
            Ok(CutoutDraft {
                source: source_info(s, &a.original_blob)?,
                params,
                name: a.name,
                size: SizeLabel::parse(&a.size_label).unwrap_or_default().as_str().into(),
                asset_id: Some(a.id),
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::write_test_png;
    use crate::temp_service;

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn render_save_and_reopen_with_trace() {
        let (d, svc) = temp_service();
        let src = svc
            .image_source_from_path(write_test_png(d.path(), "brazier.png", 200, 150))
            .await
            .unwrap();
        let params = CutoutParams {
            points: square(20.0, 20.0, 120.0, 100.0),
            outline_px: 4,
            ..Default::default()
        };
        let png = svc.cutout_render(src.hash.clone(), params.clone(), true).await.unwrap();
        // 101×81 selection (PIL-inclusive) + 4px outline on each side.
        assert_eq!(imaging::dimensions(&png), Some((109, 89)));

        let a = svc
            .cutout_save_to_library(
                src.hash.clone(),
                params.clone(),
                "Brazier".into(),
                "Small".into(),
                None,
            )
            .await
            .unwrap();
        assert_eq!((a.kind.as_str(), a.size_label.as_str()), ("cutout", "Small"));
        assert_eq!(a.original_blob, src.hash);
        let draft = svc.cutout_open_asset(a.id).await.unwrap();
        assert_eq!(draft.params, params, "trace restored (Python lost it)");

        let updated = svc
            .cutout_save_to_library(
                src.hash,
                CutoutParams { outline_px: 0, ..params },
                "Brazier".into(),
                "Medium".into(),
                Some(a.id),
            )
            .await
            .unwrap();
        assert_eq!((updated.id, updated.size_label.as_str()), (a.id, "Medium"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalid_selections_are_rejected() {
        let (d, svc) = temp_service();
        let src =
            svc.image_source_from_path(write_test_png(d.path(), "x.png", 50, 50)).await.unwrap();
        let two = CutoutParams { points: vec![[0.0, 0.0], [10.0, 10.0]], ..Default::default() };
        assert_eq!(
            svc.cutout_render(src.hash.clone(), two, true).await.unwrap_err().code(),
            crate::ErrorCode::InvalidInput
        );
        let outside =
            CutoutParams { points: square(500.0, 500.0, 600.0, 600.0), ..Default::default() };
        assert!(
            svc.cutout_render(src.hash, outside, true)
                .await
                .unwrap_err()
                .to_string()
                .contains("empty")
        );
    }
}
