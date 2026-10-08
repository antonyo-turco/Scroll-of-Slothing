import { beforeEach, describe, expect, it } from 'vitest';
import { setBackend } from './api';
import { createMockBackend } from './mock';
import { ALL_VIEWS, DESTINATIONS, findView, isRoute } from './nav';
import { app } from './state.svelte';
import { DEFAULT_SETTINGS } from './types';

describe('navigation model', () => {
  it('exposes the 7 rail destinations, all enabled', () => {
    expect(DESTINATIONS.map((d) => d.id)).toEqual([
      'library', 'tokens', 'cutouts', 'cards', 'maps', 'campaigns', 'music',
    ]);
    expect(DESTINATIONS.filter((d) => d.disabled)).toEqual([]);
  });

  it('has route ids identical to the Rust ROUTES list', () => {
    // Keep in sync with crates/core/src/settings.rs::ROUTES (persisted ids).
    expect(ALL_VIEWS.map((v) => v.id).sort()).toEqual(
      [
        'library', 'tokens.cutter', 'tokens.sheet', 'cutouts.cutter', 'cutouts.sheet',
        'cards', 'maps.paginator', 'maps.aoe', 'campaigns', 'music', 'settings',
      ].sort(),
    );
  });

  it('resolves views to their destination', () => {
    expect(findView('maps.aoe')?.dest.id).toBe('maps');
    expect(findView('music')?.dest.id).toBe('music');
    expect(isRoute('dnd.combat')).toBe(false);
  });
});

describe('app state', () => {
  beforeEach(() => {
    setBackend(createMockBackend({ start_view: 'cards' }));
    location.hash = '';
  });

  it('starts on the configured start view', async () => {
    await app.init();
    expect(app.route).toBe('cards');
    expect(app.ready).toBe(true);
  });

  it('a valid hash overrides the start view', async () => {
    location.hash = '#/maps.aoe';
    await app.init();
    expect(app.route).toBe('maps.aoe');
  });

  it('ui_navigate events switch views and hand over the asset to open', async () => {
    await app.init();
    app.onEvent({ type: 'ui_navigate', route: 'maps.paginator', asset_id: 7 });
    expect(app.route).toBe('maps.paginator');
    expect(app.openRequest).toEqual({ route: 'maps.paginator', assetId: 7 });
    app.openRequest = null;
    app.onEvent({ type: 'ui_navigate', route: 'music', asset_id: null });
    expect(app.route).toBe('music');
    expect(app.openRequest).toBeNull();
  });

  it('navigate ignores unknown routes and updates the hash', async () => {
    await app.init();
    app.navigate('library');
    expect(app.route).toBe('library');
    expect(location.hash).toBe('#/library');
    app.navigate('nope');
    expect(app.route).toBe('library');
  });

  it('updateSettings applies valid keys and returns errors for invalid ones', async () => {
    await app.init();
    const errors = await app.updateSettings({ map_square_in: -1, token_page_format: 'A4' });
    expect(errors).toHaveProperty('map_square_in');
    expect(app.settings.token_page_format).toBe('A4');
    expect(app.settings.map_square_in).toBe(DEFAULT_SETTINGS.map_square_in);
  });

  it('theme setting drives the data-theme attribute', async () => {
    await app.init();
    await app.updateSettings({ theme: 'dark' });
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    await app.updateSettings({ theme: 'system' });
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
  });

  it('job events are tracked and failures raise an error toast', async () => {
    await app.init();
    app.toasts = [];
    app.onEvent({ type: 'job', id: 'j1', kind: 'export', state: 'running', progress: 0.2, message: '', result: null, error: null });
    expect(app.jobs.find((j) => j.id === 'j1')?.progress).toBe(0.2);
    app.onEvent({ type: 'job', id: 'j1', kind: 'export', state: 'failed', progress: 0.2, message: '', result: null, error: 'disk full' });
    expect(app.jobs.find((j) => j.id === 'j1')?.state).toBe('failed');
    expect(app.toasts.at(-1)).toMatchObject({ kind: 'error', detail: 'disk full' });
  });

  it('reset restores defaults', async () => {
    await app.init();
    await app.updateSettings({ map_overlap_in: 0.5 });
    await app.resetSettings();
    expect(app.settings).toEqual(DEFAULT_SETTINGS);
  });
});
