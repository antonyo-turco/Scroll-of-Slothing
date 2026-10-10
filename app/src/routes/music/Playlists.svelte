<script lang="ts">
  // Playlists (Python ui/dm_music_playlist_panel.py): auto-advancing playback
  // on the dedicated "playlist" channel; duplicates allowed.
  import { onMount } from 'svelte';
  import Modal from '../../lib/components/Modal.svelte';
  import PlayIndicator from '../../lib/components/PlayIndicator.svelte';
  import { music, type Playlist, type PlaylistAction, type Track } from '../../lib/music';
  import { musicUi } from '../../lib/music-ui.svelte';
  import { app } from '../../lib/state.svelte';
  import type { Campaign } from '../../lib/types';

  let { campaigns }: { campaigns: Campaign[] } = $props();

  let list = $state<Playlist[]>([]);
  let campaignId = $state<string | null>(app.campaignId);
  let open = $state<Record<string, boolean>>({});
  const pb = $derived(musicUi.state?.playlist ?? null);

  async function load() {
    try {
      list = await music.playlistsList(campaignId);
    } catch (e) {
      app.error('Could not load playlists', e);
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

  const control = (a: PlaylistAction) => musicUi.act('Playlist control failed', () => music.playlistControl(a));

  async function remove(p: Playlist) {
    if (!confirm(`Delete playlist “${p.name}”?`)) return;
    try {
      await music.playlistDelete(p.id);
      void load();
    } catch (e) {
      app.error('Could not delete', e);
    }
  }

  // ── editor ──
  let edOpen = $state(false);
  let edId = $state<string | null>(null);
  let edName = $state('');
  let edCampaign = $state('');
  let edItems = $state<{ key: number; track_id: string; title: string }[]>([]);
  let allTracks = $state<Track[]>([]);
  let pickAll = $state<string[]>([]);
  let pickMine = $state<number[]>([]);
  let keySeq = 0;

  async function openEditor(p: Playlist | null) {
    try {
      allTracks = await music.tracksList({});
    } catch (e) {
      return app.error('Could not load tracks', e);
    }
    edId = p?.id ?? null;
    edName = p?.name ?? '';
    edCampaign = p?.campaign_id ?? app.campaignId ?? '';
    edItems = (p?.items ?? []).map((i) => ({ key: ++keySeq, track_id: i.track_id, title: i.title }));
    pickAll = [];
    pickMine = [];
    edOpen = true;
  }

  function addSelected() {
    for (const id of pickAll) {
      const t = allTracks.find((x) => x.id === id);
      if (t) edItems.push({ key: ++keySeq, track_id: t.id, title: t.title });
    }
  }
  function removeSelected() {
    edItems = edItems.filter((i) => !pickMine.includes(i.key));
    pickMine = [];
  }
  function move(dir: -1 | 1) {
    const idx = edItems.map((x, i) => (pickMine.includes(x.key) ? i : -1)).filter((i) => i >= 0);
    const order = dir < 0 ? idx : [...idx].reverse();
    for (const i of order) {
      const j = i + dir;
      if (j < 0 || j >= edItems.length || pickMine.includes(edItems[j].key)) continue;
      [edItems[i], edItems[j]] = [edItems[j], edItems[i]];
    }
  }

  async function saveEditor() {
    if (!edName.trim()) return;
    try {
      await music.playlistSave({ id: edId, name: edName.trim(), campaign_id: edCampaign || null, track_ids: edItems.map((i) => i.track_id) });
      edOpen = false;
      void load();
    } catch (e) {
      app.error('Could not save the playlist', e);
    }
  }
</script>

<div class="view">
  {#if pb}
    <div class="now card" aria-label="Now playing">
      <PlayIndicator status={musicUi.statusFor('playlist', !pb.paused)} size={18} />
      <div class="info">
        <strong>{pb.name} — Track {pb.index + 1}/{pb.track_ids.length}</strong>
        <span class="muted">{pb.title}</span>
      </div>
      <button class="btn" aria-label="Previous" onclick={() => control('prev')}>⏮</button>
      <button class="btn" aria-label={pb.paused ? 'Resume' : 'Pause'} onclick={() => control(pb.paused ? 'resume' : 'pause')}>{pb.paused ? '▶' : '⏸'}</button>
      <button class="btn" aria-label="Next" onclick={() => control('next')}>⏭</button>
      <button class="btn" aria-label="Stop playlist" onclick={() => control('stop')}>⏹</button>
    </div>
  {/if}
  <div class="bar">
    <select class="input filter" aria-label="Campaign filter" bind:value={campaignId} onchange={() => void load()}>
      <option value={null}>All campaigns</option>
      {#each campaigns as c (c.id)}<option value={c.id}>{c.name}</option>{/each}
    </select>
    <button class="btn primary" onclick={() => openEditor(null)}>+ New Playlist</button>
  </div>
  {#each list as p (p.id)}
    <div class="card pl">
      <div class="head">
        <button class="btn ghost" aria-expanded={!!open[p.id]} aria-label="Expand {p.name}" onclick={() => (open[p.id] = !open[p.id])}>{open[p.id] ? '▾' : '▸'}</button>
        <strong class="name">{p.name}</strong>
        <span class="muted small">{p.items.length} track(s)</span>
        <button class="btn" disabled={!p.items.length} onclick={() => musicUi.act('Could not play', () => music.playlistPlay(p.id, 0))}>▶ Play</button>
        <button class="btn" onclick={() => openEditor(p)}>Edit</button>
        <button class="btn ghost" aria-label="Delete {p.name}" onclick={() => remove(p)}>✕</button>
      </div>
      {#if open[p.id]}
        <ol class="tracks">
          {#each p.items as it, i (it.id)}
            <li>
              <button class="link" onclick={() => musicUi.act('Could not play', () => music.playlistPlay(p.id, i))}>{it.title}</button>
              {#if pb?.playlist_id === p.id && pb.index === i}<span class="cur">◄</span>{/if}
            </li>
          {/each}
        </ol>
      {/if}
    </div>
  {:else}
    <p class="muted">No playlists yet.</p>
  {/each}
</div>

<Modal title={edId ? 'Edit playlist' : 'New playlist'} bind:open={edOpen} width={700}>
  <form class="form" onsubmit={(e) => (e.preventDefault(), void saveEditor())}>
    <div class="row">
      <label class="field grow"><span class="label">Name</span><input class="input" bind:value={edName} required /></label>
      <label class="field"><span class="label">Campaign</span>
        <select class="input" bind:value={edCampaign}>
          <option value="">(none)</option>
          {#each campaigns as c (c.id)}<option value={c.id}>{c.name}</option>{/each}
        </select>
      </label>
    </div>
    <div class="lists">
      <label class="field"><span class="label">All Tracks</span>
        <select class="input" multiple size="12" bind:value={pickAll} ondblclick={addSelected}>
          {#each allTracks as t (t.id)}<option value={t.id}>{t.title}</option>{/each}
        </select>
      </label>
      <div class="mid">
        <button type="button" class="btn" onclick={addSelected}>Add →</button>
        <button type="button" class="btn" onclick={removeSelected}>← Remove</button>
        <button type="button" class="btn" onclick={() => move(-1)}>↑ Up</button>
        <button type="button" class="btn" onclick={() => move(1)}>↓ Down</button>
      </div>
      <label class="field"><span class="label">Playlist Tracks (ordered)</span>
        <select class="input" multiple size="12" bind:value={pickMine}>
          {#each edItems as it, i (it.key)}<option value={it.key}>{i + 1}. {it.title}</option>{/each}
        </select>
      </label>
    </div>
    <div class="actions">
      <button type="button" class="btn" onclick={() => (edOpen = false)}>Cancel</button>
      <button type="submit" class="btn primary" disabled={!edName.trim()}>Save</button>
    </div>
  </form>
</Modal>

<style>
  .view {
    padding: var(--space-3) var(--space-4);
    display: grid;
    gap: var(--space-2);
    align-content: start;
  }
  .now {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    padding: var(--space-2) var(--space-3);
    border-color: var(--accent);
  }
  .info {
    flex: 1;
    display: grid;
  }
  .bar {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .filter {
    width: auto;
  }
  .pl {
    padding: var(--space-2) var(--space-3);
  }
  .head {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .name {
    flex: 1;
  }
  .small {
    font-size: 12px;
  }
  .tracks {
    margin: 6px 0 0;
    font-size: 13px;
  }
  .link {
    border: 0;
    background: none;
    color: var(--text);
    cursor: pointer;
    padding: 1px 0;
    font: inherit;
  }
  .link:hover {
    text-decoration: underline;
  }
  .cur {
    color: var(--accent);
    margin-left: 6px;
  }
  .form {
    display: grid;
    gap: var(--space-3);
  }
  .row {
    display: flex;
    gap: var(--space-2);
  }
  .grow {
    flex: 1;
  }
  .lists {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    gap: var(--space-2);
    align-items: center;
  }
  .mid {
    display: grid;
    gap: 6px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
