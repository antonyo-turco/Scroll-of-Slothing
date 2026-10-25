<script lang="ts">
  // Pick 5etools creatures from the cached bestiary (Settings → Combat syncs it).
  // `multi`: several creatures with a count each (token sheet); otherwise one
  // creature plus token-or-art (token cutter).
  import { api } from '../api';
  import { app } from '../state.svelte';
  import type { BestiaryEntry } from '../types';
  import Modal from './Modal.svelte';

  let {
    open = $bindable(false),
    title,
    multi = false,
    action,
    defaultKind = 'token',
    onPick,
  }: {
    open: boolean;
    title: string;
    multi?: boolean;
    action: string;
    /** Which picture a single pick starts on (the cutter wants the art). */
    defaultKind?: 'token' | 'art';
    onPick: (picked: { entry: BestiaryEntry; count: number }[], kind: 'token' | 'art') => void | Promise<void>;
  } = $props();

  let query = $state('');
  let hits = $state<BestiaryEntry[]>([]);
  let picked = $state<{ entry: BestiaryEntry; count: number }[]>([]);
  let kind = $state<'token' | 'art'>('token');
  let busy = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    if (open) {
      picked = [];
      kind = defaultKind;
      search();
    }
  });
  function search() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(async () => {
      try {
        hits = await api.bestiarySearch(query, 150);
      } catch (e) {
        app.error('Search failed', e);
      }
    }, 120);
  }
  function choose(h: BestiaryEntry) {
    if (!multi) return void (picked = [{ entry: h, count: 1 }]);
    const p = picked.find((x) => x.entry.hash === h.hash);
    if (p) p.count++;
    else picked.push({ entry: h, count: 1 });
  }
  async function confirm() {
    busy = true;
    try {
      await onPick($state.snapshot(picked) as typeof picked, kind);
      open = false;
    } finally {
      busy = false;
    }
  }
</script>

<Modal {title} bind:open width={multi ? 960 : 720}>
  <div class="cp" class:multi>
    <div class="cp-left">
      <input class="input big" type="search" aria-label="Search the bestiary" placeholder="Search creatures…" bind:value={query} oninput={search} />
      <ul class="cp-list" aria-label="Creatures">
        {#each hits as h (h.hash)}
          <li>
            <button class:sel={picked.some((p) => p.entry.hash === h.hash)} onclick={() => choose(h)} aria-label="Pick {h.name} ({h.source})">
              <span class="thumb">{#if h.token_url}<img src={h.token_url} alt="" loading="lazy" />{/if}</span>
              <span class="nm">{h.name}</span>
              <span class="meta">{h.source} · CR {h.cr ?? '—'} · {h.size}</span>
            </button>
          </li>
        {:else}
          <li class="muted pad">No matches. Is the bestiary synced (Settings → Combat)?</li>
        {/each}
      </ul>
    </div>
    {#if multi}
      <div class="cp-right">
        <h3>Tokens to add</h3>
        <ul class="cp-picked">
          {#each picked as p, i (p.entry.hash)}
            <li>
              <span class="thumb">{#if p.entry.token_url}<img src={p.entry.token_url} alt="" />{/if}</span>
              <span class="nm">{p.entry.name} <span class="meta">{p.entry.source}</span></span>
              <span class="stepper">
                <button class="btn ghost icon" aria-label="One fewer {p.entry.name}" onclick={() => (p.count > 1 ? p.count-- : picked.splice(i, 1))}>−</button>
                <span class="cnt">{p.count}</span>
                <button class="btn ghost icon" aria-label="One more {p.entry.name}" onclick={() => p.count++}>+</button>
              </span>
            </li>
          {:else}
            <li class="muted pad">Click creatures on the left. Each token is printed at the creature's size.</li>
          {/each}
        </ul>
      </div>
    {/if}
  </div>
  {#snippet footer()}
    {#if !multi}
      <div class="segmented" role="group" aria-label="Picture">
        <button aria-pressed={kind === 'art'} onclick={() => (kind = 'art')}>Full art</button>
        <button aria-pressed={kind === 'token'} onclick={() => (kind = 'token')} title="The ready-made round token">Token</button>
      </div>
      <span class="sel-name">{picked[0] ? `${picked[0].entry.name} · ${picked[0].entry.size}` : ''}</span>
    {/if}
    <span class="grow"></span>
    <button class="btn primary big-btn" disabled={!picked.length || busy} onclick={confirm}>{busy ? 'Downloading…' : action}</button>
  {/snippet}
</Modal>

<style>
  .cp {
    display: grid;
    gap: var(--space-4);
    height: 62vh;
    font-size: 15px;
  }
  .cp.multi {
    grid-template-columns: 1.2fr 1fr;
  }
  .cp-left,
  .cp-right {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 0;
  }
  .cp-right h3 {
    margin: 0;
    font-size: 17px;
  }
  .big {
    height: 40px;
    font-size: 15px;
  }
  .cp-list,
  .cp-picked {
    flex: 1;
    overflow: auto;
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .cp-list button {
    width: 100%;
    display: grid;
    grid-template-columns: 40px minmax(0, 1fr) auto;
    gap: var(--space-3);
    align-items: center;
    padding: 6px 12px;
    border: 0;
    border-bottom: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .cp-list button:hover,
  .cp-list button.sel {
    background: var(--accent-soft);
  }
  .cp-picked li {
    display: grid;
    grid-template-columns: 40px minmax(0, 1fr) auto;
    gap: var(--space-3);
    align-items: center;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
  }
  .cp-picked li.pad,
  .cp-list li.pad {
    display: block;
    padding: 12px;
  }
  .thumb {
    width: 40px;
    height: 40px;
    display: grid;
    place-items: center;
  }
  .thumb img {
    width: 40px;
    height: 40px;
    object-fit: contain;
  }
  .nm {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    font-size: 13px;
    font-weight: 400;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .stepper {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .cnt {
    min-width: 2.5ch;
    text-align: center;
    font-weight: 700;
    font-size: 16px;
  }
  .grow {
    flex: 1;
  }
  .sel-name {
    font-size: 14px;
    color: var(--text-muted);
  }
  .big-btn {
    height: 40px;
    font-size: 15px;
    padding: 0 18px;
  }
</style>
