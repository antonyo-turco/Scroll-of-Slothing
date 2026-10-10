<script lang="ts">
  // Saved mixes (Python ui/dm_music_soundscape_panel.py) + .dmix export.
  import { onMount } from 'svelte';
  import { pickSavePath } from '../../lib/api';
  import { DMIX_EXTENSION, music, type Soundscape } from '../../lib/music';
  import { musicUi } from '../../lib/music-ui.svelte';
  import { app } from '../../lib/state.svelte';
  let list = $state<Soundscape[]>([]);
  let open = $state<Record<string, boolean>>({});

  async function load() {
    try {
      list = await music.soundscapesList();
    } catch (e) {
      app.error('Could not load soundscapes', e);
    }
  }
  onMount(() => void load());
  let seen = app.dataVersion;
  $effect(() => {
    if (app.dataVersion !== seen) {
      seen = app.dataVersion;
      void load();
    }
  });

  async function loadInto(s: Soundscape) {
    await musicUi.act('Could not load the soundscape', () => music.soundscapeLoad(s.id));
    musicUi.tab = 'mixer';
  }

  async function remove(s: Soundscape) {
    if (!confirm(`Delete soundscape “${s.name}”?`)) return;
    try {
      await music.soundscapeDelete(s.id);
      void load();
    } catch (e) {
      app.error('Could not delete', e);
    }
  }

  async function exportDmix(s: Soundscape) {
    const path = await pickSavePath('Export soundscape as .dmix', `${s.name.replace(/[\\/:*?"<>|]/g, '_')}.${DMIX_EXTENSION}`, [DMIX_EXTENSION]);
    if (!path) return;
    try {
      await music.mixExport(path, s.id);
      app.toast('success', `Exported “${s.name}”`, path);
    } catch (e) {
      app.error('Could not export', e);
    }
  }
</script>

<div class="view">
  <div class="bar">
    <button class="btn" onclick={() => void load()}>⟳ Refresh</button>
    <span class="muted small">Save a soundscape from the Mixer (💾 Save as New).</span>
  </div>
  <div class="cards">
    {#each list as s (s.id)}
      <div class="card sc" class:active={musicUi.state?.soundscape_id === s.id}>
        <div class="head">
          <div class="name">
            <strong>{s.name}</strong>
            <span class="muted small">{s.channels.length} channel(s){#if s.soundbites.length} · {s.soundbites.length} soundbite(s){/if}</span>
          </div>
          <button class="btn primary" onclick={() => loadInto(s)}>▶ Load</button>
          <button class="btn" aria-expanded={!!open[s.id]} onclick={() => (open[s.id] = !open[s.id])}>{open[s.id] ? '▲' : '▼'} Preview</button>
          <button class="btn" onclick={() => exportDmix(s)}>Export .dmix…</button>
          <button class="btn ghost" aria-label="Delete {s.name}" onclick={() => remove(s)}>✕</button>
        </div>
        {#if s.tags.length}
          <div class="tags">{#each s.tags as t (t)}<span class="pill">{t}</span>{/each}</div>
        {/if}
        {#if open[s.id]}
          <ul class="summary">
            {#each s.channels as c, i (i)}
              <li><span class="muted">{c.role}</span> {c.title} · {c.volume}{c.is_looping ? ' ↻' : ''}</li>
            {/each}
            {#each s.soundbites as b, i (i)}
              <li>🔔 {b.title} · every {b.min_interval}–{b.max_interval} s · {b.volume}</li>
            {/each}
            {#if s.notes}<li class="muted">{s.notes}</li>{/if}
          </ul>
        {/if}
      </div>
    {:else}
      <p class="muted">No soundscapes yet.</p>
    {/each}
  </div>
</div>

<style>
  .view {
    padding: var(--space-3) var(--space-4);
    display: grid;
    gap: var(--space-3);
    align-content: start;
  }
  .bar {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .small {
    font-size: 12px;
  }
  .cards {
    display: grid;
    gap: var(--space-2);
  }
  .sc {
    padding: var(--space-2) var(--space-3);
  }
  .sc.active {
    border-color: var(--accent);
  }
  .head {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .name {
    flex: 1;
    display: grid;
  }
  .tags {
    display: flex;
    gap: 4px;
    margin-top: 6px;
  }
  .pill {
    font-size: 11px;
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--accent-soft);
  }
  .summary {
    margin: var(--space-2) 0 0;
    padding-left: var(--space-4);
    font-size: 13px;
    display: grid;
    gap: 2px;
  }
</style>
