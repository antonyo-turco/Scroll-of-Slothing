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
  await page.getByLabel('Corrections', { exact: true }).fill('beholdr = beholder');
  await page.getByRole('button', { name: 'Transcribe file' }).click();

  const segs = page.getByTestId('segments');
  await expect(segs).toContainText('Benvenuti al tavolo.');
  await expect(segs).toContainText('Un beholder vi osserva');
  await expect(segs.locator('time').nth(1)).toHaveText('0:02');
  await expect(page.getByText(/0:12 of audio · 4 segments · large-v3-turbo-q5_0 · it/)).toBeVisible();
  await expect(page.getByRole('button', { name: 'SRT' })).toBeVisible();
});

test('time needed for both engines; cloud shows the price and hides the glossary', async ({ page }) => {
  await expect(page.getByText('Open a file to see how long each engine takes.')).toBeVisible();
  await page.getByRole('button', { name: 'Open audio…' }).click();
  const est = page.getByTestId('estimates');
  // Mock file: 12.6 s; large-v3-turbo-q5_0 at 6.5 s/s + 1 s ≈ 83 s; cloud 0.06 s/s + 3 s ≈ 4 s.
  await expect(est.getByRole('button', { name: /This computer/ })).toContainText('≈ 1 min');
  await expect(est.getByRole('button', { name: /This computer/ })).toContainText('measured here');
  await expect(est.getByRole('button', { name: /Cloud/ })).toContainText('≈ 4 s');
  await expect(est.getByRole('button', { name: /Cloud/ })).toContainText('$0.0013');
  await expect(page.getByLabel('Glossary', { exact: true })).toBeVisible();

  // Clicking the cloud row switches the engine.
  await est.getByRole('button', { name: /Cloud/ }).click();
  await expect(page.getByRole('group', { name: 'Engine' }).getByRole('button', { name: 'Cloud' })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByTestId('cloud-model')).toContainText('$0.006 / min');
  await expect(page.getByLabel('Glossary', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('group', { name: 'Search' })).toHaveCount(0);
  await page.getByLabel('Cloud model').selectOption('mistralai/voxtral-mini-transcribe');
  await expect(page.getByTestId('cloud-model')).toContainText('$0.003 / min');
  await expect(est.getByRole('button', { name: /Cloud/ })).toContainText('$0.0006');
});

test('info tips explain on hover and focus', async ({ page }) => {
  const tip = page.getByRole('button', { name: 'About corrections' });
  await tip.hover();
  await expect(page.getByRole('tooltip').filter({ hasText: 'wrong = right' })).toBeVisible();
  await page.mouse.move(0, 0);
  await expect(page.getByRole('tooltip').filter({ hasText: 'wrong = right' })).toBeHidden();
  await tip.focus();
  await expect(page.getByRole('tooltip').filter({ hasText: 'wrong = right' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('tooltip').filter({ hasText: 'wrong = right' })).toBeHidden();
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
  await page.getByLabel('Glossary', { exact: true }).fill('Strahd\nBarovia');
  await page.getByRole('group', { name: 'Search' }).getByRole('button', { name: 'Fast' }).click();
  await page.waitForTimeout(600);
  // Hash navigation only: a reload would reset the in-memory mock.
  await page.evaluate(() => (location.hash = '#/cards'));
  await expect(page.getByLabel('Deck markdown')).toBeVisible();
  await page.evaluate(() => (location.hash = '#/transcribe.file'));
  await expect(page.getByLabel('Glossary', { exact: true })).toHaveValue('Strahd\nBarovia');
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

  const card = page.locator('section.card').filter({ has: page.getByRole('heading', { name: 'Transcription' }) });
  await card.getByText('Advanced', { exact: true }).click();
  await page.getByLabel('API address', { exact: true }).fill('ftp://nope');
  await page.getByLabel('API address', { exact: true }).press('Enter');
  await page.getByLabel('API address', { exact: true }).blur();
  await expect(page.getByText('must be an http(s) URL')).toBeVisible();
});
