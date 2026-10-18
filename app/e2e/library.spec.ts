import { expect, test } from '@playwright/test';

// Runs against the in-memory mock backend (seeded with 7 assets).

test.beforeEach(async ({ page }) => {
  await page.goto('/#/library');
  await expect(page.getByTestId('asset-card')).toHaveCount(7);
});

test('search, kind and tag filters narrow the grid', async ({ page }) => {
  await page.getByLabel('Search library').fill('dragon');
  await expect(page.getByTestId('asset-card')).toHaveCount(1);
  await page.getByLabel('Search library').fill('');
  await page.getByRole('group', { name: 'Kind' }).getByRole('button', { name: 'Map' }).click();
  await expect(page.getByTestId('asset-card')).toHaveCount(2);
  await page.getByRole('group', { name: 'Kind' }).getByRole('button', { name: 'All' }).click();
  await page.getByRole('group', { name: 'Tag filter' }).getByRole('button', { name: /^boss/ }).click();
  await expect(page.getByTestId('asset-card')).toHaveCount(2);
  await page.getByRole('button', { name: 'Clear filters' }).click();
  await expect(page.getByTestId('asset-card')).toHaveCount(7);
});

test('inspector edits name and tags', async ({ page }) => {
  await page.getByRole('button', { name: 'Owlbear (Token)' }).click();
  const inspector = page.getByRole('complementary', { name: 'Asset details' });
  await expect(inspector).toBeVisible();
  await inspector.getByLabel('Name').fill('Owlbear Matriarch');
  await inspector.getByLabel('Name').press('Enter');
  await inspector.getByLabel('Add tag').fill('forest');
  await inspector.getByLabel('Add tag').press('Enter');
  await expect(inspector.getByText('forest')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Owlbear Matriarch (Token)' })).toBeVisible();
  // First Escape leaves the text field, second deselects (the panel stays, empty).
  await page.keyboard.press('Escape');
  await expect(inspector).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(inspector.getByLabel('Name')).toBeHidden();
  await expect(inspector).toContainText('No asset selected');
});

test('bulk select, pin and delete with confirmation', async ({ page }) => {
  await page.getByRole('button', { name: 'Zombie (Token)' }).click({ modifiers: ['Control'] });
  await page.getByRole('button', { name: 'Brazier (Cut-out)' }).click();
  const bar = page.getByRole('toolbar', { name: 'Bulk actions' });
  await expect(bar.getByText('2 selected')).toBeVisible();
  await bar.getByRole('button', { name: 'Pin', exact: true }).click();
  await page.getByLabel('★ Pinned').check();
  await expect(page.getByTestId('asset-card')).toHaveCount(2);
  await bar.getByRole('button', { name: 'Delete…' }).click();
  await bar.getByRole('button', { name: 'Delete 2' }).click();
  await expect(page.getByText('No assets match these filters.')).toBeVisible();
});

test('size can be changed directly on the card', async ({ page }) => {
  const select = page.getByLabel('Print size of Zombie');
  await select.selectOption('Huge');
  await page.getByRole('button', { name: 'Zombie (Token)' }).click();
  await expect(page.getByRole('complementary', { name: 'Asset details' }).getByLabel('Print size')).toHaveValue('Huge');
});

test('open in editor routes each kind to its editor with the asset loaded', async ({ page }) => {
  const cases: [string, string, RegExp][] = [
    ['Owlbear (Token)', 'Open in Token Cutter', /#\/tokens\.cutter$/],
    ['Brazier (Cut-out)', 'Open in Object Cutter', /#\/cutouts\.cutter$/],
    ['Forest Road (Map)', 'Open in Map Paginator', /#\/maps\.paginator$/],
  ];
  for (const [card, open, url] of cases) {
    await page.goto('/#/library');
    await page.getByRole('button', { name: card }).click();
    await page.getByRole('complementary', { name: 'Asset details' }).getByRole('button', { name: open }).click();
    await expect(page).toHaveURL(url);
    await expect(page.getByText(/Editing library asset #\d+/)).toBeVisible();
  }
});

test('list and pill views show the same assets and are remembered', async ({ page }) => {
  const view = page.getByRole('group', { name: 'View' });
  await view.getByRole('button', { name: 'List view' }).click();
  await expect(page.getByTestId('asset-card')).toHaveCount(7);
  await expect(page.getByText('Updated', { exact: true })).toBeVisible();
  // Rows still select and pin.
  const first = page.getByTestId('asset-card').first();
  await first.getByRole('button', { name: /^Pin / }).click();
  await expect(first.getByRole('button', { name: /^Unpin / })).toBeVisible();

  await view.getByRole('button', { name: 'Pills view' }).click();
  await expect(page.getByTestId('asset-card')).toHaveCount(7);
  await expect(view.getByRole('button', { name: 'Pills view' })).toHaveAttribute('aria-pressed', 'true');

  // Hash navigation keeps the in-memory mock (a reload would reset it).
  await page.evaluate(() => (location.hash = '#/cards'));
  await expect(page.getByLabel('Deck markdown')).toBeVisible();
  await page.evaluate(() => (location.hash = '#/library'));
  await expect(page.getByRole('group', { name: 'View' }).getByRole('button', { name: 'Pills view' })).toHaveAttribute('aria-pressed', 'true');
});
