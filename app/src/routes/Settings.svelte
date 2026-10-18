<script lang="ts">
  // Settings: auto-saved on change, validated server-side (errors shown inline).
  // Spec: docs/features/settings.md
  import { onMount } from 'svelte';
  import { api, isTauri, pickFile } from '../lib/api';
  import { ALL_VIEWS, findView } from '../lib/nav';
  import { app } from '../lib/state.svelte';
  import { CLOUD_STT_MODELS } from '../lib/types';
  import type { CloudKeyInfo, CloudKeyStatus, CutLineStyle, PageFormat, Settings, SttEngine, Theme } from '../lib/types';

  let errors = $state<Record<string, string>>({});
  let confirmingReset = $state(false);
  let dataDir = $state('');

  onMount(() => {
    void api.appInfo().then((i) => (dataDir = i.data_dir));
    void api.sttCloudKeyStatus().then((s) => (keyStatus = s)).catch(() => {});
  });

  // ── transcription: the OpenRouter key lives in the Windows Credential Manager ──
  const ENGINES: { id: SttEngine; label: string }[] = [
    { id: 'local', label: 'On this computer' },
    { id: 'cloud', label: 'OpenRouter (cloud)' },
  ];
  let keyStatus = $state<CloudKeyStatus | null>(null);
  let keyInput = $state('');
  let keyBusy = $state(false);
  let keyCheck = $state<{ ok: true; info: CloudKeyInfo } | { ok: false; message: string } | null>(null);

  async function saveKey() {
    if (!keyInput.trim()) return;
    keyBusy = true;
    try {
      keyStatus = await api.sttCloudKeySet(keyInput);
      keyInput = '';
      keyCheck = null;
      app.toast('success', 'API key saved');
    } catch (e) {
      app.error('Could not save the key', e);
    } finally {
      keyBusy = false;
    }
  }

  async function clearKey() {
    keyStatus = await api.sttCloudKeyClear().catch((e) => (app.error('Could not remove the key', e), keyStatus));
    keyCheck = null;
  }

  async function testKey() {
    keyBusy = true;
    try {
      keyCheck = { ok: true, info: await api.sttCloudKeyTest() };
    } catch (e) {
      keyCheck = { ok: false, message: (e as { message?: string })?.message ?? String(e) };
    } finally {
      keyBusy = false;
    }
  }

  const usd = (v: number | null) => (v === null ? 'no limit' : `$${v.toFixed(2)}`);

  let mcp = $state<{ url: string; token: string } | null>(null);
  $effect(() => {
    // re-query whenever the toggle changes (the server starts/stops asynchronously)
    if (!app.settings.mcp_http_enabled) return void (mcp = null);
    const t = setTimeout(() => void api.mcpHttpStatus().then((u) => (mcp = u)).catch(() => {}), 400);
    return () => clearTimeout(t);
  });

  async function importLegacy() {
    if (!isTauri()) return app.toast('info', 'Legacy import is only available in the desktop app.');
    const db = await pickFile('Select Orb of Many Functions db/creatures.db', ['db'], 'SQLite database');
    if (!db) return;
    // Optional siblings in the old project root (ignored by the service if absent).
    // <root>/db/creatures.db -> <root>
    const root = db.replace(/[\\/][^\\/]+[\\/][^\\/]+$/, '');
    const sep = db.includes('\\') ? '\\' : '/';
    await api.legacyImport(db, `${root}${sep}app_settings.json`, `${root}${sep}aoe_presets.json`);
    app.toast('info', 'Import started — progress is shown bottom-right.');
  }

  const FORMATS: PageFormat[] = ['Letter', 'A4'];
  const MAP_FORMATS: PageFormat[] = ['Letter', 'A4', 'A3'];
  const STYLES: CutLineStyle[] = ['Dashed', 'Dotted', 'Solid'];
  const THEMES: { id: Theme; label: string }[] = [
    { id: 'system', label: 'System' },
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' },
  ];
  const PREVIEW_DASH: Record<CutLineStyle, string> = { Dashed: '3 2', Dotted: '1 2', Solid: '' };

  const startViews = ALL_VIEWS.map((v) => {
    const d = findView(v.id)!.dest;
    return { id: v.id, label: d.views.length > 1 ? `${d.label} › ${v.label}` : d.label };
  });

  async function save<K extends keyof Settings>(key: K, value: Settings[K]) {
    const errs = await app.updateSettings({ [key]: value } as Partial<Settings>);
    const next = { ...errors };
    if (errs[key]) next[key] = errs[key];
    else delete next[key];
    errors = next;
    if (!errs[key]) app.toast('success', 'Saved');
  }

  function saveNumber(key: keyof Settings, raw: string) {
    const n = Number(raw);
    if (raw.trim() === '' || !Number.isFinite(n)) {
      errors = { ...errors, [key]: 'enter a number' };
      return;
    }
    void save(key, n as never);
  }

  async function reset() {
    await app.resetSettings();
    errors = {};
    confirmingReset = false;
    app.toast('success', 'Settings restored to defaults');
  }
