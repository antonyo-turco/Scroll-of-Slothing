//! Token Cutter service. Spec: docs/features/token-cutter.md.
//!
//! Everything is addressed by blob hash: a dropped/pasted/opened image becomes
//! a stored blob ("source"), the live preview renders `source + params`, and
//! saving stores the composed PNG plus the params for non-destructive re-edits.

use std::path::PathBuf;
use std::sync::Arc;

use image::RgbaImage;
use serde::{Deserialize, Serialize};
use sleuth_core::imaging;
use sleuth_core::token::{self, TokenParams, TokenShape, compose_shaped, detect_shape};
use sleuth_core::units::SizeLabel;
use sleuth_store::{AssetImages, AssetPatch, NewAsset, Store};

use crate::library::{check_size, store_encoded};
use crate::{Asset, FilterCatalog, FilterPreset, Result, Service, ServiceError};

const NONE_PNG: &[u8] = include_bytes!("../../../assets/token_borders/none.png");

/// Bundled borders: the Python app's `images/token_borders`, thin versions of
/// both (ring squeezed to 45 % of its width, so more image shows), the gold
/// ring as a square, hexagon, octagon and decagon, normal and thin
/// (`tools/token_borders/shape_borders.py`), and
/// a blank "No border" per shape (the image is only clipped to the shape).
const BUILTIN_BORDERS: &[(&str, &str, TokenShape, &[u8])] = &[
    (
        "builtin:roll20",
        "Roll20 gold",
        TokenShape::Circle,
        include_bytes!("../../../assets/token_borders/roll20.png"),
    ),
    (
        "builtin:roll20_greyscale",
        "Roll20 grey",
        TokenShape::Circle,
        include_bytes!("../../../assets/token_borders/roll20_greyscale.png"),
    ),
    (
        "builtin:roll20_thin",
        "Roll20 gold, thin",
        TokenShape::Circle,
        include_bytes!("../../../assets/token_borders/roll20_thin.png"),
    ),
    (
        "builtin:roll20_greyscale_thin",
        "Roll20 grey, thin",
        TokenShape::Circle,
        include_bytes!("../../../assets/token_borders/roll20_greyscale_thin.png"),
    ),
    ("builtin:none", "No border", TokenShape::Circle, NONE_PNG),
    (
        "builtin:roll20_square",
        "Roll20 gold, square",
        TokenShape::Square,
        include_bytes!("../../../assets/token_borders/roll20_square.png"),
    ),
    (
        "builtin:roll20_square_thin",
        "Roll20 gold, square, thin",
        TokenShape::Square,
        include_bytes!("../../../assets/token_borders/roll20_square_thin.png"),
    ),
    ("builtin:none_square", "No border, square", TokenShape::Square, NONE_PNG),
    (
        "builtin:roll20_hex",
        "Roll20 gold, hexagon",
        TokenShape::Hex,
        include_bytes!("../../../assets/token_borders/roll20_hex.png"),
    ),
    (
        "builtin:roll20_hex_thin",
        "Roll20 gold, hexagon, thin",
        TokenShape::Hex,
        include_bytes!("../../../assets/token_borders/roll20_hex_thin.png"),
    ),
    ("builtin:none_hex", "No border, hexagon", TokenShape::Hex, NONE_PNG),
    (
        "builtin:roll20_octagon",
        "Roll20 gold, octagon",
        TokenShape::Octagon,
        include_bytes!("../../../assets/token_borders/roll20_octagon.png"),
    ),
    (
        "builtin:roll20_octagon_thin",
        "Roll20 gold, octagon, thin",
        TokenShape::Octagon,
        include_bytes!("../../../assets/token_borders/roll20_octagon_thin.png"),
    ),
    ("builtin:none_octagon", "No border, octagon", TokenShape::Octagon, NONE_PNG),
    (
        "builtin:roll20_decagon",
        "Roll20 gold, decagon",
        TokenShape::Decagon,
        include_bytes!("../../../assets/token_borders/roll20_decagon.png"),
    ),
    (
        "builtin:roll20_decagon_thin",
        "Roll20 gold, decagon, thin",
        TokenShape::Decagon,
        include_bytes!("../../../assets/token_borders/roll20_decagon_thin.png"),
    ),
    ("builtin:none_decagon", "No border, decagon", TokenShape::Decagon, NONE_PNG),
];

