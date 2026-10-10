// Library view state: query/filter model, results, selection, reference data.
// Spec: docs/features/library.md.

import { SvelteSet } from 'svelte/reactivity';
import { api } from './api';
import { app } from './state.svelte';
import type { Asset, AssetQuery, AssetSort, TagCount } from './types';

/** Upper bound fetched in one go; the grid is virtualized so this stays fast. */
const FETCH_LIMIT = 5000;

export const SORTS: { id: AssetSort; label: string }[] = [
  { id: 'newest', label: 'Newest' },
  { id: 'oldest', label: 'Oldest' },
  { id: 'name_asc', label: 'Name A–Z' },
  { id: 'name_desc', label: 'Name Z–A' },
  { id: 'kind', label: 'Kind' },
  { id: 'size', label: 'Size' },
];

/** Editor route for each asset kind ("Open in editor"). */
export const EDITOR_ROUTE: Record<string, string> = {
  token: 'tokens.cutter',
  cutout: 'cutouts.cutter',
  map: 'maps.paginator',
};

export class LibraryState {
  q = $state('');
  kinds = $state<string[]>([]);
  tags = $state<string[]>([]);
  pinnedOnly = $state(false);
  sort = $state<AssetSort>('newest');

  items = $state<Asset[]>([]);
  total = $state(0);
  loading = $state(false);
  loaded = $state(false);

  selected = new SvelteSet<number>();
  /** Asset shown in the inspector. */
  activeId = $state<number | null>(null);
  #anchor: number | null = null;

  allTags = $state<TagCount[]>([]);

  #gen = 0;
  #timer: ReturnType<typeof setTimeout> | null = null;

  readonly active = $derived(this.items.find((a) => a.id === this.activeId) ?? null);
  readonly filtersActive = $derived(
    !!this.q.trim() || this.kinds.length > 0 || this.tags.length > 0 || this.pinnedOnly,
  );

  query(): AssetQuery {
    return {
      q: this.q.trim() || null,
      kinds: [...this.kinds],
      tags: [...this.tags],
      pinned_only: this.pinnedOnly,
      sort: this.sort,
      limit: FETCH_LIMIT,
    };
  }

  /** Fetches results; responses from superseded requests are dropped. */
  async refresh(): Promise<void> {
    const gen = ++this.#gen;
    this.loading = true;
    try {
      const page = await api.librarySearch(this.query());
      if (gen !== this.#gen) return;
      this.items = page.items;
      this.total = page.total;
      for (const id of [...this.selected]) if (!page.items.some((a) => a.id === id)) this.selected.delete(id);
      if (this.activeId !== null && !page.items.some((a) => a.id === this.activeId)) this.activeId = null;
    } catch (e) {
      if (gen === this.#gen) app.error('Search failed', e);
    } finally {
      if (gen === this.#gen) {
        this.loading = false;
        this.loaded = true;
      }
    }
  }

  /** Debounced refresh for typing. */
  schedule(delay = 150): void {
    if (this.#timer) clearTimeout(this.#timer);
    this.#timer = setTimeout(() => void this.refresh(), delay);
  }

  async loadReference(): Promise<void> {
    this.allTags = await api.tagsList();
  }

  toggleKind(kind: string | null): void {
    this.kinds = kind === null ? [] : this.kinds.includes(kind) ? this.kinds.filter((k) => k !== kind) : [kind];
    void this.refresh();
  }

  toggleTag(tag: string): void {
    const lower = tag.toLowerCase();
    this.tags = this.tags.some((t) => t.toLowerCase() === lower)
      ? this.tags.filter((t) => t.toLowerCase() !== lower)
      : [...this.tags, tag];
    void this.refresh();
  }

  clearFilters(): void {
    this.q = '';
    this.kinds = [];
    this.tags = [];
    this.pinnedOnly = false;
    void this.refresh();
  }

  /** Click semantics: plain = open in inspector; ctrl/meta = toggle; shift = range. */
  click(id: number, e: { ctrlKey?: boolean; metaKey?: boolean; shiftKey?: boolean }): void {
    if (e.shiftKey && this.#anchor !== null) {
      const ids = this.items.map((a) => a.id);
      const [a, b] = [ids.indexOf(this.#anchor), ids.indexOf(id)].sort((x, y) => x - y);
      if (a >= 0 && b >= 0) for (const x of ids.slice(a, b + 1)) this.selected.add(x);
      return;
    }
    if (e.ctrlKey || e.metaKey || this.selected.size > 0) {
      if (this.selected.has(id)) this.selected.delete(id);
      else this.selected.add(id);
      this.#anchor = id;
      return;
    }
    this.activeId = this.activeId === id ? null : id;
    this.#anchor = id;
  }

  selectAll(): void {
    for (const a of this.items) this.selected.add(a.id);
  }

  clearSelection(): void {
    this.selected.clear();
  }

  /** Replaces one asset in the result list after an edit. */
  patchItem(asset: Asset): void {
    const i = this.items.findIndex((a) => a.id === asset.id);
    if (i >= 0) this.items[i] = asset;
  }
}
