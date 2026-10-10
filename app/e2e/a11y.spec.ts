import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

// Accessibility pass (Phase 6): every route is scanned with axe (WCAG 2.x A
// and AA rules) against the mock backend, and the main keyboard paths are
// exercised. See docs/ux.md, "Accessibility".

const ROUTES = [
  'library',
  'tokens.cutter',
  'tokens.sheet',
  'cutouts.cutter',
  'cutouts.sheet',
  'cards',
  'maps.paginator',
  'maps.aoe',
  'campaigns',
  'music',
  'settings',
];
const MUSIC_TABS = ['Mixer', 'Soundscapes', 'Playlists', 'Soundboard'];

async function scan(page: Page, where: string) {
  const r = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
  const found = r.violations.map((v) => ({
    where,
    rule: v.id,
    impact: v.impact,
    nodes: v.nodes.slice(0, 4).map((n) => n.target.join(' ')),
  }));
  return found;
}

for (const theme of ['light', 'dark'] as const) {
  test.describe(`axe (${theme})`, () => {
    test.describe.configure({ timeout: 120_000 });
    for (const r of ROUTES) {
      test(`no WCAG A/AA violations: ${r}`, async ({ page }) => {
        await page.emulateMedia({ colorScheme: theme });
        await page.goto(`/#/${r}`);
        await page.locator('main').waitFor();
        await page.waitForTimeout(400);
        const found = await scan(page, r);
        expect(found, JSON.stringify(found, null, 2)).toEqual([]);
      });
    }
    for (const t of MUSIC_TABS) {
      test(`no WCAG A/AA violations: music/${t}`, async ({ page }) => {
        await page.emulateMedia({ colorScheme: theme });
        await page.goto('/#/music');
        await page.getByRole('tablist', { name: 'Music views' }).getByRole('tab', { name: t }).click();
        await page.waitForTimeout(300);
        const found = await scan(page, `music/${t}`);
        expect(found, JSON.stringify(found, null, 2)).toEqual([]);
      });
    }
  });
}

test('keyboard: skip link, rail and command palette', async ({ page }) => {
  await page.goto('/#/library');
  await page.keyboard.press('Tab');
  const skip = page.getByRole('link', { name: 'Skip to content' });
  await expect(skip).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator('main')).toBeFocused();

  // the rail is reachable and operable from the keyboard
  await page.getByRole('navigation', { name: 'Main' }).getByRole('button', { name: 'Music' }).focus();
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/#\/music/);

  // Ctrl+K opens the palette, Esc closes it and focus returns
  const before = page.getByRole('navigation', { name: 'Main' }).getByRole('button', { name: 'Music' });
  await before.focus();
  await page.keyboard.press('Control+k');
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).toBeHidden();
  await expect(before).toBeFocused();
});

test('keyboard: dialogs trap focus and restore it', async ({ page }) => {
  await page.goto('/#/music');
  const open = page.getByRole('button', { name: '+ Add Track' });
  await open.focus();
  await page.keyboard.press('Enter');
  const dlg = page.getByRole('dialog');
  await expect(dlg).toBeVisible();
  // focus starts inside and stays inside while tabbing
  for (let i = 0; i < 25; i++) {
    await page.keyboard.press('Tab');
    expect(await dlg.evaluate((d) => d.contains(document.activeElement))).toBe(true);
  }
  await page.keyboard.press('Escape');
  await expect(dlg).toBeHidden();
  await expect(open).toBeFocused();
});
