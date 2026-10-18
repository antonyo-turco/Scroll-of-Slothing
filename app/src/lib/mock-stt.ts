// In-memory speech-to-text backend for browser dev and tests. Jobs report a few
// progress steps on a timer, then finish with canned Italian segments.

import type {
  CloudKeyStatus, CloudModelSpec, Correction, Segment, Settings, SttModel, TranscribeEstimate, TranscribeOptions,
  TranscriptFormat,
} from './types';

type Emit = (e: Record<string, unknown>) => void;
type Handler = (args: Record<string, unknown>) => unknown;

const MB = 1_048_576;

const SAMPLE: Segment[] = [
  { start_ms: 0, end_ms: 2_600, text: 'Benvenuti al tavolo.' },
  { start_ms: 2_600, end_ms: 6_100, text: 'Il Dungeon Master descrive una caverna buia.' },
  { start_ms: 6_100, end_ms: 8_200, text: 'Un beholdr vi osserva dall’alto.' },
  { start_ms: 8_200, end_ms: 12_600, text: 'Tirate per l’iniziativa, poi il guerriero perde 12 hit points.' },
];

const clock = (ms: number, sep: string | null) => {
  const h = Math.floor(ms / 3_600_000), m = Math.floor(ms / 60_000) % 60, s = Math.floor(ms / 1000) % 60;
  const p = (n: number, w = 2) => String(n).padStart(w, '0');
  return sep === null ? `${h}:${p(m)}:${p(s)}` : `${p(h)}:${p(m)}:${p(s)}${sep}${p(ms % 1000, 3)}`;
};

/** Mirrors sleuth_core::stt::export. */
export function mockExport(segments: Segment[], format: TranscriptFormat, title: string | null): string {
  const segs = segments.filter((s) => s.text.trim());
  switch (format) {
    case 'txt':
      return segs.map((s) => s.text.trim() + '\n').join('');
    case 'md':
      return (title ? `# ${title}\n\n` : '') + segs.map((s) => `**[${clock(s.start_ms, null)}]** ${s.text.trim()}\n\n`).join('');
    case 'srt':
      return segs.map((s, i) => `${i + 1}\n${clock(s.start_ms, ',')} --> ${clock(s.end_ms, ',')}\n${s.text.trim()}\n\n`).join('');
    case 'vtt':
      return 'WEBVTT\n\n' + segs.map((s) => `${clock(s.start_ms, '.')} --> ${clock(s.end_ms, '.')}\n${s.text.trim()}\n\n`).join('');
  }
}

const applyCorrections = (text: string, rules: Correction[]) =>
  rules
    .filter((r) => r.from.trim())
    .reduce((t, r) => {
      const esc = r.from.trim().replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      return t.replace(new RegExp(`(?<![\\p{L}\\p{N}_])${esc}(?![\\p{L}\\p{N}_])`, 'giu'), r.to);
    }, text);

