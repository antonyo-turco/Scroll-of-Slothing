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

test('orientation and the fewest-pages suggestion', async ({ page }) => {
  await openMap(page);
  const inspector = page.getByRole('complementary', { name: 'Map settings' });
  const summary = page.getByTestId('tiling-summary');
  const pages = async () => Number((await summary.textContent())!.match(/(\d+) page/)![1]);
  const orient = inspector.getByRole('group', { name: 'Page orientation' });

  await orient.getByRole('button', { name: 'Portrait' }).click();
  await expect(summary).toContainText('page');
  const portrait = await pages();
  await orient.getByRole('button', { name: 'Landscape' }).click();
  await expect.poll(pages).not.toBe(-1);
  const landscape = await pages();
  expect(portrait).not.toBe(landscape); // the mock map differs between orientations

  // pick the worse orientation: the suggestion appears and fixes it
  const worse = portrait > landscape ? 'Portrait' : 'Landscape';
  await orient.getByRole('button', { name: worse }).click();
  const hint = page.getByTestId('fewest-pages');
  await expect(hint).toContainText(`${Math.min(portrait, landscape)} page`);
  await hint.getByRole('button', { name: 'Use fewest pages' }).click();
  await expect(hint).toBeHidden();
  await expect.poll(pages).toBe(Math.min(portrait, landscape));
  await expect(orient.getByRole('button', { name: worse })).toHaveAttribute('aria-pressed', 'false');
});

test('touchpad gestures: pinch zooms gently, two-finger scroll pans, a wheel notch zooms', async ({ page }) => {
  await openMap(page);
  const stage = page.getByTestId('map-stage');
  const zoom = async () => Number((await page.locator('.zoom').textContent())!.replace('%', ''));
  const layer = () => stage.locator('.layer').evaluate((el) => (el as HTMLElement).style.transform);
  const box = (await stage.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);

  // a pinch arrives as Ctrl + small wheel deltas: ~5% per event, not 10% steps
  const z0 = await zoom();
  await page.keyboard.down('Control');
  for (let i = 0; i < 4; i++) await page.mouse.wheel(0, -5);
  await page.keyboard.up('Control');
  await expect.poll(zoom).toBeGreaterThan(z0);
  const z1 = await zoom();
  expect(z1).toBeLessThan(z0 * 1.3);

  // two-finger scroll: pans, zoom unchanged
  const before = await layer();
  await page.mouse.wheel(30, 20);
  await expect.poll(layer).not.toBe(before);
  expect(await zoom()).toBe(z1);

  // one mouse wheel notch (deltaY 100) zooms one step
  await page.mouse.wheel(0, -100);
  await expect.poll(zoom).toBeGreaterThan(z1);
});
