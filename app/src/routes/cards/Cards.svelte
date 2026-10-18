<script lang="ts">
  // Card printer: markdown editor + exact live preview + duplex PDF export.
  // Spec: docs/features/cards.md
  import { onMount } from 'svelte';
  import { api, isTauri, pickFile, pickSavePath } from '../../lib/api';
  import Modal from '../../lib/components/Modal.svelte';
  import { registerCardFonts } from '../../lib/fonts';
  import InfoTip from '../../lib/components/InfoTip.svelte';
  import { app } from '../../lib/state.svelte';
  import ViewActions from '../../lib/components/ViewActions.svelte';
  import type { CardOptions, CardTemplates, DeckLayout, DeckSummary, Flip, PageFormat } from '../../lib/types';
  import CardPage from './CardPage.svelte';

  const EDITOR_KEY = 'cards.editor';
  // Python button order (the JSON map itself is unordered).
  const TEMPLATE_ORDER = ['Spell', 'Feat', 'Item', 'Ability'];

  interface EditorState {
    markdown: string;
    opts: CardOptions;
    deck_id: number | null;
    deck_name: string;
  }

  let s = $state<EditorState>({
    markdown: '',
    opts: { format: app.settings.card_page_format, double_sided: false, flip: 'long' },
    deck_id: null,
    deck_name: '',
  });
  let layout = $state<DeckLayout | null>(null);
  let templates = $state<CardTemplates | null>(null);
  let decks = $state<DeckSummary[]>([]);
  let loaded = $state(false);
  let refOpen = $state(false);
  let saveOpen = $state(false);
  let zoomPage = $state<number | null>(null);
  let editor: HTMLTextAreaElement | undefined = $state();

  const frontCount = $derived(layout?.pages.filter((p) => !p.back).length ?? 0);

  // ── live layout (debounced, latest wins) ──
  let timer: ReturnType<typeof setTimeout> | null = null;
  let gen = 0;
  $effect(() => {
    const md = s.markdown;
    const opts = $state.snapshot(s.opts) as CardOptions;
    if (timer) clearTimeout(timer);
    timer = setTimeout(async () => {
      const g = ++gen;
      try {
        const l = await api.cardsLayout(md, opts);
        if (g === gen) layout = l;
      } catch (e) {
        app.error('Preview failed', e);
      }
    }, 250);
  });

  // ── autosave ──
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const snap = $state.snapshot(s);
    if (!loaded) return;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void api.workspaceSet(EDITOR_KEY, snap).catch(() => {}), 500);
  });

  onMount(async () => {
    registerCardFonts();
    try {
      const [t, saved, list] = await Promise.all([
        api.cardsTemplates(),
        api.workspaceGet<EditorState>(EDITOR_KEY).catch(() => null),
        api.decksList().catch(() => []),
      ]);
      templates = t;
      decks = list;
      if (saved) s = saved;
    } finally {
      loaded = true;
    }
  });

  function insertTemplate(name: string) {
    const t = templates?.templates[name];
    if (!t) return;
    // Like Python: append, separated by --- when the deck already has content.
    const cur = s.markdown.replace(/\s+$/, '');
    s.markdown = cur ? `${cur}\n\n---\n\n${t}` : t;
    queueMicrotask(() => {
      editor?.focus();
      editor?.setSelectionRange(s.markdown.length, s.markdown.length);
    });
  }

  async function loadFile() {
    const p = await pickFile('Load deck', ['md', 'txt'], 'Markdown');
    if (!p) return void (!isTauri() && app.toast('info', 'File dialogs are only available in the desktop app.'));
    try {
      s.markdown = await api.deckReadFile(p);
      s.deck_id = null;
      s.deck_name = p.split(/[\\/]/).pop()!.replace(/\.[^.]+$/, '');
    } catch (e) {
      app.error('Could not load deck', e);
    }
  }

  async function saveFile() {
    const p = isTauri() ? await pickSavePath('Save deck', `${s.deck_name || 'deck'}.md`, ['md', 'txt']) : null;
    if (!p) return;
    try {
      app.toast('success', `Saved ${await api.deckWriteFile(s.markdown, p)}`);
    } catch (e) {
      app.error('Could not save deck', e);
    }
  }

  async function saveDeck(asNew: boolean) {
    if (!s.deck_name.trim()) return app.toast('error', 'Please name the deck.');
    try {
      const d = await api.decksSave(asNew ? null : s.deck_id, s.deck_name, s.markdown, $state.snapshot(s.opts) as CardOptions);
      s.deck_id = d.id;
      decks = await api.decksList();
      saveOpen = false;
      app.toast('success', `Deck “${d.name}” saved`);
    } catch (e) {
      app.error('Could not save deck', e);
    }
  }

  async function openDeck(id: number) {
    try {
      const d = await api.decksGet(id);
      const opts = JSON.parse(d.settings_json || 'null') as CardOptions | null;
      s = { markdown: d.markdown, opts: opts ?? s.opts, deck_id: d.id, deck_name: d.name };
    } catch (e) {
      app.error('Could not open deck', e);
    }
  }

  async function exportPdf() {
    if (!layout?.cards) return;
    const path = isTauri() ? await pickSavePath('Export cards', `${s.deck_name || 'cards'}.pdf`, ['pdf']) : 'cards.pdf';
    if (!path) return;
    try {
      await api.cardsExportPdf(s.markdown, $state.snapshot(s.opts) as CardOptions, path);
      app.toast('info', 'Exporting… progress is shown bottom-right.');
    } catch (e) {
      app.error('Export failed', e);
    }
  }

  async function copyReference() {
    try {
      await navigator.clipboard.writeText(templates?.reference ?? '');
      app.toast('success', 'Format reference copied');
    } catch (e) {
      app.error('Copy failed', e);
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'e') {
      e.preventDefault();
      void exportPdf();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      saveOpen = true;
    }
  }

  const pageLabel = (i: number) => {
    const p = layout!.pages[i];
    const front = layout!.pages.slice(0, i + 1).filter((x) => !x.back).length;
    return p.back ? `Back of page ${front}` : `Page ${front}`;
  };
