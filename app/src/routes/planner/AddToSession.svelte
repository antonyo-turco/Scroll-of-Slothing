<script lang="ts">
  // Picks items from the other tools to reference in a session: music, saved
  // encounters, library tokens/cut-outs/maps and card decks. Nothing is copied.
  import { api, blobUrl } from '../../lib/api';
  import Modal from '../../lib/components/Modal.svelte';
  import { music } from '../../lib/music';
  import { app } from '../../lib/state.svelte';
  import type { PlanRef, PlanRefKind, PlanSection, SessionView } from '../../lib/types';

  let {
    section = $bindable(null),
    session,
    onAdd,
  }: { section: PlanSection | null; session: SessionView; onAdd: (refs: PlanRef[]) => Promise<void> | void } = $props();

  type Source = { id: string; label: string };
  const SOURCES: Record<PlanSection, Source[]> = {
    music: [{ id: 'soundscape', label: 'Soundscapes' }, { id: 'playlist', label: 'Playlists' }, { id: 'track', label: 'Tracks' }],
    encounters: [{ id: 'encounter', label: 'Encounters' }],
    tokens: [{ id: 'token', label: 'Tokens' }, { id: 'cutout', label: 'Cut-outs' }],
    maps: [{ id: 'map', label: 'Maps' }],
    handouts: [{ id: 'deck', label: 'Card decks' }],
  };
  const TITLES: Record<PlanSection, string> = {
    music: 'Add music', encounters: 'Add encounters', tokens: 'Add tokens and cut-outs', maps: 'Add maps', handouts: 'Add handouts',
  };

  type Row = { kind: PlanRefKind; ref_id: string; name: string; detail: string; thumb: string | null; starred: boolean };

  let open = $state(false);
  let source = $state('');
  let query = $state('');
  let rows = $state<Row[]>([]);
  let picked = $state<Set<string>>(new Set());
  let busy = $state(false);

  const key = (r: { kind: string; ref_id: string }) => `${r.kind}:${r.ref_id}`;
  const planned = $derived(new Set(session.items.map(key)));
  const shown = $derived(rows.filter((r) => !query.trim() || r.name.toLowerCase().includes(query.trim().toLowerCase())));

  // opening for a section picks its first source
  $effect(() => {
    if (section) {
      open = true;
      picked = new Set();
      query = '';
      void choose(SOURCES[section][0].id);
    }
  });
  $effect(() => {
    if (!open) section = null;
  });

  async function choose(id: string) {
    source = id;
    try {
      rows = await load(id);
    } catch (e) {
      rows = [];
      app.error('Could not list the items', e);
    }
  }
  async function load(id: string): Promise<Row[]> {
    if (id === 'soundscape') {
      return (await music.soundscapesList()).map((x) => ({
        kind: 'soundscape', ref_id: x.id, name: x.name, detail: `${x.channels.length} channels`, thumb: null, starred: x.pinned,
      }));
    }
    if (id === 'playlist') {
      return (await music.playlistsList()).map((x) => ({
        kind: 'playlist', ref_id: x.id, name: x.name, detail: `${x.items.length} tracks`, thumb: null, starred: false,
      }));
    }
    if (id === 'track') {
      return (await music.tracksList()).map((x) => ({
        kind: 'track', ref_id: x.id, name: x.title, detail: x.category, thumb: null, starred: x.favorite,
      }));
    }
    if (id === 'encounter') {
      return (await api.encountersList()).map((x) => ({
        kind: 'encounter', ref_id: x.id, name: x.name,
        detail: x.lines.map((l) => (l.count > 1 ? `${l.count} × ${l.name}` : l.name)).join(', ') || 'saved combat',
        thumb: null, starred: x.pinned,
      }));
    }
    if (id === 'deck') {
      return (await api.decksList()).map((x) => ({
        kind: 'deck', ref_id: String(x.id), name: x.name, detail: 'card deck', thumb: null, starred: false,
      }));
    }
    // library assets of one kind
    const page = await api.librarySearch({ kinds: [id], limit: 500, sort: 'name_asc' });
    return page.items.map((a) => ({
      kind: 'asset', ref_id: String(a.id), name: a.name, detail: a.size_label || a.kind, thumb: a.thumb_blob ?? a.rendered_blob, starred: a.pinned,
    }));
  }

  function toggle(r: Row) {
    const k = key(r);
    const next = new Set(picked);
    if (next.has(k)) next.delete(k);
    else next.add(k);
    picked = next;
  }
  async function add() {
    if (!picked.size) return;
    busy = true;
    try {
      await onAdd([...picked].map((k) => {
        const i = k.indexOf(':');
        return { kind: k.slice(0, i) as PlanRefKind, ref_id: k.slice(i + 1) };
      }));
      open = false;
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={section ? TITLES[section] : 'Add'} bind:open width={760}>
  {#if section}
    <div class="bar">
      {#if SOURCES[section].length > 1}
        <div class="segmented" role="group" aria-label="Source">
          {#each SOURCES[section] as src (src.id)}
            <button aria-pressed={source === src.id} onclick={() => choose(src.id)}>{src.label}</button>
          {/each}
        </div>
      {/if}
      <input class="input grow" type="search" placeholder="Filter by name…" aria-label="Filter by name" bind:value={query} />
    </div>
    <ul class="rows" aria-label="Items">
      {#each shown as r (key(r))}
        {@const already = planned.has(key(r))}
        <li>
          <label class="row" class:on={picked.has(key(r))} class:already>
            <input type="checkbox" checked={already || picked.has(key(r))} disabled={already} onchange={() => toggle(r)} />
            {#if r.thumb}<img src={blobUrl(r.thumb)} alt="" loading="lazy" />{/if}
            <span class="what">
              <span class="name">{r.name}</span>
              <span class="detail">{already ? 'already in the session' : r.detail}{#if r.starred}<span class="star">★ Starred</span>{/if}</span>
            </span>
          </label>
        </li>
      {:else}
        <li class="empty">Nothing here yet. Make it in its own tool first.</li>
      {/each}
    </ul>
  {/if}
  {#snippet footer()}
    <span class="grow"></span>
    <button class="btn" onclick={() => (open = false)}>Cancel</button>
    <button class="btn primary" disabled={!picked.size || busy} onclick={add}>Add {picked.size || ''}</button>
  {/snippet}
</Modal>

<style>
  .bar {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    margin-bottom: var(--space-3);
  }
  .grow {
    flex: 1;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    height: 55vh;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
    cursor: pointer;
  }
  .row:hover {
    background: var(--surface-2);
  }
  .row.on {
    background: var(--accent-soft);
  }
  .row.already {
    opacity: 0.6;
    cursor: default;
  }
  .row input {
    width: 18px;
    height: 18px;
    flex: none;
  }
  .row img {
    width: 44px;
    height: 44px;
    object-fit: contain;
    flex: none;
  }
  .what {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .name {
    font-size: 16px;
    font-weight: 600;
  }
  .detail {
    font-size: 14px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .star {
    margin-left: var(--space-2);
    color: #d6a21a;
    font-weight: 600;
  }
  .empty {
    padding: var(--space-4);
    font-size: 15px;
    color: var(--text-muted);
  }
</style>
