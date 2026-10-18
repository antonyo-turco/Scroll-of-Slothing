//! The cloud engine: an OpenRouter-format speech-to-text API.
//!
//! `POST {base_url}/audio/transcriptions` with a JSON body
//! `{ model, input_audio: { data: <base64>, format: "wav" }, language? }`;
//! the reply carries `text` and optionally `usage` (with `cost`). The reply has no
//! timestamps, so the audio is split into chunks at pauses
//! ([`sleuth_core::stt::plan_chunks`]) and each chunk becomes one segment.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sleuth_core::stt::{CHUNK_MAX_MS, CHUNK_MIN_MS, plan_chunks, wav_pcm16};

use crate::{CancelFn, Result, SAMPLE_RATE, Segment, SttError};

/// One chunk's reply: the text and the reported cost.
type ChunkReply = (String, Option<f64>);

/// Parallel requests per transcription.
const CONCURRENCY: usize = 4;
/// Attempts per chunk on rate limits, server errors and network failures.
const ATTEMPTS: u32 = 4;

#[derive(Debug, Clone)]
pub struct CloudConfig {
    /// e.g. `https://openrouter.ai/api/v1`, no trailing slash.
    pub base_url: String,
    pub api_key: String,
    /// e.g. `openai/whisper-1`.
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudResult {
    pub segments: Vec<Segment>,
    /// Sum of `usage.cost` over the requests, when the API reports it (USD).
    pub cost: Option<f64>,
    /// Chunks sent (silent chunks are skipped).
    pub requests: usize,
}

/// What `GET {base_url}/key` reports about the key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyInfo {
    pub label: Option<String>,
    /// Credit limit in USD; `None` means unlimited.
    pub limit: Option<f64>,
    /// Credits used so far, USD.
    pub usage: Option<f64>,
}

fn client(timeout: Duration) -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("sleuth/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .timeout(timeout)
        .build()
        .map_err(|e| SttError::Cloud(e.to_string()))
}

/// The API's own error message (`{"error": {"message": ...}}`), else the raw body.
fn api_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(String::from))
        .unwrap_or_else(|| body.chars().take(300).collect())
}

/// Maps an HTTP failure to an error, or `None` if the request should be retried.
fn status_error(status: u16, body: &str) -> Option<SttError> {
    let msg = api_message(body);
    match status {
        401 | 403 => Some(SttError::Auth(format!("the API key was rejected ({status}): {msg}"))),
        402 => Some(SttError::Cloud(format!("not enough credits (402): {msg}"))),
        408 | 429 | 500..=599 => None,
        _ => Some(SttError::Cloud(format!("request failed ({status}): {msg}"))),
    }
}

