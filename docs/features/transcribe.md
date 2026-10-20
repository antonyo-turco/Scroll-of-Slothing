# Transcribe (speech to text)

You turn speech into text, either from an audio file or live from the microphone. Transcripts are saved with timestamps and can be exported as TXT, Markdown, SRT or VTT. This is a new feature: Orb of Many Functions has no equivalent, so there is no parity checklist. The feature checklist at the end tracks it instead.

Status: in progress (phase 8, after parity). Done so far:
- Step 1: `crates/stt`, file transcription and model management in the service and CLI.
- The cloud engine for file transcription (OpenRouter format), with the key kept in the Windows Credential Manager and set from Settings → Transcription.
- A first slice of the UI: the **File** tab (`transcribe.file`, `routes/transcribe/TranscribeFile.svelte`).
  - Pick or drop a file; choose model, language, Accurate/Fast, glossary and corrections. These are kept in the workspace store under `transcribe.file`.
  - Progress with cancel, timestamped segments, copy, export.
  - Matching Tauri commands and MCP tools. Transcripts are not saved yet: the last one is kept in memory for the session.

## Engines
Two engines sit behind one `SttEngine` trait. You pick the engine in Settings, and you can override it per transcription.

### Local (default): whisper.cpp through `whisper-rs`
- Runs offline. Audio never leaves the machine.
- Models are ggml files downloaded from the whisper.cpp Hugging Face repo into `<data-dir>/models/`.
  - Each download runs as a Job with progress and cancel.
  - It is written to a `.part` file and checked against the SHA-256 that Hugging Face publishes, pinned in `sleuth_stt::models::CATALOG`. It is renamed only if the hash matches.
  - A model counts as installed when its file has the expected size.
- Model choices, all **multilingual** (the `.en` models are not offered, because the main language is Italian):

  | id | Size | Use |
  |---|---|---|
  | `base` | 141 MB | Fast, rough |
  | `small` | 465 MB | Default for live |
  | `medium-q5_0` | 514 MB | |
  | `large-v3-turbo-q5_0` | 547 MB | Default for files |
  | `large-v3-turbo` | 1549 MB | Best, slow on the CPU |
- The last loaded model stays in memory, so repeated transcriptions skip the load.
- Backend: CPU. A `vulkan` cargo feature is optional; it works on the dev machine's Intel Iris Xe.
- Uses whisper.cpp's built-in Silero VAD, its progress callback (which drives Job progress) and its abort callback (which handles cancel).
- Build: needs LLVM (`libclang`, for bindgen) and CMake (the Visual Studio generator is used).
  - `.cargo/config.toml` sets `LIBCLANG_PATH` to the default LLVM path and `CMAKE_BUILD_TYPE=Release`. In dev, whisper-rs-sys would otherwise build `RelWithDebInfo`, and on MSVC that limits inlining.
  - On MSVC, ggml detects the CPU's SIMD itself (`FindSIMD.cmake`). The dev machine gets AVX-512.
- **Measured speed** (dev machine, i7-1185G7 at about 1.5 GHz under load because it is power-throttled, 12.6 s Italian TTS clip, with the glossary):

  | Model | Time | Result |
  |---|---|---|
  | `base` | ~5 s | misses "beholder" |
  | `small` | 11 s (beam 1), 17 s (beam 5) | misses "beholder" |
  | `medium-q5_0` | 53 s | all terms right |
  | `large-v3-turbo-q5_0` | ~76 s | "Beholdar" |

  On this CPU only `base` runs faster than real time, so live mode with `small` needs the GPU (Vulkan) or a faster machine. Release vs RelWithDebInfo, OpenMP, AVX2 vs AVX-512 and clang-cl vs MSVC were all tried: none changed these numbers by more than about 20%.
- **whisper-rs 0.16 bug:** `set_abort_callback_safe` casts its user data to the wrong type, so a plain closure reads garbage and aborts at random ("failed to encode", -6). `local.rs` passes a pre-boxed `Box<dyn FnMut() -> bool>`, which makes the types agree. Recheck this when upgrading whisper-rs.

