//! Speech to text: transcript segments, export formats, the glossary prompt and
//! post-transcription corrections. See `docs/features/transcribe.md`.

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

/// One timed piece of a transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Txt,
    Md,
    Srt,
    Vtt,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Txt => "txt",
            ExportFormat::Md => "md",
            ExportFormat::Srt => "srt",
            ExportFormat::Vtt => "vtt",
        }
    }
}

/// `h:mm:ss` for Markdown, `hh:mm:ss,mmm` (SRT) or `hh:mm:ss.mmm` (VTT).
fn clock(ms: u64, millis_sep: Option<char>) -> String {
    let (h, m, s, frac) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    match millis_sep {
        Some(sep) => format!("{h:02}:{m:02}:{s:02}{sep}{frac:03}"),
        None => format!("{h}:{m:02}:{s:02}"),
    }
}

/// Renders segments in `fmt`. Segments with blank text are skipped, and text is trimmed.
pub fn export(segments: &[Segment], fmt: ExportFormat, title: Option<&str>) -> String {
    let segs = segments.iter().filter(|s| !s.text.trim().is_empty());
    let mut out = String::new();
    match fmt {
        ExportFormat::Txt => {
            for s in segs {
                out.push_str(s.text.trim());
                out.push('\n');
            }
        }
        ExportFormat::Md => {
            if let Some(t) = title {
                out.push_str(&format!("# {t}\n\n"));
            }
            for s in segs {
                out.push_str(&format!("**[{}]** {}\n\n", clock(s.start_ms, None), s.text.trim()));
            }
        }
        ExportFormat::Srt => {
            for (i, s) in segs.enumerate() {
                out.push_str(&format!(
                    "{}\n{} --> {}\n{}\n\n",
                    i + 1,
                    clock(s.start_ms, Some(',')),
                    clock(s.end_ms, Some(',')),
                    s.text.trim()
                ));
            }
        }
        ExportFormat::Vtt => {
            out.push_str("WEBVTT\n\n");
            for s in segs {
                out.push_str(&format!(
                    "{} --> {}\n{}\n\n",
                    clock(s.start_ms, Some('.')),
                    clock(s.end_ms, Some('.')),
                    s.text.trim()
                ));
            }
        }
    }
    out
}

/// Whisper's initial prompt: the glossary terms as one comma-separated line, so
/// the decoder favours their spelling. Blank and duplicate terms are dropped.
/// Whisper keeps only the last ~224 prompt tokens, so the list is capped at
/// `max_chars`, keeping whole terms from the front.
pub fn glossary_prompt(terms: &[String], max_chars: usize) -> Option<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = String::new();
    for t in terms.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        let key = t.to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        let extra = if out.is_empty() { t.len() } else { t.len() + 2 };
        if out.len() + extra > max_chars {
            break;
        }
        if !out.is_empty() {
            out.push_str(", ");
        }
        out.push_str(t);
        seen.push(key);
    }
    (!out.is_empty()).then(|| format!("{out}."))
}

/// A find/replace rule applied after transcription: whole words, case-insensitive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Correction {
    pub from: String,
    pub to: String,
}

/// Compiled correction rules. Rules with a blank `from` are ignored.
pub struct Corrections(Vec<(Regex, String)>);

impl Corrections {
    pub fn new(rules: &[Correction]) -> Self {
        let compiled = rules
            .iter()
            .filter(|r| !r.from.trim().is_empty())
            .filter_map(|r| {
                RegexBuilder::new(&regex::escape(r.from.trim()))
                    .case_insensitive(true)
                    .build()
                    .ok()
                    .map(|re| (re, r.to.clone()))
            })
            .collect();
        Corrections(compiled)
    }

