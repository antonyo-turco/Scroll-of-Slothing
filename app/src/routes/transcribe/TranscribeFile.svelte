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
  import Icon from '../../lib/components/Icon.svelte';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import Modal from '../../lib/components/Modal.svelte';
  import ViewActions from '../../lib/components/ViewActions.svelte';
  import { app } from '../../lib/state.svelte';
  import type {
    CloudKeyStatus, CloudModelSpec, Correction, SttEngine, SttModel, TranscribeEstimate, TranscriptFormat,
  } from '../../lib/types';

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
  let cloudModels = $state<CloudModelSpec[]>([]);
  let keyStatus = $state<CloudKeyStatus | null>(null);
  let estimate = $state<TranscribeEstimate | null>(null);
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
  const cloudModel = $derived(prefs.cloudModel.trim() || app.settings.stt_cloud_model);
  const cloudSpec = $derived(cloudModels.find((m) => m.id === cloudModel));
  const job = $derived(jobId ? app.jobs.find((j) => j.id === jobId) : undefined);
  const dlJob = $derived(dlJobId ? app.jobs.find((j) => j.id === dlJobId) : undefined);
  const running = $derived(job?.state === 'running');
  const selected = $derived(models.find((m) => m.id === prefs.model));
  const ready = $derived(cloud ? !!keyStatus?.configured : !!selected?.installed);
  const result = $derived(last && last.path === path ? last.result : null);
  const fileName = $derived(path ? path.split(/[\\/]/).pop()! : '');
  const title = $derived(fileName.replace(/\.[^.]+$/, '') || 'Transcript');
  const localEstimate = $derived(estimate?.local.find((e) => e.model === prefs.model));

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
  /** "≈ 40 s", "≈ 14 min", "≈ 2 h 05 min" */
  function about(seconds: number | null): string {
    if (seconds === null) return 'unknown';
    if (seconds < 60) return `≈ ${Math.max(1, Math.round(seconds))} s`;
    const min = Math.round(seconds / 60);
    return min < 60 ? `≈ ${min} min` : `≈ ${Math.floor(min / 60)} h ${String(min % 60).padStart(2, '0')} min`;
  }
  const usd = (v: number) => (v < 0.01 ? `$${v.toFixed(4)}` : `$${v.toFixed(2)}`);
  const perMin = (v: number) => `$${v.toFixed(4).replace(/0+$/, '')} / min`;

  function summary(r: TranscribeResult): string {
    const n = r.segments.length;
    return [
      `${clock(r.duration_ms)} of audio`,
      `${n} segment${n === 1 ? '' : 's'}`,
      r.model,
      r.language,
      ...(r.engine === 'cloud' ? ['cloud'] : []),
      ...(r.cost != null ? [usd(r.cost)] : []),
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
      cloudModels = await api.sttCloudModelsList().catch(() => []);
      await refreshModels();
      unlisten = await onFileDrop((paths) => {
        const p = paths.find((x) => AUDIO_EXTENSIONS.includes(x.split('.').pop()?.toLowerCase() ?? ''));
        if (p) path = p;
        else app.toast('info', 'Drop an audio file', AUDIO_EXTENSIONS.join(', '));
      });
    })();
    return () => {
      unlisten();
      flushSave();
    };
  });

  // Remember the options between sessions (debounced; flushed when the view closes,
  // so a choice made just before switching views isn't lost).
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let pendingSave: Prefs | null = null;
  const flushSave = () => {
    clearTimeout(saveTimer);
    if (pendingSave) void api.workspaceSet(PREFS_KEY, pendingSave).catch(() => {});
    pendingSave = null;
  };
  $effect(() => {
    const snapshot = $state.snapshot(prefs);
    if (!prefsLoaded) return;
    pendingSave = snapshot;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(flushSave, 400);
  });

  // Time and cost for the open file; refreshed when the options change.
  let estimateSeq = 0;
  async function refreshEstimate() {
    const p = path;
    const seq = ++estimateSeq;
    if (!p) return void (estimate = null);
    const e = await api.transcribeEstimate(p, prefs.beam, cloudModel).catch(() => null);
    if (seq === estimateSeq) estimate = e;
  }
  $effect(() => {
    void path;
    void prefs.beam;
    void cloudModel;
    void refreshEstimate();
  });

  // Transcription finished: keep the result (failures already show a toast),
  // and re-estimate, since the speed was just measured.
  $effect(() => {
    const j = job;
    if (!j || j.state === 'running') return;
    if (j.state === 'done' && path) last = { path, result: j.result as TranscribeResult };
    jobId = null;
    void refreshModels();
    void refreshEstimate();
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
        model: cloud ? cloudModel : prefs.model,
        language: prefs.language,
        beam_size: prefs.beam,
        glossary: cloud ? [] : glossaryTerms(),
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
        {#if estimate?.duration_ms != null}<span class="label">{clock(estimate.duration_ms)}</span>{/if}
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
      {:else}
        <span>{cloud ? 'Cloud: the audio is sent to OpenRouter.' : 'This computer: the audio never leaves it.'}</span>
      {/if}
    </div>
  </section>

  <aside class="inspector" aria-label="Transcription options">
    <section class="section">
      <h3 class="section-title">
        Engine
        <InfoTip
          label="About engines"
          text="This computer runs Whisper locally: free and private, but long files can take longer than the recording. Cloud sends the audio to OpenRouter: much faster, billed per minute of audio."
        />
      </h3>
      <div class="segmented fill" role="group" aria-label="Engine">
        <button aria-pressed={!cloud} onclick={() => (prefs.engine = 'local')}><Icon name="cpu" size={16} /> This computer</button>
        <button aria-pressed={cloud} onclick={() => (prefs.engine = 'cloud')}><Icon name="cloud" size={16} /> Cloud</button>
      </div>

      {#if cloud}
        <select class="input" aria-label="Cloud model" value={cloudModel} onchange={(e) => (prefs.cloudModel = e.currentTarget.value)}>
          {#each cloudModels as m (m.id)}<option value={m.id}>{m.label}</option>{/each}
          {#if !cloudSpec}<option value={cloudModel}>{cloudModel}</option>{/if}
        </select>
        <div class="model-card" data-testid="cloud-model">
          <span class="mono">{cloudModel}</span>
          <span class="price">{cloudSpec ? perMin(cloudSpec.usd_per_min) : 'price: see openrouter.ai'}</span>
        </div>
        {#if keyStatus?.configured}
          <p class="note ok" data-testid="cloud-key">OpenRouter key {keyStatus.hint}</p>
        {:else}
          <p class="note warn" data-testid="cloud-key">
            No OpenRouter key yet.
            <button class="btn ghost link" onclick={() => app.navigate('settings')}>Add one in Settings</button>
          </p>
        {/if}
      {:else}
        <select class="input" aria-label="Model" bind:value={prefs.model}>
          {#each models as m (m.id)}
            <option value={m.id}>{m.label}{m.installed ? '' : ` · ${mb(m.size)} to download`}</option>
          {/each}
        </select>
        {#if selected}
          <div class="model-card">
            <span>{selected.hint}</span>
            <span class="price">{mb(selected.size)}</span>
          </div>
        {/if}
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
        <div class="prop">
          <span>Search <InfoTip label="About search" text="Accurate (beam search) tries several readings and keeps the best; Fast (greedy) takes the first, about a third quicker." /></span>
          <div class="segmented" role="group" aria-label="Search">
            <button aria-pressed={prefs.beam > 1} onclick={() => (prefs.beam = 5)}>Accurate</button>
            <button aria-pressed={prefs.beam === 1} onclick={() => (prefs.beam = 1)}>Fast</button>
          </div>
        </div>
      {/if}
    </section>

    {#if !cloud}
      <section class="section">
        <h3 class="section-title">
          Glossary
          <InfoTip label="About the glossary" text="One term per line: names and English words that Whisper should spell right, such as monsters, spells and NPCs. Used by this computer's engine only." />
        </h3>
        <textarea class="input terms" aria-label="Glossary" rows="4" spellcheck="false" bind:value={prefs.glossary} placeholder={'Dungeon Master\nbeholder\nhit points'}></textarea>
      </section>
    {/if}

    <section class="section">
      <h3 class="section-title">
        Corrections
        <InfoTip label="About corrections" text="One “wrong = right” per line. Applied to whole words after transcription, with either engine, e.g. beholdar = beholder." />
      </h3>
      <textarea class="input terms" aria-label="Corrections" rows="3" spellcheck="false" bind:value={prefs.corrections} placeholder={'beholdar = beholder'}></textarea>
    </section>

    <section class="section" aria-label="Time needed">
      <h3 class="section-title">
        Time needed
        <InfoTip
          label="About the estimates"
          text="Estimates start from speeds measured on a laptop CPU and learn from every transcription you run here. Cloud time depends on your upload speed."
        />
      </h3>
      {#if !path}
        <p class="note">Open a file to see how long each engine takes.</p>
      {:else if estimate && estimate.duration_ms === null}
        <p class="note">Couldn't read this file's length.</p>
      {:else if estimate}
        <div class="estimates" data-testid="estimates">
          <button class="est" aria-pressed={!cloud} onclick={() => (prefs.engine = 'local')}>
            <Icon name="cpu" size={16} />
            <span class="est-name">This computer<small>{selected?.label ?? prefs.model}</small></span>
            <span class="est-val">
              {about(localEstimate?.seconds ?? null)}
              <small>{localEstimate?.measured ? 'measured here' : 'estimate'}</small>
            </span>
          </button>
          <button class="est" aria-pressed={cloud} onclick={() => (prefs.engine = 'cloud')}>
            <Icon name="cloud" size={16} />
            <span class="est-name">Cloud<small>{cloudSpec?.label ?? cloudModel}</small></span>
            <span class="est-val">
              {about(estimate.cloud.seconds)}
              <small>{estimate.cloud.cost != null ? usd(estimate.cloud.cost) : 'price unknown'}</small>
            </span>
          </button>
        </div>
      {/if}
    </section>
  </aside>
</div>

<Modal title="Send this audio to OpenRouter?" bind:open={confirmOpen} width={460}>
  <p>
    The cloud engine uploads <strong>{fileName}</strong> to OpenRouter, which passes it to the provider of
    <code>{cloudModel}</code>.{#if estimate?.cloud.cost != null} Expected cost: about {usd(estimate.cloud.cost)}.{/if}
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
  /* Grid items default to min-width: auto, so a long model id or the engine
     buttons would widen the column past the panel. Let it shrink instead. */
  .inspector .section {
    grid-template-columns: minmax(0, 1fr);
  }
  .segmented.fill button {
    min-width: 0;
    white-space: nowrap;
  }
  .section-title {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .terms {
    resize: vertical;
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .inspector progress {
    width: 100%;
  }
  .model-card {
    display: flex;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    font-size: 12px;
    color: var(--text-muted);
  }
  .model-card .mono {
    min-width: 0;
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .model-card .price {
    color: var(--text);
    font-weight: 600;
    white-space: nowrap;
  }
  .estimates {
    display: grid;
    gap: var(--space-2);
  }
  .est {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text-muted);
    text-align: left;
    cursor: pointer;
  }
  .est[aria-pressed='true'] {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }
  .est-name,
  .est-val {
    display: grid;
    line-height: 1.3;
  }
  .est-name {
    min-width: 0;
  }
  .est-name small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .est-val {
    text-align: right;
    font-weight: 650;
    color: var(--text);
    white-space: nowrap;
  }
  .est small {
    font-size: 11px;
    font-weight: 400;
    color: var(--text-muted);
  }
  .link {
    padding: 0 2px;
    height: auto;
    text-decoration: underline;
  }
</style>
