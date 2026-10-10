import { expect, test } from '@playwright/test';

// Runs against the in-memory mock backend (seeded with 7 assets, 1 campaign).

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
  // First Escape leaves the text field, second closes the inspector.
  await page.keyboard.press('Escape');
  await expect(inspector).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(inspector).toBeHidden();
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

test('campaign view: create campaign and session, then jump to its assets', async ({ page }) => {
  await page.goto('/#/campaigns');
  await page.getByLabel('New campaign name').fill('Tomb of Annihilation');
  await page.getByRole('button', { name: '+ Campaign' }).click();
  await page.getByLabel('New session in Tomb of Annihilation').fill('Port Nyanzaru');
  await page.getByRole('button', { name: '+ Session' }).last().click();
  await expect(page.getByText('Port Nyanzaru')).toBeVisible();

  // The seeded campaign's session has one asset (Zombie).
  const toggle = page.getByRole('button', { name: 'Toggle sessions of Curse of Strahd' });
  if ((await toggle.getAttribute('aria-expanded')) !== 'true') await toggle.click();
  await page.locator('li', { hasText: 'Death House' }).getByRole('button', { name: 'Show assets' }).click();
  await expect(page).toHaveURL(/#\/library$/);
  await expect(page.getByTestId('asset-card')).toHaveCount(1);
  await expect(page.getByRole('button', { name: 'Zombie (Token)' })).toBeVisible();
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
