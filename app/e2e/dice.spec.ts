import { expect, test } from '@playwright/test';

// Floating dice roller (docs/features/dice-roller.md), against the mock backend.
test('dice roller: buttons, expression line, errors, collapse and hide', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto('/#/tokens.sheet');

  const roller = page.getByRole('region', { name: 'Dice roller' });
  await roller.getByRole('button', { name: 'Open the dice roller' }).click();
  const line = roller.getByRole('textbox', { name: 'Dice expression' });
  await expect(line).toBeFocused();

  await roller.getByRole('button', { name: 'd20', exact: true }).click();
  await expect(roller.getByRole('list', { name: 'Recent rolls' }).getByRole('listitem')).toHaveCount(1);

  await line.fill('2d6+3');
  await line.press('Enter');
  const newest = roller.locator('.rolls li.newest');
  await expect(newest).toContainText('2d6 + 3');
  await expect(newest.locator('.detail')).toHaveText(/^\[\d, \d\] \+ 3 = \d+$/);
  await expect(line).toHaveValue('');

  await line.fill('2d0');
  await line.press('Enter');
  await expect(roller.getByRole('alert')).toContainText("'2d0'");
  await expect(roller.locator('.rolls li')).toHaveCount(2);
  await expect(line).toHaveValue('2d0');

  // Clicking the page behind collapses to the latest-roll pill.
  await page.locator('main').click({ position: { x: 40, y: 200 } });
  await expect(roller.getByTestId('dice-latest')).toBeVisible();
  await expect(line).toBeHidden();

  // D hides and reopens; the rail button toggles too.
  await page.keyboard.press('d');
  await expect(roller).toBeHidden();
  const railButton = page.getByRole('navigation', { name: 'Main' }).getByRole('button', { name: 'Dice roller' });
  await expect(railButton).toHaveAttribute('aria-pressed', 'false');
  await railButton.click();
  await expect(railButton).toHaveAttribute('aria-pressed', 'true');
  await expect(roller.getByRole('textbox', { name: 'Dice expression' })).toBeFocused();
  // The unsent draft survives hiding, and typing D in the line is text, not the shortcut.
  await page.keyboard.type('d');
  await expect(roller.getByRole('textbox', { name: 'Dice expression' })).toHaveValue('2d0d');
  await expect(roller).toBeVisible();

  expect(errors).toEqual([]);
});

test('dice roller: the dice_history setting limits the list', async ({ page }) => {
  await page.goto('/#/settings');
  const field = page.getByRole('spinbutton', { name: 'Recent rolls to show' });
  await field.fill('2');
  await field.press('Tab');
  const roller = page.getByRole('region', { name: 'Dice roller' });
  await roller.getByRole('button', { name: 'Open the dice roller' }).click();
  for (let i = 0; i < 4; i++) await roller.getByRole('button', { name: 'd6', exact: true }).click();
  await expect(roller.locator('.rolls li')).toHaveCount(2);
  await field.fill('0');
  await field.press('Tab');
  await expect(page.getByText('must be a whole number from 1 to 50')).toBeVisible();
});