</script>

{#snippet formatPicker(key: 'token_page_format' | 'object_page_format' | 'card_page_format' | 'map_page_format')}
  <div class="segmented" role="group" aria-label="Page format">
    {#each key === 'map_page_format' ? MAP_FORMATS : FORMATS as f (f)}
      <button aria-pressed={app.settings[key] === f} onclick={() => save(key, f)}>{f}</button>
    {/each}
  </div>
{/snippet}

{#snippet numberField(key: keyof Settings, label: string, step: number, unit: string)}
  <div class="field">
    <label for={key}>{label}</label>
    <div class="with-unit">
      <input
        id={key}
        class="input"
        type="number"
        {step}
        min="0"
        value={app.settings[key] as number}
        aria-invalid={errors[key] ? 'true' : undefined}
        onchange={(e) => saveNumber(key, e.currentTarget.value)}
      />
      <span class="muted">{unit}</span>
    </div>
    {#if errors[key]}<span class="error-text">{errors[key]}</span>{/if}
  </div>
{/snippet}

{#snippet toggle(key: keyof Settings, label: string)}
  <label class="toggle">
    <input type="checkbox" checked={app.settings[key] as boolean} onchange={(e) => save(key, e.currentTarget.checked as never)} />
    <span>{label}</span>
  </label>
{/snippet}

<div class="page">
  <div class="sections">
    <section class="card">
      <h2>General</h2>
      <div class="field">
        <label for="start_view">Start in</label>
        <select id="start_view" class="input" value={app.settings.start_view} onchange={(e) => save('start_view', e.currentTarget.value)}>
          {#each startViews as v (v.id)}<option value={v.id}>{v.label}</option>{/each}
        </select>
      </div>
      <div class="field">
        <span class="label">Theme</span>
        <div class="segmented" role="group" aria-label="Theme">
          {#each THEMES as t (t.id)}
            <button aria-pressed={app.settings.theme === t.id} onclick={() => save('theme', t.id)}>{t.label}</button>
          {/each}
        </div>
      </div>
      {@render toggle('sidebar_collapsed', 'Start with the sidebar collapsed')}
    </section>

    <section class="card">
      <h2>Cut guide line</h2>
      <p class="muted hint">Used for token cut circles and map page borders.</p>
      <div class="field">
        <span class="label">Style</span>
        <div class="segmented" role="group" aria-label="Cut line style">
          {#each STYLES as s (s)}
            <button aria-pressed={app.settings.cut_line_style === s} onclick={() => save('cut_line_style', s)}>{s}</button>
          {/each}
        </div>
      </div>
      <div class="row">
        {@render numberField('cut_line_width', 'Width', 0.1, 'pt')}
        <div class="field">
          <label for="cut_line_color">Colour</label>
          <input
            id="cut_line_color"
            type="color"
            class="swatch"
            value={app.settings.cut_line_color}
            onchange={(e) => save('cut_line_color', e.currentTarget.value)}
          />
        </div>
      </div>
      <svg class="line-preview" viewBox="0 0 240 12" aria-label="Cut line preview" role="img">
        <line
          x1="4" y1="6" x2="236" y2="6"
          stroke={app.settings.cut_line_color}
          stroke-width={Math.max(0.5, app.settings.cut_line_width * 2)}
          stroke-dasharray={PREVIEW_DASH[app.settings.cut_line_style]
            .split(' ')
            .map((x) => (x ? Number(x) * 2 : ''))
            .join(' ')}
        />
      </svg>
    </section>

    <section class="card">
      <h2>Tokens</h2>
      <div class="field"><span class="label">Page format</span>{@render formatPicker('token_page_format')}</div>
      {@render toggle('token_cut_border', 'Cut border on by default')}
    </section>

    <section class="card">
      <h2>Cut-outs &amp; cards</h2>
      <div class="field"><span class="label">Object sheet page format</span>{@render formatPicker('object_page_format')}</div>
      <div class="field"><span class="label">Card page format</span>{@render formatPicker('card_page_format')}</div>
    </section>

    <section class="card">
      <h2>Maps</h2>
      <div class="field"><span class="label">Page format</span>{@render formatPicker('map_page_format')}</div>
      <div class="row">
        {@render numberField('map_square_in', '1 square on paper', 0.05, 'in')}
        {@render numberField('map_calibration_mm', 'Printed 1 in measures', 0.1, 'mm')}
      </div>
      <div class="row">
        {@render numberField('map_margin_in', 'Print margin', 0.05, 'in')}
        {@render numberField('map_overlap_in', 'Page overlap', 0.05, 'in')}
      </div>
      {@render toggle('map_cut_border', 'Cut border on by default')}
      {@render toggle('map_align_to_grid', 'Align pages to grid')}
    </section>

    <section class="card">
      <h2>Transcription</h2>
      <div class="field">
        <span class="label">Default engine</span>
        <div class="segmented" role="group" aria-label="Transcription engine">
          {#each ENGINES as e (e.id)}
            <button aria-pressed={app.settings.stt_engine === e.id} onclick={() => save('stt_engine', e.id)}>{e.label}</button>
          {/each}
        </div>
      </div>
      <p class="muted hint">
        On this computer, audio never leaves the machine but long files are slow. The cloud engine sends the audio
        to OpenRouter: much faster, billed per minute of audio.
      </p>

      <div class="field">
        <label for="openrouter-key">OpenRouter API key</label>
        <p class="muted hint key-status" data-testid="key-status">
          {#if keyStatus?.configured}
            {keyStatus.source === 'env' ? 'From the OPENROUTER_API_KEY variable' : 'Saved'}: <code>{keyStatus.hint}</code>
          {:else}
            No key yet. Create one at openrouter.ai/keys.
          {/if}
        </p>
        <div class="key-row">
          <input
            id="openrouter-key"
            class="input"
            type="password"
            autocomplete="off"
            spellcheck="false"
            placeholder={keyStatus?.configured ? 'Paste a new key to replace it' : 'sk-or-v1-…'}
            bind:value={keyInput}
            onkeydown={(e) => e.key === 'Enter' && saveKey()}
          />
          <button class="btn" disabled={keyBusy || !keyInput.trim()} onclick={saveKey}>Save key</button>
        </div>
        <div class="key-row">
          <button class="btn" disabled={keyBusy || !keyStatus?.configured} onclick={testKey}>Test key</button>
          {#if keyStatus?.source === 'saved'}<button class="btn ghost" onclick={clearKey}>Remove key</button>{/if}
        </div>
        {#if keyCheck}
          <p class="note {keyCheck.ok ? 'ok' : 'bad'}" data-testid="key-check" aria-live="polite">
            {#if keyCheck.ok}
              Key works{keyCheck.info.label ? ` (${keyCheck.info.label})` : ''}: used {usd(keyCheck.info.usage ?? 0)}, limit {usd(keyCheck.info.limit)}.
            {:else}{keyCheck.message}{/if}
          </p>
        {/if}
        <p class="muted hint">Stored in the Windows Credential Manager, never in the app's database or settings.</p>
      </div>

      <div class="field">
        <label for="stt_cloud_model">Cloud model</label>
        <input
          id="stt_cloud_model"
          class="input"
          list="cloud-stt-models"
          spellcheck="false"
          value={app.settings.stt_cloud_model}
          aria-invalid={errors.stt_cloud_model ? 'true' : undefined}
          onchange={(e) => save('stt_cloud_model', e.currentTarget.value.trim())}
        />
        <datalist id="cloud-stt-models">
          {#each CLOUD_STT_MODELS as m (m)}<option value={m}></option>{/each}
        </datalist>
        {#if errors.stt_cloud_model}<span class="error-text">{errors.stt_cloud_model}</span>{/if}
      </div>
      <div class="field">
        <label for="stt_cloud_base_url">API address</label>
        <input
          id="stt_cloud_base_url"
          class="input"
          spellcheck="false"
          value={app.settings.stt_cloud_base_url}
          aria-invalid={errors.stt_cloud_base_url ? 'true' : undefined}
          onchange={(e) => save('stt_cloud_base_url', e.currentTarget.value.trim())}
        />
        {#if errors.stt_cloud_base_url}<span class="error-text">{errors.stt_cloud_base_url}</span>{/if}
        <p class="muted hint">OpenRouter by default. Any service that accepts the same request format works.</p>
      </div>
    </section>

    <section class="card">
      <h2>Data</h2>
      <p class="muted hint">Stored in <code>{dataDir}</code></p>
      <div>
        <button class="btn" onclick={importLegacy}>Import from Orb of Many Functions…</button>
      </div>
      <p class="muted hint">
        Pick the old <code>db/creatures.db</code>. Assets, tags, settings and AoE presets are
        copied; the old files are only read. Running it again only adds what is missing.
      </p>
    </section>

    <details class="card advanced">
      <summary><h2>Advanced</h2></summary>
      {@render toggle('mcp_http_enabled', 'Enable in-app MCP server (HTTP, localhost only)')}
      {#if app.settings.mcp_http_enabled}
        <p class="muted hint" data-testid="mcp-url">
          {#if mcp}
            Endpoint <code>{mcp.url}</code>, header <code>Authorization: Bearer {mcp.token}</code>. For Claude Code:
            <code>claude mcp add --transport http sleuth {mcp.url} --header "Authorization: Bearer {mcp.token}"</code>
          {:else}Starting… (if this stays, port 8765 may be in use){/if}
        </p>
      {/if}
      <p class="muted hint">The standalone <code>sleuth-mcp</code> server works regardless of this setting.</p>
    </details>
  </div>

  <footer>
    {#if confirmingReset}
      <span>Restore every setting to its default?</span>
      <button class="btn danger" onclick={reset}>Restore defaults</button>
      <button class="btn" onclick={() => (confirmingReset = false)}>Cancel</button>
    {:else}
      <button class="btn" onclick={() => (confirmingReset = true)}>Restore defaults…</button>
    {/if}
  </footer>
</div>

<style>
  .page {
    height: 100%;
    overflow: auto;
    padding: var(--space-6);
  }
  .sections {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: var(--space-4);
    align-items: start;
    max-width: 1100px;
  }
  section,
  details {
    padding: var(--space-4);
    display: grid;
    gap: var(--space-3);
  }
  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 650;
  }
  summary {
    cursor: pointer;
    list-style: none;
  }
  summary h2 {
    display: inline;
  }
  .hint {
    margin: calc(-1 * var(--space-2)) 0 0;
    font-size: 12px;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
  }
  .with-unit {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .with-unit .input {
    width: 100%;
  }
  .toggle {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    cursor: pointer;
  }
  .toggle input {
    accent-color: var(--accent);
    width: 16px;
    height: 16px;
  }
  .swatch {
    width: 48px;
    height: 32px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
  }
  .key-row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .key-row .input {
    flex: 1;
    min-width: 0;
  }
  .key-status {
    margin: 0;
  }
  .line-preview {
    width: 100%;
    height: 16px;
  }
  footer {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin-top: var(--space-6);
    max-width: 1100px;
  }
  code {
    font-family: var(--font-mono);
  }
</style>
