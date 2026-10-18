<script lang="ts" module>
  import type { TranscribeResult } from '../../lib/types';

  // The last transcript survives switching views (it isn't saved to the store yet).
  let last = $state<{ path: string; result: TranscribeResult } | null>(null);
</script>

<script lang="ts">
  // Speech to text from an audio file, with the local Whisper engine.
  // Spec: docs/features/transcribe.md
  import { onMount } from 'svelte';
  import { api, AUDIO_EXTENSIONS, isTauri, onFileDrop, pickFile, pickSavePath } from '../../lib/api';
  import ViewActions from '../../lib/components/ViewActions.svelte';
  import { app } from '../../lib/state.svelte';
  import type { Correction, SttModel, TranscriptFormat } from '../../lib/types';

  const PREFS_KEY = 'transcribe.file';
  const LANGUAGES: [string, string][] = [['it', 'Italian'], ['en', 'English'], ['auto', 'Detect']];
  const FORMATS: [TranscriptFormat, string][] = [['txt', 'Text'], ['md', 'Markdown'], ['srt', 'SRT'], ['vtt', 'VTT']];

  interface Prefs {
    model: string;
    language: string;
    beam: number;
    /** one term per line */
    glossary: string;
    /** one `from = to` rule per line */
    corrections: string;
  }

  let prefs = $state<Prefs>({ model: 'large-v3-turbo-q5_0', language: 'it', beam: 5, glossary: '', corrections: '' });
  let prefsLoaded = false;
  let models = $state<SttModel[]>([]);
  let path = $state<string | null>(last?.path ?? null);
  let jobId = $state<string | null>(null);
  let dlJobId = $state<string | null>(null);

  const job = $derived(jobId ? app.jobs.find((j) => j.id === jobId) : undefined);
  const dlJob = $derived(dlJobId ? app.jobs.find((j) => j.id === dlJobId) : undefined);
  const running = $derived(job?.state === 'running');
  const selected = $derived(models.find((m) => m.id === prefs.model));
  const result = $derived(last && last.path === path ? last.result : null);
  const fileName = $derived(path ? path.split(/[\\/]/).pop()! : '');
  const title = $derived(fileName.replace(/\.[^.]+$/, '') || 'Transcript');

  const glossaryTerms = () => prefs.glossary.split('\n').map((t) => t.trim()).filter(Boolean);
  function correctionRules(): Correction[] {
    return prefs.corrections
      .split('\n')
      .map((l) => l.split(/=>|=/))
      .filter((p) => p.length === 2 && p[0].trim())
      .map(([from, to]) => ({ from: from.trim(), to: to.trim() }));
  }

  const mb = (b: number) => `${Math.round(b / 1_048_576)} MB`;
  const clock = (ms: number) => {
    const s = Math.floor(ms / 1000);
    const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60, ss = s % 60;
    return `${h ? `${h}:` : ''}${String(m).padStart(h ? 2 : 1, '0')}:${String(ss).padStart(2, '0')}`;
  };

  async function refreshModels() {
    try {
      models = await api.sttModelsList();
    } catch (e) {
      app.error('Could not list the models', e);
    }
  }

  onMount(() => {
    let unlisten = () => {};
    void (async () => {
      const saved = await api.workspaceGet<Partial<Prefs>>(PREFS_KEY).catch(() => null);
      if (saved) prefs = { ...prefs, ...saved };
      prefsLoaded = true;
      await refreshModels();
      unlisten = await onFileDrop((paths) => {
        const p = paths.find((x) => AUDIO_EXTENSIONS.includes(x.split('.').pop()?.toLowerCase() ?? ''));
        if (p) path = p;
        else app.toast('info', 'Drop an audio file', AUDIO_EXTENSIONS.join(', '));
      });
    })();
    return () => unlisten();
  });

  // Remember the options between sessions.
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const snapshot = $state.snapshot(prefs);
    if (!prefsLoaded) return;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void api.workspaceSet(PREFS_KEY, snapshot).catch(() => {}), 400);
  });

  // Transcription finished: keep the result. Failures already show a toast.
  $effect(() => {
    const j = job;
    if (!j || j.state === 'running') return;
    if (j.state === 'done' && path) last = { path, result: j.result as TranscribeResult };
    jobId = null;
  });

  // Download finished: the model is now installed.
  $effect(() => {
    const j = dlJob;
    if (!j || j.state === 'running') return;
    dlJobId = null;
    void refreshModels();
  });

  async function openFile() {
    const p = isTauri() ? await pickFile('Open audio', AUDIO_EXTENSIONS, 'Audio') : 'C:\\mock\\sessione.wav';
    if (p) path = p;
  }

  async function transcribe() {
    if (!path || running) return;
    try {
      jobId = await api.transcribeFile(path, {
        model: prefs.model,
        language: prefs.language,
        beam_size: prefs.beam,
        glossary: glossaryTerms(),
        corrections: correctionRules(),
      });
    } catch (e) {
      app.error('Could not start the transcription', e);
    }
  }

  async function download() {
    try {
      dlJobId = await api.sttModelDownload(prefs.model);
    } catch (e) {
      app.error('Could not start the download', e);
    }
  }

  async function exportAs(format: TranscriptFormat) {
    if (!result) return;
    const ext = format;
    const out = isTauri() ? await pickSavePath('Export transcript', `${title}.${ext}`, [ext]) : `${title}.${ext}`;
    if (!out) return;
    try {
      await api.transcriptExport(result.segments, format, title, out);
      app.toast('success', `Saved ${out.split(/[\\/]/).pop()}`);
    } catch (e) {
      app.error('Could not export the transcript', e);
    }
  }

  async function copyText() {
    if (!result) return;
    try {
      await navigator.clipboard.writeText(await api.transcriptExport(result.segments, 'txt', null, null));
      app.toast('success', 'Transcript copied');
    } catch (e) {
      app.error('Could not copy', e);
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void transcribe();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<ViewActions>
  <button class="btn" onclick={openFile} title="Pick an audio file (or drop one on the window)">Open audio…</button>
  <button class="btn" disabled={!result} onclick={copyText}>Copy text</button>
  <button
    class="btn primary"
    disabled={!path || running || !selected?.installed}
    onclick={transcribe}
    title={selected?.installed ? 'Ctrl+Enter' : 'Download the model first'}
  >Transcribe file</button>
</ViewActions>

<div class="view">
  <section class="work" aria-label="Transcript">
    <div class="view-toolbar">
      {#if path}
        <span class="file" title={path}>{fileName}</span>
      {:else}
        <span class="label">No file</span>
      {/if}
      <div class="spacer"></div>
      {#if result}
        <span class="label">Export</span>
        {#each FORMATS as [f, label] (f)}
          <button class="btn" onclick={() => exportAs(f)}>{label}</button>
        {/each}
      {/if}
    </div>

    <div class="stage-area transcript">
      {#if running && job}
        <div class="empty-state progress" aria-live="polite">
          <strong>{job.message || 'Working'}…</strong>
          <progress max="1" value={job.progress} aria-label="Transcription progress"></progress>
          <button class="btn" onclick={() => jobId && api.jobsCancel(jobId)}>Cancel</button>
        </div>
      {:else if result}
        {#if result.segments.length}
          <ol class="segments" data-testid="segments">
            {#each result.segments as s, i (i)}
              <li>
                <time datetime="PT{Math.floor(s.start_ms / 1000)}S">{clock(s.start_ms)}</time>
                <p>{s.text}</p>
              </li>
            {/each}
          </ol>
        {:else}
          <div class="empty-state"><strong>No speech found</strong><span>The file was read, but Whisper heard nothing to write down.</span></div>
        {/if}
      {:else if path}
        <div class="empty-state">
          <strong>Ready</strong>
          <span>Check the options on the right, then press Transcribe file (Ctrl+Enter).</span>
        </div>
      {:else}
        <div class="empty-state">
          <strong>Turn a recording into text</strong>
          <span>Open an audio file or drop one here: {AUDIO_EXTENSIONS.slice(0, 6).join(', ')}, …</span>
        </div>
      {/if}
    </div>

    <div class="statusbar" aria-live="polite">
      {#if result}
        <span>{clock(result.duration_ms)} of audio · {result.segments.length} segment{result.segments.length === 1 ? '' : 's'} · {result.model} · {result.language}</span>
      {:else}
        <span>Runs on this computer; the audio never leaves it.</span>
      {/if}
    </div>
  </section>

  <aside class="inspector" aria-label="Transcription options">
    <section class="section">
      <h3 class="section-title">Model</h3>
      <select class="input" aria-label="Model" bind:value={prefs.model}>
        {#each models as m (m.id)}
          <option value={m.id}>{m.label}{m.installed ? '' : ` (${mb(m.size)}, not downloaded)`}</option>
        {/each}
      </select>
      {#if selected && !selected.installed}
        {#if dlJob?.state === 'running'}
          <progress max="1" value={dlJob.progress} aria-label="Download progress"></progress>
          <div class="row">
            <span class="note grow">{dlJob.message}</span>
            <button class="btn" onclick={() => dlJobId && api.jobsCancel(dlJobId)}>Cancel</button>
          </div>
        {:else}
          <button class="btn block" onclick={download}>Download ({mb(selected.size)})</button>
        {/if}
      {/if}
      <p class="note">Bigger models are more accurate and slower. On a laptop CPU, “Large” can take several times the length of the recording.</p>
    </section>

    <section class="section">
      <h3 class="section-title">Speech</h3>
      <label class="prop"><span>Language</span>
        <select class="input" bind:value={prefs.language}>
          {#each LANGUAGES as [code, label] (code)}<option value={code}>{label}</option>{/each}
        </select>
      </label>
      <div class="prop"><span>Search</span>
        <div class="segmented" role="group" aria-label="Search">
          <button aria-pressed={prefs.beam > 1} onclick={() => (prefs.beam = 5)} title="Beam search: a little more accurate">Accurate</button>
          <button aria-pressed={prefs.beam === 1} onclick={() => (prefs.beam = 1)} title="Greedy: faster">Fast</button>
        </div>
      </div>
    </section>

    <section class="section">
      <h3 class="section-title">Glossary</h3>
      <textarea class="input terms" aria-label="Glossary" rows="5" spellcheck="false" bind:value={prefs.glossary} placeholder={'Dungeon Master\nbeholder\nhit points'}></textarea>
      <p class="note">One term per line. Names and English words to spell right.</p>
    </section>

    <section class="section">
      <h3 class="section-title">Corrections</h3>
      <textarea class="input terms" aria-label="Corrections" rows="4" spellcheck="false" bind:value={prefs.corrections} placeholder={'beholdar = beholder'}></textarea>
      <p class="note">One <code>wrong = right</code> per line, applied afterwards to whole words.</p>
    </section>
  </aside>
</div>

<style>
  .file {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .transcript {
    overflow-y: auto;
  }
  .progress progress {
    width: min(320px, 80%);
  }
  .segments {
    list-style: none;
    margin: 0 auto;
    padding: var(--space-4);
    max-width: 820px;
    display: grid;
    gap: var(--space-2);
  }
  .segments li {
    display: grid;
    grid-template-columns: 4.5em 1fr;
    gap: var(--space-3);
    align-items: baseline;
  }
  .segments time {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-muted);
    text-align: right;
  }
  .segments p {
    margin: 0;
    line-height: 1.55;
    user-select: text;
  }
  .terms {
    resize: vertical;
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .inspector progress {
    width: 100%;
  }
</style>