const USER_BORDERS_KEY: &str = "token.borders";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Border {
    pub id: String,
    pub name: String,
    pub builtin: bool,
    pub blob: String,
    pub width: u32,
    pub height: u32,
    /// Outline of tokens made with it (user borders: detected when added).
    #[serde(default)]
    pub shape: TokenShape,
    /// A blank border: the image is only clipped to `shape`.
    #[serde(default)]
    pub borderless: bool,
}

/// A source image stored as a blob, ready for editing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceImage {
    pub hash: String,
    pub width: u32,
    pub height: u32,
}

/// Everything needed to resume editing a token (from the library or a draft).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenDraft {
    pub source: SourceImage,
    pub params: TokenParams,
    pub name: String,
    pub size: String,
    /// Set when the draft was opened from the library (enables "update existing").
    pub asset_id: Option<i64>,
}

pub(crate) fn source_info(store: &Store, hash: &str) -> Result<SourceImage> {
    let meta = store.blob_meta(hash)?;
    let (width, height) = match (meta.width, meta.height) {
        (Some(w), Some(h)) => (w, h),
        _ => imaging::dimensions(&store.read_blob(hash)?)
            .ok_or_else(|| ServiceError::InvalidInput("not an image".into()))?,
    };
    Ok(SourceImage { hash: hash.to_string(), width, height })
}

/// Stores encoded image bytes (validated) and returns the source description.
pub(crate) fn store_source(store: &Store, bytes: &[u8]) -> Result<SourceImage> {
    let mime = imaging::sniff_mime(bytes)
        .ok_or_else(|| ServiceError::InvalidInput("unsupported image format".into()))?;
    let (w, h) = imaging::dimensions(bytes)
        .ok_or_else(|| ServiceError::InvalidInput("corrupt image".into()))?;
    let meta = store.put_blob(bytes, mime, Some((w, h)))?;
    Ok(SourceImage { hash: meta.hash, width: w, height: h })
}

impl Service {
    fn user_borders(store: &Store) -> Result<Vec<Border>> {
        Ok(store.workspace_get::<Vec<Border>>(USER_BORDERS_KEY)?.unwrap_or_default())
    }

    /// All borders: built-ins first, then user-added ones.
    pub async fn token_borders_list(&self) -> Result<Vec<Border>> {
        self.blocking(|s| {
            let mut out = Vec::new();
            for (id, name, shape, bytes) in BUILTIN_BORDERS {
                let src = store_source(s, bytes)?;
                out.push(Border {
                    id: (*id).into(),
                    name: (*name).into(),
                    builtin: true,
                    blob: src.hash,
                    width: src.width,
                    height: src.height,
                    shape: *shape,
                    borderless: id.starts_with("builtin:none"),
                });
            }
            out.extend(Self::user_borders(s)?);
            Ok(out)
        })
        .await
    }

    /// Adds a border PNG (should have a transparent centre).
    pub async fn token_borders_add(&self, path: PathBuf, name: Option<String>) -> Result<Border> {
        self.blocking(move |s| {
            let bytes = std::fs::read(&path)?;
            let img =
                imaging::decode(&bytes).map_err(|e| ServiceError::InvalidInput(e.to_string()))?;
            if !img.color().has_alpha() {
                return Err(ServiceError::InvalidInput(
                    "a border needs transparency (PNG/WebP with alpha)".into(),
                ));
            }
            let src = store_source(s, &bytes)?;
            let mut list = Self::user_borders(s)?;
            let id = format!("user:{}", &src.hash[..16]);
            list.retain(|b| b.id != id);
            let border = Border {
                id,
                name: name
                    .filter(|n| !n.trim().is_empty())
                    .or_else(|| path.file_stem().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| "Border".into()),
                builtin: false,
                blob: src.hash,
                width: src.width,
                height: src.height,
                shape: detect_shape(&img.to_rgba8()),
                borderless: false,
            };
            list.push(border.clone());
            s.workspace_set(USER_BORDERS_KEY, &list)?;
            s.record_change("token_borders", &border.id, "insert")?;
            Ok(border)
        })
        .await
    }

