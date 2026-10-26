//! The local engine: whisper.cpp through `whisper-rs`, on the CPU (or Vulkan with
//! the `vulkan` feature).

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::{CancelFn, Result, SAMPLE_RATE, Segment, SttError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalOptions {
    /// ISO 639-1 code (`it`, `en`) or `auto`.
    pub language: String,
    /// Initial prompt, typically [`sleuth_core::stt::glossary_prompt`].
    pub prompt: Option<String>,
    /// Beam search width; 1 means greedy (faster, a little less accurate).
    pub beam_size: u32,
    /// 0 means all logical cores.
    pub threads: u32,
}

impl Default for LocalOptions {
    fn default() -> Self {
        LocalOptions { language: "it".into(), prompt: None, beam_size: 5, threads: 0 }
    }
}

/// A loaded model. Loading takes a while (hundreds of MB), so callers keep one
/// around between transcriptions.
pub struct LocalModel {
    ctx: WhisperContext,
}

static LOGGING: std::sync::Once = std::sync::Once::new();

impl LocalModel {
    pub fn load(path: &Path) -> Result<Self> {
        // Route whisper.cpp's chatty stderr output into `tracing`.
        LOGGING.call_once(whisper_rs::install_logging_hooks);
        let path_str =
            path.to_str().ok_or_else(|| SttError::Model("non-UTF-8 model path".into()))?;
        if !path.is_file() {
            return Err(SttError::Model(format!("model file not found: {}", path.display())));
        }
        let ctx = WhisperContext::new_with_params(path_str, WhisperContextParameters::default())
            .map_err(|e| SttError::Model(format!("{}: {e}", path.display())))?;
        Ok(LocalModel { ctx })
    }

    /// Transcribes mono 16 kHz `samples`. `progress` gets 0.0..=1.0. Returns the
    /// segments in order, with text trimmed and blank segments dropped.
    pub fn transcribe(
        &self,
        samples: &[f32],
        opts: &LocalOptions,
        progress: Arc<dyn Fn(f32) + Send + Sync>,
        cancel: CancelFn,
    ) -> Result<Vec<Segment>> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }
        let mut state =
            self.ctx.create_state().map_err(|e| SttError::Engine(format!("state: {e}")))?;

        let strategy = if opts.beam_size > 1 {
            SamplingStrategy::BeamSearch { beam_size: opts.beam_size as i32, patience: -1.0 }
        } else {
            SamplingStrategy::Greedy { best_of: 1 }
        };
        let mut params = FullParams::new(strategy);
        let lang = match opts.language.trim() {
            "" | "auto" => None,
            l => Some(l),
        };
        params.set_language(lang);
        params.set_detect_language(false);
        if let Some(p) = opts.prompt.as_deref().filter(|p| !p.trim().is_empty()) {
            params.set_initial_prompt(p);
        }
        let threads = match opts.threads {
            0 => std::thread::available_parallelism().map_or(4, |n| n.get()),
            n => n as usize,
        };
        params.set_n_threads(threads as i32);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);

        params
            .set_progress_callback_safe(move |pct: i32| progress(pct.clamp(0, 100) as f32 / 100.0));
        // The abort callback must be 'static, so it can't call the borrowed
        // `cancel`. A scoped watcher thread polls `cancel` and sets a shared flag
        // that the callback reads.
        let stop = Arc::new(AtomicBool::new(false));
        let stop_cb = stop.clone();
        // Pre-boxed on purpose: whisper-rs 0.16 boxes the closure as
        // `Box<dyn FnMut>` but its trampoline casts the pointer back to the
        // closure's own type `F`. With a plain closure it reads garbage and
        // aborts at random ("failed to encode", -6). With `F = Box<dyn FnMut>`
        // the two types agree.
        let abort: Box<dyn FnMut() -> bool> = Box::new(move || stop_cb.load(Ordering::Relaxed));
        params.set_abort_callback_safe(abort);

        let done = AtomicBool::new(false);
        let result = std::thread::scope(|s| {
            s.spawn(|| {
                while !done.load(Ordering::Relaxed) {
                    if cancel() {
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            });
            let r = state.full(params, samples);
            done.store(true, Ordering::Relaxed);
            r
        });
        if stop.load(Ordering::Relaxed) {
            return Err(SttError::Cancelled);
        }
        result.map_err(|e| SttError::Engine(e.to_string()))?;

        tracing::debug!("whisper system info: {}", whisper_rs::print_system_info());
        let max_ms = samples.len() as u64 * 1000 / SAMPLE_RATE as u64;
        let mut out = Vec::new();
        for seg in state.as_iter() {
            let text = seg.to_str_lossy().map_err(|e| SttError::Engine(e.to_string()))?;
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            // Timestamps are centiseconds.
            let start_ms = (seg.start_timestamp().max(0) as u64 * 10).min(max_ms);
            let end_ms = (seg.end_timestamp().max(0) as u64 * 10).clamp(start_ms, max_ms);
            out.push(Segment { start_ms, end_ms, text: text.to_string() });
        }
        Ok(out)
    }
}
