//! AoE templates service. Spec: docs/features/aoe-templates.md.
//!
//! Templates are fully described by an [`AoeConfig`]. The multi-template
//! sheet is autosaved in the `aoe.sheet` workspace; like Python, every sheet
//! item is re-rendered at the *current* inches-per-square and page format.

use std::path::PathBuf;

use image::RgbImage;
use serde::{Deserialize, Serialize};
use sleuth_core::aoe::{self, AoeConfig, SHEET_GAP_IN};
use sleuth_core::imaging;
use sleuth_core::maps::tiling::{self, TileParams};
use sleuth_core::pack::maxrects::SheetLayout;
use sleuth_core::units::PageFormat;
use sleuth_pdf::PdfError;
use sleuth_store::Store;

use crate::{JobId, Result, Service, ServiceError};

const KEY: &str = "aoe.sheet";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AoePreset {
    pub name: String,
    /// Built-in presets set only shape and size; user (★) presets set everything.
    pub builtin: bool,
    pub config: AoeConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AoeSheetItem {
    pub id: String,
    pub config: AoeConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AoeSheet {
    pub items: Vec<AoeSheetItem>,
    pub gap_in: f64,
}

impl Default for AoeSheet {
    fn default() -> Self {
        Self { items: Vec::new(), gap_in: SHEET_GAP_IN }
    }
}

/// The info line and page outlines for the current template.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AoeInfo {
    pub footprint_ft: (f64, f64),
    pub inches: (f64, f64),
    pub image_w: i64,
    pub image_h: i64,
    pub cell_px: f64,
    pub pages: usize,
    pub cols: u32,
    pub rows: u32,
    /// Page rectangles in image px, for the "current template" preview.
    pub page_rects: Vec<(f64, f64, f64, f64)>,
    pub label: String,
    /// Python's info line.
    pub summary: String,
}

/// Sheet packing result with item ids.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AoeSheetLayout {
    pub page_w: i64,
    pub page_h: i64,
    pub pages: usize,
    pub placed: Vec<AoePlaced>,
    /// Display names of items too big for one page.
    pub skipped: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AoePlaced {
    pub id: String,
    pub page: usize,
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

fn internal(e: impl std::fmt::Display) -> ServiceError {
    ServiceError::Internal(e.to_string())
}

fn check_size(cfg: &AoeConfig) -> Result<AoeConfig> {
    let c = cfg.sanitized();
    if c.a_ft <= 0.0 {
        return Err(ServiceError::InvalidInput("set a template size first".into()));
    }
    Ok(c)
}

/// Template tiling: the map tiler with margin 0 (Python `_render`).
fn template_tiles(c: &AoeConfig, w: i64, h: i64, cell: f64) -> tiling::TilePlan {
    tiling::plan(&TileParams {
        image_w: w as u32,
        image_h: h as u32,
        region: None,
        cell_w: cell,
        cell_h: cell,
        square_in: c.inch,
        format: c.page_format,
        margin_pt: 0.0,
        offset: (0.0, 0.0),
        overlap_in: 0.0,
        align_to_grid: false,
        grid_offset: (0.0, 0.0),
        landscape: false,
    })
}

pub fn info(cfg: &AoeConfig) -> AoeInfo {
    let c = cfg.sanitized();
    let (bw, bh) = aoe::bbox_ft(c.shape, c.a_ft, c.b_ft);
    let inches = (bw / c.feet_per_square * c.inch, bh / c.feet_per_square * c.inch);
    let (w, h, cell) = aoe::template_size(
        c.shape,
        c.a_ft,
        c.b_ft,
        c.feet_per_square,
        c.px_per_square(),
        c.line_width,
    );
    let plan = if c.a_ft > 0.0 { Some(template_tiles(&c, w, h, cell)) } else { None };
    let tiles = plan.as_ref().map(|p| p.tiles.clone()).unwrap_or_default();
    let cols = tiles.iter().map(|t| t.col + 1).max().unwrap_or(0);
    let rows = tiles.iter().map(|t| t.row + 1).max().unwrap_or(0);
    let grid = if tiles.is_empty() { String::new() } else { format!(" ({cols}×{rows})") };
    let summary = if c.a_ft <= 0.0 {
        "Set a size greater than 0.".to_string()
    } else {
        format!(
            "Footprint: {} × {} ft  ·  {:.1}\" × {:.1}\" on paper  ·  {} page(s){grid}",
            aoe::fmt_g(bw),
            aoe::fmt_g(bh),
            inches.0,
            inches.1,
            tiles.len()
        )
    };
    AoeInfo {
        footprint_ft: (bw, bh),
        inches,
        image_w: w,
        image_h: h,
        cell_px: cell,
        pages: tiles.len(),
        cols,
        rows,
        page_rects: tiles.iter().map(|t| t.page_rect).collect(),
        label: aoe::default_label(c.shape, c.a_ft, c.b_ft),
        summary,
    }
}

fn load(s: &Store) -> Result<AoeSheet> {
    Ok(s.workspace_get::<AoeSheet>(KEY)?.unwrap_or_default())
}

fn save(s: &Store, sheet: &AoeSheet) -> Result<()> {
    s.workspace_set(KEY, sheet)?;
    s.record_change("aoe_sheet", KEY, "update")?;
    Ok(())
}

/// Every item re-rendered at the current `inch` / `format`.
fn sheet_configs(sheet: &AoeSheet, inch: f64, format: PageFormat) -> Vec<AoeConfig> {
    sheet
        .items
        .iter()
        .map(|i| AoeConfig { inch, page_format: format, ..i.config.clone() }.sanitized())
        .collect()
}

fn sheet_layout(sheet: &AoeSheet, inch: f64, format: PageFormat) -> (SheetLayout, AoeSheetLayout) {
    let cfgs = sheet_configs(sheet, inch, format);
    let sizes = cfgs.iter().map(|c| {
        let (w, h, cell) = aoe::template_size(
            c.shape,
            c.a_ft,
            c.b_ft,
            c.feet_per_square,
            c.px_per_square(),
            c.line_width,
        );
        (w as u32, h as u32, cell)
    });
    let gap = if sheet.gap_in.is_finite() { sheet.gap_in.max(0.0) } else { SHEET_GAP_IN };
    let l = aoe::sheet_layout(sizes, format, inch.max(0.05), gap);
    let skipped: Vec<String> =
        l.skipped.iter().map(|&i| sheet.items[i].config.display_name()).collect();
    let summary = if sheet.items.is_empty() {
        "Sheet is empty — design a template and click 'Add current to sheet'.".to_string()
    } else if l.pages == 0 {
        format!("None of these templates fit on a single sheet: {}", skipped.join(", "))
    } else {
        let mut m = format!("Sheet: {} template(s) · {} page(s)", sheet.items.len(), l.pages);
        if !skipped.is_empty() {
            m += &format!("  ·  too big for one sheet: {}", skipped.join(", "));
        }
        m
    };
    let view = AoeSheetLayout {
        page_w: l.page_w,
        page_h: l.page_h,
        pages: l.pages,
        placed: l
            .placed
            .iter()
            .map(|p| AoePlaced {
                id: sheet.items[p.item].id.clone(),
                page: p.page,
                x: p.x,
                y: p.y,
                w: p.w,
                h: p.h,
            })
            .collect(),
        skipped,
        summary,
    };
    (l, view)
}

fn render_pages(sheet: &AoeSheet, inch: f64, format: PageFormat) -> (Vec<RgbImage>, SheetLayout) {
    let cfgs = sheet_configs(sheet, inch, format);
    let items: Vec<(RgbImage, f64)> = cfgs.iter().map(|c| c.render()).collect();
    let gap = if sheet.gap_in.is_finite() { sheet.gap_in.max(0.0) } else { SHEET_GAP_IN };
    aoe::compose_sheets(&items, format, inch.max(0.05), gap)
}

impl Service {
    /// Built-in presets first, then user (★) presets by name.
    pub async fn aoe_presets_list(&self) -> Result<Vec<AoePreset>> {
        self.blocking(|s| {
            let mut out: Vec<AoePreset> = aoe::PRESETS
                .iter()
                .map(|&(name, shape, a, b)| AoePreset {
                    name: name.into(),
                    builtin: true,
                    config: AoeConfig {
                        shape,
                        a_ft: a,
                        b_ft: b,
                        label: aoe::default_label(shape, a, b),
                        ..Default::default()
                    },
                })
                .collect();
            for (name, v) in s.list_aoe_presets()? {
                // Python configs are a subset; missing keys take defaults.
                let config: AoeConfig = serde_json::from_value(v).unwrap_or_default();
                out.push(AoePreset { name, builtin: false, config });
            }
            Ok(out)
        })
        .await
    }

    /// Saves a user preset. An existing name is only replaced with `overwrite`
    /// (the UI asks first, as Python did).
    pub async fn aoe_presets_save(
        &self,
        name: String,
        config: AoeConfig,
        overwrite: bool,
    ) -> Result<AoePreset> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(ServiceError::InvalidInput("preset name must not be empty".into()));
        }
        self.blocking(move |s| {
            if !overwrite && s.list_aoe_presets()?.iter().any(|(n, _)| *n == name) {
                return Err(ServiceError::InvalidInput(format!(
                    "a preset named '{name}' exists; overwrite it?"
                )));
            }
            let config = AoeConfig { name: name.clone(), ..config };
            s.upsert_aoe_preset(&name, &serde_json::to_value(&config).map_err(internal)?)?;
            Ok(AoePreset { name, builtin: false, config })
        })
        .await
    }

    pub async fn aoe_presets_delete(&self, name: String) -> Result<()> {
        if aoe::PRESETS.iter().any(|p| p.0 == name) {
            return Err(ServiceError::InvalidInput("built-in presets cannot be deleted".into()));
        }
        self.blocking(move |s| Ok(s.delete_aoe_preset(&name)?)).await
    }

    /// Renders the template as PNG (at `clamp(150 × inch, 60, 300)` px/square).
    pub async fn aoe_render(&self, config: AoeConfig) -> Result<Vec<u8>> {
        let c = check_size(&config)?;
        self.blocking(move |_| {
            let (img, _) = c.render();
            imaging::encode_png_fast(&image::DynamicImage::ImageRgb8(img).to_rgba8())
                .map_err(internal)
        })
        .await
    }

    pub async fn aoe_info(&self, config: AoeConfig) -> Result<AoeInfo> {
        Ok(info(&config))
    }

    pub async fn aoe_export_png(&self, config: AoeConfig, path: PathBuf) -> Result<PathBuf> {
        let c = check_size(&config)?;
        self.blocking(move |_| {
            let (img, _) = c.render();
            let png = imaging::encode_png(&image::DynamicImage::ImageRgb8(img).to_rgba8())
                .map_err(internal)?;
            std::fs::write(&path, png)?;
            Ok(path)
        })
        .await
    }

    /// Exports the template tiled at true scale (job result: `{path, pages}`).
    pub fn aoe_export_template_pdf(&self, config: AoeConfig, path: PathBuf) -> JobId {
        self.spawn_job("aoe template export", move |ctx| {
            let c = check_size(&config)?;
            ctx.progress(0.05, "rendering template");
            let (img, cell) = c.render();
            ctx.check()?;
            let plan = template_tiles(&c, i64::from(img.width()), i64::from(img.height()), cell);
            let pdf = sleuth_pdf::map_tiles::render(
                &img,
                &plan.tiles,
                c.page_format.size_pt(),
                0.0,
                None,
                &mut |p| {
                    ctx.progress(0.2 + 0.75 * p, "rendering pages");
                    !ctx.is_cancelled()
                },
            )
            .map_err(pdf_err)?
            .ok_or_else(|| ServiceError::InvalidInput("nothing to export".into()))?;
            std::fs::write(&path, pdf)?;
            Ok(serde_json::json!({ "path": path, "pages": plan.tiles.len() }))
        })
    }

    pub async fn aoe_sheet_get(&self) -> Result<AoeSheet> {
        self.blocking(load).await
    }

    async fn aoe_sheet_mutate<F>(&self, f: F) -> Result<AoeSheet>
    where
        F: FnOnce(&mut AoeSheet) -> Result<()> + Send + 'static,
    {
        self.blocking(move |s| {
            let mut sheet = load(s)?;
            f(&mut sheet)?;
            save(s, &sheet)?;
            Ok(sheet)
        })
        .await
    }

    pub async fn aoe_sheet_add(&self, config: AoeConfig) -> Result<AoeSheet> {
        let mut c = check_size(&config)?;
        c.name = c.display_name();
        self.aoe_sheet_mutate(move |sheet| {
            sheet.items.push(AoeSheetItem { id: uuid::Uuid::new_v4().to_string(), config: c });
            Ok(())
        })
        .await
    }

    pub async fn aoe_sheet_remove(&self, ids: Vec<String>) -> Result<AoeSheet> {
        self.aoe_sheet_mutate(move |sheet| {
            sheet.items.retain(|i| !ids.contains(&i.id));
            Ok(())
        })
        .await
    }

    pub async fn aoe_sheet_clear(&self) -> Result<AoeSheet> {
        self.aoe_sheet_mutate(|sheet| {
            sheet.items.clear();
            Ok(())
        })
        .await
    }

    pub async fn aoe_sheet_set_gap(&self, gap_in: f64) -> Result<AoeSheet> {
        if !gap_in.is_finite() || gap_in < 0.0 {
            return Err(ServiceError::InvalidInput("gap must be ≥ 0".into()));
        }
        self.aoe_sheet_mutate(move |sheet| {
            sheet.gap_in = gap_in;
            Ok(())
        })
        .await
    }

    pub async fn aoe_sheet_layout(&self, inch: f64, format: PageFormat) -> Result<AoeSheetLayout> {
        self.blocking(move |s| Ok(sheet_layout(&load(s)?, inch, format).1)).await
    }

    /// One composed sheet page as PNG, downscaled to at most `max_px`.
    pub async fn aoe_sheet_preview(
        &self,
        inch: f64,
        format: PageFormat,
        page: usize,
        max_px: u32,
    ) -> Result<Vec<u8>> {
        self.blocking(move |s| {
            let (pages, _) = render_pages(&load(s)?, inch, format);
            let img = pages.get(page).ok_or_else(|| {
                ServiceError::InvalidInput(format!("page {} of {}", page + 1, pages.len()))
            })?;
            let rgba = image::DynamicImage::ImageRgb8(img.clone());
            let small = imaging::downscale(&rgba, max_px.clamp(64, 4096));
            imaging::encode_png_fast(&small).map_err(internal)
        })
        .await
    }

    /// Exports the sheet, one full page per raster (job result:
    /// `{path, pages, skipped}`). Items too big for a page are reported.
    pub fn aoe_sheet_export_pdf(&self, inch: f64, format: PageFormat, path: PathBuf) -> JobId {
        self.spawn_job("aoe sheet export", move |ctx| {
            let store = ctx.service().store();
            let sheet = load(&store)?;
            if sheet.items.is_empty() {
                return Err(ServiceError::InvalidInput("add at least one template to the sheet".into()));
            }
            ctx.progress(0.05, "rendering templates");
            let (pages, layout) = render_pages(&sheet, inch, format);
            let skipped: Vec<String> = layout.skipped.iter().map(|&i| sheet.items[i].config.display_name()).collect();
            if pages.is_empty() {
                return Err(ServiceError::InvalidInput(format!(
                    "none of the templates fit on a single sheet (export the big ones individually): {}",
                    skipped.join(", ")
                )));
            }
            ctx.check()?;
            let n = pages.len();
            let pdf = sleuth_pdf::image_pages::render(pages, format, &mut |p| {
                ctx.progress(0.4 + 0.55 * p, "writing pages");
                !ctx.is_cancelled()
            })
            .map_err(pdf_err)?;
            std::fs::write(&path, pdf)?;
            Ok(serde_json::json!({ "path": path, "pages": n, "skipped": skipped }))
        })
    }
}