/// Checks the key and returns its limits. Uses `GET {base_url}/key`.
pub fn check_key(base_url: &str, api_key: &str) -> Result<KeyInfo> {
    let resp = client(Duration::from_secs(20))?
        .get(format!("{base_url}/key"))
        .bearer_auth(api_key)
        .send()
        .map_err(|e| SttError::Cloud(e.to_string()))?;
    let status = resp.status().as_u16();
    let body = resp.text().map_err(|e| SttError::Cloud(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(status_error(status, &body).unwrap_or_else(|| {
            SttError::Cloud(format!("request failed ({status}): {}", api_message(&body)))
        }));
    }
    let v: Value =
        serde_json::from_str(&body).map_err(|e| SttError::Cloud(format!("bad reply: {e}")))?;
    let d = &v["data"];
    Ok(KeyInfo {
        label: d["label"].as_str().map(String::from),
        limit: d["limit"].as_f64(),
        usage: d["usage"].as_f64(),
    })
}

/// One request: returns the text and the reported cost.
fn transcribe_chunk(
    http: &reqwest::blocking::Client,
    cfg: &CloudConfig,
    wav: &[u8],
    language: Option<&str>,
    cancel: CancelFn,
) -> Result<ChunkReply> {
    let mut body = json!({
        "model": cfg.model,
        "input_audio": {
            "data": base64::engine::general_purpose::STANDARD.encode(wav),
            "format": "wav",
        },
    });
    if let Some(l) = language {
        body["language"] = json!(l);
    }
    let body = body.to_string();
    let url = format!("{}/audio/transcriptions", cfg.base_url);
    let mut last = String::new();
    for attempt in 0..ATTEMPTS {
        if cancel() {
            return Err(SttError::Cancelled);
        }
        if attempt > 0 {
            // 1 s, 2 s, 4 s; checked for cancellation every 100 ms.
            for _ in 0..(10u64 << (attempt - 1)) {
                if cancel() {
                    return Err(SttError::Cancelled);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let resp = http
            .post(&url)
            .bearer_auth(&cfg.api_key)
            .header("Content-Type", "application/json")
            .header("X-Title", "Kastanomata's Scroll of Sleuthing")
            .body(body.clone())
            .send();
        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                last = e.to_string();
                tracing::debug!("cloud request failed (attempt {}): {last}", attempt + 1);
                continue;
            }
        };
        let status = resp.status().as_u16();
        let text = resp.text().map_err(|e| SttError::Cloud(e.to_string()))?;
        if (200..300).contains(&status) {
            let v: Value = serde_json::from_str(&text)
                .map_err(|e| SttError::Cloud(format!("bad reply: {e}")))?;
            let t = v["text"].as_str().ok_or_else(|| {
                SttError::Cloud(format!("reply has no text: {}", api_message(&text)))
            })?;
            return Ok((t.trim().to_string(), v["usage"]["cost"].as_f64()));
        }
        match status_error(status, &text) {
            Some(e) => return Err(e),
            None => {
                last = format!("{status}: {}", api_message(&text));
                tracing::debug!("cloud request retrying after {last}");
            }
        }
    }
    Err(SttError::Cloud(format!("gave up after {ATTEMPTS} attempts: {last}")))
}

/// Transcribes mono 16 kHz `samples`. `progress` gets 0.0..=1.0 as chunks complete.
/// `language` is an ISO 639-1 code; `None` lets the model detect it.
pub fn transcribe(
    cfg: &CloudConfig,
    samples: &[f32],
    language: Option<&str>,
    progress: &(dyn Fn(f32, usize, usize) + Sync),
    cancel: CancelFn,
) -> Result<CloudResult> {
    let chunks = plan_chunks(samples, SAMPLE_RATE, CHUNK_MIN_MS, CHUNK_MAX_MS);
    let n = chunks.len();
    if n == 0 {
        return Ok(CloudResult { segments: Vec::new(), cost: None, requests: 0 });
    }
    let http = client(Duration::from_secs(120))?;
    let results: Vec<Mutex<Option<ChunkReply>>> = (0..n).map(|_| Mutex::new(None)).collect();
    let first_error: Mutex<Option<SttError>> = Mutex::new(None);
    let failed = AtomicBool::new(false);
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let stop = || failed.load(Ordering::Relaxed) || cancel();

    std::thread::scope(|s| {
        for _ in 0..CONCURRENCY.min(n) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n || stop() {
                        break;
                    }
                    let (a, b) = chunks[i];
                    let wav = wav_pcm16(&samples[a..b], SAMPLE_RATE);
                    match transcribe_chunk(&http, cfg, &wav, language, &stop) {
                        Ok(r) => {
                            *results[i].lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
                            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                            progress(d as f32 / n as f32, d, n);
                        }
                        Err(e) => {
                            failed.store(true, Ordering::Relaxed);
                            let mut slot = first_error.lock().unwrap_or_else(|e| e.into_inner());
                            // Keep the real cause, not the "cancelled" of the other workers.
                            if slot.is_none() || matches!(*slot, Some(SttError::Cancelled)) {
                                *slot = Some(e);
                            }
                            break;
                        }
                    }
                }
            });
        }
    });

    if cancel() {
        return Err(SttError::Cancelled);
    }
    if let Some(e) = first_error.into_inner().unwrap_or_else(|e| e.into_inner()) {
        return Err(e);
    }
    let ms = |i: usize| i as u64 * 1000 / SAMPLE_RATE as u64;
    let mut segments = Vec::new();
    let mut cost: Option<f64> = None;
    for (i, slot) in results.into_iter().enumerate() {
        let Some((text, c)) = slot.into_inner().unwrap_or_else(|e| e.into_inner()) else {
            return Err(SttError::Cloud(format!("part {} of {n} is missing", i + 1)));
        };
        if let Some(c) = c {
            *cost.get_or_insert(0.0) += c;
        }
        if !text.is_empty() {
            let (a, b) = chunks[i];
            segments.push(Segment { start_ms: ms(a), end_ms: ms(b), text });
        }
    }
    Ok(CloudResult { segments, cost, requests: n })
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;

    use super::*;

    const NEVER: &(dyn Fn() -> bool + Sync) = &|| false;

    /// Request lines (with the auth header) and JSON bodies the fake server saw.
    type Seen = Arc<Mutex<Vec<(String, Value)>>>;

    /// A tiny HTTP server answering each request with `reply(index, body)`.
    /// Returns the base URL and the captured request bodies.
    fn server(
        reply: impl Fn(usize, &Value) -> (u16, String) + Send + Sync + 'static,
    ) -> (String, Seen) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        let reply = Arc::new(reply);
        std::thread::spawn(move || {
            for (i, stream) in listener.incoming().enumerate() {
                let Ok(mut stream) = stream else { break };
                let (seen, reply) = (seen2.clone(), reply.clone());
                std::thread::spawn(move || {
                    let mut r = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    r.read_line(&mut line).unwrap();
                    let mut len = 0;
                    let mut auth = String::new();
                    loop {
                        let mut h = String::new();
                        r.read_line(&mut h).unwrap();
                        if h == "\r\n" || h.is_empty() {
                            break;
                        }
                        let lower = h.to_ascii_lowercase();
                        if let Some(v) = lower.strip_prefix("content-length:") {
                            len = v.trim().parse().unwrap();
                        }
                        if lower.starts_with("authorization:") {
                            auth = h["authorization:".len()..].trim().to_string();
                        }
                    }
                    let mut body = vec![0; len];
                    r.read_exact(&mut body).unwrap();
                    let v: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                    let (status, text) = reply(i, &v);
                    seen.lock().unwrap().push((format!("{} {auth}", line.trim()), v));
                    let resp = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                        text.len()
                    );
                    let _ = stream.write_all(resp.as_bytes());
                });
            }
        });
        (base, seen)
    }

    fn cfg(base: String) -> CloudConfig {
        CloudConfig { base_url: base, api_key: "sk-test".into(), model: "openai/whisper-1".into() }
    }

    /// 70 s of 1 s tones separated by 0.5 s pauses, at 16 kHz.
    fn audio() -> Vec<f32> {
        let mut v = Vec::new();
        for _ in 0..47 {
            v.extend((0..SAMPLE_RATE).map(|i| (i as f32 * 0.05).sin() * 0.3));
            v.extend(std::iter::repeat_n(0.0, SAMPLE_RATE as usize / 2));
        }
        v
    }

    #[test]
    fn sends_chunks_in_order_and_sums_cost() {
        let (base, seen) = server(|_, body| {
            // Echo the chunk's size so the test can check segment order.
            let n = body["input_audio"]["data"].as_str().map_or(0, str::len);
            (200, json!({ "text": format!(" part {n} "), "usage": { "cost": 0.001 } }).to_string())
        });
        let samples = audio();
        let progressed = AtomicUsize::new(0);
        let r = transcribe(
            &cfg(base),
            &samples,
            Some("it"),
            &|_, d, _| {
                progressed.fetch_max(d, Ordering::Relaxed);
            },
            NEVER,
        )
        .unwrap();
        let chunks = plan_chunks(&samples, SAMPLE_RATE, CHUNK_MIN_MS, CHUNK_MAX_MS);
        assert_eq!(r.requests, chunks.len());
        assert!(r.requests >= 3);
        assert_eq!(progressed.load(Ordering::Relaxed), r.requests);
        assert!((r.cost.unwrap() - 0.001 * r.requests as f64).abs() < 1e-9);
        for (seg, &(a, b)) in r.segments.iter().zip(&chunks) {
            let b64_len = (44 + (b - a) * 2).div_ceil(3) * 4;
            assert_eq!(seg.text, format!("part {b64_len}"));
            assert_eq!(seg.start_ms, a as u64 * 1000 / 16_000);
        }
        let seen = seen.lock().unwrap();
        let (line, body) = &seen[0];
        assert!(line.starts_with("POST /audio/transcriptions"), "{line}");
        assert!(line.ends_with("Bearer sk-test"), "{line}");
        assert_eq!(body["model"], "openai/whisper-1");
        assert_eq!(body["language"], "it");
        assert_eq!(body["input_audio"]["format"], "wav");
    }

    #[test]
    fn retries_rate_limits_then_succeeds() {
        let (base, seen) = server(|i, _| {
            if i == 0 {
                (429, r#"{"error":{"message":"slow down"}}"#.into())
            } else {
                (200, r#"{"text":"ciao"}"#.into())
            }
        });
        let samples: Vec<f32> = audio()[..SAMPLE_RATE as usize * 5].to_vec();
        let r = transcribe(&cfg(base), &samples, None, &|_, _, _| {}, NEVER).unwrap();
        assert_eq!(r.segments.len(), 1);
        assert_eq!(r.segments[0].text, "ciao");
        assert_eq!(r.cost, None);
        assert_eq!(seen.lock().unwrap().len(), 2);
        assert!(seen.lock().unwrap()[1].1.get("language").is_none());
    }

    #[test]
    fn bad_key_fails_fast_with_the_api_message() {
        let (base, seen) =
            server(|_, _| (401, r#"{"error":{"message":"No auth credentials found"}}"#.into()));
        let err = transcribe(&cfg(base), &audio(), Some("it"), &|_, _, _| {}, NEVER).unwrap_err();
        assert!(
            matches!(&err, SttError::Auth(m) if m.contains("No auth credentials found")),
            "{err}"
        );
        // Workers stop after the first failure instead of sending every chunk.
        assert!(seen.lock().unwrap().len() <= CONCURRENCY);
    }

    #[test]
    fn silence_sends_nothing() {
        let r = transcribe(
            &cfg("http://127.0.0.1:9".into()),
            &vec![0.0; 16_000 * 40],
            None,
            &|_, _, _| {},
            NEVER,
        )
        .unwrap();
        assert_eq!((r.requests, r.segments.len()), (0, 0));
    }

    #[test]
    fn key_check_reads_limits() {
        let (base, _) = server(|_, _| {
            (200, r#"{"data":{"label":"sk-or-v1-abc...","limit":null,"usage":0.25}}"#.into())
        });
        let k = check_key(&base, "sk-test").unwrap();
        assert_eq!(k.label.as_deref(), Some("sk-or-v1-abc..."));
        assert_eq!((k.limit, k.usage), (None, Some(0.25)));
    }
}
