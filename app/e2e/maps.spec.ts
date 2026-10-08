import { expect, test, type Page } from '@playwright/test';

// Map Paginator against the in-memory mock backend.

async function openMap(page: Page) {
  await page.goto('/#/library');
  await page.getByRole('button', { name: 'Crypt of the Lich (Map)' }).click();
  await page.getByRole('button', { name: 'Open in Map Paginator' }).click();
  await expect(page).toHaveURL(/#\/maps\.paginator$/);
  await expect(page.getByTestId('tool-hint')).toBeVisible();
}

test('inspector fits without sideways scrolling', async ({ page }) => {
  await page.setViewportSize({ width: 1320, height: 900 });
  await openMap(page);
  const inspector = page.getByRole('complementary', { name: 'Map settings' });
  const overflow = await inspector.evaluate((el) => el.scrollWidth - el.clientWidth);
  expect(overflow).toBeLessThanOrEqual(0);
  for (const mode of ['Rows×Cols', 'Reference', 'PPI', 'Cell side']) {
    await expect(inspector.getByRole('button', { name: mode, exact: true })).toBeInViewport();
  }
  if (process.env.MAP_SHOT) await page.screenshot({ path: process.env.MAP_SHOT });
});

test('reference tool hides the grid and pages, and sides can be dragged', async ({ page }) => {
  await openMap(page);
  const stage = page.getByTestId('map-stage');
  await expect(stage.locator('rect.tile').first()).toBeAttached();

  await page.getByRole('button', { name: 'Reference square' }).click();
  await expect(page.getByTestId('tool-hint')).toContainText('Grid and pages are hidden');
  await expect(stage.locator('rect.tile')).toHaveCount(0);

  // Draw a square, then drag its right side outwards.
  const box = (await stage.boundingBox())!;
  const [x0, y0] = [box.x + 100, box.y + 100];
  await page.mouse.move(x0, y0);
  await page.mouse.down();
  await page.mouse.move(x0 + 80, y0 + 80, { steps: 4 });
  await page.mouse.up();
  const sides = stage.locator('rect.side');
  await expect(sides).toHaveCount(4);
  const quad = stage.locator('polygon.quad');
  const before = await quad.getAttribute('points');

  const right = (await sides.nth(1).boundingBox())!;
  const [rx, ry] = [right.x + right.width / 2, right.y + right.height / 2];
  await page.mouse.move(rx, ry);
  await page.mouse.down();
  await page.mouse.move(rx + 40, ry, { steps: 4 });
  await page.mouse.up();
  await expect(quad).not.toHaveAttribute('points', before!);

  // Back to Pan: the pages come back.
  await page.getByRole('button', { name: 'Pan' }).click();
  await expect(stage.locator('rect.tile').first()).toBeAttached();
});
