//! The Whisper model catalog and the downloader. Models are ggml files from the
//! whisper.cpp Hugging Face repo, stored as `<models_dir>/<file>` and checked
//! against the SHA-256 that Hugging Face publishes for each file.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{CancelFn, Result, SttError};

const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ModelSpec {
    /// Stable id used in settings, the CLI and MCP.
    pub id: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    /// One-line description for pickers.
    pub hint: &'static str,
    /// Seconds of work per second of audio with beam search, measured on the
    /// dev machine (i7-1185G7, power-throttled to ~1.5 GHz). A starting point:
    /// the service replaces it with what it measures on the user's machine.
    pub speed: f64,
}

/// Multilingual models only: the main language is Italian, so the `.en` models
/// are not offered. Quantized variants are much faster on the CPU at a small
/// cost in accuracy. Hashes from the Hugging Face API (2026-10-18).
pub const CATALOG: &[ModelSpec] = &[
    ModelSpec {
        id: "base",
        label: "Base",
        hint: "Fast, rough",
        speed: 0.45,
        file: "ggml-base.bin",
        size: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    },
    ModelSpec {
        id: "small",
        label: "Small",
        hint: "Quick, decent",
        speed: 1.35,
        file: "ggml-small.bin",
        size: 487_601_967,
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
    },
    ModelSpec {
        id: "medium-q5_0",
        label: "Medium (quantized)",
        hint: "Accurate, slow",
        speed: 4.8,
        file: "ggml-medium-q5_0.bin",
        size: 539_212_467,
        sha256: "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
    },
    ModelSpec {
        id: "large-v3-turbo-q5_0",
        label: "Large v3 Turbo (quantized)",
        hint: "Most accurate per minute of work",
        speed: 6.5,
        file: "ggml-large-v3-turbo-q5_0.bin",
        size: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
    },
    ModelSpec {
        id: "large-v3-turbo",
        label: "Large v3 Turbo",
        hint: "Best, slowest on a CPU",
        // Not measured: the quantized model's speed, scaled for full precision.
        speed: 9.0,
        file: "ggml-large-v3-turbo.bin",
        size: 1_624_555_275,
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
    },
];

pub const DEFAULT_FILE_MODEL: &str = "large-v3-turbo-q5_0";
pub const DEFAULT_LIVE_MODEL: &str = "small";

pub fn spec(id: &str) -> Result<&'static ModelSpec> {
    CATALOG.iter().find(|m| m.id == id).ok_or_else(|| {
        let ids: Vec<_> = CATALOG.iter().map(|m| m.id).collect();
        SttError::Model(format!("unknown model {id:?} (one of: {})", ids.join(", ")))
    })
}

pub fn model_path(models_dir: &Path, spec: &ModelSpec) -> PathBuf {
    models_dir.join(spec.file)
}

/// Installed means the file is there with the expected size. The hash is checked
/// once, at download time.
pub fn is_installed(models_dir: &Path, spec: &ModelSpec) -> bool {
    std::fs::metadata(model_path(models_dir, spec)).is_ok_and(|m| m.len() == spec.size)
}

/// Downloads `spec` into `models_dir`, reporting `(done, total)` bytes. Writes to
/// a `.part` file and renames it only after the hash matches, so an interrupted
/// or corrupt download never looks installed.
pub fn download(
    models_dir: &Path,
    spec: &ModelSpec,
    progress: &dyn Fn(u64, u64),
    cancel: CancelFn,
) -> Result<PathBuf> {
    let url = format!("{BASE_URL}/{}", spec.file);
    download_from(&url, models_dir, spec, progress, cancel)
}

fn download_from(
    url: &str,
    models_dir: &Path,
    spec: &ModelSpec,
    progress: &dyn Fn(u64, u64),
    cancel: CancelFn,
) -> Result<PathBuf> {
    std::fs::create_dir_all(models_dir)?;
    let dest = model_path(models_dir, spec);
    let part = dest.with_extension("bin.part");
    let dl_err = |e: &dyn std::fmt::Display| SttError::Download(e.to_string());

    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("sleuth/", env!("CARGO_PKG_VERSION")))
        // No overall timeout: big models take minutes. The user can cancel instead.
        .timeout(None)
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| dl_err(&e))?;
    let mut resp =
        client.get(url).send().and_then(|r| r.error_for_status()).map_err(|e| dl_err(&e))?;

    let result = (|| {
        let mut file = std::fs::File::create(&part)?;
        let mut hasher = Sha256::new();
        let mut done = 0u64;
        let mut buf = vec![0u8; 1 << 16];
        loop {
            if cancel() {
                return Err(SttError::Cancelled);
            }
            let n = resp.read(&mut buf).map_err(|e| dl_err(&e))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            done += n as u64;
            progress(done, spec.size);
        }
        file.flush()?;
        drop(file);
        let got = hex(&hasher.finalize());
        if got != spec.sha256 {
            return Err(SttError::Download(format!(
                "{} is corrupt (sha256 {got}, expected {})",
                spec.file, spec.sha256
            )));
        }
        std::fs::rename(&part, &dest)?;
        Ok(dest.clone())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique_and_defaults_exist() {
        let mut ids: Vec<_> = CATALOG.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
        assert!(spec(DEFAULT_FILE_MODEL).is_ok());
        assert!(spec(DEFAULT_LIVE_MODEL).is_ok());
        assert!(matches!(spec("tiny.en"), Err(SttError::Model(_))));
        for m in CATALOG {
            assert_eq!(m.sha256.len(), 64, "{}", m.id);
            assert!(!m.file.contains(".en."), "{} is English-only", m.id);
        }
    }

    #[test]
    fn installed_needs_the_full_size() {
        let dir = tempfile::tempdir().unwrap();
        let spec = spec("base").unwrap();
        assert!(!is_installed(dir.path(), spec));
        std::fs::write(model_path(dir.path(), spec), b"short").unwrap();
        assert!(!is_installed(dir.path(), spec));
    }

    #[test]
    fn failed_download_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let spec = spec("base").unwrap();
        // Nothing listens on port 9 (discard) locally, so this fails fast.
        let err =
            download_from("http://127.0.0.1:9/x.bin", dir.path(), spec, &|_, _| {}, &|| false)
                .unwrap_err();
        assert!(matches!(err, SttError::Download(_)), "{err}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn hex_is_lowercase() {
        assert_eq!(hex(&[0x00, 0xab, 0xff]), "00abff");
    }
}