### Cloud: OpenRouter format (`sleuth_stt::cloud`)
- **Request:** `POST {base_url}/audio/transcriptions`.
  - Headers: `Authorization: Bearer <key>`, `X-Title: Kastanomata's Scroll of Sleuthing`.
  - JSON body: `model`, `input_audio: { data: <base64 WAV>, format: "wav" }`, and `language` unless it is `auto`.
- **Reply:** `text` is required. `usage.cost` is added up across the chunks and shown in the status bar and the Job result (USD).
- **Settings:**
  - `stt_cloud_base_url` defaults to `https://openrouter.ai/api/v1`. Any server that accepts the same format works.
  - `stt_cloud_model` defaults to `openai/whisper-1`.
  - The UI suggests `openai/whisper-1`, `openai/gpt-transcribe`, `mistralai/voxtral-mini-transcribe`, `mistralai/voxtral-mini-transcribe-3-0` and `mistralai/voxtral-small-transcribe-3-0`, but any id works.
- **API key:** stored in the Windows Credential Manager (`keyring` crate, entry `sleuth` / `openrouter_api_key`), never in SQLite or settings.
  - With no saved key, the `OPENROUTER_API_KEY` environment variable is used, which is handy for the CLI and standalone MCP.
  - The service never returns the key: only `configured`, the `source` (`saved`/`env`) and a hint such as `sk-or-…a1b2`.
  - Tests use an in-memory store.
- **Key test:** `GET {base_url}/key` returns the key's label, credit limit and usage.
- **Chunks:** the reply has no timestamps, so the audio is cut into chunks (`core::stt::plan_chunks`).
  - Each chunk lasts 8 to 28 s. It ends at the quietest 30 ms frame of that window, so cuts fall in pauses.
  - Chunks quieter than -54 dBFS RMS are skipped, so silence is neither paid for nor hallucinated.
  - Each chunk is 16 kHz mono PCM16 WAV, sent as one request, and becomes one segment with the chunk's start and end times.
- **Requests:** 4 in parallel.
  - 408, 429, 5xx and network errors are retried up to 4 times, with 1, 2 and 4 s waits.
  - 401/403 ("the API key was rejected") and 402 ("not enough credits") stop at once, with the API's own message.
  - Other 4xx stop with the API's message. Waiting between retries can be cancelled.
- **Privacy:** the first cloud transcription asks for confirmation ("Send this audio to OpenRouter?"). "Don't ask again" is stored in the workspace as `transcribe.cloud_ok`. The status bar says when the cloud engine is selected.
- **Not yet:** the glossary is not sent to the cloud, because the prompt field isn't confirmed for OpenRouter's endpoint. Corrections apply to cloud results as well.

## Language and vocabulary
- Default language `it`. `auto` and `en` are also available, set per transcription.
- **Glossary.** This is a list of terms that are often English words inside Italian speech: monster names, spells, "hit points", "Dungeon Master", campaign NPCs. It is stored in settings and is editable.
  - Local: the glossary is passed to Whisper as the `initial_prompt`, which pushes it toward the right spelling.
  - Cloud: not sent yet (see above); use corrections.
- **Corrections.** These are find/replace rules applied after transcription (`core::stt::apply_corrections`), whole-word and case-insensitive, for example `idpoint` → `hit points`. They fix misspellings the glossary doesn't catch. Rules can be re-applied to existing transcripts.

## Audio pipeline (`crates/stt`)
- Decoding: `symphonia` 0.5 (wav, mp3, flac, ogg/vorbis, m4a/aac/alac, mkv/webm, caf, aiff). Multichannel audio is averaged to mono, and corrupt frames are skipped. Opus is not supported by symphonia 0.5.
- Resampling: `rubato` (FFT, fixed ratio), to 16 kHz mono f32. The filter delay is removed, so the output has exactly `round(len * 16000 / rate)` samples, aligned with the input.
- Microphone capture: `cpal`, in Rust. Capture is owned by the service, so MCP and the CLI can run live sessions too. This also means no WebView2 microphone permission and no audio over IPC.
- `crates/core` stays pure. `core::stt` holds:
  - segment merging;
  - the rule that finalises live text;
  - glossary prompt building;
  - corrections;
  - the TXT/MD/SRT/VTT formatters, with snapshot tests.

