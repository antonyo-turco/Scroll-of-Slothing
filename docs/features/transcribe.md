# Transcribe (speech to text)

You turn speech into text, either from an audio file or live from the microphone. Transcripts are saved with timestamps and can be exported as TXT, Markdown, SRT or VTT. This is a new feature: Orb of Many Functions has no equivalent, so there is no parity checklist. The feature checklist at the end tracks it instead.

Status: planned (phase 8, after parity).

## Engines
Two engines sit behind one `SttEngine` trait. You pick the engine in Settings, and you can override it per transcription.

### Local (default): whisper.cpp through `whisper-rs`
- Runs offline. Audio never leaves the machine.
- Models are ggml files downloaded from Hugging Face into `<data-dir>/models/`. Each download runs as a Job with progress and cancel and is checked against a pinned blake3 hash.
- Model choices: `base`, `small`, `medium` and `large-v3-turbo`, all **multilingual**. The `.en` models are not offered, because the main language is Italian.
  - Default for files: `large-v3-turbo`.
  - Default for live: `small`.
- Backend: CPU. A `vulkan` cargo feature is optional; it works on the dev machine's Intel Iris Xe.
- Uses whisper.cpp's built-in Silero VAD, its progress callback (which drives Job progress) and its abort callback (which handles cancel).
- Build: needs LLVM (`libclang`, for bindgen) and CMake with the Visual Studio generator. `scripts/check.ps1` checks for both and prints a clear error when either is missing.

### Cloud: OpenRouter format
- Calls `POST {base_url}/audio/transcriptions` with a JSON body. Fields:
  - `model`
  - `input_audio: { data: <base64>, format }`
  - `language`
  - optionally `provider.options`
- The response carries `text` and `usage`. Usage covers seconds and cost; cost is saved on the transcript.
- `base_url` defaults to `https://openrouter.ai/api/v1`. Any server that accepts the same format works.
- Example models: `openai/whisper-large-v3`, `elevenlabs/scribe-v2`.
- The API key lives in the Windows Credential Manager (`keyring` crate), never in SQLite or `settings.json`.
- The response has no timestamps. To get them, audio is split at pauses by VAD into chunks of 30 s or less. Each chunk becomes one request, and the chunk's start and end times become the segment's times. Requests run with a small concurrency limit and are retried with backoff.
- Audio leaves the machine, so the UI shows a "Cloud" badge, and the first use asks for confirmation.

## Language and vocabulary
- Default language `it`. `auto` and `en` are also available, set per transcription.
- **Glossary.** This is a list of terms that are often English words inside Italian speech: monster names, spells, "hit points", "Dungeon Master", campaign NPCs. It is stored in settings and is editable.
  - Local: the glossary is passed to Whisper as the `initial_prompt`, which pushes it toward the right spelling.
  - Cloud: the glossary is passed as the prompt field where the provider supports one.
- **Corrections.** These are find/replace rules applied after transcription (`core::stt::apply_corrections`), whole-word and case-insensitive, for example `idpoint` → `hit points`. They fix misspellings the glossary doesn't catch. Rules can be re-applied to existing transcripts.

## Audio pipeline (`crates/stt`)
- Decoding: `symphonia` (wav, mp3, flac, ogg/vorbis, opus, m4a/aac).
- Resampling: `rubato`, to 16 kHz mono f32.
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
| `transcribe_file` | Job. Arguments: blob or path, engine, model, language |
| `live_start` / `live_stop` / `live_status` | One live session at a time. A second start returns an error |
| `transcripts_list` / `transcript_get` | Get includes segments |
| `transcript_update` | Title, segment text |
| `transcript_delete` | Removes the rows. The audio blob is left for `store gc` |
| `transcript_export` | `txt`, `md`, `srt` or `vtt`, returned as a string or written to a path |
| `transcript_rerun` | New Job on the stored audio |
| `stt_audio_devices` | Input devices, for `live_start` |

## UI
- New rail destination **Transcribe**, route ids `transcribe.file`, `transcribe.live` and `transcribe.list`, with a `mic` icon.
- **File:** a drop zone; engine, model and language pickers; progress; segments as they arrive.
- **Live:** device picker, level meter, a big Start/Stop button, and the text scrolling as it is spoken.
- **Transcripts:** a virtualized list. The detail view has editable segments, search, export, re-run and copy.
- Settings gains a **Transcribe** section with:
  - engine;
  - default models;
  - language;
  - glossary;
  - corrections;
  - cloud base URL and API key;
  - model manager.
- The mock backend (`src/lib/mock.ts`) gets canned models, a fake Job and a timed fake live session, so the UI runs in a plain browser.

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
- [ ] Cloud engine: chunked timestamps, key in keyring, cost saved, confirmation on first use
- [ ] Live cloud
- [ ] MCP tools documented and covered by e2e
- [ ] UI: three tabs plus the Settings section, mock backend, axe pass in both themes
