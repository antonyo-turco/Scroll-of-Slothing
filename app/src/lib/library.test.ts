import { beforeEach, describe, expect, it } from 'vitest';
import { setBackend } from './api';
import { LibraryState } from './library.svelte';
import { createMockBackend } from './mock';

describe('LibraryState', () => {
  let lib: LibraryState;

  beforeEach(async () => {
    setBackend(createMockBackend());
    lib = new LibraryState();
    await lib.loadReference();
    await lib.refresh();
  });

  const names = () => lib.items.map((a) => a.name);

  it('loads everything newest-first with reference data', () => {
    expect(lib.total).toBe(7);
    expect(names()[0]).toBe('Forest Road');
    expect(lib.campaigns.map((c) => c.name)).toEqual(['Curse of Strahd']);
    expect(lib.allTags.find((t) => t.name === 'boss')?.count).toBe(2);
  });

  it('kind toggle is exclusive and "All" clears it', async () => {
    lib.toggleKind('map');
    await lib.refresh();
    expect(names()).toEqual(['Forest Road', 'Crypt of the Lich']);
    lib.toggleKind(null);
    await lib.refresh();
    expect(lib.total).toBe(7);
  });

  it('tags combine with AND and toggle case-insensitively', async () => {
    lib.toggleTag('boss');
    await lib.refresh();
    expect(lib.total).toBe(2);
    lib.toggleTag('Dragon');
    await lib.refresh();
    expect(names()).toEqual(['Ancient Red Dragon']);
    lib.toggleTag('BOSS');
    expect(lib.tags).toEqual(['Dragon']);
  });

  it('campaign filter includes session-linked assets; changing campaign drops a foreign session', async () => {
    lib.setCampaign('mock-c1');
    await lib.refresh();
    expect(names().sort()).toEqual(['Crypt of the Lich', 'Zombie']);
    lib.sessionId = 'mock-s1';
    lib.setCampaign(null);
    expect(lib.sessionId).toBe('mock-s1');
    lib.campaigns = [...lib.campaigns, { id: 'other', name: 'Other', created: '', session_count: 0, asset_count: 0 }];
    lib.setCampaign('other');
    expect(lib.sessionId).toBeNull();
  });

  it('free text uses word prefixes over names, aliases and tags', async () => {
    lib.q = 'anc dra';
    await lib.refresh();
    expect(names()).toEqual(['Ancient Red Dragon']);
    lib.q = 'undead';
    await lib.refresh();
    expect(lib.total).toBe(2);
  });

  it('click: plain opens inspector, ctrl toggles selection, shift selects a range', () => {
    const ids = lib.items.map((a) => a.id);
    lib.click(ids[1], {});
    expect(lib.activeId).toBe(ids[1]);
    lib.click(ids[1], {});
    expect(lib.activeId).toBeNull();

    lib.click(ids[0], { ctrlKey: true });
    lib.click(ids[3], { shiftKey: true });
    expect([...lib.selected].sort()).toEqual(ids.slice(0, 4).sort());
    // While selecting, a plain click toggles instead of opening the inspector.
    lib.click(ids[2], {});
    expect(lib.selected.has(ids[2])).toBe(false);
    expect(lib.activeId).toBeNull();
  });

  it('refresh prunes selection and inspector of assets that disappeared', async () => {
    const id = lib.items[0].id;
    lib.selected.add(id);
    lib.activeId = id;
    lib.toggleKind('cutout');
    await lib.refresh();
    expect(lib.selected.size).toBe(0);
    expect(lib.activeId).toBeNull();
  });

  it('stale responses are dropped', async () => {
    lib.q = 'zzz-no-match';
    const slow = lib.refresh();
    lib.q = '';
    const fast = lib.refresh();
    await Promise.all([slow, fast]);
    expect(lib.total).toBe(7);
  });
});

describe('mock backend parity', () => {
  it('set_tags creates new tags and dedupes case-insensitively (like the Rust store)', async () => {
    const b = createMockBackend();
    const page = await b.invoke<{ items: { id: number; name: string }[] }>('library_search', { query: {} });
    const owl = page.items.find((a) => a.name === 'Owlbear')!;
    const a = await b.invoke<{ tags: string[] }>('library_set_tags', { id: owl.id, values: ['forest', 'Beast', 'FOREST'] });
    expect(a.tags).toEqual(['beast', 'forest']);
  });
});
