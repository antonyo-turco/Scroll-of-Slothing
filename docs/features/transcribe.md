# Transcribe (speech to text)

You turn speech into text, either from an audio file or live from the microphone. Transcripts are saved with timestamps and can be exported as TXT, Markdown, SRT or VTT. This is a new feature: Orb of Many Functions has no equivalent, so there is no parity checklist. The feature checklist at the end tracks it instead.

Status (2026-11-05): **file transcription is built; live transcription and saved transcripts are not.** See "Not built yet" near the end.

Built:
- `crates/stt`: decoding, resampling, the local Whisper engine, model download, and the cloud engine (OpenRouter format).
- File transcription as a Job, model management, the cloud key and the time/cost estimate, each as a service fn with a Tauri command and an MCP tool (the key set/clear are not on MCP), plus the CLI.
- The **File** tab (`routes/transcribe/TranscribeFile.svelte`, route `transcribe.file`) and the Transcription tab in Settings.
- Transcripts are **not saved** as a list. Only the last finished one is kept: the Job writes it to the workspace (`transcribe.last`) before it reports done, and the File tab shows it again when it opens (`transcript_last`). Export or copy a transcript to keep it.
- Custom cloud endpoints, added in Settings (`stt_endpoints_list`).

## Engines
Two engines sit behind one `SttEngine` trait. You pick the engine in Settings, and you can override it per transcription.

### Local (default): whisper.cpp through `whisper-rs`
- Runs offline. Audio never leaves the machine.
- Models are ggml files downloaded from the whisper.cpp Hugging Face repo into `<data-dir>/models/`.
  - Each download runs as a Job with progress and cancel.
  - It is written to a `.part` file and checked against the SHA-256 that Hugging Face publishes, pinned in `sloth_stt::models::CATALOG`. It is renamed only if the hash matches.
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
- Uses whisper.cpp's progress callback (which drives Job progress) and its abort callback (which handles cancel). No VAD is used.
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

### Cloud: OpenRouter format (`sloth_stt::cloud`)
- **Request:** `POST {base_url}/audio/transcriptions`.
  - Headers: `Authorization: Bearer <key>`, `X-Title: Kastanomata's Scroll of Slothing`.
  - JSON body: `model`, `input_audio: { data: <base64 WAV>, format: "wav" }`, and `language` unless it is `auto`.
- **Reply:** `text` is required. `usage.cost` is added up across the chunks and shown in the status bar and the Job result (USD).
- **Settings:**
  - `stt_cloud_base_url` defaults to `https://openrouter.ai/api/v1`. Any server that accepts the same format works.
  - `stt_cloud_model` defaults to `openai/whisper-1`.
  - The UI suggests `openai/whisper-1`, `openai/gpt-transcribe`, `mistralai/voxtral-mini-transcribe`, `mistralai/voxtral-mini-transcribe-3-0` and `mistralai/voxtral-small-transcribe-3-0`, but any id works.
- **API key:** stored in the Windows Credential Manager (`keyring` crate, entry `sloth` / `openrouter_api_key`), never in SQLite or settings.
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
- **Nemotron 3:** `nvidia/nemotron-3.5-asr-streaming-multilingual-0.6b` is in the suggested OpenRouter models ($0.00018 / min). A NVIDIA NIM or vLLM server hosting it can be added as an endpoint (below).
- **Custom endpoints (Settings → Transcription → Cloud endpoints):**
  - Each has a name, an address, a request format, a list of model ids and an optional key. They are stored in the workspace entry `transcribe.endpoints`; keys go in the credential store (`stt_endpoint_key:<id>`), never in SQLite, and never come back from the service.
  - Formats: `openrouter` (the JSON body above) or `openai` (multipart form with `file`, `model`, `language`, `response_format=json`, as OpenAI, vLLM and NIM accept). The reply is read the same way (`text`).
  - Without a key no `Authorization` header is sent, for servers on your own network.
  - OpenRouter is always listed first with id `openrouter`; its address stays the `stt_cloud_base_url` setting. `TranscribeOptions.endpoint` / the `endpoint` argument of `transcribe_file` picks one. Service: `stt_endpoints_list`, `stt_endpoint_save`, `stt_endpoint_delete`. MCP can list endpoints but not change them.