## File transcription
1. Pick or drop a file. Its bytes become a blob (content-addressed, so the same file is stored once).
2. `transcribe_file` starts a Job and runs it on the blocking pool:
   1. decode;
   2. resample;
   3. run the engine;
   4. save the segments.
3. Segments appear as they are produced (`Event::TranscriptSegment`). The Job shows a progress bar and can be cancelled. Cancelling keeps the segments done so far, marked `partial`.

## Live transcription
- Start and stop from the Live tab, or from MCP or the CLI. You choose the input device.
- **Local engine:**
  - VAD splits speech at pauses.
  - The last ~10 s of audio are re-transcribed about once a second.
  - Text is finalised by agreement: a prefix that comes out the same in two passes in a row becomes final and is not changed again.
  - The UI shows final text normally and provisional text in grey.
- **Cloud engine:** each VAD utterance is sent once it ends. You get final text after about 1 to 2 s, with no provisional text.
- **Keep audio** (on by default): the session's audio is saved as a blob, so the transcript can be **re-run** later with a larger model or the other engine.

## Data (new migration)
- `transcripts`: `id`, `title`, `source_blob` (audio, nullable), `engine` (`local`/`cloud`), `model`, `language`, `duration_s`, `status` (`running`/`done`/`partial`/`failed`), `cost` (nullable), `created_at`, `updated_at`.
- `transcript_segments`: `transcript_id`, `idx`, `start_ms`, `end_ms`, `text`, `confidence` (nullable).
- Every write appends to `change_log`. Segments can be edited inline; an edit replaces the segment's text and keeps its timing.

## Service, Tauri and MCP
Each function is a service fn, with a Tauri command and an MCP tool of the same name.

| Function | Notes |
|---|---|
| `stt_models_list` | Installed and available models, with size and hash |
| `stt_model_download` / `stt_model_delete` | Download is a Job |
| `stt_cloud_key_status` / `stt_cloud_key_test` | Whether a key is available (never the key itself) and a live check. MCP has only these two: keys are set in Settings or with `sleuth stt key set`, so a secret never passes through an LLM's context |
| `stt_cloud_key_set` / `stt_cloud_key_clear` | UI and CLI only (see above) |
| `transcribe_file` | Job. Arguments: path, engine, model, language, glossary, corrections, beam size |
| `live_start` / `live_stop` / `live_status` | One live session at a time. A second start returns an error |
| `transcripts_list` / `transcript_get` | Get includes segments |
| `transcript_update` | Title, segment text |
| `transcript_delete` | Removes the rows. The audio blob is left for `store gc` |
| `transcript_export` | `txt`, `md`, `srt` or `vtt`, returned as a string or written to a path |
| `transcript_rerun` | New Job on the stored audio |
| `stt_audio_devices` | Input devices, for `live_start` |

## UI
- New rail destination **Transcribe**, route ids `transcribe.file`, `transcribe.live` and `transcribe.list`, with a `mic` icon.
- **File** (built):
  - The work area holds the file name and duration, progress with Cancel, timestamped segments, Copy and the exports.
  - The inspector, top to bottom:
    - **Engine:** a full-width *This computer / Cloud* switch with icons.
      - This computer: the model picker, a card with the model's hint and size, and Download when it's missing.
      - Cloud: the model picker, a card with the model id and its price per minute, and the key status (or a link to Settings).
    - **Speech:** language; *Search* (Accurate/Fast) on this computer only.
    - **Glossary:** this computer only, hidden for the cloud because it isn't sent there.
    - **Corrections:** both engines.
    - **Time needed:** for the open file, one row per engine: the time (and for the cloud, the cost) and whether it is *measured here* or an *estimate*. Clicking a row switches engine.
  - Explanations live in ⓘ tips (`components/InfoTip.svelte`): shown on hover and keyboard focus, closed with Escape, positioned `fixed` so the scrolling inspector doesn't clip them.
  - Options are kept in the workspace (`transcribe.file`). The save is debounced and flushed when the view closes.
