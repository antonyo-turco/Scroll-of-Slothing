//! Token Sheet service. Spec: docs/features/token-sheet.md.
//! The sheet is autosaved in the `token_sheet` workspace (survives restarts).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sleuth_core::imaging;
use sleuth_core::pack::Layout;
use sleuth_core::pack::token_grid::{GridMode, TokenItem, pack};
use sleuth_core::token::TokenParams;
use sleuth_core::units::{PageFormat, SizeLabel};
use sleuth_pdf::{CutLine, PdfError};
use sleuth_store::Store;

use crate::library::check_size;
use crate::tokens::store_source;
use crate::{JobId, Result, Service, ServiceError};

const KEY: &str = "token_sheet";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetItem {
    pub id: String,
    pub label: String,
    /// Size label (Tiny … Gargantuan).
    pub size: String,
    pub copies: u32,
    /// Image blob (PNG/WebP/JPEG as stored) that gets printed.
    pub image: String,
    /// Small preview blob for the UI.
    pub thumb: String,
    /// Library asset this item came from, if any.
    pub asset_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenSheet {
    pub items: Vec<SheetItem>,
    pub format: PageFormat,
    pub cut_border: bool,
}

/// Layout for display: placements reference items by id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetLayout {
    pub page_w: f64,
    pub page_h: f64,
    pub pages: Vec<Vec<PlacedItem>>,
    /// Items that cannot fit on a page.
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacedItem {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SheetItemPatch {
    pub label: Option<String>,
    pub size: Option<String>,
    pub copies: Option<u32>,
}

fn load(s: &Store) -> Result<TokenSheet> {
    if let Some(sheet) = s.workspace_get::<TokenSheet>(KEY)? {
        return Ok(sheet);
    }
    let st = s.load_settings()?;
    Ok(TokenSheet { items: vec![], format: st.token_page_format, cut_border: st.token_cut_border })
}

fn save(s: &Store, sheet: &TokenSheet) -> Result<()> {
    s.workspace_set(KEY, sheet)?;
    s.record_change("token_sheet", KEY, "update")?;
    Ok(())
}

pub(crate) fn thumb_of(s: &Store, image_hash: &str) -> Result<String> {
    let img = imaging::decode(&s.read_blob(image_hash)?)
        .map_err(|e| ServiceError::InvalidInput(e.to_string()))?;
    let t = imaging::downscale(&img, 160);
    let webp =
        imaging::encode_webp_lossless(&t).map_err(|e| ServiceError::Internal(e.to_string()))?;
    Ok(s.put_blob(&webp, "image/webp", Some(t.dimensions()))?.hash)
}

fn new_item(
    label: String,
    size: String,
    image: String,
    thumb: String,
    asset_id: Option<i64>,
) -> SheetItem {
    SheetItem {
        id: uuid::Uuid::new_v4().to_string(),
        label,
        size,
        copies: 1,
        image,
        thumb,
        asset_id,
    }
}

fn compute_layout(sheet: &TokenSheet) -> (Layout, SheetLayout) {
    let items: Vec<TokenItem> = sheet
        .items
        .iter()
        .map(|i| TokenItem {
            size_pt: SizeLabel::parse(&i.size).unwrap_or_default().points(),
            copies: i.copies,
        })
        .collect();
    let layout = pack(&items, sheet.format, GridMode::HalfInch);
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
    async fn sheet_mutate<F>(&self, f: F) -> Result<TokenSheet>
    where
        F: FnOnce(&Store, &mut TokenSheet) -> Result<()> + Send + 'static,
    {
        self.blocking(move |s| {
            let mut sheet = load(s)?;
            f(s, &mut sheet)?;
            save(s, &sheet)?;
            Ok(sheet)
        })
        .await
    }

    pub async fn token_sheet_get(&self) -> Result<TokenSheet> {
        self.blocking(load).await
    }

    /// Adds library assets (their rendered image, name and size).
    pub async fn token_sheet_add_assets(&self, ids: Vec<i64>) -> Result<TokenSheet> {
        self.sheet_mutate(move |s, sheet| {
            for id in ids {
                let a = s.get_asset(id)?;
                let thumb = match &a.thumb_blob {
                    Some(t) => t.clone(),
                    None => thumb_of(s, &a.rendered_blob)?,
                };
                let size = SizeLabel::parse(&a.size_label).unwrap_or_default().as_str().to_string();
                sheet.items.push(new_item(a.name, size, a.rendered_blob, thumb, Some(a.id)));
            }
            Ok(())
        })
        .await
    }

    /// Adds image files as tokens (default size Medium, like Python's upload).
    pub async fn token_sheet_add_files(
        &self,
        paths: Vec<PathBuf>,
        size: Option<String>,
    ) -> Result<TokenSheet> {
        let size = check_size(size.as_deref().unwrap_or("Medium"))?;
        self.sheet_mutate(move |s, sheet| {
            for p in paths {
                let src = store_source(s, &std::fs::read(&p)?)?;
                let label = p
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Token".into());
                let thumb = thumb_of(s, &src.hash)?;
                sheet.items.push(new_item(label, size.clone(), src.hash, thumb, None));
            }
            Ok(())
        })
        .await
    }

    /// Adds a token straight from the Token Cutter (composed now, at full quality).
    pub async fn token_sheet_add_composed(
        &self,
        source: String,
        params: TokenParams,
        label: String,
        size: String,
    ) -> Result<TokenSheet> {
        let size = check_size(&size)?;
        let png = self.token_render(source, params, false).await?;
        self.sheet_mutate(move |s, sheet| {
            let img = store_source(s, &png)?;
            let thumb = thumb_of(s, &img.hash)?;
            let label =
                if label.trim().is_empty() { "Token".into() } else { label.trim().to_string() };
            sheet.items.push(new_item(label, size, img.hash, thumb, None));
            Ok(())
        })
        .await
    }

    pub async fn token_sheet_update(
        &self,
        id: String,
        patch: SheetItemPatch,
    ) -> Result<TokenSheet> {
        let size = patch.size.as_deref().map(check_size).transpose()?;
        self.sheet_mutate(move |_, sheet| {
            let item = sheet
                .items
                .iter_mut()
                .find(|i| i.id == id)
                .ok_or_else(|| ServiceError::NotFound(format!("sheet item {id}")))?;
            if let Some(l) = patch.label {
                item.label = l;
            }
            if let Some(s) = size {
                item.size = s;
            }
            if let Some(c) = patch.copies {
                if !(1..=99).contains(&c) {
                    return Err(ServiceError::InvalidInput("copies must be 1–99".into()));
                }
                item.copies = c;
            }
            Ok(())
        })
        .await
    }

    pub async fn token_sheet_remove(&self, id: String) -> Result<TokenSheet> {
        self.sheet_mutate(move |_, sheet| {
            let before = sheet.items.len();
            sheet.items.retain(|i| i.id != id);
            if sheet.items.len() == before {
                return Err(ServiceError::NotFound(format!("sheet item {id}")));
            }
            Ok(())
        })
        .await
    }

    pub async fn token_sheet_clear(&self) -> Result<TokenSheet> {
        self.sheet_mutate(|_, sheet| {
            sheet.items.clear();
            Ok(())
        })
        .await
    }

    pub async fn token_sheet_set_options(
        &self,
        format: Option<PageFormat>,
        cut_border: Option<bool>,
    ) -> Result<TokenSheet> {
        self.sheet_mutate(move |_, sheet| {
            if let Some(f) = format {
                sheet.format = f;
            }
            if let Some(c) = cut_border {
                sheet.cut_border = c;
            }
            Ok(())
        })
        .await
    }

    pub async fn token_sheet_layout(&self) -> Result<SheetLayout> {
        self.blocking(|s| Ok(compute_layout(&load(s)?).1)).await
    }

    /// Exports the sheet to `path` as a background job (result: `{path, pages}`).
    pub fn token_sheet_export_pdf(&self, path: PathBuf) -> JobId {
        self.spawn_job("token sheet export", move |ctx| {
            let store = ctx.service().store();
            let sheet = load(&store)?;
            if sheet.items.is_empty() {
                return Err(ServiceError::InvalidInput("the token sheet is empty".into()));
            }
            let (layout, view) = compute_layout(&sheet);
            if !view.skipped.is_empty() {
                return Err(ServiceError::InvalidInput(format!(
                    "{} item(s) cannot fit on a page",
                    view.skipped.len()
                )));
            }
            ctx.progress(0.02, "reading images");
            let images = sheet
                .items
                .iter()
                .map(|i| png_bytes(&store, &i.image))
                .collect::<Result<Vec<_>>>()?;
            let cut = sheet
                .cut_border
                .then(|| CutLine::from_settings(&store.load_settings().unwrap_or_default()));
            let bytes = sleuth_pdf::token_sheet::render(&layout, &images, cut.as_ref(), &mut |p| {
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

/// The PDF writer embeds PNG; other stored formats are converted.
pub(crate) fn png_bytes(s: &Store, hash: &str) -> Result<Vec<u8>> {
    let meta = s.blob_meta(hash)?;
    let bytes = s.read_blob(hash)?;
    if meta.mime == "image/png" {
        return Ok(bytes);
    }
    let img = imaging::decode(&bytes).map_err(|e| ServiceError::InvalidInput(e.to_string()))?;
    imaging::encode_png(&img.to_rgba8()).map_err(|e| ServiceError::Internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::library::tests::write_test_png;
    use crate::{ImportFile, JobState, temp_service};

    async fn wait(svc: &Service, id: &str) -> crate::JobStatus {
        for _ in 0..2000 {
            let st = svc.job_status(id).unwrap();
            if st.state != JobState::Running {
                return st;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("job timeout");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn defaults_come_from_settings_and_state_persists() {
        let (d, svc) = temp_service();
        let patch = serde_json::json!({"token_page_format": "A4", "token_cut_border": true});
        svc.settings_set(patch.as_object().unwrap().clone()).await.unwrap();
        let sheet = svc.token_sheet_get().await.unwrap();
        assert_eq!((sheet.format, sheet.cut_border), (PageFormat::A4, true));

        let p = write_test_png(d.path(), "Kobold.png", 50, 50);
        svc.token_sheet_add_files(vec![p], Some("Small (0.8 inch)".into())).await.unwrap();
        // A fresh service on the same data dir sees the same sheet (autosave).
        let again = Service::open(svc.data_dir()).unwrap().token_sheet_get().await.unwrap();
        assert_eq!(again.items.len(), 1);
        assert_eq!(
            (again.items[0].label.as_str(), again.items[0].size.as_str()),
            ("Kobold", "Small")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn add_from_library_and_cutter_update_remove_clear() {
        let (d, svc) = temp_service();
        let p = write_test_png(d.path(), "a.png", 64, 64);
        let ids = svc
            .library_import(vec![ImportFile {
                path: p.clone(),
                name: Some("Ogre".into()),
                kind: "token".into(),
                size: Some("Large".into()),
            }])
            .await
            .unwrap()
            .imported;
        let sheet = svc.token_sheet_add_assets(ids.clone()).await.unwrap();
        assert_eq!(
            (sheet.items[0].label.as_str(), sheet.items[0].size.as_str(), sheet.items[0].asset_id),
            ("Ogre", "Large", Some(ids[0]))
        );

        let src = svc.image_source_from_path(p).await.unwrap();
        let params =
            TokenParams { border: "builtin:roll20_greyscale".into(), ..Default::default() };
        let sheet = svc
            .token_sheet_add_composed(src.hash, params, "Imp".into(), "Tiny".into())
            .await
            .unwrap();
        assert_eq!(sheet.items.len(), 2);
        let imp = sheet.items[1].id.clone();

        svc.token_sheet_update(
            imp.clone(),
            SheetItemPatch { copies: Some(5), ..Default::default() },
        )
        .await
        .unwrap();
        assert!(
            svc.token_sheet_update(
                imp.clone(),
                SheetItemPatch { copies: Some(0), ..Default::default() }
            )
            .await
            .is_err()
        );
        let layout = svc.token_sheet_layout().await.unwrap();
        assert_eq!(layout.pages.len(), 1);
        assert_eq!(layout.pages[0].iter().filter(|p| p.id == imp).count(), 5);
        assert_eq!(
            layout.pages[0].iter().find(|p| p.id == imp).unwrap().w,
            36.0,
            "Tiny prints at 0.5 in"
        );

        svc.token_sheet_remove(imp).await.unwrap();
        assert_eq!(svc.token_sheet_get().await.unwrap().items.len(), 1);
        assert!(svc.token_sheet_clear().await.unwrap().items.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn export_job_writes_a_pdf_with_the_right_page_count() {
        let (d, svc) = temp_service();
        let p = write_test_png(d.path(), "Rat.png", 40, 40);
        let sheet = svc.token_sheet_add_files(vec![p], None).await.unwrap();
        svc.token_sheet_update(
            sheet.items[0].id.clone(),
            SheetItemPatch { copies: Some(99), ..Default::default() },
        )
        .await
        .unwrap();
        svc.token_sheet_set_options(Some(PageFormat::Letter), Some(true)).await.unwrap();
        let out = d.path().join("sheet.pdf");
        let st = wait(&svc, &svc.token_sheet_export_pdf(out.clone())).await;
        assert_eq!(st.state, JobState::Done, "{:?}", st.error);
        assert_eq!(
            st.result.unwrap()["pages"],
            2,
            "Letter fits 8x10 one-inch tokens: 99 copies need 2 pages"
        );
        let bytes = std::fs::read(out).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exporting_an_empty_sheet_fails_cleanly() {
        let (d, svc) = temp_service();
        let st = wait(&svc, &svc.token_sheet_export_pdf(d.path().join("x.pdf"))).await;
        assert_eq!(st.state, JobState::Failed);
        assert!(st.error.unwrap().contains("empty"));
    }
}