- **Nothing is lost:**
  - Every cloud chunk is saved to the workspace entry `transcribe.partial` as it arrives. If a later chunk fails, the Job is cancelled or the app dies, running the same transcription again (same file, endpoint, model and language) reuses the saved chunks and only sends the missing ones. The error says how many parts are saved.
  - A finished transcript is written to its own file `transcripts/<unix time>-<name>.json` in the data folder (never overwritten), and also saved as `transcribe.last`. The partial entry is cleared only after both succeeded.
  - Local transcription has no partial state: whisper.cpp either finishes or fails.
- **Privacy:** the first cloud transcription asks for confirmation ("Send this audio to OpenRouter?"). "Don't ask again" is stored in the workspace as `transcribe.cloud_ok`. The status bar says when the cloud engine is selected.
- **Not yet:** the glossary is not sent to the cloud, because the prompt field isn't confirmed for OpenRouter's endpoint. Corrections apply to cloud results as well.

## Language and vocabulary
- Default language `it`. `auto` and `en` are also available, set per transcription.
- **Glossary.** This is a list of terms that are often English words inside Italian speech: monster names, spells, "hit points", "Dungeon Master", campaign NPCs. It is entered in the File tab, one term per line, and kept with the tab's options in the `transcribe.file` workspace (not in settings).
  - Local: the glossary is passed to Whisper as the `initial_prompt`, which pushes it toward the right spelling.
  - Cloud: not sent yet (see above); use corrections.
- **Corrections.** These are find/replace rules applied after transcription (`core::stt::apply_corrections`), whole-word and case-insensitive, for example `idpoint` → `hit points`. They fix misspellings the glossary doesn't catch. They are also entered in the File tab (one `find=replace` per line) and kept in `transcribe.file`.

## Audio pipeline (`crates/stt`)
- Decoding: `symphonia` 0.5 (wav, mp3, flac, ogg/vorbis, m4a/aac/alac, mkv/webm, caf, aiff). Multichannel audio is averaged to mono, and corrupt frames are skipped. Opus is not supported by symphonia 0.5.
- Resampling: `rubato` (FFT, fixed ratio), to 16 kHz mono f32. The filter delay is removed, so the output has exactly `round(len * 16000 / rate)` samples, aligned with the input.
- `crates/core` stays pure. `core::stt` holds the TXT/MD/SRT/VTT formatters (snapshot tests), glossary prompt building, corrections, the cloud chunk planner and WAV writer, and the speed estimate and blending.

## File transcription
1. Pick or drop a file in the File tab (or pass a path over MCP or the CLI). The file is read where it is; it is not copied into the blob store.
2. `transcribe_file` starts a Job on the blocking pool: decode, resample, run the engine (local, or the cloud in chunks), apply the corrections.
3. The Job shows progress and can be cancelled. Cancelling discards the work: the Job ends as cancelled with no segments.
4. The Job result is `{engine, model, language, duration_ms, segments, cost}`; the File tab shows the timestamped segments, with Copy and export.

## Service, Tauri and MCP
Each function is a service fn, with a Tauri command and an MCP tool of the same name.

| Function | Notes |
|---|---|
| `stt_models_list` | The catalog, with what is installed |
| `stt_model_download` / `stt_model_delete` | Download is a Job (hash-checked) |
| `stt_cloud_models_list` | Suggested cloud models with their price per minute |
| `stt_cloud_key_status` / `stt_cloud_key_test` | Whether a key is available (never the key itself) and a live check |
| `stt_cloud_key_set` / `stt_cloud_key_clear` | Tauri and CLI only, so a secret never passes through an LLM's context |
| `transcribe_estimate` | The file's duration, the time per local model and for the cloud, and the cloud cost |
| `transcribe_file` | Job. Arguments: path, engine, model, language, glossary, corrections, beam size |
| `stt_endpoints_list` | Built-in OpenRouter plus custom endpoints, with model ids and whether a key is set. MCP: list only |
| `stt_endpoint_save` / `stt_endpoint_delete` | UI only (they carry keys) |
| `transcript_last` | The last finished transcription, `{ path, result }`, kept in the workspace (`transcribe.last`) |
| `transcript_export` | Segments to `txt`, `md`, `srt` or `vtt`, returned as a string or written to a path |