- **Estimates** (`transcribe_estimate`):
  - The duration is read from the file's header, or by walking its packets without decoding (`audio::probe_duration_ms`).
  - Time = duration × speed + overhead (1 s local, 3 s cloud).
  - Speeds start from the catalog (local, measured on the dev laptop) or `cloud::DEFAULT_SPEED` (0.06). After each finished run of 15 s or more, the real speed is blended in (50/50 moving average) and stored in the workspace as `transcribe.speeds`. Greedy runs are scaled to beam 5 with `GREEDY_FACTOR` (0.65).
  - Cloud cost = minutes × the model's listed price (`cloud::MODELS`: Whisper $0.006, GPT Transcribe $0.0045, Voxtral Mini $0.003, Voxtral Mini Transcribe 3 $0.0035, Voxtral Small Transcribe 3 $0.004 per minute, as of 2026-10). Unknown models show "price unknown".
- **Live:** device picker, level meter, a big Start/Stop button, and the text scrolling as it is spoken.
- **Transcripts:** a virtualized list. The detail view has editable segments, search, export, re-run and copy.
- Settings has a **Transcription** card (built):
  - the default engine (the same switch);
  - the OpenRouter key: status, Save, Test (usage and credit limit) and Remove;
  - the cloud model, with prices;
  - under **Advanced**: a free model id and the API address.
  - Explanations are in ⓘ tips here too.
  - Still to come: default local models, a default glossary and corrections, and the model manager.
- The mock backend (`src/lib/mock.ts`) gets canned models, a fake Job and a timed fake live session, so the UI runs in a plain browser.

## CLI
```
sleuth stt models                      # catalog, with what is installed
sleuth stt download large-v3-turbo-q5_0
sleuth stt delete base
sleuth transcribe session.mp3 --format srt --out session.srt `
    --glossary "Dungeon Master" --glossary beholder `
    --correct "beholdar=beholder" [--model small] [--language it|en|auto] [--beam 1] [--threads 4]

sleuth stt key set                     # paste the OpenRouter key (read from stdin)
sleuth stt key status | test | clear
sleuth transcribe session.mp3 --engine cloud [--model openai/gpt-transcribe]   # prints the cost on stderr
```
`--json` prints the full result: model, language, duration and segments.

## Phases
1. `crates/stt`: decoding, resampling, the local engine, model download; `sleuth-cli transcribe <file>`.
2. Migration, service, Jobs, the File and Transcripts tabs, export, glossary and corrections.
3. Live: `cpal`, VAD, finalising by agreement, level meter.
4. Cloud engine (OpenRouter format), keyring, chunking, cost.
5. MCP tools, `docs/mcp-tools.md`, e2e (`mcp_e2e.rs` and Playwright on the mock), and a perf note in `perf.md`.
6. Optional later: Vulkan build, speaker labels, attaching a transcript to the Library.

## Feature checklist
- [ ] Local file transcription (Italian, with the glossary), Job progress and cancel
- [ ] Model download, hash check and delete
- [ ] Corrections applied and re-applicable
- [ ] Export: TXT, MD, SRT, VTT (snapshot tests)
- [ ] Live local: provisional and final text, keep-audio, re-run
- [x] Cloud engine for files: chunked timestamps, key in keyring, cost reported, confirmation on first use (cost is not *saved* yet: transcripts aren't stored)
- [ ] Live cloud
- [ ] MCP tools documented and covered by e2e
- [ ] UI: three tabs plus the Settings section, mock backend, axe pass in both themes