</script>

<svelte:window onkeydown={onKey} />

<ViewActions>
  <button class="btn" onclick={loadFile} title="Load a deck from a Markdown file">Open .md…</button>
  <button class="btn" onclick={saveFile} title="Write the deck to a Markdown file">Save as .md…</button>
  <button class="btn" onclick={() => (saveOpen = true)} title="Store the deck in the app (Ctrl+S)">Save deck…</button>
  <button class="btn primary" disabled={!layout?.cards} onclick={exportPdf} title="Ctrl+E">Export PDF</button>
</ViewActions>

<div class="view split">
  <section class="work edit" aria-label="Deck editor">
    <div class="view-toolbar">
      <span class="label">Insert</span>
      {#each TEMPLATE_ORDER.filter((t) => templates?.templates[t]) as t (t)}
        <button class="btn" onclick={() => insertTemplate(t)}>{t}</button>
      {/each}
      <div class="spacer"></div>
      <select
        class="input decks"
        aria-label="Saved decks"
        value=""
        onchange={(e) => {
          const v = Number(e.currentTarget.value);
          e.currentTarget.value = '';
          if (v) void openDeck(v);
        }}
      >
        <option value="">Saved decks…</option>
        {#each decks as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
      </select>
      <button class="btn icon" aria-label="Format reference" title="Format reference" onclick={() => (refOpen = true)}>?</button>
    </div>
    <textarea
      bind:this={editor}
      bind:value={s.markdown}
      class="md"
      spellcheck="false"
      aria-label="Deck markdown"
      placeholder={'# Fireball\n**Type:** Spell\n**Level:** 3rd-level\n\nA bright streak flashes…\n\n---\n\n# Next card…'}
    ></textarea>
    <div class="statusbar">
      <span>{s.deck_id !== null ? `Deck “${s.deck_name}”` : 'Unsaved deck'} · cards are separated by a line with ---</span>
    </div>
  </section>

  <section class="work preview" aria-label="Card preview">
    <div class="view-toolbar">
      <div class="segmented" role="group" aria-label="Page format">
        {#each ['A4', 'Letter'] as f (f)}
          <button aria-pressed={s.opts.format === f} onclick={() => (s.opts.format = f as PageFormat)}>{f}</button>
        {/each}
      </div>
      <label class="check"><input type="checkbox" bind:checked={s.opts.double_sided} /> Double-sided</label>
      <InfoTip label="About double-sided printing" text="Text that doesn’t fit on the front continues on the back. Pick the edge your printer flips on: long edge for most duplex printers." />
      {#if s.opts.double_sided}
        <div class="segmented" role="group" aria-label="Flip edge">
          {#each [['long', 'Long edge'], ['short', 'Short edge']] as [f, label] (f)}
            <button aria-pressed={s.opts.flip === f} onclick={() => (s.opts.flip = f as Flip)}>{label}</button>
          {/each}
        </div>
      {/if}
    </div>
    <div class="stage-area pages">
      {#if layout && layout.pages.length}
        {#each layout.pages as p, i (i)}
          <figure>
            <button class="page-btn" onclick={() => (zoomPage = i)} aria-label="Enlarge {pageLabel(i)}">
              <CardPage w={layout.page_w} h={layout.page_h} ops={p.ops} label={pageLabel(i)} />
            </button>
            <figcaption class="muted">{pageLabel(i)}</figcaption>
          </figure>
        {/each}
      {:else}
        <div class="empty-state wide">
          <strong>No cards yet</strong>
          <span>Write a card or insert a template; the preview updates as you type.</span>
        </div>
      {/if}
    </div>
    <div class="statusbar" aria-live="polite">
      <span>{layout?.cards ?? 0} card{layout?.cards === 1 ? '' : 's'} · {frontCount} page{frontCount === 1 ? '' : 's'}{layout?.backs ? ' + backs' : ''}</span>
      {#if layout?.overflow.length}
        <span class="warn" data-testid="overflow">
          {s.opts.double_sided ? '↪ continues on back: ' : '⚠ text overflow: '}{[...new Set(layout.overflow)].join(', ')}
        </span>
      {/if}
    </div>
  </section>
</div>

<Modal title="Card format reference" bind:open={refOpen} width={640}>
  <pre class="ref">{templates?.reference}</pre>
  {#snippet footer()}
    <button class="btn" onclick={copyReference}>Copy to clipboard</button>
    <button class="btn primary" onclick={() => (refOpen = false)}>Close</button>
  {/snippet}
</Modal>

<Modal title="Save deck" bind:open={saveOpen} width={420}>
  <div class="field">
    <label for="deck-name">Deck name</label>
    <input id="deck-name" class="input" bind:value={s.deck_name} onkeydown={(e) => e.key === 'Enter' && saveDeck(false)} />
  </div>
  {#snippet footer()}
    {#if s.deck_id !== null}<button class="btn" onclick={() => saveDeck(true)}>Save as new</button>{/if}
    <button class="btn primary" onclick={() => saveDeck(false)}>{s.deck_id !== null ? 'Save' : 'Save deck'}</button>
  {/snippet}
</Modal>

{#if zoomPage !== null && layout}
  <Modal title={pageLabel(zoomPage)} open={true} width={900}>
    <CardPage w={layout.page_w} h={layout.page_h} ops={layout.pages[zoomPage].ops} label={pageLabel(zoomPage)} />
    {#snippet footer()}
      <button class="btn" disabled={zoomPage === 0} onclick={() => (zoomPage = zoomPage! - 1)}>◀ Prev</button>
      <button class="btn" disabled={(zoomPage ?? 0) >= layout!.pages.length - 1} onclick={() => (zoomPage = zoomPage! + 1)}>Next ▶</button>
      <button class="btn primary" onclick={() => (zoomPage = null)}>Close</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .edit {
    border-right: 1px solid var(--border);
  }
  .decks {
    width: auto;
    max-width: 180px;
  }
  .md {
    flex: 1;
    min-height: 0;
    resize: none;
    font-family: var(--font-mono);
    font-size: 13px;
    line-height: 1.5;
    padding: var(--space-3) var(--space-4);
    border: 0;
    background: var(--surface);
    tab-size: 2;
  }
  .md:focus-visible {
    outline-offset: -2px;
  }
  .warn {
    color: var(--warning);
  }
  .pages {
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: var(--space-4);
    align-content: start;
    padding: var(--space-4);
  }
  figure {
    margin: 0;
    display: grid;
    gap: 4px;
  }
  figcaption {
    font-size: 12px;
    text-align: center;
  }
  .page-btn {
    border: 0;
    padding: 0;
    background: none;
    cursor: zoom-in;
  }
  .empty-state.wide {
    grid-column: 1 / -1;
  }
  .ref {
    white-space: pre-wrap;
    font-family: var(--font-mono);
    font-size: 12px;
    margin: 0;
  }
</style>
