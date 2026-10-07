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
use sleuth_core::token::{TokenParams, compose};
use sleuth_core::units::SizeLabel;
use sleuth_store::{AssetImages, AssetPatch, NewAsset, Store};

use crate::library::{check_size, store_encoded};
use crate::{Asset, Result, Service, ServiceError};

/// Bundled borders (the Python app's `images/token_borders`).
const BUILTIN_BORDERS: &[(&str, &str, &[u8])] = &[
    ("builtin:roll20", "Roll20 gold", include_bytes!("../../../assets/token_borders/roll20.png")),
    (
        "builtin:roll20_greyscale",
        "Roll20 grey",
        include_bytes!("../../../assets/token_borders/roll20_greyscale.png"),
    ),
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

fn source_info(store: &Store, hash: &str) -> Result<SourceImage> {
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
    let mime =
        imaging::sniff_mime(bytes).ok_or_else(|| ServiceError::InvalidInput("unsupported image format".into()))?;
    let (w, h) = imaging::dimensions(bytes).ok_or_else(|| ServiceError::InvalidInput("corrupt image".into()))?;
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
            for (id, name, bytes) in BUILTIN_BORDERS {
                let src = store_source(s, bytes)?;
                out.push(Border {
                    id: (*id).into(),
                    name: (*name).into(),
                    builtin: true,
                    blob: src.hash,
                    width: src.width,
                    height: src.height,
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
            let img = imaging::decode(&bytes).map_err(|e| ServiceError::InvalidInput(e.to_string()))?;
            if !img.color().has_alpha() {
                return Err(ServiceError::InvalidInput("a border needs transparency (PNG/WebP with alpha)".into()));
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

    async fn border_image(&self, id: &str) -> Result<Arc<RgbaImage>> {
        let border = self
            .token_borders_list()
            .await?
            .into_iter()
            .find(|b| b.id == id)
            .ok_or_else(|| ServiceError::NotFound(format!("border {id}")))?;
        let svc = self.clone();
        self.blocking(move |s| svc.inner.images.get(s, &border.blob)).await
    }

    /// Stores an image file as an editing source.
    pub async fn image_source_from_path(&self, path: PathBuf) -> Result<SourceImage> {
        self.blocking(move |s| store_source(s, &std::fs::read(&path)?)).await
    }

    /// Stores the clipboard image (or an image file path copied as text) as a source.
    pub async fn image_source_from_clipboard(&self) -> Result<SourceImage> {
        self.blocking(|s| {
            let mut cb = arboard::Clipboard::new().map_err(|e| ServiceError::Io(e.to_string()))?;
            if let Ok(img) = cb.get_image() {
                let rgba = RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())
                    .ok_or_else(|| ServiceError::InvalidInput("clipboard image has an odd layout".into()))?;
                let png = imaging::encode_png(&rgba).map_err(|e| ServiceError::Internal(e.to_string()))?;
                return store_source(s, &png);
            }
            // Python also accepted a copied file path.
            let text = cb.get_text().map_err(|_| ServiceError::InvalidInput("the clipboard has no image".into()))?;
            let path = PathBuf::from(text.trim().trim_matches('"'));
            if path.is_file() {
                return store_source(s, &std::fs::read(&path)?);
            }
            Err(ServiceError::InvalidInput("the clipboard has no image".into()))
        })
        .await
    }

    /// Source description for an already-stored blob.
    pub async fn image_source_info(&self, hash: String) -> Result<SourceImage> {
        self.blocking(move |s| source_info(s, &hash)).await
    }

    /// Renders the token (border-sized) and returns PNG bytes.
    /// `fast` trades file size for speed (live preview).
    pub async fn token_render(&self, source: String, params: TokenParams, fast: bool) -> Result<Vec<u8>> {
        let img = self.compose_token(&source, &params).await?;
        tokio::task::spawn_blocking(move || {
            if fast { imaging::encode_png_fast(&img) } else { imaging::encode_png(&img) }
                .map_err(|e| ServiceError::Internal(e.to_string()))
        })
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?
    }

    pub(crate) async fn compose_token(&self, source: &str, params: &TokenParams) -> Result<RgbaImage> {
        let border = self.border_image(&params.border).await?;
        let svc = self.clone();
        let (source, params) = (source.to_string(), params.clone());
        self.blocking(move |s| {
            let base = svc.inner.images.get(s, &source)?;
            Ok(compose(&base, &border, &params))
        })
        .await
    }

    /// Writes the composed token to a PNG file.
    pub async fn token_save_png(&self, source: String, params: TokenParams, path: PathBuf) -> Result<PathBuf> {
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
        let params_json = serde_json::to_string(&params).map_err(|e| ServiceError::Internal(e.to_string()))?;
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
                        &AssetPatch { name: Some(name), size_label: Some(size), ..Default::default() },
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
        let src = svc.image_source_from_path(write_test_png(d.path(), "orc.png", 300, 200)).await.unwrap();
        let params = TokenParams { border: "builtin:roll20".into(), ..Default::default() };
        (d, svc, src, params)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn builtin_borders_are_listed_with_sizes() {
        let (_d, svc) = temp_service();
        let b = svc.token_borders_list().await.unwrap();
        assert_eq!(b.iter().map(|b| (b.id.as_str(), b.width)).collect::<Vec<_>>(), [
            ("builtin:roll20", 442),
            ("builtin:roll20_greyscale", 256)
        ]);
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
        let p = TokenParams { pan: [12.0, -4.0], zoom: 1.5, ring_inset_pct: 10.0, bg: Some([1, 2, 3]), ..params };
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
            .token_save_to_library(src.hash.clone(), p2.clone(), "Orc Chief".into(), "Huge".into(), Some(a.id))
            .await
            .unwrap();
        assert_eq!(b.id, a.id, "updated in place");
        assert_eq!((b.name.as_str(), b.size_label.as_str()), ("Orc Chief", "Huge"));
        assert_eq!(svc.token_open_asset(a.id).await.unwrap().params, p2);
        assert!(svc.token_save_to_library(src.hash, p2, " ".into(), "Huge".into(), None).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn imported_assets_open_with_default_framing() {
        let (d, svc) = temp_service();
        let path = write_test_png(d.path(), "goblin.png", 64, 64);
        let id = svc
            .library_import(vec![crate::ImportFile { path, name: None, kind: "token".into(), size: None }])
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
        assert_eq!(svc.token_borders_list().await.unwrap().len(), 3);
        svc.token_borders_remove(b.id.clone()).await.unwrap();
        assert_eq!(svc.token_borders_list().await.unwrap().len(), 2);
        assert!(svc.token_borders_remove(b.id).await.is_err());
    }
}
