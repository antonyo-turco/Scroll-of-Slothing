//! Map Paginator service. Spec: docs/features/map-paginator.md, map-filters.md.
//!
//! The whole editor state is a [`MapDoc`] (all coordinates in full-resolution
//! image pixels). The UI keeps it client-side and autosaves it to the
//! `map_paginator` workspace; MCP tools load, change and save that same doc.
//! Previews work on a ≤2000 px proxy; export and Save as Asset use full
//! resolution inside cancellable jobs.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use image::RgbImage;
use serde::{Deserialize, Serialize};
use sleuth_core::imaging;
use sleuth_core::maps::degrid;
use sleuth_core::maps::filters::{self, FilterStep};
use sleuth_core::maps::grid::{self, DEFAULT_DPI, GridModel, GridStyle};
use sleuth_core::maps::homography::{self, Pt};
use sleuth_core::maps::tiling::{self, FitCheck, Tile, TileParams};
use sleuth_core::units::{PT_PER_INCH, PageFormat};
use sleuth_pdf::{CutLine, PdfError};
use sleuth_store::{AssetImages, AssetPatch, NewAsset, Store};

use crate::library::store_encoded;
use crate::tokens::{SourceImage, source_info, store_source};
use crate::{Asset, JobCtx, JobId, Result, Service, ServiceError};

const KEY: &str = "map_paginator";
/// Max side of the working proxy (Python `WORK_MAX`).
pub const WORK_MAX: u32 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SizingMode {
    RowsCols,
    Reference,
    #[default]
    Ppi,
    CellSide,
}

/// Full Map Paginator state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapDoc {
    pub source: Option<SourceImage>,
    /// The image before the last de-warp (for undo).
    pub pre_warp: Option<SourceImage>,
    pub name: String,
    /// Library asset this map was opened from.
    pub asset_id: Option<i64>,
    pub detected_dpi: f64,
    pub dpi_detected: bool,

    pub mode: SizingMode,
    pub rows: i64,
    pub cols: i64,
    pub square_cells: bool,
    pub ref_n: i64,
    /// Reference block corners (any order), image px.
    pub ref_quad: Option<[Pt; 4]>,
    pub ppi: f64,
    pub side: f64,
    /// Manual offset added on top of the mode's own offset.
    pub offset_x: f64,
    pub offset_y: f64,

    pub color: [u8; 3],
    pub thickness: u32,
    pub opacity: u8,
    pub visible: bool,
    pub style: GridStyle,

    pub remove_grid: bool,
    pub filters: Vec<FilterStep>,

    pub format: PageFormat,
    pub square_in: f64,
    pub calibration_mm: f64,
    pub margin_in: f64,
    pub overlap_in: f64,
    pub align_to_grid: bool,
    pub cut_border: bool,
    pub fit_w_ft: f64,
    pub fit_h_ft: f64,

    /// `(x, y, w, h)` print region, image px; `None` = whole map.
    pub region: Option<(f64, f64, f64, f64)>,
    /// Page grid shift, image px.
    pub page_offset: (f64, f64),
    /// Excluded pages as `(col, row)`.
    pub excluded: Vec<(u32, u32)>,
}

impl Default for MapDoc {
    fn default() -> Self {
        Self {
            source: None,
            pre_warp: None,
            name: String::new(),
            asset_id: None,
            detected_dpi: DEFAULT_DPI,
            dpi_detected: false,
            mode: SizingMode::Ppi,
            rows: 35,
            cols: 55,
            square_cells: true,
            ref_n: 3,
            ref_quad: None,
            ppi: DEFAULT_DPI,
            side: DEFAULT_DPI,
            offset_x: 0.0,
            offset_y: 0.0,
            color: [40, 40, 40],
            thickness: 1,
            opacity: 160,
            visible: true,
            style: GridStyle::Solid,
            remove_grid: false,
            filters: filters::default_pipeline(),
            format: PageFormat::A4,
            square_in: 1.0,
            calibration_mm: 25.4,
            margin_in: 0.0,
            overlap_in: 0.0,
            align_to_grid: false,
            cut_border: false,
            fit_w_ft: 3.0,
            fit_h_ft: 2.0,
            region: None,
            page_offset: (0.0, 0.0),
            excluded: Vec::new(),
        }
    }
}

