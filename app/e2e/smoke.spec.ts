import { expect, test } from '@playwright/test';

const VIEWS = [
  'library', 'tokens.cutter', 'tokens.sheet', 'cutouts.cutter', 'cutouts.sheet',
  'cards', 'maps.paginator', 'maps.aoe', 'music', 'settings',
];

test('every route renders without console errors', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
  for (const v of VIEWS) {
    await page.goto(`/#/${v}`);
    await expect(page.locator('main')).not.toBeEmpty();
  }
  expect(errors).toEqual([]);
});

test('rail navigation and sub-view tabs', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Maps' }).click();
  await expect(page.getByRole('tab', { name: 'Paginator' })).toHaveAttribute('aria-selected', 'true');
  await page.getByRole('tab', { name: 'AoE templates' }).click();
  await expect(page).toHaveURL(/#\/maps\.aoe$/);
  await page.getByRole('button', { name: 'Music' }).click();
  await expect(page).toHaveURL(/#\/music$/);
});

test('command palette navigates', async ({ page }) => {
  await page.goto('/');
  await page.keyboard.press('Control+k');
  await page.getByRole('combobox', { name: 'Command' }).fill('cards');
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/#\/cards$/);
});

test('settings validate and persist within the session', async ({ page }) => {
  await page.goto('/#/settings');
  const square = page.getByLabel('1 square on paper');
  await square.fill('-1');
  await square.blur();
  await expect(page.getByText('must be > 0')).toBeVisible();
  await page.getByRole('group', { name: 'Theme' }).getByRole('button', { name: 'Dark' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});
