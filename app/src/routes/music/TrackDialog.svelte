<script lang="ts">
  // Add/edit a track (Python ui/dm_music_track_dialog.py).
  import Modal from '../../lib/components/Modal.svelte';
  import { pickFile } from '../../lib/api';
  import {
    MEDIA_EXTENSIONS, TRACK_CATEGORIES, formatDuration, music, type Track, type TrackCategory, type TrackInput, type TrackSource,
  } from '../../lib/music';
  import { app } from '../../lib/state.svelte';

  let {
    open = $bindable(false),
    track = null,
    allTags,
    onSaved,
  }: {
    open: boolean;
    track: Track | null;
    allTags: string[];
    onSaved: (t: Track) => void;
  } = $props();

  let source = $state<TrackSource>('youtube');
  let url = $state('');
  let filePath = $state<string | null>(null);
  let title = $state('');
  let notes = $state('');
  let loop = $state(true);
  /** null = Auto: the server guesses from the title, loop flag and length. */
  let category = $state<TrackCategory | null>(null);
  let duration = $state<number | null>(null);
  let tags = $state<string[]>([]);
  let tagInput = $state('');
  let fetching = $state(false);
  let saving = $state(false);
  let error = $state('');

  // Reset the form whenever the dialog opens.
  let wasOpen = false;
  $effect(() => {
    if (open && !wasOpen) {
      const t = track;
      source = t?.source ?? 'youtube';
      url = t?.url ?? '';
      filePath = null;
      title = t?.title ?? '';
      notes = t?.notes ?? '';
      loop = t?.loop_by_default ?? true;
      category = t?.category ?? null;
      duration = t?.duration ?? null;
      tags = t ? [...t.tags] : [];
      tagInput = '';
      error = '';
    }
    wasOpen = open;
  });

  const toggle = (list: string[], id: string) => (list.includes(id) ? list.filter((x) => x !== id) : [...list, id]);

  async function fetchMeta() {
    if (source !== 'youtube' || !url.trim()) return;
    fetching = true;
    try {
      const m = await music.fetchYoutubeMeta(url.trim());
      if (!title.trim() && m.title) title = m.title;
      if (m.duration != null) duration = m.duration;
    } catch {
      /* best effort, like Python */
    } finally {
      fetching = false;
    }
  }

  async function chooseFile() {
    const p = await pickFile('Choose audio file', MEDIA_EXTENSIONS, 'Audio');
    if (!p) return;
    filePath = p;
    try {
      const m = await music.probeFile(p);
      if (!title.trim() && m.title) title = m.title;
      duration = m.duration;
    } catch {
      if (!title.trim()) title = p.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, '') ?? '';
    }
  }

  function addTag() {
    const t = tagInput.trim().replace(/,$/, '');
    if (t && !tags.includes(t)) tags = [...tags, t];
    tagInput = '';
  }

  async function save() {
    error = '';
    if (!title.trim()) return void (error = 'Title is required.');
    if (source === 'youtube' && !url.trim()) return void (error = 'Enter a YouTube URL.');
    if (source === 'upload' && !filePath && !track?.blob_hash) return void (error = 'Choose a file.');
    if (tagInput.trim()) addTag();
    const input: TrackInput = {
      source,
      url: source === 'youtube' ? url.trim() : null,
      file_path: source === 'upload' ? filePath : null,
      title: title.trim(),
      notes,
      loop_by_default: loop,
      duration,
      tags,
      category,
    };
    saving = true;
    try {
      const t = track ? await music.trackUpdate(track.id, input) : await music.trackCreate(input);
      onSaved(t);
      open = false;
    } catch (e) {
      error = (e as { message?: string })?.message ?? String(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal title={track ? 'Edit track' : 'Add track'} bind:open width={460}>
  <form class="form" onsubmit={(e) => (e.preventDefault(), void save())}>
    <div class="segmented" role="group" aria-label="Source">
      <button type="button" aria-pressed={source === 'youtube'} onclick={() => (source = 'youtube')}>YouTube</button>
      <button type="button" aria-pressed={source === 'upload'} onclick={() => (source = 'upload')}>Upload file</button>
    </div>
    {#if source === 'youtube'}
      <label class="field">
        <span class="label">YouTube URL {#if fetching}<span class="muted">Fetching…</span>{/if}</span>
        <input class="input" bind:value={url} onblur={fetchMeta} placeholder="https://www.youtube.com/watch?v=…" />
      </label>
    {:else}
      <div class="field">
        <span class="label">File</span>
        <div class="row">
          <button type="button" class="btn" onclick={chooseFile}>Choose File…</button>
          <span class="muted file">{filePath ? filePath.split(/[\\/]/).pop() : track?.blob_hash ? 'keeping the current file' : 'no file'}</span>
        </div>
      </div>
    {/if}
    <label class="field">
      <span class="label">Title {#if duration}<span class="muted">· {formatDuration(duration)}</span>{/if}</span>
      <input class="input" bind:value={title} />
    </label>
    <div class="field">
      <span class="label">Category</span>
      <div class="segmented" role="group" aria-label="Category">
        {#if !track}<button type="button" aria-pressed={category === null} onclick={() => (category = null)} title="Guess from the title and length">Auto</button>{/if}
        {#each TRACK_CATEGORIES as c (c.id)}
          <button
            type="button"
            aria-pressed={category === c.id}
            onclick={() => {
              category = c.id;
              // Soundbites are one-shots: don't loop them by default.
              if (c.id === 'soundbite') loop = false;
            }}>{c.label}</button
          >
        {/each}
      </div>
    </div>
    <label class="check"><input type="checkbox" bind:checked={loop} /> Loop by default</label>
    <div class="field">
      <span class="label">Tags</span>
      <div class="chips">
        {#each tags as t (t)}
          <button type="button" class="chip" aria-pressed="true" title="Remove" onclick={() => (tags = tags.filter((x) => x !== t))}>{t} ✕</button>
        {/each}
        <input
          class="input tag-input"
          list="music-tags"
          bind:value={tagInput}
          placeholder="add tag"
          onkeydown={(e) => {
            if (e.key === 'Enter' || e.key === ',') {
              e.preventDefault();
              addTag();
            }
          }}
        />
        <datalist id="music-tags">
          {#each allTags.filter((t) => !tags.includes(t)) as t (t)}<option value={t}></option>{/each}
        </datalist>
      </div>
    </div>
    <label class="field">
      <span class="label">Notes</span>
      <textarea class="input" rows="2" bind:value={notes}></textarea>
    </label>
    {#if error}<p class="error-text" role="alert">{error}</p>{/if}
    <div class="actions">
      <button type="button" class="btn" onclick={() => (open = false)}>Cancel</button>
      <button type="submit" class="btn primary" disabled={saving}>{track ? 'Update' : 'Save'}</button>
    </div>
  </form>
</Modal>

<style>
  .form {
    display: grid;
    gap: var(--space-3);
  }
  .row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .file {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  .chip {
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    border-radius: 999px;
    padding: 2px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .chip[aria-pressed='true'] {
    background: var(--accent-soft);
    border-color: var(--accent);
  }
  .tag-input {
    width: 110px;
    padding: 2px 8px;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: 13px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
