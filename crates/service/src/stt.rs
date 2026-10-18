//! Speech to text: Whisper models, the cloud API key, and file transcription
//! with either engine. See `docs/features/transcribe.md`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sleuth_core::settings::SttEngine;
use sleuth_core::stt::{Correction, Corrections, ExportFormat, Segment, export, glossary_prompt};
use sleuth_stt::cloud::{self, CloudConfig, KeyInfo};
use sleuth_stt::local::{LocalModel, LocalOptions};
use sleuth_stt::{SttError, audio, models};

use crate::{JobCtx, JobId, Result, Service, ServiceError};

/// Glossary prompts longer than this are cut; Whisper only keeps ~224 prompt tokens.
const PROMPT_MAX_CHARS: usize = 600;
/// Credential-store entry for the OpenRouter key.
const CLOUD_KEY: &str = "openrouter_api_key";
/// Used when no key is saved (handy for the CLI and the standalone MCP server).
const CLOUD_KEY_ENV: &str = "OPENROUTER_API_KEY";

impl From<SttError> for ServiceError {
    fn from(e: SttError) -> Self {
        match e {
            SttError::Cancelled => ServiceError::Cancelled,
            SttError::Audio(m) => ServiceError::InvalidInput(m),
            SttError::Model(m) => ServiceError::InvalidInput(m),
            SttError::Download(m) => ServiceError::Io(m),
            SttError::Engine(m) => ServiceError::Internal(m),
            SttError::Cloud(m) => ServiceError::Io(m),
            SttError::Auth(m) => ServiceError::InvalidInput(m),
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
    /// Default: the `stt_engine` setting.
    pub engine: Option<SttEngine>,
    /// Local: a model id from [`Service::stt_models_list`] (default
    /// `large-v3-turbo-q5_0`). Cloud: the API's model id (default: the
    /// `stt_cloud_model` setting).
    pub model: Option<String>,
    /// `it` (default), `en`, ... or `auto`.
    pub language: Option<String>,
    /// Terms to spell right (local Whisper's initial prompt; not sent to the
    /// cloud yet).
    pub glossary: Vec<String>,
    /// Find/replace rules applied to the result, with either engine.
    pub corrections: Vec<Correction>,
    /// Local only. 1 = greedy (faster); default 5.
    pub beam_size: Option<u32>,
    /// Local only. CPU threads; default: the engine's choice.
    pub threads: Option<u32>,
}

/// Job result of [`Service::transcribe_file`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeResult {
    pub engine: SttEngine,
    pub model: String,
    pub language: String,
    pub duration_ms: u64,
    pub segments: Vec<Segment>,
    /// Cloud only: what the API reported it charged, in USD.
    pub cost: Option<f64>,
}

/// Whether a cloud API key is available. The key itself is never returned.
#[derive(Debug, Clone, Serialize)]
pub struct CloudKeyStatus {
    pub configured: bool,
    /// `saved` (credential store) or `env` (`OPENROUTER_API_KEY`).
    pub source: Option<&'static str>,
    /// e.g. `sk-or-…a1b2`.
    pub hint: Option<String>,
}

fn key_hint(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 10 {
        return "…".into();
    }
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

fn no_key() -> ServiceError {
    ServiceError::InvalidInput(
        "no OpenRouter API key: add one in Settings → Transcription (or set OPENROUTER_API_KEY)"
            .into(),
    )
}

fn joined<T>(r: std::result::Result<Result<T>, tokio::task::JoinError>) -> Result<T> {
    r.map_err(|e| ServiceError::Internal(format!("worker panicked: {e}")))?
}

/// What `transcribe_file` resolved before starting the Job.
enum Plan {
    Local { model: String },
    Cloud(CloudConfig),
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

    // ── cloud API key ──

    fn cloud_key(&self) -> Result<Option<(String, &'static str)>> {
        if let Some(k) = self.inner.secrets.get(CLOUD_KEY)?.filter(|k| !k.trim().is_empty()) {
            return Ok(Some((k, "saved")));
        }
        Ok(std::env::var(CLOUD_KEY_ENV).ok().filter(|k| !k.trim().is_empty()).map(|k| (k, "env")))
    }

    fn cloud_key_status_now(&self) -> Result<CloudKeyStatus> {
        Ok(match self.cloud_key()? {
            Some((k, source)) => {
                CloudKeyStatus { configured: true, source: Some(source), hint: Some(key_hint(&k)) }
            }
            None => CloudKeyStatus { configured: false, source: None, hint: None },
        })
    }

    pub async fn stt_cloud_key_status(&self) -> Result<CloudKeyStatus> {
        let svc = self.clone();
        joined(tokio::task::spawn_blocking(move || svc.cloud_key_status_now()).await)
    }

    /// Saves the cloud API key in the OS credential store.
    pub async fn stt_cloud_key_set(&self, key: String) -> Result<CloudKeyStatus> {
        let key = key.trim().to_string();
        if key.is_empty() || key.chars().any(char::is_whitespace) {
            return Err(ServiceError::InvalidInput(
                "the API key must be one word, without spaces".into(),
            ));
        }
        let svc = self.clone();
        joined(
            tokio::task::spawn_blocking(move || {
                svc.inner.secrets.set(CLOUD_KEY, &key)?;
                svc.cloud_key_status_now()
            })
            .await,
        )
    }

    /// Removes the saved key (an `OPENROUTER_API_KEY` variable still applies).
    pub async fn stt_cloud_key_clear(&self) -> Result<CloudKeyStatus> {
        let svc = self.clone();
        joined(
            tokio::task::spawn_blocking(move || {
                svc.inner.secrets.delete(CLOUD_KEY)?;
                svc.cloud_key_status_now()
            })
            .await,
        )
    }

    /// Asks the API whether the key works; returns its label and credit use.
    pub async fn stt_cloud_key_test(&self) -> Result<KeyInfo> {
        let base = self.settings_get().await?.stt_cloud_base_url;
        let svc = self.clone();
        joined(
            tokio::task::spawn_blocking(move || {
                let (key, _) = svc.cloud_key()?.ok_or_else(no_key)?;
                Ok(cloud::check_key(&base, &key)?)
            })
            .await,
        )
    }

    // ── transcription ──

    /// Transcribes an audio file, locally or in the cloud. Job result: [`TranscribeResult`].
    pub fn transcribe_file(&self, path: PathBuf, opts: TranscribeOptions) -> Result<JobId> {
        // Resolve and check everything before starting the Job, so mistakes
        // (no model, no key, no file) come back as an error right away.
        let settings = self.store().load_settings()?;
        let plan = match opts.engine.unwrap_or(settings.stt_engine) {
            SttEngine::Local => {
                let model = opts.model.clone().unwrap_or_else(|| models::DEFAULT_FILE_MODEL.into());
                let spec = models::spec(&model)?;
                if !models::is_installed(&self.models_dir(), spec) {
                    return Err(ServiceError::NotFound(format!(
                        "model {model} is not installed; download it first"
                    )));
                }
                Plan::Local { model }
            }
            SttEngine::Cloud => {
                let (api_key, _) = self.cloud_key()?.ok_or_else(no_key)?;
                let model = opts
                    .model
                    .as_deref()
                    .map(str::trim)
                    .filter(|m| !m.is_empty())
                    .map_or(settings.stt_cloud_model, String::from);
                Plan::Cloud(CloudConfig { base_url: settings.stt_cloud_base_url, api_key, model })
            }
        };
        if !path.is_file() {
            return Err(ServiceError::NotFound(format!("file {}", path.display())));
        }
        let svc = self.clone();
        Ok(self.spawn_job("transcribe", move |ctx| {
            let result = svc.run_transcription(&ctx, &path, &plan, &opts)?;
            serde_json::to_value(result).map_err(|e| ServiceError::Internal(e.to_string()))
        }))
    }

    fn run_transcription(
        &self,
        ctx: &JobCtx,
        path: &std::path::Path,
        plan: &Plan,
        opts: &TranscribeOptions,
    ) -> Result<TranscribeResult> {
        let cancel = || ctx.is_cancelled();
        ctx.progress(0.0, "decoding audio");
        let samples = audio::decode_file(path, &cancel)?;
        let duration_ms = samples.len() as u64 * 1000 / sleuth_stt::SAMPLE_RATE as u64;
        ctx.check()?;

        let language = opts.language.clone().unwrap_or_else(|| "it".into());
        let (engine, model, mut segments, cost) = match plan {
            Plan::Local { model } => {
                ctx.progress(0.08, "loading model");
                let m = self.local_model(model)?;
                ctx.check()?;
                let local = LocalOptions {
                    language: language.clone(),
                    prompt: glossary_prompt(&opts.glossary, PROMPT_MAX_CHARS),
                    beam_size: opts.beam_size.unwrap_or(5).clamp(1, 8),
                    threads: opts.threads.unwrap_or(0),
                };
                let pctx = ctx.clone();
                let progress = Arc::new(move |p: f32| pctx.progress(0.1 + p * 0.9, "transcribing"));
                let segs = m.transcribe(&samples, &local, progress, &cancel)?;
                (SttEngine::Local, model.clone(), segs, None)
            }
            Plan::Cloud(cfg) => {
                ctx.progress(0.1, "sending to the cloud");
                let lang = (language != "auto").then_some(language.as_str());
                let progress = |p: f32, done: usize, total: usize| {
                    ctx.progress(0.1 + p * 0.9, format!("sent {done} of {total} parts"))
                };
                let r = cloud::transcribe(cfg, &samples, lang, &progress, &cancel)?;
                (SttEngine::Cloud, cfg.model.clone(), r.segments, r.cost)
            }
        };

        let corrections = Corrections::new(&opts.corrections);
        for s in &mut segments {
            s.text = corrections.apply(&s.text);
        }
        Ok(TranscribeResult { engine, model, language, duration_ms, segments, cost })
    }

    /// Renders segments as TXT, Markdown, SRT or VTT. With `path`, also writes
    /// the text there. Returns the text.
    pub async fn transcript_export(
        &self,
        segments: Vec<Segment>,
        format: ExportFormat,
        title: Option<String>,
        path: Option<PathBuf>,
    ) -> Result<String> {
        let text = export(&segments, format, title.as_deref());
        if let Some(p) = path {
            let t = text.clone();
            joined(tokio::task::spawn_blocking(move || Ok(std::fs::write(p, t)?)).await)?;
        }
        Ok(text)
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
    async fn cloud_key_is_stored_hidden_and_required() {
        let (d, svc) = temp_service();
        // A key in the developer's environment would count as configured.
        let env = std::env::var(CLOUD_KEY_ENV).ok().filter(|k| !k.trim().is_empty());
        assert_eq!(svc.stt_cloud_key_status().await.unwrap().configured, env.is_some());

        assert!(svc.stt_cloud_key_set("  ".into()).await.is_err());
        assert!(svc.stt_cloud_key_set("two words".into()).await.is_err());
        let st = svc.stt_cloud_key_set(" sk-or-v1-0123456789abcd ".into()).await.unwrap();
        assert!(st.configured);
        assert_eq!(st.source, Some("saved"));
        assert_eq!(st.hint.as_deref(), Some("sk-or-…abcd"));
        assert!(!serde_json::to_string(&st).unwrap().contains("0123456789"));

        let st = svc.stt_cloud_key_clear().await.unwrap();
        assert_eq!(st.source, env.as_ref().map(|_| "env"));
        if env.is_none() {
            let wav = d.path().join("a.wav");
            std::fs::write(&wav, b"RIFF").unwrap();
            let opts = TranscribeOptions { engine: Some(SttEngine::Cloud), ..Default::default() };
            let err = svc.transcribe_file(wav, opts).unwrap_err();
            assert!(err.to_string().contains("no OpenRouter API key"), "{err}");
        }
    }

    /// Answers every HTTP request with `body`; returns the base URL.
    fn fake_api(body: &'static str) -> String {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut r = BufReader::new(stream.try_clone().unwrap());
                let mut len = 0usize;
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).unwrap();
                    if h == "\r\n" || h.is_empty() {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                }
                let mut buf = vec![0; len];
                r.read_exact(&mut buf).unwrap();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        });
        base
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cloud_transcription_runs_as_a_job() {
        let (d, svc) = temp_service();
        let base = fake_api(r#"{"text":" beholdr in vista ","usage":{"cost":0.002}}"#);
        let mut patch = serde_json::Map::new();
        patch.insert("stt_cloud_base_url".into(), base.into());
        assert!(svc.settings_set(patch).await.unwrap().errors.is_empty());
        svc.stt_cloud_key_set("sk-or-v1-test-key-1234".into()).await.unwrap();

        // 3 s of tone: one chunk, one request.
        let tone: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        let wav = d.path().join("tone.wav");
        std::fs::write(&wav, sleuth_core::stt::wav_pcm16(&tone, 16_000)).unwrap();
        let opts = TranscribeOptions {
            engine: Some(SttEngine::Cloud),
            corrections: vec![Correction { from: "beholdr".into(), to: "beholder".into() }],
            ..Default::default()
        };
        let id = svc.transcribe_file(wav, opts).unwrap();
        let st = loop {
            let st = svc.job_status(&id).unwrap();
            if st.state != crate::JobState::Running {
                break st;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };
        assert_eq!(st.state, crate::JobState::Done, "{:?}", st.error);
        let r: TranscribeResult = serde_json::from_value(st.result.unwrap()).unwrap();
        assert_eq!(r.engine, SttEngine::Cloud);
        assert_eq!(r.model, sleuth_core::settings::DEFAULT_STT_CLOUD_MODEL);
        assert_eq!(r.duration_ms, 3000);
        assert_eq!(r.cost, Some(0.002));
        assert_eq!(
            r.segments,
            vec![Segment { start_ms: 0, end_ms: 3000, text: "beholder in vista".into() }]
        );
    }

    #[test]
    fn key_hint_never_shows_the_middle() {
        assert_eq!(key_hint("sk-or-v1-0123456789abcd"), "sk-or-…abcd");
        assert_eq!(key_hint("short"), "…");
    }

    #[tokio::test]
    async fn export_returns_and_writes_the_text() {
        let (d, svc) = temp_service();
        let segs = vec![Segment { start_ms: 0, end_ms: 1500, text: "Ciao.".into() }];
        let out = d.path().join("t.srt");
        let text =
            svc.transcript_export(segs, ExportFormat::Srt, None, Some(out.clone())).await.unwrap();
        assert_eq!(text, "1\n00:00:00,000 --> 00:00:01,500\nCiao.\n\n");
        assert_eq!(std::fs::read_to_string(out).unwrap(), text);
    }

    #[tokio::test]
    async fn deleting_a_missing_model_is_fine() {
        let (_d, svc) = temp_service();
        svc.stt_model_delete("base".into()).await.unwrap();
        assert!(svc.stt_model_delete("nope".into()).await.is_err());
    }
}