    /// Applies every rule in order. A match counts only when it is not inside a
    /// longer word. `\b` can't express that for terms that start or end with
    /// punctuation (`d.m.`), so the neighbouring characters are checked directly.
    pub fn apply(&self, text: &str) -> String {
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let mut s = text.to_string();
        for (re, to) in &self.0 {
            let mut out = String::with_capacity(s.len());
            let mut last = 0;
            for m in re.find_iter(&s) {
                let before = s[..m.start()].chars().next_back();
                let after = s[m.end()..].chars().next();
                let first = m.as_str().chars().next();
                let final_ = m.as_str().chars().next_back();
                let glued = |n: Option<char>, edge: Option<char>| matches!((n, edge), (Some(n), Some(e)) if is_word(n) && is_word(e));
                if glued(before, first) || glued(after, final_) {
                    continue;
                }
                out.push_str(&s[last..m.start()]);
                out.push_str(to);
                last = m.end();
            }
            out.push_str(&s[last..]);
            s = out;
        }
        s
    }
}

/// Cloud requests carry at most this much audio (OpenRouter times out long requests).
pub const CHUNK_MAX_MS: u64 = 28_000;
/// Cuts are not made before this point, so chunks don't get uselessly short.
pub const CHUNK_MIN_MS: u64 = 8_000;
/// RMS below which a chunk counts as silence (about -54 dBFS) and is not sent.
pub const SILENCE_RMS: f32 = 0.002;

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// Splits mono audio into chunks for the cloud engine. Each chunk ends at the
/// quietest 30 ms frame between `min_ms` and `max_ms` after its start, so cuts
/// fall in pauses rather than mid-word. Chunks whose RMS is below
/// [`SILENCE_RMS`] are dropped. Returns sample ranges `[start, end)`.
pub fn plan_chunks(samples: &[f32], rate: u32, min_ms: u64, max_ms: u64) -> Vec<(usize, usize)> {
    let at = |ms: u64| (ms * rate as u64 / 1000) as usize;
    let (min_len, max_len, frame) = (at(min_ms).max(1), at(max_ms).max(1), at(30).max(1));
    let mut out = Vec::new();
    let mut start = 0;
    while start < samples.len() {
        let end = if samples.len() - start <= max_len {
            samples.len()
        } else {
            // Quietest frame in the window; ties go to the latest, for longer chunks.
            let mut best = (f32::INFINITY, start + max_len);
            let mut f = start + min_len;
            while f + frame <= start + max_len {
                let e = rms(&samples[f..f + frame]);
                if e <= best.0 {
                    best = (e, f + frame / 2);
                }
                f += frame;
            }
            best.1
        };
        if rms(&samples[start..end]) >= SILENCE_RMS {
            out.push((start, end));
        }
        start = end;
    }
    out
}

