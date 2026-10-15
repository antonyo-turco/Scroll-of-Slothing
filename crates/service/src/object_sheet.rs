//! Object Sheet service: cut-outs laid out flat or as fold-over standees.
//! Spec: docs/features/object-sheet.md. Autosaved in the `object_sheet` workspace.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sleuth_core::cutout::{CutoutParams, standee_block};
use sleuth_core::imaging;
use sleuth_core::pack::Layout;
use sleuth_core::pack::shelf::{SheetMode, ShelfItem, pack};
use sleuth_core::units::{PageFormat, SizeLabel};
use sleuth_pdf::PdfError;
use sleuth_store::Store;

use crate::library::check_size;
use crate::token_sheet::{PlacedItem, SheetLayout, png_bytes, thumb_of};
use crate::tokens::store_source;
use crate::{JobId, Result, Service, ServiceError};

const KEY: &str = "object_sheet";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectItem {
    pub id: String,
    pub label: String,
    pub size: String,
    /// 10–400 %.
    pub scale: u32,
    pub copies: u32,
    pub image: String,
    pub thumb: String,
    pub w_px: u32,
    pub h_px: u32,
    pub asset_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectSheet {
    pub items: Vec<ObjectItem>,
    pub format: PageFormat,
    pub mode: SheetMode,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObjectItemPatch {
    pub label: Option<String>,
    pub size: Option<String>,
    pub scale: Option<u32>,
    pub copies: Option<u32>,
}

fn load(s: &Store) -> Result<ObjectSheet> {
    if let Some(sheet) = s.workspace_get::<ObjectSheet>(KEY)? {
        return Ok(sheet);
    }
    Ok(ObjectSheet {
        items: vec![],
        format: s.load_settings()?.object_page_format,
        mode: SheetMode::Flat,
    })
}

fn save(s: &Store, sheet: &ObjectSheet) -> Result<()> {
    s.workspace_set(KEY, sheet)?;
    s.record_change("object_sheet", KEY, "update")?;
    Ok(())
}

fn new_item(
    s: &Store,
    label: String,
    size: String,
    image: String,
    asset_id: Option<i64>,
) -> Result<ObjectItem> {
    let (w_px, h_px) = imaging::dimensions(&s.read_blob(&image)?)
        .ok_or_else(|| ServiceError::InvalidInput("not an image".into()))?;
    Ok(ObjectItem {
        id: uuid::Uuid::new_v4().to_string(),
        label: if label.trim().is_empty() { "Cut-Out".into() } else { label.trim().into() },
        size,
        scale: 100,
        copies: 1,
        thumb: thumb_of(s, &image)?,
        image,
        w_px,
        h_px,
        asset_id,
    })
}

fn compute(sheet: &ObjectSheet) -> (Layout, SheetLayout) {
    let items: Vec<ShelfItem> = sheet
        .items
        .iter()
        .map(|i| ShelfItem {
            w_px: i.w_px,
            h_px: i.h_px,
            size_pt: SizeLabel::parse(&i.size).unwrap_or_default().points(),
            scale_pct: f64::from(i.scale),
            copies: i.copies,
        })
        .collect();
    let layout = pack(&items, sheet.format, sheet.mode);
    let view = SheetLayout {
        page_w: layout.page_w,
        page_h: layout.page_h,
        pages: layout
            .pages
            .iter()
            .map(|p| {
                p.iter()
                    .map(|pl| PlacedItem {
                        id: sheet.items[pl.item].id.clone(),
                        x: pl.x,
                        y: pl.y,
                        w: pl.w,
                        h: pl.h,
                    })
                    .collect()
            })
            .collect(),
        skipped: layout.skipped.iter().map(|&i| sheet.items[i].id.clone()).collect(),
    };
    (layout, view)
}

impl Service {
    async fn object_mutate<F>(&self, f: F) -> Result<ObjectSheet>
    where
        F: FnOnce(&Store, &mut ObjectSheet) -> Result<()> + Send + 'static,
    {
        self.blocking(move |s| {
            let mut sheet = load(s)?;
            f(s, &mut sheet)?;
            save(s, &sheet)?;
            Ok(sheet)
        })
        .await
    }

    pub async fn object_sheet_get(&self) -> Result<ObjectSheet> {
        self.blocking(load).await
    }

    /// Adds library assets (their rendered image; any kind, typically cut-outs).
    pub async fn object_sheet_add_assets(&self, ids: Vec<i64>) -> Result<ObjectSheet> {
        self.object_mutate(move |s, sheet| {
            for id in ids {
                let a = s.get_asset(id)?;
                let size = SizeLabel::parse(&a.size_label).unwrap_or_default().as_str().to_string();
                sheet.items.push(new_item(s, a.name, size, a.rendered_blob, Some(a.id))?);
            }
            Ok(())
        })
        .await
    }

    pub async fn object_sheet_add_files(
        &self,
        paths: Vec<PathBuf>,
        size: Option<String>,
    ) -> Result<ObjectSheet> {
        let size = check_size(size.as_deref().unwrap_or("Medium"))?;
        self.object_mutate(move |s, sheet| {
            for p in paths {
                let src = store_source(s, &std::fs::read(&p)?)?;
                let label =
                    p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                sheet.items.push(new_item(s, label, size.clone(), src.hash, None)?);
            }
            Ok(())
        })
        .await
    }

    /// Adds the cut-out currently in the Object Cutter.
    pub async fn object_sheet_add_composed(
        &self,
        source: String,
        params: CutoutParams,
        label: String,
        size: String,
    ) -> Result<ObjectSheet> {
        let size = check_size(&size)?;
        let png = self.cutout_render(source, params, false).await?;
        self.object_mutate(move |s, sheet| {
            let img = store_source(s, &png)?;
            sheet.items.push(new_item(s, label, size, img.hash, None)?);
            Ok(())
        })
        .await
    }

    pub async fn object_sheet_update(
        &self,
        id: String,
        patch: ObjectItemPatch,
    ) -> Result<ObjectSheet> {
        let size = patch.size.as_deref().map(check_size).transpose()?;
        self.object_mutate(move |_, sheet| {
            let it = sheet
                .items
                .iter_mut()
                .find(|i| i.id == id)
                .ok_or_else(|| ServiceError::NotFound(format!("sheet item {id}")))?;
            if let Some(l) = patch.label {
                it.label = l;
            }
            if let Some(s) = size {
                it.size = s;
            }
            if let Some(sc) = patch.scale {
                if !(10..=400).contains(&sc) {
                    return Err(ServiceError::InvalidInput("scale must be 10–400 %".into()));
                }
                it.scale = sc;
            }
            if let Some(c) = patch.copies {
                if !(1..=99).contains(&c) {
                    return Err(ServiceError::InvalidInput("copies must be 1–99".into()));
                }
                it.copies = c;
            }
            Ok(())
        })
        .await
    }

    pub async fn object_sheet_remove(&self, id: String) -> Result<ObjectSheet> {
        self.object_mutate(move |_, sheet| {
            let n = sheet.items.len();
            sheet.items.retain(|i| i.id != id);
            if sheet.items.len() == n {
                return Err(ServiceError::NotFound(format!("sheet item {id}")));
            }
            Ok(())
        })
        .await
    }

    pub async fn object_sheet_clear(&self) -> Result<ObjectSheet> {
        self.object_mutate(|_, sheet| {
            sheet.items.clear();
            Ok(())
        })
        .await
    }

    pub async fn object_sheet_set_options(
        &self,
        format: Option<PageFormat>,
        mode: Option<SheetMode>,
    ) -> Result<ObjectSheet> {
        self.object_mutate(move |_, sheet| {
            if let Some(f) = format {
                sheet.format = f;
            }
            if let Some(m) = mode {
                sheet.mode = m;
            }
            Ok(())
        })
        .await
    }

    pub async fn object_sheet_layout(&self) -> Result<SheetLayout> {
        self.blocking(|s| Ok(compute(&load(s)?).1)).await
    }

    /// Exports the object sheet as a background job (result: `{path, pages}`).
    pub fn object_sheet_export_pdf(&self, path: PathBuf) -> JobId {
        self.spawn_job("object sheet export", move |ctx| {
            let store = ctx.service().store();
            let sheet = load(&store)?;
            if sheet.items.is_empty() {
                return Err(ServiceError::InvalidInput("the object sheet is empty".into()));
            }
            let (layout, view) = compute(&sheet);
            if !view.skipped.is_empty() {
                return Err(ServiceError::InvalidInput(format!(
                    "{} item(s) cannot fit on a page",
                    view.skipped.len()
                )));
            }
            ctx.progress(0.02, "preparing images");
            let images = sheet
                .items
                .iter()
                .map(|i| match sheet.mode {
                    SheetMode::Flat => png_bytes(&store, &i.image),
                    SheetMode::Standee => {
                        let img = imaging::decode(&store.read_blob(&i.image)?)
                            .map_err(|e| ServiceError::InvalidInput(e.to_string()))?;
                        imaging::encode_png(&standee_block(&img.to_rgba8()))
                            .map_err(|e| ServiceError::Internal(e.to_string()))
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            // Placements already have the item's aspect, so the shared sheet writer
            // draws them at exact size (no cut circles on object sheets).
            let bytes = sleuth_pdf::token_sheet::render(&layout, &images, None, &mut |p| {
                ctx.progress(0.05 + 0.9 * p, "rendering pages");
                !ctx.is_cancelled()
            })
            .map_err(|e| match e {
                PdfError::Cancelled => ServiceError::Cancelled,
                other => ServiceError::Internal(other.to_string()),
            })?;
            std::fs::write(&path, bytes)?;
            Ok(serde_json::json!({ "path": path, "pages": layout.pages.len() }))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::library::tests::write_test_png;
    use crate::{JobState, temp_service};

    #[tokio::test(flavor = "multi_thread")]
    async fn add_update_layout_modes_and_export() {
        let (d, svc) = temp_service();
        let patch = serde_json::json!({"object_page_format": "A4"});
        svc.settings_set(patch.as_object().unwrap().clone()).await.unwrap();
        let p = write_test_png(d.path(), "Lamp.png", 100, 200);
        let sheet =
            svc.object_sheet_add_files(vec![p.clone()], Some("Medium".into())).await.unwrap();
        assert_eq!((sheet.format, sheet.mode), (PageFormat::A4, SheetMode::Flat));
        let it = &sheet.items[0];
        assert_eq!((it.label.as_str(), it.w_px, it.h_px, it.scale), ("Lamp", 100, 200, 100));

        let src = svc.image_source_from_path(p).await.unwrap();
        let params = CutoutParams {
            points: vec![[10.0, 10.0], [90.0, 10.0], [90.0, 190.0], [10.0, 190.0]],
            ..Default::default()
        };
        svc.object_sheet_add_composed(src.hash, params, "".into(), "Large".into()).await.unwrap();
        let sheet = svc.object_sheet_get().await.unwrap();
        assert_eq!(sheet.items[1].label, "Cut-Out", "default label like Python");

        let id = sheet.items[0].id.clone();
        svc.object_sheet_update(
            id.clone(),
            ObjectItemPatch { scale: Some(150), copies: Some(3), ..Default::default() },
        )
        .await
        .unwrap();
        assert!(
            svc.object_sheet_update(
                id.clone(),
                ObjectItemPatch { scale: Some(5), ..Default::default() }
            )
            .await
            .is_err()
        );
        let flat = svc.object_sheet_layout().await.unwrap();
        let lamp = flat.pages[0].iter().find(|p| p.id == id).unwrap();
        assert_eq!((lamp.w, lamp.h), (54.0, 108.0), "Medium 72pt × 150% on the longest side");

        svc.object_sheet_set_options(None, Some(SheetMode::Standee)).await.unwrap();
        let st = svc.object_sheet_layout().await.unwrap();
        assert_eq!(
            st.pages[0].iter().find(|p| p.id == id).unwrap().h,
            216.0,
            "standee doubles height"
        );

        let out = d.path().join("objects.pdf");
        let job = svc.object_sheet_export_pdf(out.clone());
        let mut state = JobState::Running;
        for _ in 0..2000 {
            state = svc.job_status(&job).unwrap().state;
            if state != JobState::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(state, JobState::Done, "{:?}", svc.job_status(&job).unwrap().error);
        assert!(std::fs::read(out).unwrap().starts_with(b"%PDF"));

        svc.object_sheet_remove(id).await.unwrap();
        assert_eq!(svc.object_sheet_clear().await.unwrap().items.len(), 0);
    }
}