    pub async fn token_borders_remove(&self, id: String) -> Result<()> {
        self.blocking(move |s| {
            let mut list = Self::user_borders(s)?;
            let before = list.len();
            list.retain(|b| b.id != id);
            if list.len() == before {
                return Err(ServiceError::NotFound(format!("user border {id}")));
            }
            s.workspace_set(USER_BORDERS_KEY, &list)?;
            s.record_change("token_borders", &id, "delete")?;
            Ok(())
        })
        .await
    }

    async fn border_image(&self, id: &str) -> Result<(Arc<RgbaImage>, TokenShape)> {
        let border = self
            .token_borders_list()
            .await?
            .into_iter()
            .find(|b| b.id == id)
            .ok_or_else(|| ServiceError::NotFound(format!("border {id}")))?;
        let svc = self.clone();
        let shape = border.shape;
        let img = self.blocking(move |s| svc.inner.images.get(s, &border.blob)).await?;
        Ok((img, shape))
    }

    /// The map filters a token can use, and the map presets built only from them.
    pub fn token_filter_catalog(&self) -> FilterCatalog {
        FilterCatalog {
            defs: token::token_filter_defs(),
            presets: token::token_preset_names()
                .into_iter()
                .map(|name| FilterPreset {
                    name: name.to_string(),
                    pipeline: token::token_preset_pipeline(name).unwrap_or_default(),
                })
                .collect(),
        }
    }

    /// Stores an image file as an editing source.
    pub async fn image_source_from_path(&self, path: PathBuf) -> Result<SourceImage> {
        self.blocking(move |s| store_source(s, &std::fs::read(&path)?)).await
    }

    /// Stores the clipboard image (or an image file path copied as text) as a source.
    pub async fn image_source_from_clipboard(&self) -> Result<SourceImage> {
        self.blocking(|s| store_source(s, &crate::clipboard::read_image()?)).await
    }

    /// Source description for an already-stored blob.
    pub async fn image_source_info(&self, hash: String) -> Result<SourceImage> {
        self.blocking(move |s| source_info(s, &hash)).await
    }

    /// Renders the token (border-sized) and returns PNG bytes.
    /// `fast` trades file size for speed (live preview).
    pub async fn token_render(
        &self,
        source: String,
        params: TokenParams,
        fast: bool,
    ) -> Result<Vec<u8>> {
        let img = self.compose_token(&source, &params).await?;
        tokio::task::spawn_blocking(move || {
            if fast { imaging::encode_png_fast(&img) } else { imaging::encode_png(&img) }
                .map_err(|e| ServiceError::Internal(e.to_string()))
        })
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?
    }

    pub(crate) async fn compose_token(
        &self,
        source: &str,
        params: &TokenParams,
    ) -> Result<RgbaImage> {
        let (border, shape) = self.border_image(&params.border).await?;
        let svc = self.clone();
        let (source, params) = (source.to_string(), params.clone());
        self.blocking(move |s| {
            let base = svc.inner.images.get(s, &source)?;
            Ok(compose_shaped(&base, &border, shape, &params))
        })
        .await
    }

    /// Writes the composed token to a PNG file.
    pub async fn token_save_png(
        &self,
        source: String,
        params: TokenParams,
        path: PathBuf,
    ) -> Result<PathBuf> {
        let png = self.token_render(source, params, false).await?;
        self.blocking(move |_| {
            std::fs::write(&path, png)?;
            Ok(path)
        })
        .await
    }

