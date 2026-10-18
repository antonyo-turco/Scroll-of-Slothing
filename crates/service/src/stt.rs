//! Speech to text: Whisper models and file transcription.
//! See `docs/features/transcribe.md`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sleuth_core::stt::{Correction, Corrections, ExportFormat, Segment, export, glossary_prompt};
use sleuth_stt::local::{LocalModel, LocalOptions};
use sleuth_stt::{SttError, audio, models};

use crate::{JobCtx, JobId, Result, Service, ServiceError};

/// Glossary prompts longer than this are cut; Whisper only keeps ~224 prompt tokens.
const PROMPT_MAX_CHARS: usize = 600;

impl From<SttError> for ServiceError {
    fn from(e: SttError) -> Self {
        match e {
            SttError::Cancelled => ServiceError::Cancelled,
            SttError::Audio(m) => ServiceError::InvalidInput(m),
            SttError::Model(m) => ServiceError::InvalidInput(m),
            SttError::Download(m) => ServiceError::Io(m),
            SttError::Engine(m) => ServiceError::Internal(m),
            SttError::Io(e) => ServiceError::Io(e.to_string()),
        }
    }
}

/// The last loaded model, kept so repeated transcriptions skip the load.
#[derive(Default)]
pub(crate) struct SttRuntime {
    loaded: Mutex<Option<(String, Arc<LocalModel>)>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SttModel {
    pub id: String,
    pub label: String,
    pub size: u64,
    pub installed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TranscribeOptions {
    /// Model id from [`Service::stt_models_list`]; default `large-v3-turbo-q5_0`.
    pub model: Option<String>,
    /// `it` (default), `en`, ... or `auto`.
    pub language: Option<String>,
    /// Terms to spell right (passed to Whisper as its initial prompt).
    pub glossary: Vec<String>,
    /// Find/replace rules applied to the result.
    pub corrections: Vec<Correction>,
    /// 1 = greedy (faster); default 5.
    pub beam_size: Option<u32>,
    /// CPU threads; default: the engine's choice.
    pub threads: Option<u32>,
}

/// Job result of [`Service::transcribe_file`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeResult {
    pub model: String,
    pub language: String,
    pub duration_ms: u64,
    pub segments: Vec<Segment>,
}

impl Service {
    fn models_dir(&self) -> PathBuf {
        self.data_dir().join("models")
    }

    pub async fn stt_models_list(&self) -> Result<Vec<SttModel>> {
        let dir = self.models_dir();
        Ok(models::CATALOG
            .iter()
            .map(|m| SttModel {
                id: m.id.into(),
                label: m.label.into(),
                size: m.size,
                installed: models::is_installed(&dir, m),
            })
            .collect())
    }

    /// Downloads a model in the background. Job result: `{ id, path }`.
    pub fn stt_model_download(&self, id: String) -> Result<JobId> {
        let spec = models::spec(&id)?;
        let dir = self.models_dir();
        Ok(self.spawn_job("model download", move |ctx| {
            let mb = |b: u64| b as f64 / 1_048_576.0;
            let path = models::download(
                &dir,
                spec,
                &|done, total| {
                    ctx.progress(
                        done as f32 / total.max(1) as f32,
                        format!("{:.0} / {:.0} MB", mb(done), mb(total)),
                    )
                },
                &|| ctx.is_cancelled(),
            )?;
            Ok(serde_json::json!({ "id": spec.id, "path": path }))
        }))
    }

