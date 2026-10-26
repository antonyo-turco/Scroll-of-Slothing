<script lang="ts">
  // Soundboards (Python ui/dm_music_soundboard_panel.py): a 4-column grid of
  // buttons playing one-shots, restartable or toggled channels. New: stable
  // button ids, per-button volume and hotkeys (1–9 by position by default).
  import { onMount } from 'svelte';
  import Modal from '../../lib/components/Modal.svelte';
  import PlayIndicator from '../../lib/components/PlayIndicator.svelte';
  import { BUTTON_COLORS, PLAY_MODE_SYMBOLS, music, type PlayMode, type Soundboard, type SoundboardButton, type Track } from '../../lib/music';
  import { musicUi } from '../../lib/music-ui.svelte';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import { app } from '../../lib/state.svelte';
  let boards = $state<Soundboard[]>([]);
  let boardId = $state<string | null>(null);
  let editing = $state(false);
  let draft = $state<SoundboardButton[]>([]);
  let tracks = $state<Track[]>([]);
  const board = $derived(boards.find((b) => b.id === boardId) ?? null);
  const MODES: PlayMode[] = ['oneshot', 'restart', 'toggle'];
  // #, icon, label, track, mode, colour, volume, loop, key, actions
  const EDIT_COLUMNS = '32px 64px minmax(120px, 1fr) minmax(160px, 1.4fr) 150px auto 72px 48px 56px auto';

  async function load() {
    try {
      boards = await music.soundboardsList();
      if (!boards.some((b) => b.id === boardId)) boardId = boards[0]?.id ?? null;
    } catch (e) {
      app.error('Could not load soundboards', e);
    }
  }
  onMount(() => void load());
  let seen = app.dataVersion;
  $effect(() => {
    if (app.dataVersion !== seen) {
      seen = app.dataVersion;
      if (!editing) void load();
    }
  });

  const press = (b: SoundboardButton) => musicUi.act('Could not play', () => music.soundboardPress(b.id));
  const playing = (b: SoundboardButton) => musicUi.state?.toggles_playing.includes(b.id) ?? false;

  function onKey(e: KeyboardEvent) {
    if (musicUi.tab !== 'soundboard' || editing || !board || e.ctrlKey || e.metaKey || e.altKey) return;
    const t = e.target as HTMLElement;
    if (t.closest('input, textarea, select, [contenteditable]')) return;
    const sorted = [...board.buttons].sort((a, b) => a.position - b.position);
    const hit = sorted.find((b) => b.hotkey && b.hotkey.toLowerCase() === e.key.toLowerCase())
      ?? (/^[1-9]$/.test(e.key) && !sorted.some((b) => b.hotkey) ? sorted[+e.key - 1] : undefined);
    if (hit) {
      e.preventDefault();
      void press(hit);
    }
  }

  // ── new board ──
  let newOpen = $state(false);
  let newName = $state('');
  async function createBoard() {
    if (!newName.trim()) return;
    try {
      const b = await music.soundboardSave({ name: newName.trim(), buttons: [] });
      newOpen = false;
      await load();
      boardId = b.id;
      void startEdit();
    } catch (e) {
      app.error('Could not create the board', e);
    }
  }

  async function deleteBoard() {
    if (!board || !confirm(`Delete soundboard “${board.name}”?`)) return;
    try {
      await music.soundboardDelete(board.id);
      boardId = null;
      void load();
    } catch (e) {
      app.error('Could not delete', e);
    }
  }

  // ── edit mode ──
  async function startEdit() {
    if (!board) return;
    try {
      tracks = await music.tracksList({});
    } catch (e) {
      return app.error('Could not load tracks', e);
    }
    draft = board.buttons.map((b) => ({ ...b })).sort((a, b) => a.position - b.position);
    editing = true;
  }
  function addButton() {
    draft.push({
      id: '', track_id: tracks[0]?.id ?? null, label: tracks[0]?.title.slice(0, 18) ?? 'Sound', icon: '',
      color: BUTTON_COLORS[draft.length % BUTTON_COLORS.length], position: draft.length, loop_by_default: false,
      volume: 80, play_mode: 'oneshot', hotkey: null,
    });
  }
  function moveBtn(i: number, d: -1 | 1) {
    const j = i + d;
    if (j < 0 || j >= draft.length) return;
    [draft[i], draft[j]] = [draft[j], draft[i]];
  }
  async function saveEdit() {
    if (!board) return;
    if (draft.some((b) => !b.track_id)) return app.toast('error', 'Every button needs a track');
    try {
      await music.soundboardSave({
        id: board.id, name: board.name, notes: board.notes,
        buttons: draft.map((b, i) => ({ ...b, position: i, hotkey: b.hotkey?.trim() || null })),
      });
      editing = false;
      void load();
    } catch (e) {
      app.error('Could not save the board', e);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="pane">
  <div class="view-toolbar">
    {#if boards.length}
      <select class="input sel" aria-label="Board" bind:value={boardId} disabled={editing}>
        {#each boards as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
      </select>
    {/if}
    {#if editing}
      <button class="btn" onclick={addButton}>+ Add Button</button>
      <div class="spacer"></div>
      <button class="btn" onclick={() => (editing = false)}>Cancel</button>
      <button class="btn primary" onclick={saveEdit}>Save</button>
    {:else}
      <button class="btn" onclick={() => ((newName = ''), (newOpen = true))}>New Board</button>
      <button class="btn" disabled={!board} onclick={startEdit}>Edit</button>
      <button class="btn ghost" disabled={!board} onclick={deleteBoard}>Delete Board</button>
    {/if}
  </div>
  <div class="pane-body">

  {#if !board}
    <div class="empty-state">
      <strong>No soundboards yet</strong>
      <span>A board is a grid of buttons that play sounds, with hotkeys 1–9.</span>
      <button class="btn primary" onclick={() => ((newName = ''), (newOpen = true))}>New Board</button>
    </div>
  {:else if editing}
    <div class="list" role="table" aria-label="Buttons">
      <div class="list-head" role="row" style:grid-template-columns={EDIT_COLUMNS}>
        <span role="columnheader">#</span><span role="columnheader">Icon</span><span role="columnheader">Label</span><span role="columnheader">Track</span>
        <span role="columnheader">Mode <InfoTip label="About button modes" text="One-shot plays the sound once. Restart plays it again from the start on every press. Toggle starts it on the first press and stops it on the next." /></span>
        <span role="columnheader">Colour</span><span role="columnheader">Vol</span><span role="columnheader">Loop</span><span role="columnheader">Key</span>
        <span role="columnheader"><span class="visually-hidden">Actions</span></span>
      </div>
      {#each draft as b, i (i)}
        <div class="list-row" role="row" style:grid-template-columns={EDIT_COLUMNS}>
          <span role="cell" class="list-muted">{i + 1}</span>
          <span role="cell"><input class="input" bind:value={b.icon} aria-label="Icon" maxlength="4" /></span>
          <span role="cell"><input class="input" bind:value={b.label} aria-label="Label" /></span>
          <span role="cell">
            <select class="input" bind:value={b.track_id} aria-label="Track">
              {#each tracks as t (t.id)}<option value={t.id}>{t.title}</option>{/each}
            </select>
          </span>
          <span role="cell">
            <select class="input" bind:value={b.play_mode} aria-label="Mode">
              {#each MODES as m (m)}<option value={m}>{PLAY_MODE_SYMBOLS[m]} {m}</option>{/each}
            </select>
          </span>
          <span role="cell" class="swatches">
            {#each BUTTON_COLORS as c (c)}
              <button class="sw" style:background={c} aria-pressed={b.color === c} aria-label="Colour {c}" onclick={() => (b.color = c)}></button>
            {/each}
          </span>
          <span role="cell"><input class="input" type="number" min="0" max="100" bind:value={b.volume} aria-label="Volume" /></span>
          <span role="cell"><input type="checkbox" bind:checked={b.loop_by_default} aria-label="Loop" /></span>
          <span role="cell"><input class="input" maxlength="1" value={b.hotkey ?? ''} oninput={(e) => (b.hotkey = e.currentTarget.value || null)} aria-label="Hotkey" /></span>
          <span role="cell" class="list-actions">
            <button class="btn ghost" aria-label="Move up" onclick={() => moveBtn(i, -1)}>↑</button>
            <button class="btn ghost" aria-label="Move down" onclick={() => moveBtn(i, 1)}>↓</button>
            <button class="btn ghost" aria-label="Remove button" onclick={() => draft.splice(i, 1)}>✕</button>
          </span>
        </div>
      {/each}
    </div>
  {:else}
    <div class="tile-grid pads" style:--tile-min="140px">
      {#each [...board.buttons].sort((a, b) => a.position - b.position) as b, i (b.id)}
        <button class="tile pad" class:active={playing(b)} style:background={b.color} onclick={() => press(b)} title="{b.play_mode}{b.hotkey ? ` · key ${b.hotkey}` : i < 9 ? ` · key ${i + 1}` : ''}">
          {#if b.icon}<span class="ico">{b.icon}</span>{/if}
          <span class="lbl">{b.label}</span>
          <span class="mode">{PLAY_MODE_SYMBOLS[b.play_mode]}</span>
          {#if musicUi.playback[b.id]}<span class="pad-pi"><PlayIndicator status={musicUi.playback[b.id]} size={12} inherit /></span>{/if}
        </button>
      {:else}
        <p class="muted">This board has no buttons. Click Edit to add some.</p>
      {/each}
    </div>
  {/if}
  </div>
</div>

<Modal title="New soundboard" bind:open={newOpen} width={400}>
  <form class="form" onsubmit={(e) => (e.preventDefault(), void createBoard())}>
    <label class="field"><span class="label">Name</span><input class="input" bind:value={newName} required /></label>
    <div class="actions">
      <button type="button" class="btn" onclick={() => (newOpen = false)}>Cancel</button>
      <button type="submit" class="btn primary" disabled={!newName.trim()}>Create</button>
    </div>
  </form>
</Modal>

<style>
  .sel {
    width: auto;
  }
  .spacer {
    flex: 1;
  }
  /* a pad: a square colour button on the shared tile grid; .active while it plays */
  .pads {
    padding: 0;
    overflow: visible;
  }
  .pad {
    aspect-ratio: 1;
    border: 0;
    color: #fff;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: var(--space-2);
    box-shadow: var(--shadow);
    font: inherit;
  }
  .pad:hover {
    filter: brightness(1.15);
  }
  .pad:active {
    transform: scale(0.97);
  }
  .pad.active {
    outline: 3px solid var(--success);
    outline-offset: 2px;
  }
  .pad .ico {
    font-size: 32px;
    line-height: 1;
  }
  .pad .lbl {
    font-size: var(--fs-md);
    font-weight: 600;
    text-align: center;
    line-height: 1.2;
    overflow: hidden;
    max-height: 2.4em;
  }
  .pad .mode {
    position: absolute;
    top: 6px;
    left: 8px;
    font-size: var(--fs-xs);
    opacity: 0.85;
  }
  .pad-pi {
    position: absolute;
    top: 8px;
    right: 8px;
    filter: drop-shadow(0 0 1px rgb(0 0 0 / 0.6));
  }
  .swatches {
    display: flex;
    gap: 3px;
  }
  .sw {
    width: 20px;
    height: 20px;
    border-radius: 3px;
    border: 2px solid transparent;
    cursor: pointer;
    padding: 0;
  }
  .sw[aria-pressed='true'] {
    border-color: var(--text);
  }
  .form {
    display: grid;
    gap: var(--space-3);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