    /// Saves to the library: a new asset, or (with `update`) replaces the images
    /// of an existing one. Stores `params` so the token can be re-edited later.
    pub async fn token_save_to_library(
        &self,
        source: String,
        params: TokenParams,
        name: String,
        size: String,
        update: Option<i64>,
    ) -> Result<Asset> {
        if name.trim().is_empty() {
            return Err(ServiceError::InvalidInput("please enter a name".into()));
        }
        let size = check_size(&size)?;
        let png = self.token_render(source.clone(), params.clone(), false).await?;
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
                    kind: "token".into(),
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

    /// Opens a library asset for editing in the Token Cutter. Assets without
    /// stored params (imports, legacy) start from default framing, like Python.
    pub async fn token_open_asset(&self, id: i64) -> Result<TokenDraft> {
        let default_border =
            self.token_borders_list().await?.first().map(|b| b.id.clone()).unwrap_or_default();
        self.blocking(move |s| {
            let a = s.get_asset(id)?;
            let mut params = a
                .params_json
                .as_deref()
                .and_then(|j| serde_json::from_str::<TokenParams>(j).ok())
                .unwrap_or_default();
            if params.border.is_empty() {
                params.border = default_border;
            }
            Ok(TokenDraft {
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

    async fn setup() -> (tempfile::TempDir, Service, SourceImage, TokenParams) {
        let (d, svc) = temp_service();
        let src = svc
            .image_source_from_path(write_test_png(d.path(), "orc.png", 300, 200))
            .await
            .unwrap();
        let params = TokenParams { border: "builtin:roll20".into(), ..Default::default() };
        (d, svc, src, params)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn builtin_borders_are_listed_with_sizes() {
        let (_d, svc) = temp_service();
        let b = svc.token_borders_list().await.unwrap();
        assert_eq!(
            b.iter().map(|b| (b.id.as_str(), b.width, b.shape.as_str())).collect::<Vec<_>>(),
            [
                ("builtin:roll20", 442, "circle"),
                ("builtin:roll20_greyscale", 256, "circle"),
                ("builtin:roll20_thin", 442, "circle"),
                ("builtin:roll20_greyscale_thin", 256, "circle"),
                ("builtin:none", 442, "circle"),
                ("builtin:roll20_square", 442, "square"),
                ("builtin:roll20_square_thin", 442, "square"),
                ("builtin:none_square", 442, "square"),
                ("builtin:roll20_hex", 442, "hex"),
                ("builtin:roll20_hex_thin", 442, "hex"),
                ("builtin:none_hex", 442, "hex"),
                ("builtin:roll20_octagon", 442, "octagon"),
                ("builtin:roll20_octagon_thin", 442, "octagon"),
                ("builtin:none_octagon", 442, "octagon"),
                ("builtin:roll20_decagon", 442, "decagon"),
                ("builtin:roll20_decagon_thin", 442, "decagon"),
                ("builtin:none_decagon", 442, "decagon"),
            ]
        );
        // The bundled art matches its declared shape.
        for border in b.iter().filter(|b| !b.borderless) {
            let (img, shape) = svc.border_image(&border.id).await.unwrap();
            assert_eq!((detect_shape(&img), shape), (border.shape, border.shape), "{}", border.id);
        }
        assert_eq!(b.iter().filter(|b| b.borderless).count(), TokenShape::ALL.len());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shaped_and_borderless_tokens_keep_their_shape() {
        let (_d, svc, src, params) = setup().await;
        for (border, shape) in [
            ("builtin:roll20_square", TokenShape::Square),
            ("builtin:roll20_hex_thin", TokenShape::Hex),
            ("builtin:none_square", TokenShape::Square),
            ("builtin:none_hex", TokenShape::Hex),
            ("builtin:none", TokenShape::Circle),
            ("builtin:roll20_octagon", TokenShape::Octagon),
            ("builtin:none_octagon", TokenShape::Octagon),
            ("builtin:roll20_decagon_thin", TokenShape::Decagon),
            ("builtin:none_decagon", TokenShape::Decagon),
        ] {
            let p = TokenParams { border: border.into(), ..params.clone() };
            let png = svc.token_render(src.hash.clone(), p, true).await.unwrap();
            let img = imaging::decode(&png).unwrap().to_rgba8();
            assert_eq!(detect_shape(&img), shape, "{border}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn filters_render_and_survive_save_and_reopen() {
        let (_d, svc, src, params) = setup().await;
        let cat = svc.token_filter_catalog();
        assert!(cat.defs.iter().all(|d| token::TOKEN_FILTERS.contains(&d.key)));
        let sepia = cat.presets.iter().find(|p| p.name == "Sepia print").unwrap();
        let p = TokenParams { filters: sepia.pipeline.clone(), ..params.clone() };
        let plain = svc.token_render(src.hash.clone(), params, true).await.unwrap();
        let tinted = svc.token_render(src.hash.clone(), p.clone(), true).await.unwrap();
        assert_ne!(plain, tinted);
        let a = svc
            .token_save_to_library(src.hash, p.clone(), "Orc".into(), "Medium".into(), None)
            .await
            .unwrap();
        assert_eq!(svc.token_open_asset(a.id).await.unwrap().params.filters, p.filters);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn render_is_border_sized_png() {
        let (_d, svc, src, params) = setup().await;
        assert_eq!((src.width, src.height), (300, 200));
        let png = svc.token_render(src.hash.clone(), params.clone(), true).await.unwrap();
        assert_eq!(imaging::dimensions(&png), Some((442, 442)));
        let err = svc
            .token_render(src.hash, TokenParams { border: "nope".into(), ..params }, true)
            .await
            .unwrap_err();
        assert_eq!(err.code(), crate::ErrorCode::NotFound);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn save_new_then_update_existing_and_reopen_with_params() {
        let (_d, svc, src, params) = setup().await;
        let p = TokenParams {
            pan: [12.0, -4.0],
            zoom: 1.5,
            ring_inset_pct: 10.0,
            bg: Some([1, 2, 3]),
            ..params
        };
        let a = svc
            .token_save_to_library(src.hash.clone(), p.clone(), "Orc".into(), "Large".into(), None)
            .await
            .unwrap();
        assert_eq!((a.kind.as_str(), a.size_label.as_str()), ("token", "Large"));
        assert_eq!(a.original_blob, src.hash, "the unprocessed source is the original");
        let meta = svc.store().blob_meta(&a.rendered_blob).unwrap();
        assert_eq!((meta.width, meta.height), (Some(442), Some(442)));

        let draft = svc.token_open_asset(a.id).await.unwrap();
        assert_eq!(draft.params, p, "params restored for non-destructive re-edit");
        assert_eq!((draft.name.as_str(), draft.asset_id), ("Orc", Some(a.id)));

        let p2 = TokenParams { zoom: 2.0, ..p };
        let b = svc
            .token_save_to_library(
                src.hash.clone(),
                p2.clone(),
                "Orc Chief".into(),
                "Huge".into(),
                Some(a.id),
            )
            .await
            .unwrap();
        assert_eq!(b.id, a.id, "updated in place");
        assert_eq!((b.name.as_str(), b.size_label.as_str()), ("Orc Chief", "Huge"));
        assert_eq!(svc.token_open_asset(a.id).await.unwrap().params, p2);
        assert!(
            svc.token_save_to_library(src.hash, p2, " ".into(), "Huge".into(), None).await.is_err()
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn imported_assets_open_with_default_framing() {
        let (d, svc) = temp_service();
        let path = write_test_png(d.path(), "goblin.png", 64, 64);
        let id = svc
            .library_import(vec![crate::ImportFile {
                path,
                name: None,
                kind: "token".into(),
                size: None,
            }])
            .await
            .unwrap()
            .imported[0];
        let draft = svc.token_open_asset(id).await.unwrap();
        assert_eq!(draft.params.border, "builtin:roll20");
        assert_eq!(draft.params.zoom, 1.0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn user_borders_require_alpha_and_can_be_removed() {
        let (d, svc) = temp_service();
        let opaque = d.path().join("opaque.jpg");
        image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3])).save(&opaque).unwrap();
        assert!(svc.token_borders_add(opaque, None).await.is_err());
        let ring = write_test_png(d.path(), "My Ring.png", 128, 128);
        let b = svc.token_borders_add(ring, None).await.unwrap();
        assert_eq!((b.name.as_str(), b.builtin), ("My Ring", false));
        let builtin = BUILTIN_BORDERS.len();
        assert_eq!(svc.token_borders_list().await.unwrap().len(), builtin + 1);
        svc.token_borders_remove(b.id.clone()).await.unwrap();
        assert_eq!(svc.token_borders_list().await.unwrap().len(), builtin);
        assert!(svc.token_borders_remove(b.id).await.is_err());
    }
}
