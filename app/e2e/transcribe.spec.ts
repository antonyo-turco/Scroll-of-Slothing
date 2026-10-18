import { expect, test } from '@playwright/test';

// Transcribe tab against the in-memory mock (canned segments; the engine is
// covered by the Rust tests).

test.beforeEach(async ({ page }) => {
  await page.goto('/#/transcribe.file');
  await expect(page.getByText('Turn a recording into text')).toBeVisible();
});

test('open, transcribe with corrections, see timed segments', async ({ page }) => {
  await page.getByRole('button', { name: 'Open audio…' }).click();
  await expect(page.getByText('sessione.wav')).toBeVisible();
  await page.getByLabel('Corrections').fill('beholdr = beholder');
  await page.getByRole('button', { name: 'Transcribe file' }).click();

  const segs = page.getByTestId('segments');
  await expect(segs).toContainText('Benvenuti al tavolo.');
  await expect(segs).toContainText('Un beholder vi osserva');
  await expect(segs.locator('time').nth(1)).toHaveText('0:02');
  await expect(page.getByText(/0:12 of audio · 4 segments · large-v3-turbo-q5_0 · it/)).toBeVisible();
  await expect(page.getByRole('button', { name: 'SRT' })).toBeVisible();
});

test('a model that is not downloaded must be downloaded first', async ({ page }) => {
  await page.getByRole('button', { name: 'Open audio…' }).click();
  await page.getByLabel('Model').selectOption('base');
  const go = page.getByRole('button', { name: 'Transcribe file' });
  await expect(go).toBeDisabled();
  await page.getByRole('button', { name: /Download \(141 MB\)/ }).click();
  await expect(go).toBeEnabled();
  await expect(page.getByRole('button', { name: /Download/ })).toHaveCount(0);
});

test('options are remembered across visits', async ({ page }) => {
  await page.getByLabel('Glossary').fill('Strahd\nBarovia');
  await page.getByRole('group', { name: 'Search' }).getByRole('button', { name: 'Fast' }).click();
  await page.waitForTimeout(600);
  // Hash navigation only: a reload would reset the in-memory mock.
  await page.evaluate(() => (location.hash = '#/cards'));
  await expect(page.getByLabel('Deck markdown')).toBeVisible();
  await page.evaluate(() => (location.hash = '#/transcribe.file'));
  await expect(page.getByLabel('Glossary')).toHaveValue('Strahd\nBarovia');
  await expect(page.getByRole('group', { name: 'Search' }).getByRole('button', { name: 'Fast' })).toHaveAttribute('aria-pressed', 'true');
});
