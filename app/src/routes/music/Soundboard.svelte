<script lang="ts">
  // Soundboards (Python ui/dm_music_soundboard_panel.py): a 4-column grid of
  // buttons playing one-shots, restartable or toggled channels. New: stable
  // button ids, per-button volume and hotkeys (1–9 by position by default).
  import { onMount } from 'svelte';
  import Modal from '../../lib/components/Modal.svelte';
  import PlayIndicator from '../../lib/components/PlayIndicator.svelte';
  import { BUTTON_COLORS, PLAY_MODE_SYMBOLS, music, type PlayMode, type Soundboard, type SoundboardButton, type Track } from '../../lib/music';
  import { musicUi } from '../../lib/music-ui.svelte';
  import { app } from '../../lib/state.svelte';
  let boards = $state<Soundboard[]>([]);
  let boardId = $state<string | null>(null);
  let editing = $state(false);
  let draft = $state<SoundboardButton[]>([]);
  let tracks = $state<Track[]>([]);
  const board = $derived(boards.find((b) => b.id === boardId) ?? null);
  const MODES: PlayMode[] = ['oneshot', 'restart', 'toggle'];

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

<div class="view">
  <div class="bar">
    <select class="input sel" aria-label="Board" bind:value={boardId} disabled={editing}>
      {#each boards as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
    </select>
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

  {#if !board}
    <p class="muted">No soundboards yet. Create one with New Board.</p>
  {:else if editing}
    <table>
      <thead><tr><th>Pos</th><th>Icon</th><th>Label</th><th>Track</th><th>Mode</th><th>Colour</th><th>Vol</th><th>Loop</th><th>Key</th><th></th></tr></thead>
      <tbody>
        {#each draft as b, i (i)}
          <tr>
            <td>{i + 1}</td>
            <td><input class="input icon-in" bind:value={b.icon} aria-label="Icon" maxlength="4" /></td>
            <td><input class="input" bind:value={b.label} aria-label="Label" /></td>
            <td>
              <select class="input" bind:value={b.track_id} aria-label="Track">
                {#each tracks as t (t.id)}<option value={t.id}>{t.title}</option>{/each}
              </select>
            </td>
            <td>
              <select class="input" bind:value={b.play_mode} aria-label="Mode">
                {#each MODES as m (m)}<option value={m}>{PLAY_MODE_SYMBOLS[m]} {m}</option>{/each}
              </select>
            </td>
            <td class="swatches">
              {#each BUTTON_COLORS as c (c)}
                <button class="sw" style:background={c} aria-pressed={b.color === c} aria-label="Colour {c}" onclick={() => (b.color = c)}></button>
              {/each}
            </td>
            <td><input class="input num" type="number" min="0" max="100" bind:value={b.volume} aria-label="Volume" /></td>
            <td><input type="checkbox" bind:checked={b.loop_by_default} aria-label="Loop" /></td>
            <td><input class="input key" maxlength="1" value={b.hotkey ?? ''} oninput={(e) => (b.hotkey = e.currentTarget.value || null)} aria-label="Hotkey" /></td>
            <td class="row-actions">
              <button class="btn ghost" aria-label="Move up" onclick={() => moveBtn(i, -1)}>↑</button>
              <button class="btn ghost" aria-label="Move down" onclick={() => moveBtn(i, 1)}>↓</button>
              <button class="btn ghost" aria-label="Remove button" onclick={() => draft.splice(i, 1)}>✕</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <div class="grid">
      {#each [...board.buttons].sort((a, b) => a.position - b.position) as b, i (b.id)}
        <button class="tile" class:on={playing(b)} style:background={b.color} onclick={() => press(b)} title="{b.play_mode}{b.hotkey ? ` · key ${b.hotkey}` : i < 9 ? ` · key ${i + 1}` : ''}">
          {#if b.icon}<span class="ico">{b.icon}</span>{/if}
          <span class="lbl">{b.label}</span>
          <span class="mode">{PLAY_MODE_SYMBOLS[b.play_mode]}</span>
          {#if musicUi.playback[b.id]}<span class="tile-pi"><PlayIndicator status={musicUi.playback[b.id]} size={12} inherit /></span>{/if}
        </button>
      {:else}
        <p class="muted">This board has no buttons. Click Edit to add some.</p>
      {/each}
    </div>
  {/if}
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
  .sel {
    width: auto;
  }
  .spacer {
    flex: 1;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(4, 80px);
    gap: 10px;
  }
  .tile {
    width: 80px;
    height: 80px;
    border: 0;
    border-radius: var(--radius);
    color: #fff;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    padding: 4px;
    position: relative;
    box-shadow: var(--shadow);
    font: inherit;
  }
  .tile:hover {
    filter: brightness(1.15);
  }
  .tile:active {
    transform: scale(0.97);
  }
  .tile {
    position: relative;
  }
  .tile-pi {
    position: absolute;
    top: 5px;
    right: 5px;
    filter: drop-shadow(0 0 1px rgb(0 0 0 / 0.6));
  }
  .tile.on {
    outline: 3px solid #00cc66;
    outline-offset: 2px;
  }
  .ico {
    font-size: 22px;
    line-height: 1;
  }
  .lbl {
    font-size: 11px;
    text-align: center;
    line-height: 1.15;
    overflow: hidden;
    max-height: 2.4em;
  }
  .mode {
    position: absolute;
    top: 2px;
    right: 4px;
    font-size: 9px;
    opacity: 0.8;
  }
  table {
    border-collapse: collapse;
    font-size: 13px;
  }
  th {
    text-align: left;
    font-size: 12px;
    color: var(--text-muted);
    padding: 4px 6px;
  }
  td {
    padding: 2px 6px;
  }
  td .input {
    padding: 2px 6px;
  }
  .icon-in {
    width: 48px;
  }
  .num {
    width: 60px;
  }
  .key {
    width: 36px;
  }
  .swatches {
    display: flex;
    gap: 3px;
  }
  .sw {
    width: 16px;
    height: 16px;
    border-radius: 3px;
    border: 2px solid transparent;
    cursor: pointer;
    padding: 0;
  }
  .sw[aria-pressed='true'] {
    border-color: var(--text);
  }
  .row-actions {
    white-space: nowrap;
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
