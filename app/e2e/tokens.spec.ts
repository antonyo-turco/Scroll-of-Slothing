import { expect, test, type Page } from '@playwright/test';

// Token Cutter + Token Sheet against the in-memory mock backend.

async function openInCutter(page: Page, name: string) {
  await page.goto('/#/library');
  await page.getByRole('button', { name: `${name} (Token)` }).click();
  await page.getByRole('button', { name: 'Open in Token Cutter' }).click();
  await expect(page).toHaveURL(/#\/tokens\.cutter$/);
  await expect(page.getByTestId('token-preview')).toBeVisible();
}

test('library asset opens in the cutter, edits re-render, update-or-new choice', async ({ page }) => {
  await openInCutter(page, 'Owlbear');
  await expect(page.getByLabel('Name')).toHaveValue('Owlbear');
  await expect(page.getByLabel('Size')).toHaveValue('Large');

  const preview = page.getByTestId('token-preview');
  const before = await preview.getAttribute('src');
  await page.getByLabel(/Ring thickness/).fill('30');
  await expect(preview).not.toHaveAttribute('src', before!);

  // Background colour + border switch.
  await page.getByRole('group', { name: 'Background' }).getByRole('button', { name: 'Colour' }).click();
  await expect(page.getByLabel('Background colour')).toBeVisible();
  await page.getByRole('radio', { name: 'Roll20 grey' }).click();
  await expect(page.getByRole('radio', { name: 'Roll20 grey' })).toHaveAttribute('aria-checked', 'true');

  // Opened from the library → saving asks update vs new.
  await page.getByRole('button', { name: 'Save to library' }).click();
  const choice = page.getByRole('group', { name: 'Save choice' });
  await expect(choice).toBeVisible();
  await choice.getByRole('button', { name: 'Update existing' }).click();
  await expect(page.getByText('“Owlbear” updated in the library')).toBeVisible();

  // Reopening restores the saved framing (ring 30%).
  await openInCutter(page, 'Owlbear');
  await expect(page.getByLabel(/Ring thickness/)).toHaveValue('30');
});

test('drag pans and wheel zooms the preview', async ({ page }) => {
  await openInCutter(page, 'Zombie');
  const stage = page.getByRole('application', { name: 'Drag to pan, scroll to zoom' });
  await expect(page.getByText(/100%/)).toBeVisible();
  await stage.hover();
  await page.mouse.wheel(0, -100);
  await expect(page.getByText(/110%/)).toBeVisible();
  const box = (await stage.boundingBox())!;
  const preview = page.getByTestId('token-preview');
  const before = await preview.getAttribute('src');
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 60, box.y + box.height / 2 + 20, { steps: 4 });
  await page.mouse.up();
  await expect(preview).not.toHaveAttribute('src', before!);
  await page.getByRole('button', { name: 'Reset view' }).click();
  await expect(page.getByText(/100%/)).toBeVisible();
});

test('cutter → sheet: copies, sizes, pages, cut border and clear', async ({ page }) => {
  await openInCutter(page, 'Goblin Boss');
  await page.getByRole('button', { name: 'Add to token sheet' }).click();
  await expect(page.getByText('added to the token sheet')).toBeVisible();

  await page.getByRole('tab', { name: 'Sheet' }).click();
  await expect(page.getByTestId('sheet-item')).toHaveCount(1);
  await expect(page.getByLabel('Size of Goblin Boss')).toHaveValue('Small');

  await page.getByLabel('Copies of Goblin Boss').fill('99');
  await page.getByLabel('Copies of Goblin Boss').press('Enter');
  await page.getByLabel('Size of Goblin Boss').selectOption('Large');
  // Large = 2 in: 4×5 per Letter page → 99 copies need 5 pages.
  await expect(page.getByText('Page 1 of 5')).toBeVisible();
  await page.getByRole('button', { name: 'Next page' }).click();
  await expect(page.getByText('Page 2 of 5')).toBeVisible();

  const svg = page.getByTestId('sheet-page');
  await expect(svg.locator('circle')).toHaveCount(0);
  await page.getByLabel('Cut border').check();
  await expect(svg.locator('circle')).toHaveCount(20);

  await page.getByRole('group', { name: 'Page format' }).getByRole('button', { name: 'A4' }).click();
  await expect(page.getByRole('group', { name: 'Page format' }).getByRole('button', { name: 'A4' })).toHaveAttribute('aria-pressed', 'true');

  await page.getByLabel('Copies of Goblin Boss').fill('150');
  await page.getByLabel('Copies of Goblin Boss').press('Enter');
  await expect(page.getByText('Copies must be between 1 and 99.')).toBeVisible();

  await page.getByRole('button', { name: 'Clear all…' }).click();
  await page.getByRole('button', { name: 'Clear all', exact: true }).click();
  await expect(page.getByText('No tokens yet.')).toBeVisible();
});

test('library inspector adds a token straight to the sheet', async ({ page }) => {
  await page.goto('/#/library');
  await page.getByRole('button', { name: 'Zombie (Token)' }).click();
  await page.getByRole('button', { name: 'Add to token sheet' }).click();
  await page.goto('/#/tokens.sheet');
  await expect(page.getByTestId('sheet-item')).toHaveCount(1);
  await expect(page.getByLabel('Label')).toHaveValue('Zombie');
});
