<script lang="ts" module>
  import type { TranscribeResult } from '../../lib/types';

  // The open file and the last transcript survive switching views (e.g. to add a
  // key in Settings). Transcripts aren't saved to the store yet.
  let openPath = $state<string | null>(null);
  let last = $state<{ path: string; result: TranscribeResult } | null>(null);
</script>

<script lang="ts">
  // Speech to text from an audio file, with local Whisper or the cloud (OpenRouter).
  // Spec: docs/features/transcribe.md
  import { onMount } from 'svelte';
  import { api, AUDIO_EXTENSIONS, isTauri, onFileDrop, pickFile, pickSavePath } from '../../lib/api';
  import Modal from '../../lib/components/Modal.svelte';
  import ViewActions from '../../lib/components/ViewActions.svelte';
  import { app } from '../../lib/state.svelte';
  import { CLOUD_STT_MODELS } from '../../lib/types';
  import type { CloudKeyStatus, Correction, SttEngine, SttModel, TranscriptFormat } from '../../lib/types';

  const PREFS_KEY = 'transcribe.file';
  /** Set once the user agreed to send audio to the cloud. */
  const CLOUD_OK_KEY = 'transcribe.cloud_ok';
  const LANGUAGES: [string, string][] = [['it', 'Italian'], ['en', 'English'], ['auto', 'Detect']];
  const FORMATS: [TranscriptFormat, string][] = [['txt', 'Text'], ['md', 'Markdown'], ['srt', 'SRT'], ['vtt', 'VTT']];

  interface Prefs {
    /** null = the default engine from Settings */
    engine: SttEngine | null;
    model: string;
    /** '' = the cloud model from Settings */
    cloudModel: string;
    language: string;
    beam: number;
    /** one term per line */
    glossary: string;
    /** one `from = to` rule per line */
    corrections: string;
  }

  let prefs = $state<Prefs>({
    engine: null, model: 'large-v3-turbo-q5_0', cloudModel: '', language: 'it', beam: 5, glossary: '', corrections: '',
  });
  // Reactive, so a change made while the saved options load is saved once they arrive.
  let prefsLoaded = $state(false);
  let models = $state<SttModel[]>([]);
  let keyStatus = $state<CloudKeyStatus | null>(null);
  let cloudOk = false;
  let confirmOpen = $state(false);
  let confirmRemember = $state(true);
  let path = $state<string | null>(openPath);
  $effect(() => {
    openPath = path;
  });
  let jobId = $state<string | null>(null);
  let dlJobId = $state<string | null>(null);

  const engine = $derived<SttEngine>(prefs.engine ?? app.settings.stt_engine);
  const cloud = $derived(engine === 'cloud');
  const job = $derived(jobId ? app.jobs.find((j) => j.id === jobId) : undefined);
  const dlJob = $derived(dlJobId ? app.jobs.find((j) => j.id === dlJobId) : undefined);
  const running = $derived(job?.state === 'running');
  const selected = $derived(models.find((m) => m.id === prefs.model));
  const ready = $derived(cloud ? !!keyStatus?.configured : !!selected?.installed);
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

  function summary(r: TranscribeResult): string {
    const n = r.segments.length;
    return [
      `${clock(r.duration_ms)} of audio`,
      `${n} segment${n === 1 ? '' : 's'}`,
      r.model,
      r.language,
      ...(r.engine === 'cloud' ? ['cloud'] : []),
      ...(r.cost != null ? [`$${r.cost.toFixed(4)}`] : []),
    ].join(' · ');
  }

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
      cloudOk = (await api.workspaceGet<boolean>(CLOUD_OK_KEY).catch(() => null)) === true;
      keyStatus = await api.sttCloudKeyStatus().catch(() => null);
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
    if (!path || running || !ready) return;
    if (cloud && !cloudOk) {
      confirmOpen = true;
      return;
    }
    try {
      jobId = await api.transcribeFile(path, {
        engine,
        model: cloud ? prefs.cloudModel.trim() || null : prefs.model,
        language: prefs.language,
        beam_size: prefs.beam,
        glossary: glossaryTerms(),
        corrections: correctionRules(),
      });
    } catch (e) {
      app.error('Could not start the transcription', e);
    }
  }

  async function confirmCloud() {
    cloudOk = true;
    confirmOpen = false;
    if (confirmRemember) void api.workspaceSet(CLOUD_OK_KEY, true).catch(() => {});
    await transcribe();
    // Without "don't ask again", ask again next time.
    if (!confirmRemember) cloudOk = false;
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
    disabled={!path || running || !ready}
    onclick={transcribe}
    title={ready ? 'Ctrl+Enter' : cloud ? 'Add an OpenRouter key in Settings first' : 'Download the model first'}
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
          <div class="empty-state"><strong>No speech found</strong><span>The file was read, but nothing was heard to write down.</span></div>
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
        <span>{summary(result)}</span>
      {:else if cloud}
        <span>Cloud: the audio is sent to OpenRouter.</span>
      {:else}
        <span>Runs on this computer; the audio never leaves it.</span>
      {/if}
    </div>
  </section>

  <aside class="inspector" aria-label="Transcription options">
    <section class="section">
      <h3 class="section-title">Engine</h3>
      <div class="segmented" role="group" aria-label="Engine">
        <button aria-pressed={!cloud} onclick={() => (prefs.engine = 'local')}>This computer</button>
        <button aria-pressed={cloud} onclick={() => (prefs.engine = 'cloud')}>Cloud</button>
      </div>
      {#if cloud}
        <label class="prop"><span>Model</span>
          <input
            class="input"
            list="transcribe-cloud-models"
            spellcheck="false"
            placeholder={app.settings.stt_cloud_model}
            bind:value={prefs.cloudModel}
          />
        </label>
        <datalist id="transcribe-cloud-models">
          {#each CLOUD_STT_MODELS as m (m)}<option value={m}></option>{/each}
        </datalist>
        {#if keyStatus?.configured}
          <p class="note ok" data-testid="cloud-key">OpenRouter key {keyStatus.hint}</p>
        {:else}
          <p class="note warn" data-testid="cloud-key">
            No OpenRouter key yet.
            <button class="btn ghost link" onclick={() => app.navigate('settings')}>Add one in Settings</button>
          </p>
        {/if}
        <p class="note">Much faster than on this computer; billed by OpenRouter per minute of audio.</p>
      {:else}
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
      {/if}
    </section>

    <section class="section">
      <h3 class="section-title">Speech</h3>
      <label class="prop"><span>Language</span>
        <select class="input" bind:value={prefs.language}>
          {#each LANGUAGES as [code, label] (code)}<option value={code}>{label}</option>{/each}
        </select>
      </label>
      {#if !cloud}
        <div class="prop"><span>Search</span>
          <div class="segmented" role="group" aria-label="Search">
            <button aria-pressed={prefs.beam > 1} onclick={() => (prefs.beam = 5)} title="Beam search: a little more accurate">Accurate</button>
            <button aria-pressed={prefs.beam === 1} onclick={() => (prefs.beam = 1)} title="Greedy: faster">Fast</button>
          </div>
        </div>
      {/if}
    </section>

    <section class="section">
      <h3 class="section-title">Glossary</h3>
      <textarea class="input terms" aria-label="Glossary" rows="5" spellcheck="false" bind:value={prefs.glossary} placeholder={'Dungeon Master\nbeholder\nhit points'}></textarea>
      <p class="note">
        One term per line. Names and English words to spell right.{#if cloud} Used by the local engine only: for the cloud, add corrections below.{/if}
      </p>
    </section>

    <section class="section">
      <h3 class="section-title">Corrections</h3>
      <textarea class="input terms" aria-label="Corrections" rows="4" spellcheck="false" bind:value={prefs.corrections} placeholder={'beholdar = beholder'}></textarea>
      <p class="note">One <code>wrong = right</code> per line, applied afterwards to whole words.</p>
    </section>
  </aside>
</div>

<Modal title="Send this audio to OpenRouter?" bind:open={confirmOpen} width={460}>
  <p>
    The cloud engine uploads <strong>{fileName}</strong> to OpenRouter, which passes it to the provider of
    <code>{prefs.cloudModel.trim() || app.settings.stt_cloud_model}</code>. OpenRouter bills your account per minute of audio.
  </p>
  <label class="check"><input type="checkbox" bind:checked={confirmRemember} /> Don't ask again</label>
  {#snippet footer()}
    <button class="btn" onclick={() => (confirmOpen = false)}>Cancel</button>
    <button class="btn primary" onclick={confirmCloud}>Send and transcribe</button>
  {/snippet}
</Modal>

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
  .link {
    padding: 0 2px;
    height: auto;
    text-decoration: underline;
  }
</style>