/// Encodes mono f32 samples as a 16-bit PCM WAV file.
pub fn wav_pcm16(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + samples.len() * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round_ties_even() as i16;
        b.extend_from_slice(&v.to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start_ms: u64, end_ms: u64, text: &str) -> Segment {
        Segment { start_ms, end_ms, text: text.into() }
    }

    fn sample() -> Vec<Segment> {
        vec![
            seg(0, 2_500, " Benvenuti al tavolo. "),
            seg(2_500, 3_000, "   "),
            seg(3_723_004, 3_725_100, "Tira per i hit points."),
        ]
    }

    #[test]
    fn export_txt() {
        assert_eq!(
            export(&sample(), ExportFormat::Txt, None),
            "Benvenuti al tavolo.\nTira per i hit points.\n"
        );
    }

    #[test]
    fn export_md() {
        assert_eq!(
            export(&sample(), ExportFormat::Md, Some("Sessione 3")),
            "# Sessione 3\n\n**[0:00:00]** Benvenuti al tavolo.\n\n**[1:02:03]** Tira per i hit points.\n\n"
        );
    }

    #[test]
    fn export_srt() {
        assert_eq!(
            export(&sample(), ExportFormat::Srt, None),
            "1\n00:00:00,000 --> 00:00:02,500\nBenvenuti al tavolo.\n\n\
             2\n01:02:03,004 --> 01:02:05,100\nTira per i hit points.\n\n"
        );
    }

    #[test]
    fn export_vtt() {
        assert_eq!(
            export(&sample(), ExportFormat::Vtt, None),
            "WEBVTT\n\n00:00:00.000 --> 00:00:02.500\nBenvenuti al tavolo.\n\n\
             01:02:03.004 --> 01:02:05.100\nTira per i hit points.\n\n"
        );
    }

    #[test]
    fn glossary_dedupes_and_caps() {
        let terms: Vec<String> =
            ["Dungeon Master", " ", "hit points", "dungeon master", "Beholder"]
                .map(String::from)
                .into();
        assert_eq!(
            glossary_prompt(&terms, 100).as_deref(),
            Some("Dungeon Master, hit points, Beholder.")
        );
        assert_eq!(glossary_prompt(&terms, 28).as_deref(), Some("Dungeon Master, hit points."));
        assert_eq!(glossary_prompt(&[], 100), None);
    }

    #[test]
    fn corrections_whole_word_case_insensitive() {
        let c = Corrections::new(&[
            Correction { from: "idpoint".into(), to: "hit points".into() },
            Correction { from: "beholdr".into(), to: "Beholder".into() },
            Correction { from: "  ".into(), to: "x".into() },
        ]);
        assert_eq!(
            c.apply("Ha 12 Idpoint, il beholdr ride."),
            "Ha 12 hit points, il Beholder ride."
        );
        // Not inside other words.
        assert_eq!(c.apply("idpointer"), "idpointer");
        // Adjacent matches.
        assert_eq!(c.apply("beholdr beholdr"), "Beholder Beholder");
    }

    #[test]
    fn corrections_handle_regex_characters() {
        let c = Corrections::new(&[Correction { from: "d.m.".into(), to: "DM".into() }]);
        assert_eq!(c.apply("il d.m. dice"), "il DM dice");
        assert_eq!(c.apply("il dxmx dice"), "il dxmx dice");
    }

    /// 1 s of tone, then `gap_ms` of silence, repeated.
    fn speech(rate: u32, words: usize, gap_ms: u64) -> Vec<f32> {
        let mut v = Vec::new();
        for _ in 0..words {
            v.extend((0..rate).map(|i| (i as f32 * 0.05).sin() * 0.3));
            v.extend(std::iter::repeat_n(0.0, (gap_ms * rate as u64 / 1000) as usize));
        }
        v
    }

    #[test]
    fn chunks_cover_speech_and_cut_in_pauses() {
        let rate = 1000;
        let audio = speech(rate, 40, 500); // 60 s
        let chunks = plan_chunks(&audio, rate, CHUNK_MIN_MS, CHUNK_MAX_MS);
        assert!(chunks.len() >= 3, "{chunks:?}");
        for w in chunks.windows(2) {
            assert_eq!(w[0].1, w[1].0, "contiguous");
        }
        assert_eq!(chunks[0].0, 0);
        assert_eq!(chunks.last().unwrap().1, audio.len());
        for &(a, b) in &chunks {
            assert!(b - a <= 28_000, "{a}..{b} too long");
            if b < audio.len() {
                assert_eq!(audio[b - 1], 0.0, "cut at {b} is not in a pause");
            }
        }
    }

    #[test]
    fn silent_chunks_are_dropped_and_short_audio_is_one_chunk() {
        let rate = 1000;
        let mut audio = vec![0.0; 60_000];
        assert!(plan_chunks(&audio, rate, CHUNK_MIN_MS, CHUNK_MAX_MS).is_empty());
        audio.truncate(5_000);
        audio.extend(speech(rate, 2, 0));
        assert_eq!(plan_chunks(&audio, rate, CHUNK_MIN_MS, CHUNK_MAX_MS), vec![(0, 7_000)]);
        assert!(plan_chunks(&[], rate, CHUNK_MIN_MS, CHUNK_MAX_MS).is_empty());
    }

    #[test]
    fn wav_header_and_samples() {
        let w = wav_pcm16(&[0.0, 1.0, -1.0, 2.0], 16_000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 16_000);
        let pcm: Vec<i16> = w[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(pcm, vec![0, 32767, -32767, 32767]);
    }
}
