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
  await page.getByRole('group', { name: 'Tag filter' }).getByRole('button', { name: 'sfx' }).click();
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

test('library: favourite a track, then filter by favourites, length and loop', async ({ page }) => {
  // Tavern Ambience is a favourite in the mock seed.
  await page.getByRole('button', { name: 'Add Battle Drums to favourites' }).click();
  await expect(page.getByRole('button', { name: 'Remove Battle Drums from favourites' })).toHaveAttribute('aria-pressed', 'true');
  const filters = page.getByRole('group', { name: 'Filters' });
  await filters.getByRole('button', { name: '★ Favourites' }).click();
  await expect(page.getByRole('row')).toHaveCount(3); // header + Tavern + Battle Drums
  await filters.getByLabel('Length').selectOption('mid'); // 1–5 min: Battle Drums (4:05)
  await expect(page.getByRole('row')).toHaveCount(2);
  await expect(page.getByRole('row', { name: /Battle Drums/ })).toBeVisible();
  await filters.getByRole('button', { name: 'Clear filters' }).click();
  await filters.getByLabel('Loop').selectOption('once');
  await expect(page.getByRole('row')).toHaveCount(2);
  await expect(page.getByRole('row', { name: /Thunder Clap/ })).toBeVisible();
  await expect(page.getByText('1 track of 3')).toBeVisible();
});

test('library: columns sort by title and length', async ({ page }) => {
  const firstTitle = () => page.getByRole('row').nth(1);
  await page.getByRole('button', { name: /^Title/ }).click();
  await expect(firstTitle()).toContainText('Battle Drums');
  await page.getByRole('button', { name: /^Title/ }).click();
  await expect(firstTitle()).toContainText('Thunder Clap');
  await page.getByRole('button', { name: /^Length/ }).click();
  await expect(firstTitle()).toContainText('Tavern Ambience'); // longest first
  await expect(page.getByRole('columnheader', { name: /Length/ })).toHaveAttribute('aria-sort', 'descending');
});

test('soundscapes: pin keeps a mix at the top', async ({ page }) => {
  await page.getByRole('button', { name: 'Add Tavern Ambience to mix' }).click();
  await tab(page, 'Mixer').click();
  for (const name of ['Alpha', 'Zulu']) {
    await page.getByRole('button', { name: '💾 Save as New' }).click();
    await page.getByRole('dialog').getByLabel('Name').fill(name);
    await page.getByRole('dialog').getByRole('button', { name: 'Save' }).click();
  }
  await tab(page, 'Soundscapes').click();
  const names = page.locator('.sc strong');
  await expect(names).toHaveText(['Alpha', 'Zulu']);
  await page.getByRole('button', { name: 'Pin Zulu' }).click();
  await expect(names).toHaveText(['Zulu', 'Alpha']);
  await expect(page.getByRole('button', { name: 'Unpin Zulu' })).toHaveAttribute('aria-pressed', 'true');
});

test('library: category filter, and Add to mix follows the category', async ({ page }) => {
  const cats = page.getByRole('group', { name: 'Category' });
  await cats.getByRole('button', { name: 'Soundbites' }).click();
  await expect(page.getByRole('row')).toHaveCount(2);
  await expect(page.getByRole('row', { name: /Thunder Clap/ })).toContainText('Soundbite');
  await cats.getByRole('button', { name: 'All' }).click();
  await expect(page.getByRole('row')).toHaveCount(4);
  // YouTube tracks show their thumbnail.
  await expect(page.getByRole('row', { name: /Tavern Ambience/ }).locator('img')).toHaveAttribute('src', 'https://i.ytimg.com/vi/dQw4w9WgXcQ/mqdefault.jpg');
  // Odd URLs and uploads fall back to the category icon.
  await expect(page.getByRole('row', { name: /Battle Drums/ }).locator('img')).toHaveCount(0);

  // Ambient → ambient channel; soundbite → the soundbite list.
  await page.getByRole('button', { name: 'Add Tavern Ambience to mix' }).click();
  await page.getByRole('button', { name: 'Add Thunder Clap to mix' }).click();
  await tab(page, 'Mixer').click();
  await expect(page.getByText('(1 channels)')).toBeVisible();
  await expect(page.getByLabel('Role')).toHaveValue('ambient');
  await expect(page.getByText('Thunder Clap')).toBeVisible();
});

test('library: suggested tags are reviewed, then added', async ({ page }) => {
  await page.getByRole('button', { name: 'Suggest tags…' }).click();
  const dlg = page.getByRole('dialog', { name: 'Suggested tags' });
  const list = dlg.getByTestId('tag-suggestions');
  await expect(list).toContainText('Battle Drums');
  await expect(list).toContainText('+ combat');
  await expect(list).toContainText('+ weather');
  // Skip Thunder Clap.
  await list.getByRole('listitem').filter({ hasText: 'Thunder Clap' }).getByRole('checkbox').uncheck();
  await dlg.getByRole('button', { name: 'Add tags to 1 track' }).click();
  await expect(page.getByRole('row', { name: /Battle Drums/ })).toContainText('combat');
  await expect(page.getByRole('row', { name: /Thunder Clap/ })).not.toContainText('weather');
});

test('track dialog: Auto category for new tracks, soundbite turns looping off', async ({ page }) => {
  await page.getByRole('button', { name: '+ Add Track' }).click();
  const dlg = page.getByRole('dialog', { name: 'Add track' });
  await expect(dlg.getByRole('group', { name: 'Category' }).getByRole('button', { name: 'Auto' })).toHaveAttribute('aria-pressed', 'true');
  await dlg.getByRole('group', { name: 'Category' }).getByRole('button', { name: 'Soundbite' }).click();
  await expect(dlg.getByLabel('Loop by default')).not.toBeChecked();
});