export function createSttMockHandlers(emit: Emit, settings: () => Settings): Record<string, Handler> {
  let seq = 0;
  let key: string | null = null;
  const keyStatus = (): CloudKeyStatus =>
    key ? { configured: true, source: 'saved', hint: `${key.slice(0, 6)}…${key.slice(-4)}` } : { configured: false, source: null, hint: null };
  const models: SttModel[] = [
    { id: 'base', label: 'Base', hint: 'Fast, rough', size: 141 * MB, installed: false, speed: 0.45, measured: false },
    { id: 'small', label: 'Small', hint: 'Quick, decent', size: 465 * MB, installed: true, speed: 1.35, measured: false },
    { id: 'medium-q5_0', label: 'Medium (quantized)', hint: 'Accurate, slow', size: 514 * MB, installed: false, speed: 4.8, measured: false },
    { id: 'large-v3-turbo-q5_0', label: 'Large v3 Turbo (quantized)', hint: 'Most accurate per minute of work', size: 547 * MB, installed: true, speed: 6.5, measured: true },
    { id: 'large-v3-turbo', label: 'Large v3 Turbo', hint: 'Best, slowest on a CPU', size: 1549 * MB, installed: false, speed: 9, measured: false },
  ];
  const cloudModels: CloudModelSpec[] = [
    { id: 'openai/whisper-1', label: 'OpenAI Whisper', usd_per_min: 0.006 },
    { id: 'openai/gpt-transcribe', label: 'OpenAI GPT Transcribe', usd_per_min: 0.0045 },
    { id: 'mistralai/voxtral-mini-transcribe', label: 'Mistral Voxtral Mini', usd_per_min: 0.003 },
  ];
  const cancelled = new Set<string>();

  /** Runs `steps` progress ticks, then finishes with `result()`. */
  const job = (kind: string, steps: string[], result: () => unknown) => {
    const id = `mock-job-stt-${++seq}`;
    const base = { id, kind, result: null, error: null };
    steps.forEach((message, i) =>
      setTimeout(() => {
        if (cancelled.has(id)) return;
        emit({ type: 'job', ...base, state: 'running', progress: i / steps.length, message });
      }, 30 * i),
    );
    setTimeout(() => {
      const state = cancelled.has(id) ? 'cancelled' : 'done';
      emit({ type: 'job', ...base, state, progress: 1, message: '', result: state === 'done' ? result() : null });
    }, 30 * steps.length);
    return id;
  };

  const model = (id: unknown) => {
    const m = models.find((x) => x.id === id);
    if (!m) throw { code: 'invalid_input', message: `invalid input: model: unknown model "${String(id)}"` };
    return m;
  };

  return {
    stt_models_list: () => models.map((m) => ({ ...m })),
    stt_model_download: ({ id }) => {
      const m = model(id);
      return job('model download', ['0 / 500 MB', '250 / 500 MB'], () => {
        m.installed = true;
        return { id: m.id, path: `C:\\mock\\models\\${m.id}.bin` };
      });
    },
    stt_model_delete: ({ id }) => {
      model(id).installed = false;
      return null;
    },
    stt_cloud_models_list: () => cloudModels,
    // The mock "file" is always 12.6 s long.
    transcribe_estimate: ({ beamSize, cloudModel }): TranscribeEstimate => {
      const d = 12_600, f = beamSize === 1 ? 0.65 : 1;
      const model = String(cloudModel ?? '').trim() || settings().stt_cloud_model;
      const price = cloudModels.find((m) => m.id === model)?.usd_per_min ?? null;
      return {
        duration_ms: d,
        local: models.map((m) => ({ model: m.id, seconds: (d / 1000) * m.speed * f + 1, measured: m.measured, cost: null })),
        cloud: { model, seconds: (d / 1000) * 0.06 + 3, measured: false, cost: price === null ? null : (d / 60_000) * price },
      };
    },
    stt_cloud_key_status: () => keyStatus(),
    stt_cloud_key_set: ({ key: k }) => {
      const v = String(k ?? '').trim();
      if (!v || /\s/.test(v)) throw { code: 'invalid_input', message: 'invalid input: the API key must be one word, without spaces' };
      key = v;
      return keyStatus();
    },
    stt_cloud_key_clear: () => {
      key = null;
      return keyStatus();
    },
    stt_cloud_key_test: () => {
      if (!key) throw { code: 'invalid_input', message: 'invalid input: no OpenRouter API key: add one in Settings → Transcription' };
      if (key.includes('bad')) throw { code: 'invalid_input', message: 'invalid input: cloud: the API key was rejected (401): No auth credentials found' };
      return { label: `${key.slice(0, 10)}...`, limit: null, usage: 0.42 };
    },
    transcribe_file: ({ path, opts }) => {
      const o = (opts ?? {}) as TranscribeOptions;
      if (!String(path).trim()) throw { code: 'not_found', message: 'not found: file' };
      const segments = () => SAMPLE.map((s) => ({ ...s, text: applyCorrections(s.text, o.corrections ?? []) }));
      if ((o.engine ?? settings().stt_engine) === 'cloud') {
        if (!key) throw { code: 'invalid_input', message: 'invalid input: no OpenRouter API key: add one in Settings → Transcription (or set OPENROUTER_API_KEY)' };
        const cloudModel = o.model?.trim() || settings().stt_cloud_model;
        return job('transcribe', ['decoding audio', 'sent 0 of 1 parts'], () => ({
          engine: 'cloud', model: cloudModel, language: o.language ?? 'it', duration_ms: 12_600, segments: segments(), cost: 0.0013,
        }));
      }
      const m = model(o.model ?? 'large-v3-turbo-q5_0');
      if (!m.installed) throw { code: 'not_found', message: `not found: model ${m.id} is not installed; download it first` };
      return job('transcribe', ['decoding audio', 'loading model', 'transcribing', 'transcribing'], () => ({
        engine: 'local', model: m.id, language: o.language ?? 'it', duration_ms: 12_600, segments: segments(), cost: null,
      }));
    },
    transcript_export: ({ segments, format, title }) =>
      mockExport(segments as Segment[], format as TranscriptFormat, (title as string | null) ?? null),
    // Replaces the shared no-op: the other mocks' jobs finish within 10 ms, so
    // only these ones can still be cancelled.
    jobs_cancel: ({ id }) => {
      cancelled.add(String(id));
      return null;
    },
  };
}
