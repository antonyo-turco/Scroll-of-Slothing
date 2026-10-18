<script lang="ts">
  // Live mixer (Python ui/dm_music_mixer_panel.py): channels, master volume,
  // ducking, soundbites, soundscape save/update and .dmix files.
  import Modal from '../../lib/components/Modal.svelte';
  import ViewActions from '../../lib/components/ViewActions.svelte';
  import PlayIndicator from '../../lib/components/PlayIndicator.svelte';
  import { pickFile, pickSavePath } from '../../lib/api';
  import { DMIX_EXTENSION, music, type Channel, type ChannelRole, type DuckMode, type Soundbite, type Track } from '../../lib/music';
  import { musicUi } from '../../lib/music-ui.svelte';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import { app } from '../../lib/state.svelte';
  let { onImport }: { onImport: (path: string) => Promise<void> } = $props();

  const st = $derived(musicUi.state);
  const ROLES: ChannelRole[] = ['music', 'ambient', 'sfx'];
  const DUCKS: [DuckMode, string][] = [['off', 'Off'], ['soft', 'Soft'], ['strong', 'Strong']];

  // Values being dragged (applied live, throttled) so the slider does not jump.
  let dragging = $state<Record<string, number>>({});
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  function throttled(key: string, f: () => Promise<unknown>) {
    if (timers.has(key)) clearTimeout(timers.get(key));
    timers.set(
      key,
      setTimeout(() => {
        timers.delete(key);
        void musicUi.act('Could not update the mixer', f);
      }, 60),
    );
  }

  function channelVolume(c: Channel, v: number) {
    dragging[c.id] = v;
    throttled(c.id, async () => {
      const r = await music.channelUpdate(c.id, { volume: v });
      delete dragging[c.id];
      return r;
    });
  }

  function masterVolume(v: number) {
    dragging.__master = v;
    throttled('__master', async () => {
      const r = await music.setMaster(v);
      delete dragging.__master;
      return r;
    });
  }

  function soundbiteVolume(s: Soundbite, v: number) {
    dragging[s.id] = v;
    throttled(s.id, async () => {
      const r = await music.soundbiteUpdate(s.id, { volume: v });
      delete dragging[s.id];
      return r;
    });
  }

  const trunc = (s: string, n = 42) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);

  // ── soundbite dialog ──
  let sbOpen = $state(false);
  let sbTracks = $state<Track[]>([]);
  let sbTrack = $state('');
  let sbMin = $state(30);
  let sbMax = $state(90);
  let sbVol = $state(80);
  async function openSoundbite() {
    try {
      sbTracks = await music.tracksList({});
      sbTrack = sbTracks[0]?.id ?? '';
      sbMin = 30;
      sbMax = 90;
      sbVol = 80;
      sbOpen = true;
    } catch (e) {
      app.error('Could not load tracks', e);
    }
  }
  async function addSoundbite() {
    if (!sbTrack) return;
    sbOpen = false;
    await musicUi.act('Could not add the soundbite', () => music.soundbiteAdd(sbTrack, sbMin, sbMax, sbVol));
  }

  // ── save soundscape dialog ──
  let saveOpen = $state(false);
  let ssName = $state('');
  let ssNotes = $state('');
  let ssTags = $state('');
  function openSave() {
    ssName = '';
    ssNotes = '';
    ssTags = '';
    saveOpen = true;
  }
  async function saveSoundscape() {
    if (!ssName.trim()) return;
    try {
      await music.soundscapeSave({
        name: ssName.trim(),
        notes: ssNotes,
        tags: ssTags.split(',').map((t) => t.trim()).filter(Boolean),
      });
      saveOpen = false;
      app.toast('success', `Soundscape “${ssName.trim()}” saved`);
      void musicUi.refresh();
    } catch (e) {
      app.error('Could not save the soundscape', e);
    }
  }

  let updated = $state(false);
  async function updateSoundscape() {
    if (!st?.soundscape_id) return;
    try {
      await music.soundscapeUpdate(st.soundscape_id);
      updated = true;
      setTimeout(() => (updated = false), 1300);
    } catch (e) {
      app.error('Could not update the soundscape', e);
    }
  }

  // ── .dmix ──
  async function saveDmix() {
    const base = (st?.soundscape_name ?? 'mix').replace(/[\\/:*?"<>|]/g, '_');
    const path = await pickSavePath('Save mix as .dmix', `${base}.${DMIX_EXTENSION}`, [DMIX_EXTENSION]);
    if (!path) return;
    try {
      await music.mixExport(path, null);
      app.toast('success', 'Mix saved', path);
    } catch (e) {
      app.error('Could not save the mix', e);
    }
  }
  async function openDmix() {
    const path = await pickFile('Open a .dmix mix', [DMIX_EXTENSION], 'DM Music Mix');
    if (path) await onImport(path);
  }
</script>

<ViewActions>
  <button class="btn" onclick={openDmix} title="Load a mix file (.dmix)">Open .dmix…</button>
  <button class="btn" onclick={saveDmix} disabled={!st?.channels.length && !st?.soundbites.length} title="Write the live mix to a .dmix file">Save .dmix…</button>
</ViewActions>

<div class="pane">
  <div class="view-toolbar">
    <button class="btn" onclick={() => musicUi.act('Could not play', () => music.setAllPlaying(true))}>▶ All</button>
    <button class="btn" onclick={() => musicUi.act('Could not pause', () => music.setAllPlaying(false))}>⏸ All</button>
    <button class="btn danger" onclick={() => musicUi.act('Could not stop', () => music.stopAll())}>⏹ Stop All</button>
    {#if st?.soundscape_id}
      <button class="btn" onclick={updateSoundscape}>{updated ? '✓ Updated!' : `✓ Update “${st.soundscape_name}”`}</button>
    {/if}
    <button class="btn" onclick={openSave} disabled={!st?.channels.length && !st?.soundbites.length}>💾 Save as New</button>
    <button class="btn" onclick={() => void music.toggleVideo()}>📹 Toggle Video</button>
    <div class="spacer"></div>
    <InfoTip label="About ducking" text="Ducking lowers the music while a soundbite or sound effect plays, so it can be heard. Soft lowers it a little, Strong a lot." />
    <div class="segmented" role="group" aria-label="Duck under SFX" title="Duck the music while a soundbite plays">
      <span class="seg-label">🔉 Duck</span>
      {#each DUCKS as [mode, label] (mode)}
        <button aria-pressed={st?.duck === mode} onclick={() => musicUi.act('Could not set ducking', () => music.setDuck(mode))}>{label}</button>
      {/each}
    </div>
  </div>
  <div class="mixer">
    <h2>
      Active Mix{#if st?.soundscape_name} — {st.soundscape_name}{/if}
      <span class="muted">({st?.channels.length ?? 0} channels)</span>
    </h2>

  <div class="master card">
    <label for="master">Master</label>
    <input
      id="master"
      type="range"
      min="0"
      max="100"
      value={dragging.__master ?? st?.master_volume ?? 100}
      oninput={(e) => masterVolume(+e.currentTarget.value)}
    />
    <span class="num">{dragging.__master ?? st?.master_volume ?? 100}</span>
    {#if st?.ducked}<span class="pill">ducked</span>{/if}
    {#if st && !st.engine_attached}<span class="muted small">no audio engine attached</span>{/if}
  </div>

  {#if !st?.channels.length}
    <p class="muted empty">The mix is empty. Add tracks from the Library, or load a soundscape or a .dmix file (you can also drop one here).</p>
  {/if}
  <div class="channels">
    {#each st?.channels ?? [] as c (c.id)}
      <div class="channel card">
        <div class="ch-head">
          <PlayIndicator status={musicUi.statusFor(c.id, c.is_playing)} />
          <span class="title" title={c.title}>{trunc(c.title)}</span>
          <select class="input role" value={c.role} aria-label="Role" onchange={(e) => musicUi.act('Could not set the role', () => music.channelUpdate(c.id, { role: e.currentTarget.value as ChannelRole }))}>
            {#each ROLES as r (r)}<option value={r}>{r}</option>{/each}
          </select>
          <button class="btn ghost" aria-label="Remove {c.title}" onclick={() => musicUi.act('Could not remove the channel', () => music.channelRemove(c.id))}>✕</button>
        </div>
        <div class="ch-row">
          <button
            class="btn icon"
            aria-label={c.is_playing ? `Pause ${c.title}` : `Play ${c.title}`}
            onclick={() => musicUi.act('Could not toggle playback', () => music.channelUpdate(c.id, { is_playing: !c.is_playing }))}
          >{c.is_playing ? '⏸' : '▶'}</button>
          <button
            class="btn icon"
            aria-pressed={c.is_looping}
            title="Loop"
            aria-label="Loop {c.title}"
            onclick={() => musicUi.act('Could not toggle loop', () => music.channelUpdate(c.id, { is_looping: !c.is_looping }))}
          >↻</button>
          <input
            type="range"
            min="0"
            max="100"
            aria-label="Volume {c.title}"
            value={dragging[c.id] ?? c.volume}
            oninput={(e) => channelVolume(c, +e.currentTarget.value)}
          />
          <span class="num">{dragging[c.id] ?? c.volume}</span>
        </div>
      </div>
    {/each}
  </div>

  <div class="toolbar">
    <h2>Soundbites <InfoTip label="About soundbites" text="Short sounds that play on their own at random times between the minimum and maximum interval, such as birds or distant thunder." /> <PlayIndicator status={musicUi.oneshotStatus} /> {#if st?.soundbites_running}<span class="pill">running</span>{/if}</h2>
    <div class="spacer"></div>
    {#if st?.sfx_queue}<span class="muted small">{st.sfx_queue} queued</span>{/if}
    <button class="btn" onclick={openSoundbite}>+ Soundbite</button>
  </div>
  <div class="soundbites">
    {#each st?.soundbites ?? [] as s (s.id)}
      <div class="sb card">
        <span class="title" title={s.title}>🔔 {trunc(s.title, 30)}</span>
        <button class="btn ghost" title="Play now (ducks the music)" aria-label="Play {s.title}" onclick={() => musicUi.act('Could not play the soundbite', () => music.soundbitePlay(s.id))}>▶</button>
        <label class="small">Every
          <input class="input num-in" type="number" min="1" max="3600" value={s.min_interval}
            onchange={(e) => musicUi.act('Could not update', () => music.soundbiteUpdate(s.id, { min_interval: +e.currentTarget.value }))} />
          –
          <input class="input num-in" type="number" min="1" max="3600" value={s.max_interval}
            onchange={(e) => musicUi.act('Could not update', () => music.soundbiteUpdate(s.id, { max_interval: +e.currentTarget.value }))} />
          s
        </label>
        <input type="range" min="0" max="100" aria-label="Volume {s.title}" value={dragging[s.id] ?? s.volume} oninput={(e) => soundbiteVolume(s, +e.currentTarget.value)} />
        <span class="num">{dragging[s.id] ?? s.volume}</span>
        <button class="btn ghost" aria-label="Remove {s.title}" onclick={() => musicUi.act('Could not remove', () => music.soundbiteRemove(s.id))}>✕</button>
      </div>
    {:else}
      <p class="muted empty">No soundbites. They play at random intervals and can duck the music.</p>
    {/each}
  </div>
  </div>
</div>

<Modal title="Add soundbite" bind:open={sbOpen} width={420}>
  <form class="form" onsubmit={(e) => (e.preventDefault(), void addSoundbite())}>
    <label class="field"><span class="label">Track</span>
      <select class="input" bind:value={sbTrack}>
        {#each sbTracks as t (t.id)}<option value={t.id}>{t.title}</option>{/each}
      </select>
    </label>
    <div class="row">
      <label class="field"><span class="label">Every min (s)</span><input class="input" type="number" min="1" max="3600" bind:value={sbMin} /></label>
      <label class="field"><span class="label">max (s)</span><input class="input" type="number" min="1" max="3600" bind:value={sbMax} /></label>
      <label class="field"><span class="label">Volume</span><input class="input" type="number" min="0" max="100" bind:value={sbVol} /></label>
    </div>
    <div class="actions">
      <button type="button" class="btn" onclick={() => (sbOpen = false)}>Cancel</button>
      <button type="submit" class="btn primary" disabled={!sbTrack}>Add</button>
    </div>
  </form>
</Modal>

<Modal title="Save soundscape" bind:open={saveOpen} width={420}>
  <form class="form" onsubmit={(e) => (e.preventDefault(), void saveSoundscape())}>
    <label class="field"><span class="label">Name</span><input class="input" bind:value={ssName} required /></label>
    <label class="field"><span class="label">Notes</span><textarea class="input" rows="2" bind:value={ssNotes}></textarea></label>
    <label class="field"><span class="label">Tags (comma separated)</span><input class="input" bind:value={ssTags} /></label>
    <div class="actions">
      <button type="button" class="btn" onclick={() => (saveOpen = false)}>Cancel</button>
      <button type="submit" class="btn primary" disabled={!ssName.trim()}>Save</button>
    </div>
  </form>
</Modal>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .mixer {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--space-3) var(--space-4);
    display: grid;
    /* one column that may shrink below its content's min width: long track
       titles and sliders must never widen the page */
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-3);
    align-content: start;
  }
  h2 {
    font-size: 15px;
    margin: 0;
  }
  .spacer {
    flex: 1;
  }
  .seg-label {
    font-size: 12px;
    padding: 0 8px;
    align-self: center;
    color: var(--text-muted);
  }
  .master {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    padding: var(--space-2) var(--space-3);
  }
  .master input[type='range'] {
    flex: 0 1 320px;
    min-width: 80px;
  }
  .channels {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 300px), 1fr));
    gap: var(--space-2);
  }
  .channel {
    min-width: 0;
    padding: var(--space-2) var(--space-3);
    display: grid;
    gap: 6px;
  }
  .ch-head,
  .ch-row {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  .ch-head > :not(.title),
  .ch-row > :not(input[type='range']) {
    flex: none;
  }
  /* range inputs have an intrinsic min width; let them shrink */
  input[type='range'] {
    min-width: 0;
  }
  .ch-row input[type='range'] {
    flex: 1 1 0;
  }
  .title {
    flex: 1 1 0;
    min-width: 0;
    font-weight: 600;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .role {
    width: auto;
    padding: 2px 6px;
    font-size: 12px;
  }
  .btn[aria-pressed='true'] {
    background: var(--accent-soft);
    border-color: var(--accent);
  }
  .num {
    flex: none;
    width: 3ch;
    text-align: right;
    font-variant-numeric: tabular-nums;
    font-size: 12px;
  }
  .soundbites {
    display: grid;
    gap: 6px;
  }
  .sb {
    display: flex;
    flex-wrap: wrap;
    gap: 4px var(--space-2);
    align-items: center;
    padding: 4px var(--space-3);
    min-width: 0;
  }
  .sb .title {
    flex: 1 1 12ch;
  }
  .sb input[type='range'] {
    flex: 1 1 100px;
  }
  .num-in {
    width: 64px;
    padding: 2px 6px;
  }
  .small {
    font-size: 12px;
  }
  .pill {
    font-size: 11px;
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--text);
    font-weight: 500;
  }
  .empty {
    margin: 0;
    font-size: 13px;
  }
  .form {
    display: grid;
    gap: var(--space-3);
  }
  .row {
    display: flex;
    gap: var(--space-2);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