fn pdf_err(e: PdfError) -> ServiceError {
    match e {
        PdfError::Cancelled => ServiceError::Cancelled,
        other => internal(other),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sleuth_core::aoe::Shape;

    use super::*;
    use crate::{JobState, temp_service};

    async fn wait(svc: &Service, id: &str) -> crate::JobStatus {
        for _ in 0..4000 {
            let st = svc.job_status(id).unwrap();
            if st.state != JobState::Running {
                return st;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("job timeout");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn presets_builtin_and_user() {
        let (_d, svc) = temp_service();
        let list = svc.aoe_presets_list().await.unwrap();
        assert_eq!(list.len(), 11);
        assert_eq!(list[1].config.label, "20 ft radius");
        let cfg =
            AoeConfig { shape: Shape::Cone, a_ft: 25.0, fill_alpha: 120, ..Default::default() };
        svc.aoe_presets_save("Dragon breath".into(), cfg.clone(), false).await.unwrap();
        assert!(
            svc.aoe_presets_save("Dragon breath".into(), cfg.clone(), false).await.is_err(),
            "asks first"
        );
        svc.aoe_presets_save("Dragon breath".into(), cfg, true).await.unwrap();
        let list = svc.aoe_presets_list().await.unwrap();
        let user = list.last().unwrap();
        assert_eq!((user.builtin, user.config.fill_alpha, user.config.a_ft), (false, 120, 25.0));
        assert!(svc.aoe_presets_delete(list[0].name.clone()).await.is_err());
        svc.aoe_presets_delete("Dragon breath".into()).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn render_info_and_template_export() {
        let (d, svc) = temp_service();
        let fireball = AoeConfig::default();
        let i = info(&fireball);
        // 40 ft at 5 ft/square and 1 in/square = 8 in → fits one A4 page
        assert_eq!((i.footprint_ft, i.inches, i.pages), ((40.0, 40.0), (8.0, 8.0), 1));
        assert_eq!(
            i.summary,
            "Footprint: 40 × 40 ft  ·  8.0\" × 8.0\" on paper  ·  1 page(s) (1×1)"
        );
        let png = svc.aoe_render(fireball.clone()).await.unwrap();
        assert_eq!(imaging::dimensions(&png), Some((1216, 1216)));
        assert!(svc.aoe_render(AoeConfig { a_ft: 0.0, ..Default::default() }).await.is_err());
        let out = d.path().join("t.pdf");
        let st = wait(
            &svc,
            &svc.aoe_export_template_pdf(AoeConfig { a_ft: 60.0, ..fireball.clone() }, out.clone()),
        )
        .await;
        assert_eq!(st.state, JobState::Done, "{:?}", st.error);
        assert!(st.result.unwrap()["pages"].as_u64().unwrap() > 1);

        let png = d.path().join("t.png");
        assert_eq!(svc.aoe_export_png(fireball.clone(), png.clone()).await.unwrap(), png);
        let img = image::open(&png).unwrap();
        assert_eq!((img.width(), img.height()), (1216, 1216));
        assert!(svc.aoe_export_png(AoeConfig { a_ft: 0.0, ..fireball }, png).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sheet_packs_reports_oversized_and_exports() {
        let (d, svc) = temp_service();
        svc.aoe_sheet_add(AoeConfig { shape: Shape::Square, a_ft: 15.0, ..Default::default() })
            .await
            .unwrap();
        svc.aoe_sheet_add(AoeConfig { shape: Shape::Cone, a_ft: 15.0, ..Default::default() })
            .await
            .unwrap();
        let sheet = svc
            .aoe_sheet_add(AoeConfig {
                shape: Shape::Line,
                a_ft: 100.0,
                label: "Lightning".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(sheet.items.len(), 3);
        let l = svc.aoe_sheet_layout(1.0, PageFormat::A4).await.unwrap();
        assert_eq!((l.pages, l.placed.len()), (1, 2));
        assert_eq!(l.skipped, vec!["Lightning".to_string()]);
        assert!(l.summary.contains("too big for one sheet: Lightning"));
        let png = svc.aoe_sheet_preview(1.0, PageFormat::A4, 0, 300).await.unwrap();
        assert!(imaging::dimensions(&png).unwrap().1 <= 300);

        let out = d.path().join("sheet.pdf");
        let st = wait(&svc, &svc.aoe_sheet_export_pdf(1.0, PageFormat::A4, out.clone())).await;
        assert_eq!(st.state, JobState::Done, "{:?}", st.error);
        let r = st.result.unwrap();
        assert_eq!((r["pages"].as_u64(), r["skipped"][0].as_str()), (Some(1), Some("Lightning")));

        let id = sheet.items[0].id.clone();
        assert_eq!(svc.aoe_sheet_remove(vec![id]).await.unwrap().items.len(), 2);
        svc.aoe_sheet_set_gap(0.5).await.unwrap();
        let cleared = svc.aoe_sheet_clear().await.unwrap();
        assert!(cleared.items.is_empty());
        assert_eq!(cleared.gap_in, 0.5);
        let st = wait(&svc, &svc.aoe_sheet_export_pdf(1.0, PageFormat::A4, out)).await;
        assert_eq!(st.state, JobState::Failed);
    }
}
