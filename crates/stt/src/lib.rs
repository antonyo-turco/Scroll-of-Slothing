//! Speech to text. Audio decoding and resampling, the Whisper model catalog and
//! downloader, and the transcription engines. The service layer wraps all of it;
//! see `docs/features/transcribe.md`.

pub mod audio;
pub mod local;
pub mod models;

pub use sleuth_core::stt::Segment;

/// Whisper's input rate: mono f32 at 16 kHz.
pub const SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("cancelled")]
    Cancelled,
    #[error("unsupported or unreadable audio: {0}")]
    Audio(String),
    #[error("model: {0}")]
    Model(String),
    #[error("download: {0}")]
    Download(String),
    #[error("transcription: {0}")]
    Engine(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SttError>;

/// Cancellation check handed to long operations; returns `true` to stop.
pub type CancelFn<'a> = &'a (dyn Fn() -> bool + Sync);
