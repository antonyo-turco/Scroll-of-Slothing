import { expect, test } from '@playwright/test';

// Card editor against the in-memory mock (simplified layout; the exact layout
// engine is verified by Rust golden tests).

test.beforeEach(async ({ page }) => {
  await page.goto('/#/cards');
  await expect(page.getByLabel('Deck markdown')).toBeVisible();
});

test('templates append with a separator and the preview follows', async ({ page }) => {
  await page.getByRole('button', { name: 'Spell', exact: true }).click();
  await expect(page.getByLabel('Deck markdown')).toHaveValue(/^# Spell Name\n\*\*Type:\*\* Spell/);
  await expect(page.getByText(/1 card · 1 page/)).toBeVisible();
  await page.getByRole('button', { name: 'Feat', exact: true }).click();
  await expect(page.getByLabel('Deck markdown')).toHaveValue(/\n---\n\n# Feat Name/);
  await expect(page.getByText(/2 cards · 1 page/)).toBeVisible();
  await expect(page.getByRole('img', { name: 'Page 1' })).toBeVisible();
});

test('copies expand across pages; overflow warnings switch with duplex', async ({ page }) => {
  const long = 'lorem ipsum '.repeat(80);
  await page.getByLabel('Deck markdown').fill(`# Swarm\n**Copies:** 10\n\nBzz.\n---\n# Essay\n**Type:** Item\n\n${long}`);
  await expect(page.getByText(/11 cards · 2 pages/)).toBeVisible();
  await expect(page.getByTestId('overflow')).toHaveText('⚠ text overflow: Essay');

  await page.getByLabel('Double-sided').check();
  await expect(page.getByTestId('overflow')).toHaveText('↪ continues on back: Essay');
  await expect(page.getByText(/2 pages \+ backs/)).toBeVisible();
  await expect(page.getByRole('img', { name: 'Back of page 1' })).toBeVisible();
  await page.getByRole('group', { name: 'Flip edge' }).getByRole('button', { name: 'Short edge' }).click();
  await expect(page.getByRole('group', { name: 'Flip edge' }).getByRole('button', { name: 'Short edge' })).toHaveAttribute('aria-pressed', 'true');
});

test('reference dialog, enlarge page, save and reopen a deck', async ({ page }) => {
  await page.getByRole('button', { name: 'Format reference' }).click();
  await expect(page.getByRole('dialog', { name: 'Card format reference' })).toContainText('BUILT-IN TYPES');
  await page.getByRole('button', { name: 'Close' }).last().click();

  await page.getByLabel('Deck markdown').fill('# Bless\n**Type:** Spell\n\nUp to three creatures.');
  await page.getByRole('button', { name: 'Enlarge Page 1' }).click();
  await expect(page.getByRole('dialog', { name: 'Page 1' })).toBeVisible();
  await page.getByRole('dialog', { name: 'Page 1' }).getByRole('button', { name: 'Close' }).last().click();

  await page.getByRole('button', { name: 'Save deck…' }).click();
  await page.getByLabel('Deck name').fill('Clerics');
  await page.getByRole('button', { name: 'Save deck', exact: true }).click();
  await expect(page.getByText('Deck “Clerics” saved')).toBeVisible();

  await page.getByLabel('Deck markdown').fill('');
  await expect(page.getByText(/0 cards/)).toBeVisible();
  await page.getByLabel('Saved decks').selectOption({ label: 'Clerics' });
  await expect(page.getByLabel('Deck markdown')).toHaveValue(/# Bless/);
});

test('export starts a job', async ({ page }) => {
  await page.getByLabel('Deck markdown').fill('# A\n\nb');
  await page.getByRole('button', { name: 'Export PDF' }).click();
  await expect(page.getByText('card export finished')).toBeVisible();
});
