//! Decoding any supported audio file to mono f32 at [`SAMPLE_RATE`].

use std::io::Cursor;
use std::path::Path;

use rubato::{FftFixedIn, Resampler};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::{CancelFn, Result, SAMPLE_RATE, SttError};

/// File extensions the decoder accepts (wav, mp3, flac, ogg/vorbis, m4a/aac, ...).
pub const EXTENSIONS: &[&str] =
    &["wav", "mp3", "flac", "ogg", "oga", "m4a", "mp4", "aac", "mka", "mkv", "webm", "caf", "aiff"];

/// Decodes `path` to mono 16 kHz samples.
pub fn decode_file(path: &Path, cancel: CancelFn) -> Result<Vec<f32>> {
    let file = std::fs::File::open(path)?;
    let ext = path.extension().and_then(|e| e.to_str());
    decode(Box::new(file), ext, cancel)
}

/// Decodes in-memory audio (e.g. a blob) to mono 16 kHz samples. `ext` is a
/// format hint; the content is probed either way.
pub fn decode_bytes(bytes: Vec<u8>, ext: Option<&str>, cancel: CancelFn) -> Result<Vec<f32>> {
    decode(Box::new(Cursor::new(bytes)), ext, cancel)
}

fn decode(src: Box<dyn MediaSource>, ext: Option<&str>, cancel: CancelFn) -> Result<Vec<f32>> {
    let audio_err = |e: SymError| SttError::Audio(e.to_string());
    let mss = MediaSourceStream::new(src, Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = ext {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(audio_err)?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| SttError::Audio("no audio track".into()))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(audio_err)?;

    let mut rate = track.codec_params.sample_rate;
    let mut mono: Vec<f32> = Vec::new();
    let mut buf: Option<SampleBuffer<f32>> = None;
    loop {
        if cancel() {
            return Err(SttError::Cancelled);
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(audio_err(e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // A corrupt frame: skip it, as players do.
            Err(SymError::DecodeError(e)) => {
                tracing::debug!("skipping undecodable frame: {e}");
                continue;
            }
            Err(e) => return Err(audio_err(e)),
        };
        let spec = *decoded.spec();
        rate.get_or_insert(spec.rate);
        let channels = spec.channels.count().max(1);
        let sb = match &mut buf {
            Some(b) if b.capacity() >= decoded.capacity() * channels => b,
            _ => buf.insert(SampleBuffer::new(decoded.capacity() as u64, spec)),
        };
        sb.copy_interleaved_ref(decoded);
        mono.extend(
            sb.samples().chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32),
        );
    }
    let rate = rate.ok_or_else(|| SttError::Audio("unknown sample rate".into()))?;
    resample(&mono, rate, SAMPLE_RATE, cancel)
}

/// Resamples mono audio from `from` Hz to `to` Hz. The output has exactly
/// `round(len * to / from)` samples, aligned with the input (the filter delay is
/// removed).
pub fn resample(input: &[f32], from: u32, to: u32, cancel: CancelFn) -> Result<Vec<f32>> {
    if from == to || input.is_empty() {
        return Ok(input.to_vec());
    }
    let rs_err = |e: &dyn std::fmt::Display| SttError::Audio(format!("resampling: {e}"));
    let mut rs =
        FftFixedIn::<f32>::new(from as usize, to as usize, 1024, 2, 1).map_err(|e| rs_err(&e))?;
    let want = (input.len() as f64 * to as f64 / from as f64).round() as usize;
    let delay = rs.output_delay();
    let mut out: Vec<f32> = Vec::with_capacity(want + delay + 2048);
    let mut pos = 0;
    while pos < input.len() {
        if cancel() {
            return Err(SttError::Cancelled);
        }
        let need = rs.input_frames_next();
        let chunk = if pos + need <= input.len() {
            rs.process(&[&input[pos..pos + need]], None)
        } else {
            rs.process_partial(Some(&[&input[pos..]]), None)
        }
        .map_err(|e| rs_err(&e))?;
        out.extend_from_slice(&chunk[0]);
        pos += need;
    }
    // Flush the filter tail until the delayed samples have all come out.
    while out.len() < want + delay {
        let chunk = rs.process_partial(None::<&[&[f32]]>, None).map_err(|e| rs_err(&e))?;
        if chunk[0].is_empty() {
            break;
        }
        out.extend_from_slice(&chunk[0]);
    }
    let end = (delay + want).min(out.len());
    let mut v = out[delay.min(end)..end].to_vec();
    v.resize(want, 0.0);
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEVER: &(dyn Fn() -> bool + Sync) = &|| false;

    /// A 16-bit PCM mono/stereo WAV, built in memory.
    fn wav(rate: u32, channels: u16, frames: &[Vec<i16>]) -> Vec<u8> {
        let data_len = (frames.len() * channels as usize * 2) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&channels.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        b.extend_from_slice(&(channels * 2).to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for f in frames {
            for s in f {
                b.extend_from_slice(&s.to_le_bytes());
            }
        }
        b
    }

    fn sine(rate: u32, hz: f32, secs: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n).map(|i| (i as f32 / rate as f32 * hz * std::f32::consts::TAU).sin() * 0.5).collect()
    }

    #[test]
    fn decodes_16k_mono_wav_unchanged() {
        let src = sine(16_000, 440.0, 0.5);
        let frames: Vec<Vec<i16>> = src.iter().map(|s| vec![(s * 32767.0) as i16]).collect();
        let out = decode_bytes(wav(16_000, 1, &frames), Some("wav"), NEVER).unwrap();
        assert_eq!(out.len(), src.len());
        for (a, b) in out.iter().zip(&src) {
            assert!((a - b).abs() < 1e-3);
        }
    }

    #[test]
    fn downmixes_stereo_and_resamples_48k() {
        let left = sine(48_000, 440.0, 1.0);
        // Right channel silent: the mono mix is half the left channel.
        let frames: Vec<Vec<i16>> = left.iter().map(|s| vec![(s * 32767.0) as i16, 0]).collect();
        let out = decode_bytes(wav(48_000, 2, &frames), None, NEVER).unwrap();
        assert_eq!(out.len(), 16_000);
        // Compare against the expected 16 kHz sine away from the edges.
        let want = sine(16_000, 440.0, 1.0);
        let err: f32 = (2000..14000).map(|i| (out[i] - want[i] * 0.5).abs()).fold(0.0, f32::max);
        assert!(err < 0.02, "max error {err}");
    }

    #[test]
    fn rejects_garbage() {
        let err = decode_bytes(b"definitely not audio".to_vec(), None, NEVER).unwrap_err();
        assert!(matches!(err, SttError::Audio(_)), "{err}");
    }

    #[test]
    fn cancel_stops_decoding() {
        let frames: Vec<Vec<i16>> = vec![vec![0]; 16_000];
        let err = decode_bytes(wav(16_000, 1, &frames), None, &|| true).unwrap_err();
        assert!(matches!(err, SttError::Cancelled));
    }
}
