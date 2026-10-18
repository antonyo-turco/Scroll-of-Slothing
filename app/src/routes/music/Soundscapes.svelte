<script lang="ts">
  // Saved mixes (Python ui/dm_music_soundscape_panel.py) + .dmix export.
  import { onMount } from 'svelte';
  import { pickSavePath } from '../../lib/api';
  import InfoTip from '../../lib/components/InfoTip.svelte';
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

  async function togglePin(s: Soundscape) {
    try {
      await music.soundscapeSetPinned(s.id, !s.pinned);
      await load(); // the server orders pinned mixes first
    } catch (e) {
      app.error('Could not pin', e);
    }
  }

  const pinnedCount = $derived(list.filter((s) => s.pinned).length);

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

<div class="pane">
  <div class="view-toolbar">
    <span class="label">Saved mixes</span>
    <InfoTip
      label="About soundscapes"
      text="A soundscape is a saved mix: its channels, volumes, loops and soundbites. Load one into the Mixer, or build a mix there and press 💾 Save as New. Pin (★) the ones you use most to keep them at the top."
    />
    <div class="spacer"></div>
    <button class="btn" onclick={() => void load()}>⟳ Refresh</button>
  </div>
  <div class="pane-body">
    {#each list as s, i (s.id)}
      {#if pinnedCount > 0 && i === pinnedCount}<div class="divider" role="separator"></div>{/if}
      <div class="card sc" class:active={musicUi.state?.soundscape_id === s.id} class:pinned={s.pinned}>
        <div class="head">
          <button
            class="star"
            class:on={s.pinned}
            aria-pressed={s.pinned}
            aria-label={s.pinned ? `Unpin ${s.name}` : `Pin ${s.name}`}
            title={s.pinned ? 'Pinned to the top' : 'Pin to the top'}
            onclick={() => togglePin(s)}
          >{s.pinned ? '★' : '☆'}</button>
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
      <div class="empty-state">
        <strong>No soundscapes yet</strong>
        <span>Build a mix in the Mixer, then 💾 Save as New to keep it here.</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .pane-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--space-3) var(--space-4);
    display: grid;
    gap: var(--space-2);
    align-content: start;
  }
  .small {
    font-size: 12px;
  }
  .sc {
    padding: var(--space-2) var(--space-3);
  }
  .sc.active {
    border-color: var(--accent);
  }
  .sc.pinned {
    background: color-mix(in srgb, var(--accent-soft) 45%, var(--surface));
  }
  .divider {
    height: 1px;
    margin: var(--space-1) 0;
    background: var(--border);
  }
  .star {
    border: 0;
    background: none;
    padding: 0 2px;
    font-size: 18px;
    line-height: 1;
    color: var(--text-muted);
    cursor: pointer;
  }
  .star.on {
    color: #e0a400;
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
