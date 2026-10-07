<script lang="ts">
  // Campaigns & sessions. Spec: docs/features/campaigns-sessions.md
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import type { Campaign, Session } from '../lib/types';

  let campaigns = $state<Campaign[]>([]);
  let sessions = $state<Session[]>([]);
  let loaded = $state(false);
  let newCampaign = $state('');
  let newSession = $state<Record<string, string>>({});
  let open = $state<Record<string, boolean>>({});
  let editing = $state<string | null>(null);
  let editName = $state('');
  let confirming = $state<string | null>(null);

  async function load() {
    try {
      [campaigns, sessions] = await Promise.all([api.campaignsList(), api.sessionsList(null)]);
      if (campaigns.length === 1 && Object.keys(open).length === 0) open[campaigns[0].id] = true;
    } catch (e) {
      app.error('Could not load campaigns', e);
    } finally {
      loaded = true;
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

  async function act(what: string, f: () => Promise<unknown>) {
    try {
      await f();
      await load();
    } catch (e) {
      app.error(what, e);
    }
  }

  async function createCampaign() {
    const name = newCampaign.trim();
    if (!name) return;
    await act('Could not create campaign', async () => {
      const c = await api.campaignsCreate(name);
      open[c.id] = true;
      newCampaign = '';
    });
  }

  async function createSession(c: Campaign) {
    const name = (newSession[c.id] ?? '').trim();
    if (!name) return;
    await act('Could not create session', () => api.sessionsCreate(c.id, name));
    newSession[c.id] = '';
  }

  function startRename(id: string, name: string) {
    editing = id;
    editName = name;
  }

  async function commitRename(kind: 'campaign' | 'session', id: string) {
    const name = editName.trim();
    editing = null;
    if (!name) return;
    await act('Rename failed', () => (kind === 'campaign' ? api.campaignsRename(id, name) : api.sessionsUpdate(id, name, null)));
  }

  function showInLibrary(preset: { sessionId?: string; campaignId?: string }) {
    app.libraryPreset = preset;
    app.navigate('library');
  }
</script>

<div class="page">
  <form class="create" onsubmit={(e) => (e.preventDefault(), createCampaign())}>
    <input class="input" placeholder="New campaign name" aria-label="New campaign name" bind:value={newCampaign} />
    <button class="btn primary" disabled={!newCampaign.trim()}>+ Campaign</button>
  </form>

  {#if loaded && campaigns.length === 0}
    <div class="empty card">
      <p><strong>No campaigns yet.</strong></p>
      <p class="muted">Campaigns group sessions; link assets to them to find them quickly in the Library.</p>
    </div>
  {/if}

  <div class="list">
    {#each campaigns as c (c.id)}
      {@const cs = sessions.filter((s) => s.campaign_id === c.id)}
      <section class="card campaign" class:current={app.campaignId === c.id}>
        <header>
          <button class="btn ghost icon" aria-expanded={!!open[c.id]} aria-label="Toggle sessions of {c.name}" onclick={() => (open[c.id] = !open[c.id])}>
            {open[c.id] ? '▾' : '▸'}
          </button>
          {#if editing === c.id}
            <input
              class="input title-input"
              aria-label="Campaign name"
              bind:value={editName}
              onkeydown={(e) => (e.key === 'Enter' ? commitRename('campaign', c.id) : e.key === 'Escape' && (editing = null))}
              onblur={() => commitRename('campaign', c.id)}
            />
          {:else}
            <h2 ondblclick={() => startRename(c.id, c.name)}>{c.name}</h2>
          {/if}
          <span class="muted counts">{c.session_count} session{c.session_count === 1 ? '' : 's'} · {c.asset_count} asset{c.asset_count === 1 ? '' : 's'}</span>
          <div class="spacer"></div>
          {#if app.campaignId === c.id}
            <button class="btn" onclick={() => app.setCampaign(null)}>● Active</button>
          {:else}
            <button class="btn ghost" onclick={() => app.setCampaign(c.id)}>Set active</button>
          {/if}
          <button class="btn ghost" onclick={() => showInLibrary({ campaignId: c.id })}>Show assets</button>
          <button class="btn ghost" onclick={() => startRename(c.id, c.name)}>Rename</button>
          {#if confirming === c.id}
            <button class="btn danger" onclick={() => act('Delete failed', () => api.campaignsDelete(c.id)).then(() => { if (app.campaignId === c.id) void app.setCampaign(null); })}>
              Delete campaign &amp; sessions
            </button>
            <button class="btn" onclick={() => (confirming = null)}>Cancel</button>
          {:else}
            <button class="btn ghost danger" onclick={() => (confirming = c.id)}>Delete…</button>
          {/if}
        </header>
        {#if confirming === c.id}
          <p class="warn">Sessions and links are removed. <strong>Assets are kept.</strong></p>
        {/if}

        {#if open[c.id]}
          <ul class="sessions">
            {#each cs as s (s.id)}
              <li>
                <div class="srow">
                  {#if editing === s.id}
                    <input
                      class="input"
                      aria-label="Session name"
                      bind:value={editName}
                      onkeydown={(e) => (e.key === 'Enter' ? commitRename('session', s.id) : e.key === 'Escape' && (editing = null))}
                      onblur={() => commitRename('session', s.id)}
                    />
                  {:else}
                    <strong ondblclick={() => startRename(s.id, s.name)}>{s.name}</strong>
                  {/if}
                  <span class="muted">{s.asset_count} asset{s.asset_count === 1 ? '' : 's'}</span>
                  <div class="spacer"></div>
                  <button class="btn ghost" onclick={() => showInLibrary({ sessionId: s.id })}>Show assets</button>
                  <button class="btn ghost" onclick={() => startRename(s.id, s.name)}>Rename</button>
                  {#if confirming === s.id}
                    <button class="btn danger" onclick={() => act('Delete failed', () => api.sessionsDelete(s.id))}>Delete</button>
                    <button class="btn" onclick={() => (confirming = null)}>Cancel</button>
                  {:else}
                    <button class="btn ghost danger" aria-label="Delete session {s.name}" onclick={() => (confirming = s.id)}>Delete…</button>
                  {/if}
                </div>
                <textarea
                  class="input notes"
                  rows="2"
                  placeholder="Session notes…"
                  aria-label="Notes for {s.name}"
                  value={s.notes}
                  onblur={(e) => e.currentTarget.value !== s.notes && act('Could not save notes', () => api.sessionsUpdate(s.id, null, e.currentTarget.value))}
                ></textarea>
              </li>
            {/each}
            <li>
              <form class="srow" onsubmit={(e) => (e.preventDefault(), createSession(c))}>
                <input class="input" placeholder="New session name" aria-label="New session in {c.name}" bind:value={newSession[c.id]} />
                <button class="btn" disabled={!(newSession[c.id] ?? '').trim()}>+ Session</button>
              </form>
            </li>
          </ul>
        {/if}
      </section>
    {/each}
  </div>
</div>

<style>
  .page {
    height: 100%;
    overflow: auto;
    padding: var(--space-6);
    display: grid;
    align-content: start;
    gap: var(--space-4);
    max-width: 1000px;
  }
  .create {
    display: flex;
    gap: var(--space-2);
  }
  .create .input {
    flex: 0 1 320px;
  }
  .empty {
    padding: var(--space-6);
    text-align: center;
  }
  .empty p {
    margin: 0 0 var(--space-1);
  }
  .list {
    display: grid;
    gap: var(--space-3);
  }
  .campaign {
    padding: var(--space-2) var(--space-3);
  }
  .campaign.current {
    border-color: var(--accent);
  }
  header,
  .srow {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  h2 {
    margin: 0;
    font-size: 15px;
  }
  .title-input {
    font-weight: 600;
  }
  .counts {
    font-size: 12px;
  }
  .spacer {
    flex: 1;
  }
  .warn {
    margin: var(--space-1) 0 0 40px;
    font-size: 13px;
    color: var(--warning);
  }
  .sessions {
    list-style: none;
    margin: var(--space-2) 0 var(--space-1) 40px;
    padding: 0;
    display: grid;
    gap: var(--space-3);
  }
  .notes {
    width: 100%;
    margin-top: var(--space-1);
    padding: var(--space-2);
    resize: vertical;
    min-height: 40px;
  }
</style>