impl MapDoc {
    fn dims(&self) -> (u32, u32) {
        self.source.as_ref().map_or((0, 0), |s| (s.width, s.height))
    }

    /// The sizing mode's own `(cell_w, cell_h, off_x, off_y)` (before the
    /// manual offset). Exposed so the UI can solve nudge-align.
    pub fn base_grid(&self) -> grid::GridFit {
        match self.mode {
            SizingMode::RowsCols => {
                let (cw, ch, ox, oy) = grid::from_rows_cols(self.dims(), self.rows, self.cols);
                if self.square_cells {
                    let m = cw.min(ch);
                    (m, m, ox, oy)
                } else {
                    (cw, ch, ox, oy)
                }
            }
            SizingMode::Reference => {
                if self.pre_warp.is_some() {
                    grid::from_cell_side(self.side)
                } else if let Some(q) = &self.ref_quad {
                    grid::from_quad(q, self.ref_n)
                } else {
                    (self.detected_dpi, self.detected_dpi, 0.0, 0.0)
                }
            }
            SizingMode::Ppi => grid::from_ppi(self.ppi, 1.0),
            SizingMode::CellSide => grid::from_cell_side(self.side),
        }
    }

    /// The grid in full-resolution pixels.
    pub fn grid(&self) -> GridModel {
        let (cw, ch, ox, oy) = self.base_grid();
        GridModel {
            cell_w: cw,
            cell_h: ch,
            offset_x: ox + finite(self.offset_x),
            offset_y: oy + finite(self.offset_y),
            color: self.color,
            thickness: self.thickness.clamp(1, 10),
            opacity: self.opacity,
            visible: self.visible,
            style: self.style,
        }
    }

    /// Proxy scale (Python `_work_scale`).
    pub fn work_scale(&self) -> f64 {
        let (w, h) = self.dims();
        (f64::from(WORK_MAX) / f64::from(w.max(h).max(1))).min(1.0)
    }

    /// The grid scaled onto the proxy (thickness rounded, at least 1).
    pub fn work_grid(&self) -> GridModel {
        let g = self.grid();
        let s = self.work_scale();
        GridModel {
            cell_w: g.cell_w * s,
            cell_h: g.cell_h * s,
            offset_x: g.offset_x * s,
            offset_y: g.offset_y * s,
            thickness: ((f64::from(g.thickness) * s).round_ties_even() as u32).max(1),
            ..g
        }
    }

    /// Calibrated printed square size, inches.
    pub fn printed_square_in(&self) -> f64 {
        let sq =
            if self.square_in.is_finite() && self.square_in > 0.0 { self.square_in } else { 1.0 };
        sq * tiling::calibration_factor(self.calibration_mm)
    }

    pub fn tile_params(&self) -> TileParams {
        let (w, h) = self.dims();
        let g = self.grid();
        TileParams {
            image_w: w,
            image_h: h,
            region: self.region,
            cell_w: g.cell_w,
            cell_h: g.cell_h,
            square_in: self.printed_square_in(),
            format: self.format,
            margin_pt: finite(self.margin_in).max(0.0) * PT_PER_INCH,
            offset: self.page_offset,
            overlap_in: finite(self.overlap_in),
            align_to_grid: self.align_to_grid,
            grid_offset: (g.offset_x, g.offset_y),
        }
    }

    fn require_source(&self) -> Result<&SourceImage> {
        self.source.as_ref().ok_or_else(|| ServiceError::InvalidInput("load a map first".into()))
    }
}

fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

/// Everything the UI and MCP need to show the pagination.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MapLayout {
    pub grid: GridModel,
    pub base_offset: (f64, f64),
    pub work_scale: f64,
    pub proxy_w: u32,
    pub proxy_h: u32,
    pub scale_x: f64,
    pub scale_y: f64,
    pub tiles: Vec<Tile>,
    pub cols: u32,
    pub rows: u32,
    pub selected: usize,
    pub fit: Option<FitCheck>,
    /// "X/Y selected · N page(s) · c×r grid · 1 square = s\"" (Python label).
    pub summary: String,
    pub page_w_pt: f64,
    pub page_h_pt: f64,
    pub margin_pt: f64,
}

