import { expect, test } from '@playwright/test';

// AoE Templates against the in-memory mock backend.

test('the label follows the size until edited, and Reset label brings it back', async ({ page }) => {
  await page.goto('/#/maps.aoe');
  const inspector = page.getByRole('complementary', { name: 'Template settings' });
  const label = inspector.getByLabel('Label text');
  const size = inspector.locator('label.prop').nth(1).locator('input');
  await size.fill('30');
  await expect(label).toHaveValue(/30 ft/);

  await label.fill('Dragon breath');
  await size.fill('45');
  await expect(label).toHaveValue('Dragon breath');

  await inspector.getByRole('button', { name: 'Reset label' }).click();
  await expect(label).toHaveValue(/45 ft/);
  await expect(inspector.getByRole('button', { name: 'Reset label' })).toBeHidden();
});

test('preview switches between the current template and the full sheet', async ({ page }) => {
  await page.goto('/#/maps.aoe');
  const modes = page.getByRole('group', { name: 'Preview' });
  await expect(page.getByTestId('aoe-preview')).toBeVisible();

  await modes.getByRole('button', { name: /Full sheet/ }).click();
  await expect(page.getByText('No templates on the sheet yet')).toBeVisible();

  await modes.getByRole('button', { name: 'Current template' }).click();
  // a template small enough to share a page
  const inspector = page.getByRole('complementary', { name: 'Template settings' });
  await inspector.locator('label.prop').nth(1).locator('input').fill('10');
  await page.getByRole('button', { name: '+ Add to sheet' }).click();
  await expect(modes.getByRole('button', { name: 'Full sheet (1)' })).toBeVisible();
  await modes.getByRole('button', { name: 'Full sheet (1)' }).click();
  await expect(page.getByTestId('aoe-sheet').locator('img').first()).toBeVisible();
  await expect(page.getByTestId('aoe-preview')).toBeHidden();
  await expect(page.getByTestId('aoe-info')).not.toBeEmpty();

  await modes.getByRole('button', { name: 'Current template' }).click();
  await expect(page.getByTestId('aoe-preview')).toBeVisible();
});
