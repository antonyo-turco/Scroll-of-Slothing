import { expect, test, type Page } from '@playwright/test';

// Object Cutter + the shared token sheet against the in-memory mock backend.

async function openInCutter(page: Page, name: string) {
  await page.goto('/#/library');
  await page.getByRole('button', { name: `${name} (Cut-out)` }).click();
  await page.getByRole('button', { name: 'Open in Object Cutter' }).click();
  await expect(page).toHaveURL(/#\/cutouts\.cutter$/);
  await expect(page.locator('[data-testid="cutter-stage"] img')).toBeVisible();
}

async function clickAt(page: Page, fx: number, fy: number) {
  const box = (await page.getByTestId('cutter-stage').boundingBox())!;
  await page.mouse.click(box.x + box.width * fx, box.y + box.height * fy);
}

test('polygon trace: add points, undo, close with Enter, result renders, save', async ({ page }) => {
  await openInCutter(page, 'Brazier');
  await expect(page.getByRole('button', { name: 'Save to library…' })).toBeDisabled();

  await clickAt(page, 0.3, 0.3);
  await clickAt(page, 0.7, 0.3);
  await clickAt(page, 0.6, 0.7);
  await clickAt(page, 0.4, 0.8);
  await expect(page.getByText('4 points')).toBeVisible();
  await page.getByRole('button', { name: 'Undo point' }).click();
  await expect(page.getByText('3 points')).toBeVisible();

  await page.getByRole('button', { name: 'Close shape' }).click();
  await expect(page.getByTestId('cutout-result')).toBeVisible();
  await expect(page.getByText('Shape closed')).toBeVisible();

  // Outline changes re-render.
  const result = page.getByTestId('cutout-result');
  const before = await result.getAttribute('src');
  await page.getByLabel('Outline thickness').fill('20');
  await expect(result).not.toHaveAttribute('src', before!);

  // Opened from the library → update-or-new choice.
  await page.getByRole('button', { name: 'Save to library…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Save cut-out to library' });
  await expect(dialog.getByLabel('Name')).toHaveValue('Brazier');
  await dialog.getByLabel('Name').fill('');
  await expect(dialog.getByRole('button', { name: 'Update existing' })).toBeDisabled();
  await dialog.getByLabel('Name').fill('Brazier');
  await dialog.getByRole('button', { name: 'Update existing' }).click();
  await expect(page.getByText('“Brazier” updated in the library')).toBeVisible();

  // Re-opening restores the closed trace (Python lost it).
  await openInCutter(page, 'Brazier');
  await expect(page.getByTestId('cutout-result')).toBeVisible();
  await expect(page.getByLabel('Outline thickness')).toHaveValue('20');
});

test('freehand drag closes on release; reset clears; zoom buttons', async ({ page }) => {
  await openInCutter(page, 'Brazier');
  await page.getByRole('group', { name: 'Selection mode' }).getByRole('button', { name: 'Freehand' }).click();
  const box = (await page.getByTestId('cutter-stage').boundingBox())!;
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  await page.mouse.move(cx - 60, cy - 40);
  await page.mouse.down();
  for (const [dx, dy] of [[60, -50], [70, 30], [0, 60], [-70, 30]]) await page.mouse.move(cx + dx, cy + dy, { steps: 5 });
  await page.mouse.up();
  await expect(page.getByTestId('cutout-result')).toBeVisible();

  await page.getByRole('button', { name: 'Reset' }).click();
  await expect(page.getByTestId('cutout-result')).toHaveCount(0);

  const zoom = page.locator('.zoom-readout');
  const z0 = Number((await zoom.textContent())!.replace('%', ''));
  await page.getByRole('button', { name: 'Zoom in' }).click();
  await expect
    .poll(async () => Math.abs(Number((await zoom.textContent())!.replace('%', '')) - z0 * 1.25))
    .toBeLessThanOrEqual(1);
});

test('cutter → object sheet: scale, copies, standee mode, clear', async ({ page }) => {
  await openInCutter(page, 'Brazier');
  await clickAt(page, 0.3, 0.3);
  await clickAt(page, 0.7, 0.3);
  await clickAt(page, 0.5, 0.7);
  await clickAt(page, 0.5, 0.7); // second click of a double-click is dropped on close
  await page.getByTestId('cutter-stage').dblclick({ position: { x: 5, y: 5 } }).catch(() => {});
  await page.keyboard.press('Enter');
  await page.getByRole('button', { name: 'Add to sheet', exact: true }).click();
  await expect(page.getByText('added to the sheet')).toBeVisible();

  await page.getByRole('tab', { name: 'Sheet' }).click();
  await expect(page.getByTestId('object-item')).toHaveCount(1);
  await page.getByLabel('Copies of Brazier').fill('6');
  await page.getByLabel('Copies of Brazier').press('Enter');
  const svg = page.getByTestId('sheet-page');
  await expect(svg.locator('image')).toHaveCount(6);

  await page.getByRole('group', { name: 'Cut-out mode' }).getByRole('button', { name: 'Standee' }).click();
  await expect(svg.locator('image')).toHaveCount(12, { timeout: 5000 });
  await expect(svg.locator('line.fold')).toHaveCount(6);

  await page.getByLabel('Scale of Brazier').fill('5');
  await page.getByLabel('Scale of Brazier').press('Enter');
  await expect(page.getByText('Scale must be between 10 and 400 %.')).toBeVisible();

  await page.getByRole('button', { name: 'Clear all…' }).click();
  await page.getByRole('button', { name: 'Clear all', exact: true }).click();
  await expect(page.getByText('No cut-outs yet.')).toBeVisible();
});

test('library bulk "Add to sheets" routes by kind and skips maps', async ({ page }) => {
  await page.goto('/#/library');
  await page.getByRole('button', { name: 'Brazier (Cut-out)' }).click({ modifiers: ['Control'] });
  await page.getByRole('button', { name: 'Zombie (Token)' }).click();
  await page.getByRole('button', { name: 'Forest Road (Map)' }).click();
  await page.getByRole('toolbar', { name: 'Bulk actions' }).getByRole('button', { name: 'Add to sheets' }).click();
  await expect(page.getByText('Added 1 token(s) and 1 cut-out(s) to the sheet · skipped 1 map(s)')).toBeVisible();
});