pub fn compute_layout(doc: &MapDoc) -> MapLayout {
    let g = doc.grid();
    let (_, _, bx, by) = doc.base_grid();
    let plan = tiling::plan(&doc.tile_params());
    let (w, h) = doc.dims();
    let s = doc.work_scale();
    let total = plan.tiles.len();
    let excluded = plan.tiles.iter().filter(|t| doc.excluded.contains(&(t.col, t.row))).count();
    let (rw, rh) = doc.region.map_or((f64::from(w), f64::from(h)), |r| (r.2, r.3));
    let fit = tiling::fit_check(
        (rw, rh),
        (g.cell_w, g.cell_h),
        doc.square_in,
        (doc.fit_w_ft, doc.fit_h_ft),
    );
    let summary = if total == 0 {
        String::new()
    } else {
        let cols = plan.tiles.iter().map(|t| t.col).max().unwrap_or(0) + 1;
        let rows = plan.tiles.iter().map(|t| t.row).max().unwrap_or(0) + 1;
        let prefix = if excluded > 0 {
            format!("{}/{total} selected · ", total - excluded)
        } else {
            String::new()
        };
        format!(
            "{prefix}{total} page(s) · {cols}×{rows} grid · 1 square = {}\"",
            sleuth_core::aoe::fmt_g(doc.square_in)
        )
    };
    let (pw, ph) = doc.format.size_pt();
    MapLayout {
        base_offset: (bx, by),
        work_scale: s,
        proxy_w: ((f64::from(w) * s) as u32).max(1),
        proxy_h: ((f64::from(h) * s) as u32).max(1),
        scale_x: plan.scale_x,
        scale_y: plan.scale_y,
        cols: plan.cols,
        rows: plan.rows,
        selected: total - excluded,
        tiles: plan.tiles,
        fit,
        summary,
        page_w_pt: pw,
        page_h_pt: ph,
        margin_pt: finite(doc.margin_in).max(0.0) * PT_PER_INCH,
        grid: g,
    }
}

/// Small caches so repeated previews skip the decode/downscale and the FFT.
#[derive(Default)]
pub(crate) struct MapCache {
    proxy: Mutex<Option<(String, Arc<RgbImage>)>>,
    degrid: Mutex<Option<(String, Arc<RgbImage>)>>,
}

fn load(s: &Store) -> Result<Option<MapDoc>> {
    Ok(s.workspace_get::<MapDoc>(KEY)?)
}

fn save(s: &Store, doc: &MapDoc) -> Result<()> {
    s.workspace_set(KEY, doc)?;
    s.record_change("map_paginator", KEY, "update")?;
    Ok(())
}

fn internal(e: impl std::fmt::Display) -> ServiceError {
    ServiceError::Internal(e.to_string())
}

fn pdf_err(e: PdfError) -> ServiceError {
    match e {
        PdfError::Cancelled => ServiceError::Cancelled,
        other => internal(other),
    }
}

impl Service {
    fn full_rgb(&self, s: &Store, hash: &str) -> Result<RgbImage> {
        let img = imaging::decode(&s.read_blob(hash)?)
            .map_err(|e| ServiceError::InvalidInput(e.to_string()))?;
        Ok(img.to_rgb8())
    }

    /// The ≤2000 px working copy (Lanczos, truncated size like Python).
    fn proxy(&self, s: &Store, src: &SourceImage) -> Result<Arc<RgbImage>> {
        let cache = &self.inner.maps.proxy;
        if let Some((h, img)) = cache.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
            && *h == src.hash
        {
            return Ok(img.clone());
        }
        let full = self.full_rgb(s, &src.hash)?;
        let scale = (f64::from(WORK_MAX) / f64::from(src.width.max(src.height).max(1))).min(1.0);
        let img = if scale < 1.0 {
            let w = ((f64::from(src.width) * scale) as u32).max(1);
            let h = ((f64::from(src.height) * scale) as u32).max(1);
            imaging::resize_rgb(&full, w, h)
        } else {
            full
        };
        let img = Arc::new(img);
        *cache.lock().unwrap_or_else(|p| p.into_inner()) = Some((src.hash.clone(), img.clone()));
        Ok(img)
    }

