import { expect, test, type Page } from '@playwright/test';

// Runs against the in-memory mock backend (seeded with 3 tracks). The mock has
// no audio engine, so channels the service plays show no status, and paused
// ones show the paused indicator. Engine statuses are unit-tested in vitest.

const tab = (page: Page, name: string) => page.getByRole('tablist', { name: 'Music views' }).getByRole('tab', { name });

test.beforeEach(async ({ page }) => {
  await page.goto('/#/music');
  await expect(page.getByRole('row')).toHaveCount(4); // header + 3 tracks
});

test('library filters by text and tag', async ({ page }) => {
  await page.getByLabel('Search tracks').fill('drum');
  await expect(page.getByRole('row')).toHaveCount(2);
  await page.getByLabel('Search tracks').fill('');
  await page.getByRole('checkbox', { name: 'sfx' }).check();
  await expect(page.getByRole('row', { name: /Thunder Clap/ })).toBeVisible();
  await expect(page.getByRole('row')).toHaveCount(2);
});

test('mixer: add, pause shows the paused indicator, stop all empties', async ({ page }) => {
  await page.getByRole('button', { name: 'Add Tavern Ambience to mix' }).click();
  await page.getByRole('button', { name: 'Add Battle Drums to mix' }).click();
  await tab(page, 'Mixer').click();
  await expect(page.getByText('Active Mix')).toBeVisible();
  await expect(page.getByText('(2 channels)')).toBeVisible();
  await page.getByRole('button', { name: 'Pause Battle Drums' }).click();
  await expect(page.getByRole('button', { name: 'Play Battle Drums' })).toBeVisible();
  await expect(page.getByRole('img', { name: 'Paused' })).toHaveCount(1);
  // volume applies while dragging (keyboard too)
  const vol = page.getByLabel('Volume Tavern Ambience');
  await vol.focus();
  await vol.press('ArrowLeft');
  await expect(vol).toHaveValue('79');
  await page.getByRole('button', { name: 'Stop All' }).click();
  await expect(page.getByText('The mix is empty.')).toBeVisible();
});

test('mixer: ducking mode and soundbites', async ({ page }) => {
  await tab(page, 'Mixer').click();
  const duck = page.getByRole('group', { name: 'Duck under SFX' });
  await duck.getByRole('button', { name: 'Soft' }).click();
  await expect(duck.getByRole('button', { name: 'Soft' })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: '+ Soundbite' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Add' }).click();
  await expect(page.getByRole('button', { name: 'Remove Tavern Ambience' })).toBeVisible();
  await expect(page.getByText('running')).toBeVisible();
});

test('playlists: create, play and control', async ({ page }) => {
  await tab(page, 'Playlists').click();
  await page.getByRole('button', { name: '+ New Playlist' }).click();
  const dlg = page.getByRole('dialog');
  await dlg.getByLabel('Name').fill('Night watch');
  await dlg.getByLabel('All Tracks').selectOption(['mock-t1', 'mock-t3']);
  await dlg.getByRole('button', { name: 'Add →' }).click();
  await dlg.getByRole('button', { name: 'Save' }).click();
  await page.getByRole('button', { name: '▶ Play' }).click();
  const now = page.getByLabel('Now playing');
  await expect(now.getByText('Night watch — Track 1/2')).toBeVisible();
  await now.getByRole('button', { name: 'Next' }).click();
  await expect(now.getByText('Track 2/2')).toBeVisible();
  await now.getByRole('button', { name: 'Pause' }).click();
  await expect(now.getByRole('img', { name: 'Paused' })).toBeVisible();
  await now.getByRole('button', { name: 'Stop playlist' }).click();
  await expect(now).toBeHidden();
});