    pub async fn stt_model_delete(&self, id: String) -> Result<()> {
        let spec = models::spec(&id)?;
        let path = models::model_path(&self.models_dir(), spec);
        {
            let mut loaded = self.inner.stt.loaded.lock().unwrap_or_else(|e| e.into_inner());
            if loaded.as_ref().is_some_and(|(m, _)| m == spec.id) {
                *loaded = None;
            }
        }
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn local_model(&self, id: &str) -> Result<Arc<LocalModel>> {
        let spec = models::spec(id)?;
        let mut loaded = self.inner.stt.loaded.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((m, model)) = loaded.as_ref()
            && m == spec.id
        {
            return Ok(model.clone());
        }
        let dir = self.models_dir();
        if !models::is_installed(&dir, spec) {
            return Err(ServiceError::NotFound(format!(
                "model {} is not installed; download it first",
                spec.id
            )));
        }
        // Drop the old model before loading, so two big models never sit in memory.
        *loaded = None;
        let model = Arc::new(LocalModel::load(&models::model_path(&dir, spec))?);
        *loaded = Some((spec.id.to_string(), model.clone()));
        Ok(model)
    }

    /// Transcribes an audio file with the local engine. Job result: [`TranscribeResult`].
    pub fn transcribe_file(&self, path: PathBuf, opts: TranscribeOptions) -> Result<JobId> {
        let model_id = opts.model.clone().unwrap_or_else(|| models::DEFAULT_FILE_MODEL.into());
        // Fail fast on a bad id or a missing model, before starting the Job.
        let spec = models::spec(&model_id)?;
        if !models::is_installed(&self.models_dir(), spec) {
            return Err(ServiceError::NotFound(format!(
                "model {model_id} is not installed; download it first"
            )));
        }
        if !path.is_file() {
            return Err(ServiceError::NotFound(format!("file {}", path.display())));
        }
        let svc = self.clone();
        Ok(self.spawn_job("transcribe", move |ctx| {
            let result = svc.run_transcription(&ctx, &path, &model_id, &opts)?;
            serde_json::to_value(result).map_err(|e| ServiceError::Internal(e.to_string()))
        }))
    }

    fn run_transcription(
        &self,
        ctx: &JobCtx,
        path: &std::path::Path,
        model_id: &str,
        opts: &TranscribeOptions,
    ) -> Result<TranscribeResult> {
        let cancel = || ctx.is_cancelled();
        ctx.progress(0.0, "decoding audio");
        let samples = audio::decode_file(path, &cancel)?;
        let duration_ms = samples.len() as u64 * 1000 / sleuth_stt::SAMPLE_RATE as u64;
        ctx.check()?;

        ctx.progress(0.08, "loading model");
        let model = self.local_model(model_id)?;
        ctx.check()?;

        let language = opts.language.clone().unwrap_or_else(|| "it".into());
        let local = LocalOptions {
            language: language.clone(),
            prompt: glossary_prompt(&opts.glossary, PROMPT_MAX_CHARS),
            beam_size: opts.beam_size.unwrap_or(5).clamp(1, 8),
            threads: opts.threads.unwrap_or(0),
        };
        let pctx = ctx.clone();
        let progress = Arc::new(move |p: f32| pctx.progress(0.1 + p * 0.9, "transcribing"));
        let mut segments = model.transcribe(&samples, &local, progress, &cancel)?;

        let corrections = Corrections::new(&opts.corrections);
        for s in &mut segments {
            s.text = corrections.apply(&s.text);
        }
        Ok(TranscribeResult { model: model_id.into(), language, duration_ms, segments })
    }

    /// Renders segments as TXT, Markdown, SRT or VTT.
    pub fn transcript_format(
        &self,
        segments: &[Segment],
        format: ExportFormat,
        title: Option<&str>,
    ) -> String {
        export(segments, format, title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temp_service;

    #[tokio::test]
    async fn lists_the_catalog_with_nothing_installed() {
        let (_d, svc) = temp_service();
        let list = svc.stt_models_list().await.unwrap();
        assert_eq!(list.len(), models::CATALOG.len());
        assert!(list.iter().all(|m| !m.installed));
    }

    #[tokio::test]
    async fn transcribe_needs_an_installed_model() {
        let (d, svc) = temp_service();
        let wav = d.path().join("a.wav");
        std::fs::write(&wav, b"RIFF").unwrap();
        let err = svc.transcribe_file(wav, TranscribeOptions::default()).unwrap_err();
        assert!(matches!(err, ServiceError::NotFound(_)), "{err}");
        let err = svc
            .transcribe_file(
                d.path().join("a.wav"),
                TranscribeOptions { model: Some("nope".into()), ..Default::default() },
            )
            .unwrap_err();
        assert!(matches!(err, ServiceError::InvalidInput(_)), "{err}");
    }

    #[tokio::test]
    async fn deleting_a_missing_model_is_fine() {
        let (_d, svc) = temp_service();
        svc.stt_model_delete("base".into()).await.unwrap();
        assert!(svc.stt_model_delete("nope".into()).await.is_err());
    }
}