    /// Proxy → embedded-grid removal (if chosen) → filters.
    fn processed_proxy(&self, s: &Store, doc: &MapDoc) -> Result<RgbImage> {
        let src = doc.require_source()?;
        let proxy = self.proxy(s, src)?;
        let base: Arc<RgbImage> = if doc.remove_grid {
            let g = doc.work_grid();
            let key = format!("{}:{}:{}", src.hash, g.cell_w, g.cell_h);
            let cache = &self.inner.maps.degrid;
            let hit = cache
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .as_ref()
                .filter(|(k, _)| *k == key)
                .map(|c| c.1.clone());
            match hit {
                Some(img) => img,
                None => {
                    let out = degrid::remove_grid_lines(
                        &proxy,
                        g.cell_w,
                        g.cell_h,
                        degrid::DEFAULT_STRENGTH,
                    )
                    .map(Arc::new)
                    .unwrap_or_else(|| proxy.clone());
                    *cache.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, out.clone()));
                    out
                }
            }
        } else {
            proxy
        };
        Ok(if filters::is_identity(&doc.filters) {
            (*base).clone()
        } else {
            filters::apply_pipeline(&base, &doc.filters)
        })
    }

    /// Full-resolution composite for export: removal → filters → grid overlay.
    fn full_composite(&self, s: &Store, doc: &MapDoc, ctx: &JobCtx) -> Result<RgbImage> {
        let src = doc.require_source()?;
        ctx.progress(0.02, "loading map");
        let mut img = self.full_rgb(s, &src.hash)?;
        ctx.check()?;
        let g = doc.grid();
        if doc.remove_grid {
            ctx.progress(0.08, "removing embedded grid");
            if let Some(out) =
                degrid::remove_grid_lines(&img, g.cell_w, g.cell_h, degrid::DEFAULT_STRENGTH)
            {
                img = out;
            }
            ctx.check()?;
        }
        if !filters::is_identity(&doc.filters) {
            ctx.progress(0.2, "applying filters");
            img = filters::apply_pipeline(&img, &doc.filters);
            ctx.check()?;
        }
        if g.visible {
            ctx.progress(0.3, "drawing grid");
            let mut rgba = image::DynamicImage::ImageRgb8(img).to_rgba8();
            grid::render_overlay(&mut rgba, &g);
            img = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
        }
        Ok(img)
    }

    /// The autosaved doc, if a map was ever opened.
    pub async fn map_get(&self) -> Result<Option<MapDoc>> {
        self.blocking(load).await
    }

    /// Replaces the autosaved doc (UI autosave; MCP edits).
    pub async fn map_set(&self, doc: MapDoc) -> Result<MapDoc> {
        self.blocking(move |s| {
            save(s, &doc)?;
            Ok(doc)
        })
        .await
    }

    /// Starts a fresh doc on a stored image: print defaults from Settings, DPI
    /// from metadata (70 otherwise). Saved as the current doc.
    pub async fn map_open_source(&self, hash: String, name: Option<String>) -> Result<MapDoc> {
        self.blocking(move |s| {
            let src = source_info(s, &hash)?;
            let dpi = imaging::detect_dpi(&s.read_blob(&hash)?);
            let st = s.load_settings()?;
            let d = dpi.unwrap_or(DEFAULT_DPI);
            let r1 = (d * 10.0).round_ties_even() / 10.0;
            let doc = MapDoc {
                source: Some(src),
                name: name.unwrap_or_default(),
                detected_dpi: d,
                dpi_detected: dpi.is_some(),
                ppi: r1,
                side: r1,
                format: st.map_page_format,
                square_in: st.map_square_in,
                calibration_mm: st.map_calibration_mm,
                margin_in: st.map_margin_in,
                overlap_in: st.map_overlap_in,
                align_to_grid: st.map_align_to_grid,
                cut_border: st.map_cut_border,
                ..Default::default()
            };
            save(s, &doc)?;
            Ok(doc)
        })
        .await
    }

    /// Opens a map file (stores it as a source first).
    pub async fn map_open_file(&self, path: PathBuf) -> Result<MapDoc> {
        let name = path.file_stem().map(|n| n.to_string_lossy().into_owned());
        let src = self.image_source_from_path(path).await?;
        self.map_open_source(src.hash, name).await
    }

    /// Opens a library asset, restoring the full paginator state when it was
    /// saved by this app (Python restored only the image).
    pub async fn map_open_asset(&self, id: i64) -> Result<MapDoc> {
        let (a, stored) = self
            .blocking(move |s| {
                let a = s.get_asset(id)?;
                let stored =
                    a.params_json.as_deref().and_then(|j| serde_json::from_str::<MapDoc>(j).ok());
                Ok((a, stored))
            })
            .await?;
        let mut doc = match stored {
            Some(d) if d.source.as_ref().is_some_and(|s| s.hash == a.original_blob) => d,
            _ => self.map_open_source(a.original_blob.clone(), None).await?,
        };
        doc.name = a.name;
        doc.asset_id = Some(a.id);
        self.map_set(doc).await
    }

    /// The processed working proxy (removal + filters, no grid) as PNG.
    pub async fn map_preview(&self, doc: MapDoc) -> Result<Vec<u8>> {
        let svc = self.clone();
        self.blocking(move |s| {
            let img = svc.processed_proxy(s, &doc)?;
            imaging::encode_png_fast(&image::DynamicImage::ImageRgb8(img).to_rgba8())
                .map_err(internal)
        })
        .await
    }

    pub async fn map_layout(&self, doc: MapDoc) -> Result<MapLayout> {
        Ok(compute_layout(&doc))
    }

    /// One printed page as it will come out (at `px_per_pt` × page points),
    /// rendered from the proxy with the grid overlay; excluded pages are greyed.
    pub async fn map_preview_page(
        &self,
        doc: MapDoc,
        index: usize,
        px_per_pt: f64,
    ) -> Result<Vec<u8>> {
        let svc = self.clone();
        self.blocking(move |s| {
            let layout = compute_layout(&doc);
            let tile = layout
                .tiles
                .get(index)
                .ok_or_else(|| {
                    ServiceError::InvalidInput(format!(
                        "page {} of {}",
                        index + 1,
                        layout.tiles.len()
                    ))
                })?
                .clone();
            let mut proxy =
                image::DynamicImage::ImageRgb8(svc.processed_proxy(s, &doc)?).to_rgba8();
            grid::render_overlay(&mut proxy, &doc.work_grid());
            let proxy = image::DynamicImage::ImageRgba8(proxy).to_rgb8();
            let k = px_per_pt.clamp(0.1, 4.0);
            let (pw, ph) = ((layout.page_w_pt * k) as u32, (layout.page_h_pt * k) as u32);
            let mut page = RgbImage::from_pixel(pw.max(1), ph.max(1), image::Rgb([255, 255, 255]));
            let ws = layout.work_scale;
            let (l, t, r, b) = tile.crop;
            let bx = (
                l * ws,
                t * ws,
                (r * ws).min(f64::from(proxy.width())),
                (b * ws).min(f64::from(proxy.height())),
            );
            let (w, h) = (((tile.w_pt * k) as u32).max(1), ((tile.h_pt * k) as u32).max(1));
            if bx.2 > bx.0 && bx.3 > bx.1 {
                let mut sub = imaging::resize_box_rgb(&proxy, bx, w, h);
                if doc.excluded.contains(&(tile.col, tile.row)) {
                    for p in sub.pixels_mut() {
                        p.0 = p.0.map(|v| v / 2 + 40);
                    }
                }
                let x = ((layout.margin_pt + tile.page_x_pt) * k) as i64;
                let y = ((layout.margin_pt + tile.page_y_pt) * k) as i64;
                image::imageops::replace(&mut page, &sub, x, y);
            }
            imaging::encode_png_fast(&image::DynamicImage::ImageRgb8(page).to_rgba8())
                .map_err(internal)
        })
        .await
    }

    /// Rectifies the whole map from the reference block (full resolution).
    /// The result becomes the source; the previous one is kept for undo.
    pub async fn map_dewarp(&self, doc: MapDoc) -> Result<MapDoc> {
        let quad = doc.ref_quad.ok_or_else(|| {
            ServiceError::InvalidInput("draw a reference square over a known block first, then drag its 4 corners onto the block".into())
        })?;
        let src = doc.require_source()?.clone();
        let svc = self.clone();
        self.blocking(move |s| {
            let full = svc.full_rgb(s, &src.hash)?;
            let (out, plan) = homography::dewarp(&full, &quad, doc.ref_n).map_err(|e| {
                ServiceError::InvalidInput(format!("could not de-warp the map: {e}"))
            })?;
            let png = imaging::encode_png_fast(&image::DynamicImage::ImageRgb8(out).to_rgba8())
                .map_err(internal)?;
            let new_src = store_source(s, &png)?;
            let mut d = doc;
            d.pre_warp = Some(src);
            d.source = Some(new_src);
            d.side = f64::from(plan.cell_px);
            d.offset_x = plan.offset_x;
            d.offset_y = plan.offset_y;
            d.detected_dpi = f64::from(plan.cell_px);
            d.ref_quad = None;
            d.region = None;
            d.excluded.clear();
            d.page_offset = (0.0, 0.0);
            save(s, &d)?;
            Ok(d)
        })
        .await
    }

    pub async fn map_undo_dewarp(&self, doc: MapDoc) -> Result<MapDoc> {
        let prev = doc
            .pre_warp
            .clone()
            .ok_or_else(|| ServiceError::InvalidInput("nothing to undo".into()))?;
        self.blocking(move |s| {
            let mut d = doc;
            d.source = Some(prev);
            d.pre_warp = None;
            d.ref_quad = None;
            d.region = None;
            d.excluded.clear();
            d.page_offset = (0.0, 0.0);
            d.offset_x = 0.0;
            d.offset_y = 0.0;
            save(s, &d)?;
            Ok(d)
        })
        .await
    }

    /// Checks shared by export and save; returns the included tiles.
    fn printable_tiles(doc: &MapDoc) -> Result<Vec<Tile>> {
        doc.require_source()?;
        let g = doc.grid();
        if g.cell_w <= 0.0 || g.cell_h <= 0.0 {
            return Err(ServiceError::InvalidInput("set a valid grid size first".into()));
        }
        let tiles: Vec<Tile> = compute_layout(doc)
            .tiles
            .into_iter()
            .filter(|t| !doc.excluded.contains(&(t.col, t.row)))
            .collect();
        if tiles.is_empty() {
            return Err(ServiceError::InvalidInput(
                "no pages selected — include at least one page".into(),
            ));
        }
        Ok(tiles)
    }

    fn render_map_pdf(
        &self,
        s: &Store,
        doc: &MapDoc,
        ctx: &JobCtx,
    ) -> Result<(Vec<u8>, RgbImage, usize)> {
        let tiles = Self::printable_tiles(doc)?;
        let composite = self.full_composite(s, doc, ctx)?;
        let cut =
            doc.cut_border.then(|| CutLine::from_settings(&s.load_settings().unwrap_or_default()));
        let margin = finite(doc.margin_in).max(0.0) * PT_PER_INCH;
        let pdf = sleuth_pdf::map_tiles::render(
            &composite,
            &tiles,
            doc.format,
            margin,
            cut.as_ref(),
            &mut |p| {
                ctx.progress(0.35 + 0.6 * p, "rendering pages");
                !ctx.is_cancelled()
            },
        )
        .map_err(pdf_err)?
        .ok_or_else(|| {
            ServiceError::InvalidInput(
                "nothing to paginate — check the grid and page selection".into(),
            )
        })?;
        Ok((pdf, composite, tiles.len()))
    }

    /// Exports the tiled PDF (job result: `{path, pages}`).
    pub fn map_export_pdf(&self, doc: MapDoc, path: PathBuf) -> JobId {
        let svc = self.clone();
        self.spawn_job("map export", move |ctx| {
            let store = svc.store();
            let (pdf, _, pages) = svc.render_map_pdf(&store, &doc, &ctx)?;
            std::fs::write(&path, pdf)?;
            Ok(serde_json::json!({ "path": path, "pages": pages }))
        })
    }

    /// Saves the map as a library asset of kind `map`: the (de-warped)
    /// original, the gridded composite, the PDF and the whole doc as params.
    /// `update` replaces that asset instead of creating one. Job result: the asset.
    pub fn map_save_asset(&self, doc: MapDoc, name: String, update: Option<i64>) -> JobId {
        let svc = self.clone();
        self.spawn_job("map save", move |ctx| {
            let store = svc.store();
            let name =
                if name.trim().is_empty() { "Map".to_string() } else { name.trim().to_string() };
            let (pdf, composite, _) = svc.render_map_pdf(&store, &doc, &ctx)?;
            ctx.progress(0.96, "saving to library");
            let src = doc.require_source()?;
            let original = store.read_blob(&src.hash)?;
            let rendered =
                imaging::encode_png_fast(&image::DynamicImage::ImageRgb8(composite).to_rgba8())
                    .map_err(internal)?;
            let stored = store_encoded(&store, &original, &rendered)?;
            let pdf_blob = store.put_blob(&pdf, "application/pdf", None)?.hash;
            let mut saved = doc.clone();
            let id = match update {
                Some(id) => {
                    saved.asset_id = Some(id);
                    saved.name = name.clone();
                    store.update_asset_images(
                        id,
                        &AssetImages {
                            original_blob: stored.original_blob,
                            rendered_blob: stored.rendered_blob,
                            pdf_blob: Some(pdf_blob),
                            thumb_blob: Some(stored.thumb_blob),
                            preview_blob: Some(stored.preview_blob),
                            params_json: Some(serde_json::to_string(&saved).map_err(internal)?),
                        },
                    )?;
                    store.update_asset_meta(
                        id,
                        &AssetPatch { name: Some(name), ..Default::default() },
                    )?;
                    id
                }
                None => {
                    saved.name = name.clone();
                    let id = store.insert_asset(&NewAsset {
                        kind: "map".into(),
                        name,
                        aliases: String::new(),
                        size_label: "Medium".into(),
                        original_blob: stored.original_blob,
                        rendered_blob: stored.rendered_blob,
                        pdf_blob: Some(pdf_blob),
                        thumb_blob: Some(stored.thumb_blob),
                        preview_blob: Some(stored.preview_blob),
                        params_json: None,
                        legacy_id: None,
                        pinned: false,
                    })?;
                    saved.asset_id = Some(id);
                    let a = store.get_asset(id)?;
                    store.update_asset_images(
                        id,
                        &AssetImages {
                            original_blob: a.original_blob,
                            rendered_blob: a.rendered_blob,
                            pdf_blob: a.pdf_blob,
                            thumb_blob: a.thumb_blob,
                            preview_blob: a.preview_blob,
                            params_json: Some(serde_json::to_string(&saved).map_err(internal)?),
                        },
                    )?;
                    id
                }
            };
            // the editor now edits the saved asset
            save(&store, &saved)?;
            let asset: Asset = store.get_asset(id)?;
            serde_json::to_value(asset).map_err(internal)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

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

    /// A 700×500 map with a dark grid every 50 px.
    fn map_png(dir: &std::path::Path) -> PathBuf {
        let img = RgbImage::from_fn(700, 500, |x, y| {
            let v = if x % 50 == 0 || y % 50 == 0 { 30 } else { 200 - ((x + y) % 60) as u8 };
            image::Rgb([v, v, v / 2 + 60])
        });
        let p = dir.join("Crypt.png");
        img.save(&p).unwrap();
        p
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn open_layout_preview_and_page() {
        let (d, svc) = temp_service();
        let doc = svc.map_open_file(map_png(d.path())).await.unwrap();
        assert_eq!((doc.name.as_str(), doc.detected_dpi, doc.dpi_detected), ("Crypt", 70.0, false));
        assert_eq!(svc.map_get().await.unwrap().unwrap(), doc, "autosaved");

        let doc = MapDoc { mode: SizingMode::CellSide, side: 50.0, ..doc };
        let layout = svc.map_layout(doc.clone()).await.unwrap();
        // 1 in squares of 50 px → 14×10 in map; A4 usable 8.27×11.69 in → 2 pages
        assert_eq!((layout.cols, layout.rows, layout.tiles.len()), (2, 1, 2));
        assert_eq!(layout.summary, "2 page(s) · 2×1 grid · 1 square = 1\"");
        assert!(layout.fit.is_some());

        let png = svc.map_preview(MapDoc { remove_grid: true, ..doc.clone() }).await.unwrap();
        assert_eq!(imaging::dimensions(&png), Some((700, 500)));
        let page = svc.map_preview_page(doc.clone(), 1, 0.5).await.unwrap();
        assert!(imaging::dimensions(&page).is_some());
        assert!(svc.map_preview_page(doc, 5, 0.5).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn export_respects_excluded_pages_and_save_round_trips() {
        let (d, svc) = temp_service();
        let doc = svc.map_open_file(map_png(d.path())).await.unwrap();
        let doc = MapDoc { mode: SizingMode::CellSide, side: 50.0, excluded: vec![(1, 0)], ..doc };
        let out = d.path().join("map.pdf");
        let st = wait(&svc, &svc.map_export_pdf(doc.clone(), out.clone())).await;
        assert_eq!(st.state, JobState::Done, "{:?}", st.error);
        assert_eq!(st.result.unwrap()["pages"], 1);
        assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF"));

        let all_out = MapDoc { excluded: vec![(0, 0), (1, 0)], ..doc.clone() };
        let st = wait(&svc, &svc.map_export_pdf(all_out, out.clone())).await;
        assert!(st.error.unwrap().contains("no pages selected"));

        let st = wait(&svc, &svc.map_save_asset(doc.clone(), "Crypt".into(), None)).await;
        assert_eq!(st.state, JobState::Done, "{:?}", st.error);
        let asset: Asset = serde_json::from_value(st.result.unwrap()).unwrap();
        assert_eq!(asset.kind, "map");
        assert!(asset.pdf_blob.is_some());
        let reopened = svc.map_open_asset(asset.id).await.unwrap();
        assert_eq!(
            (reopened.side, reopened.excluded.clone(), reopened.asset_id),
            (50.0, vec![(1, 0)], Some(asset.id))
        );

        let st = wait(&svc, &svc.map_save_asset(reopened, "Crypt v2".into(), Some(asset.id))).await;
        assert_eq!(st.state, JobState::Done, "{:?}", st.error);
        assert_eq!(svc.library_get(asset.id).await.unwrap().name, "Crypt v2");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dewarp_and_undo() {
        let (d, svc) = temp_service();
        let doc = svc.map_open_file(map_png(d.path())).await.unwrap();
        assert!(svc.map_dewarp(doc.clone()).await.is_err(), "needs a reference block");
        let quad = [(100.0, 100.0), (250.0, 100.0), (250.0, 250.0), (100.0, 250.0)];
        let doc = MapDoc { mode: SizingMode::Reference, ref_quad: Some(quad), ref_n: 3, ..doc };
        assert_eq!(doc.grid().cell_w, 50.0);
        let warped = svc.map_dewarp(doc).await.unwrap();
        assert_eq!(warped.side, 50.0);
        assert!(warped.pre_warp.is_some() && warped.ref_quad.is_none());
        assert_eq!(warped.grid().cell_w, 50.0, "after de-warp the even grid uses the side");
        let undone = svc.map_undo_dewarp(warped.clone()).await.unwrap();
        assert_eq!(undone.source, warped.pre_warp);
        assert!(svc.map_undo_dewarp(undone).await.is_err());
    }
}
