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

test('cloud: key in Settings, confirm once, cost in the status bar', async ({ page }) => {
  // Without a key the cloud engine can't start.
  await page.getByRole('group', { name: 'Engine' }).getByRole('button', { name: 'Cloud' }).click();
  await page.getByRole('button', { name: 'Open audio…' }).click();
  await expect(page.getByTestId('cloud-key')).toContainText('No OpenRouter key yet');
  await expect(page.getByRole('button', { name: 'Transcribe file' })).toBeDisabled();

  // Save and test a key in Settings (hash navigation keeps the in-memory mock).
  await page.getByRole('button', { name: 'Add one in Settings' }).click();
  await expect(page.getByTestId('key-status')).toContainText('No key yet');
  await page.getByLabel('OpenRouter API key').fill('sk-or-v1-0123456789abcd');
  await page.getByRole('button', { name: 'Save key' }).click();
  await expect(page.getByTestId('key-status')).toContainText('Saved: sk-or-…abcd');
  await expect(page.getByLabel('OpenRouter API key')).toHaveValue('');
  await page.getByRole('button', { name: 'Test key' }).click();
  await expect(page.getByTestId('key-check')).toContainText('Key works');

  await page.evaluate(() => (location.hash = '#/transcribe.file'));
  await expect(page.getByTestId('cloud-key')).toContainText('sk-or-…abcd');
  await page.getByRole('button', { name: 'Transcribe file' }).click();
  const dialog = page.getByRole('dialog', { name: 'Send this audio to OpenRouter?' });
  await expect(dialog).toContainText('openai/whisper-1');
  await dialog.getByRole('button', { name: 'Send and transcribe' }).click();
  await expect(page.getByTestId('segments')).toContainText('Benvenuti al tavolo.');
  await expect(page.getByText(/openai\/whisper-1 · it · cloud · \$0\.0013/)).toBeVisible();

  // Remembered: no second confirmation.
  await page.getByRole('button', { name: 'Transcribe file' }).click();
  await expect(dialog).toHaveCount(0);
});

test('settings: a rejected key is reported, bad URLs are refused', async ({ page }) => {
  await page.evaluate(() => (location.hash = '#/settings'));
  await page.getByLabel('OpenRouter API key').fill('sk-or-bad-key-0000');
  await page.getByRole('button', { name: 'Save key' }).click();
  await page.getByRole('button', { name: 'Test key' }).click();
  await expect(page.getByTestId('key-check')).toContainText('rejected (401)');
  await page.getByRole('button', { name: 'Remove key' }).click();
  await expect(page.getByTestId('key-status')).toContainText('No key yet');

  await page.getByLabel('API address').fill('ftp://nope');
  await page.getByLabel('API address').press('Enter');
  await page.getByLabel('API address').blur();
  await expect(page.getByText('must be an http(s) URL')).toBeVisible();
});
