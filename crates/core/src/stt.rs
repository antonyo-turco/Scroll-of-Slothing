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
}
