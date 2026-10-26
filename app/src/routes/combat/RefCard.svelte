<script lang="ts" module>
  export type RefRequest = { kind: 'spell' | 'item'; name: string; source: string };
</script>

<script lang="ts">
  // A spell or item named in a stat block ({@spell …}, {@item …}), shown as its
  // card text from the synced 5etools spells and items.
  import { api } from '../../lib/api';
  import Modal from '../../lib/components/Modal.svelte';
  import type { CompendiumRef } from '../../lib/types';

  let { req = $bindable(null) }: { req: RefRequest | null } = $props();

  // 5etools defaults when a tag names no source
  const DEFAULT_SOURCE = { spell: 'PHB', item: 'DMG' };
  /** The 5etools hash as the compendium keys it: `encodeURIComponent(name_source)`, lower-cased. */
  const hashOf = (name: string, source: string) => encodeURIComponent(`${name}_${source}`).toLowerCase();

  let open = $state(false);
  let md = $state('');
  let failed = $state('');
  let gen = 0;

  $effect(() => {
    const r = req;
    if (!r) return;
    open = true;
    md = '';
    failed = '';
    const g = ++gen;
    void find(r).then(
      (text) => {
        if (g !== gen) return;
        if (text) md = text;
        else failed = `“${r.name}” is not in the synced ${r.kind === 'spell' ? 'spells' : 'items'}. Download them in Handouts → Spells and items.`;
      },
      () => g === gen && (failed = 'Could not read the spell and item cache.'),
    );
  });
  $effect(() => {
    if (!open) req = null;
  });

  /** The tag's own source first, then the newest printing with the same name. */
  async function find(r: RefRequest): Promise<string> {
    const ref: CompendiumRef = { kind: r.kind, id: hashOf(r.name, r.source || DEFAULT_SOURCE[r.kind]) };
    let out = await api.compendiumCards([ref]);
    if (out.cards) return out.markdown;
    const hits = await api.compendiumSearch({ kind: r.kind, query: r.name, level: null, class: null, rarity: null, newest_only: true, limit: 20 });
    const hit = hits.find((h) => h.name.toLowerCase() === r.name.toLowerCase());
    if (!hit) return '';
    out = await api.compendiumCards([{ kind: hit.kind, id: hit.id }]);
    return out.cards ? out.markdown : '';
  }

  // card markdown: "# Name", "**Key:** value" lines, then paragraphs
  type Line = { head: string } | { key: string; value: string } | { para: string };
  const lines = $derived.by<Line[]>(() =>
    md
      .split('\n')
      .map((l) => l.trim())
      .filter(Boolean)
      .flatMap((l): Line[] => {
        if (l.startsWith('# ')) return [{ head: l.slice(2) }];
        const kv = l.match(/^\*\*([^*]+):\*\*\s*(.*)$/);
        if (kv) return kv[1] === 'Type' ? [] : [{ key: kv[1], value: kv[2] }];
        return [{ para: l }];
      }),
  );
  const title = $derived((lines.find((l) => 'head' in l) as { head: string } | undefined)?.head ?? req?.name ?? '');

  /** `**bold**` and `*italic*` runs of a paragraph. */
  function runs(s: string): { t: string; b?: boolean; i?: boolean }[] {
    return s.split(/(\*\*[^*]+\*\*|\*[^*]+\*)/).filter(Boolean).map((p) =>
      p.startsWith('**') ? { t: p.slice(2, -2), b: true } : p.startsWith('*') && p.length > 2 ? { t: p.slice(1, -1), i: true } : { t: p },
    );
  }
</script>

<Modal title={title || 'Spell'} bind:open width={640}>
  <div class="ref" data-testid="ref-card">
    {#if failed}
      <p class="muted">{failed}</p>
    {:else if !md}
      <p class="muted">Loading…</p>
    {:else}
      <dl class="props">
        {#each lines as l, i (i)}{#if 'key' in l}<dt>{l.key}</dt><dd>{l.value}</dd>{/if}{/each}
      </dl>
      {#each lines as l, i (i)}
        {#if 'para' in l}
          <p>{#each runs(l.para) as r, j (j)}{#if r.b}<b>{r.t}</b>{:else if r.i}<i>{r.t}</i>{:else}{r.t}{/if}{/each}</p>
        {/if}
      {/each}
    {/if}
  </div>
</Modal>

<style>
  .ref {
    display: grid;
    gap: var(--space-3);
    font-size: 16px;
    line-height: 1.55;
    max-height: 65vh;
    overflow: auto;
  }
  .ref p {
    margin: 0;
  }
  .props {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 14px;
    margin: 0;
    padding-bottom: var(--space-2);
    border-bottom: 1px solid var(--border);
  }
  .props dt {
    font-weight: 600;
  }
  .props dd {
    margin: 0;
  }
</style>