## UI
- Rail destination **Transcribe** with a `mic` icon. Only the route `transcribe.file` exists.
- **File**:
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
- Settings has a **Transcription** tab:
  - the default engine (the same switch);
  - the OpenRouter key: status, Save, Test (usage and credit limit) and Remove;
  - the cloud model, with prices;
  - under **Advanced**: a free model id and the API address.
  - Explanations are in ⓘ tips here too.
  - Still to come: default local models, a default glossary and corrections, and the model manager.
- The mock backend (`src/lib/mock-stt.ts`) has canned models and a fake Job, so the File tab runs in a plain browser.

## CLI
```
sloth stt models                      # catalog, with what is installed
sloth stt download large-v3-turbo-q5_0
sloth stt delete base
sloth transcribe session.mp3 --format srt --out session.srt `
    --glossary "Dungeon Master" --glossary beholder `
    --correct "beholdar=beholder" [--model small] [--language it|en|auto] [--beam 1] [--threads 4]

sloth stt key set                     # paste the OpenRouter key (read from stdin)
sloth stt key status | test | clear
sloth transcribe session.mp3 --engine cloud [--model openai/gpt-transcribe]   # prints the cost on stderr
```
`--json` prints the full result: model, language, duration and segments.

## Not built yet
Designed but not implemented. Nothing below exists in the code.
- **Saved transcripts.** A migration with `transcripts` (`id`, `title`, `source_blob`, `engine`, `model`, `language`, `duration_s`, `status`, `cost`, timestamps) and `transcript_segments` (`transcript_id`, `idx`, `start_ms`, `end_ms`, `text`, `confidence`); a **Transcripts** tab with editable segments, search, export and re-run; the tools `transcripts_list`, `transcript_get`, `transcript_update`, `transcript_delete`, `transcript_rerun`; storing the input audio as a blob; streaming segments while the Job runs and keeping them as `partial` on cancel.
- **Live transcription.** Microphone capture with `cpal` in the service, VAD, re-transcribing the last ~10 s about once a second and finalising text by agreement (local), one request per utterance (cloud), "keep audio" for re-runs; a **Live** tab with device picker and level meter; `live_start`, `live_stop`, `live_status`, `stt_audio_devices`.
- **Settings:** default local models, a default glossary and corrections, a model manager.
- **Cloud glossary:** the glossary is not sent to the cloud engine.
- **Tests:** MCP e2e coverage and a perf note in `perf.md`.
- Optional later: a Vulkan build (the `vulkan` cargo feature exists but is not used in builds), speaker labels, attaching a transcript to the Library.

## Feature checklist
An item is ticked only when it is built **and** tested.
- [ ] Local file transcription (Italian, with the glossary), Job progress and cancel. *Built; no automated test runs Whisper (it needs a downloaded model).*
- [ ] Model download, hash check and delete. *Built; `crates/stt/src/models.rs` tests the catalog, the size check and a failed download, but not a real download.*
- [x] Corrections: whole-word, case-insensitive (`core::stt` tests). Re-applying them to saved transcripts waits for saved transcripts.
- [x] Export: TXT, MD, SRT, VTT (snapshot tests)
- [x] Cloud engine for files: chunked timestamps, key in keyring, cost reported, confirmation on first use (`core::stt` and `crates/stt/src/cloud.rs` tests; cost is not saved because transcripts aren't stored)
- [ ] Saved transcripts and the Transcripts tab. *Not built.*
- [ ] Live local and live cloud. *Not built.*
- [ ] MCP tools covered by e2e. *Documented in `docs/mcp-tools.md`; no e2e yet.*
- [ ] UI: File tab and the Settings section are built, with a Playwright spec on the mock (`app/e2e/transcribe.spec.ts`); the Live and Transcripts tabs are not built.
