import { expect, test, type Page } from '@playwright/test';

// Combat tracker against the in-memory mock backend (mock-combat.ts).

async function openWithGoblins(page: Page) {
  await page.goto('/#/combat');
  const inspector = page.getByRole('complementary', { name: 'Tracker settings' });
  await expect(inspector.getByTestId('bestiary-status')).toContainText('Not synced');
  await inspector.getByRole('button', { name: 'Sync from mirror' }).click();
  await expect(inspector.getByTestId('bestiary-status')).toContainText('4 creatures');

  await page.getByRole('button', { name: '+ Player' }).click();
  await page.getByLabel('Player name').fill('Vex');
  await page.getByLabel('Player name').press('Tab');

  await page.getByRole('button', { name: '+ Creature…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Add creature' });
  await dialog.getByLabel('Search the bestiary').fill('gob');
  await dialog.getByRole('option', { name: /^Goblin MM/ }).click();
  await dialog.getByLabel('Count').fill('2');
  await dialog.getByRole('button', { name: 'Add 2 × Goblin' }).click();
  await expect(page.getByTestId('combat-row')).toHaveCount(3);
}

test('add players and creatures from the synced bestiary, then run turns', async ({ page }) => {
  await openWithGoblins(page);
  await expect(page.getByText('Goblin (2)')).toBeVisible();
  const status = page.getByTestId('combat-status');
  await expect(status).toContainText('Press Next turn to start');

  const toolbar = page.getByRole('region', { name: 'Combat tracker' });
  await toolbar.getByRole('button', { name: 'Next turn' }).click();
  // goblins that roll the same initiative share a turn (5etools)
  await expect.poll(() => page.locator('tr.active').count()).toBeGreaterThan(0);
  for (let i = 0; i < 3; i++) await toolbar.getByRole('button', { name: 'Next turn' }).click();
  await expect(status).toContainText('Round 2');
  await toolbar.getByRole('button', { name: 'Previous turn' }).click();
  await expect(status).toContainText('Round 2');
});

test('conditions count down and HP accepts relative damage', async ({ page }) => {
  await openWithGoblins(page);
  await page.getByRole('button', { name: 'Add a condition to Goblin (1)' }).click();
  const dlg = page.getByRole('dialog', { name: /Add condition/ });
  await dlg.getByLabel('Turns').fill('1');
  await dlg.getByRole('button', { name: 'Prone' }).click();
  await expect(page.getByRole('button', { name: 'Remove Prone' })).toHaveText('Prone (1)');

  const hp = page.getByLabel('Current HP of Goblin (1)');
  await hp.fill('-3');
  await hp.press('Enter');
  await expect(hp).toHaveValue('4');
  await expect(page.getByLabel('Current HP of Goblin (1)').locator('..')).toHaveAttribute('title', 'Hurt');

  await page.getByRole('button', { name: 'Remove Prone' }).click();
  await expect(page.getByRole('button', { name: 'Remove Prone' })).toHaveCount(0);
});

test('lock blocks edits; encounter links report missing creatures', async ({ page }) => {
  await openWithGoblins(page);
  await page.getByRole('button', { name: '🔓 Lock' }).click();
  await expect(page.getByRole('button', { name: '+ Player' })).toBeDisabled();
  await expect(page.getByLabel('Current HP of Goblin (1)')).toBeDisabled();
  await page.getByRole('button', { name: '🔒 Locked' }).click();

  await page.getByRole('button', { name: 'Open link…' }).click();
  const dlg = page.getByRole('dialog', { name: 'Open a 5etools link' });
  await dlg.getByLabel('Combat or encounter link').fill('http://localhost:8080/dmscreen.html#encounter=abc');
  await dlg.getByRole('button', { name: 'Open' }).click();
  await expect(page.getByRole('status')).toContainText('kraken_mm');
});

test('stat columns fill from the stat block', async ({ page }) => {
  await openWithGoblins(page);
  const inspector = page.getByRole('complementary', { name: 'Tracker settings' });
  await inspector.getByLabel('Add a column').selectOption({ label: 'Armor Class' });
  await expect(page.getByRole('columnheader', { name: 'AC', exact: true })).toBeVisible();
  await inspector.getByRole('button', { name: 'Remove column AC' }).click();
  await expect(page.getByRole('columnheader', { name: 'AC', exact: true })).toHaveCount(0);
});

test('HP buttons appear on hover and step the current HP', async ({ page }) => {
  await openWithGoblins(page);
  const hp = page.getByLabel('Current HP of Goblin (1)');
  const steps = page.getByRole('group', { name: 'Change the HP of Goblin (1)' });
  await expect(steps).toBeHidden();
  await hp.hover();
  await expect(steps).toBeVisible();
  await steps.getByRole('button', { name: '-5 HP to Goblin (1)' }).click();
  await expect(hp).toHaveValue('2');
  await steps.getByRole('button', { name: '+1 HP to Goblin (1)' }).click();
  await expect(hp).toHaveValue('3');
  await expect(page.getByRole('button', { name: /Shown to players/ })).toHaveCount(0);
});

test('encounters: build one, start it, and import a 5etools encounter file', async ({ page }) => {
  await openWithGoblins(page);
  await page.getByRole('tab', { name: 'Encounters' }).click();
  await page.getByRole('button', { name: '+ New encounter' }).first().click();
  const dlg = page.getByRole('dialog', { name: 'New encounter' });
  await dlg.getByLabel('Add from the bestiary').fill('ogre');
  await dlg.getByRole('button', { name: 'Add Ogre' }).click();
  await dlg.getByRole('button', { name: 'One more Ogre' }).click();
  await dlg.getByRole('button', { name: 'Save encounter' }).click();
  const list = page.getByRole('table', { name: 'Encounters' });
  await expect(list.getByText('2 × Ogre').first()).toBeVisible();

  await list.getByRole('button', { name: 'Add 2 × Ogre to the current combat' }).click();
  await expect(page.getByRole('tab', { name: 'Tracker' })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByTestId('combat-row')).toHaveCount(5);

  await page.getByRole('tab', { name: 'Encounters' }).click();
  await page.getByRole('button', { name: 'Import from 5etools…' }).click();
  const imp = page.getByRole('dialog', { name: 'Import from 5etools' });
  await imp.getByLabel(/Encounter link/).fill(JSON.stringify({ name: 'Bridge', items: [{ h: 'goblin_mm', c: 4 }], fileType: 'encounter' }));
  await imp.getByRole('button', { name: 'Import' }).click();
  await expect(list.getByText('Bridge')).toBeVisible();
  await expect(list.getByRole('row').filter({ hasText: 'Bridge' })).toContainText('4');
});

test('parties: save heroes and add them as player rows', async ({ page }) => {
  await page.goto('/#/combat');
  await page.getByRole('tab', { name: 'Parties' }).click();
  await page.getByRole('button', { name: '+ New party' }).first().click();
  const dlg = page.getByRole('dialog', { name: 'New party' });
  await dlg.getByLabel('Party name').fill('Vox Machina');
  await dlg.getByLabel('Name of hero 1').fill('Vex');
  await dlg.getByLabel('Species of hero 1').fill('Half-elf');
  await dlg.getByLabel('Class of hero 1').fill('Ranger');
  await dlg.getByLabel('Level of hero 1').fill('5');
  await dlg.getByLabel('Hit points of hero 1').fill('42');
  await dlg.getByLabel('Hit points of hero 1').press('Tab');
  await dlg.getByRole('button', { name: 'Save party' }).click();
  const list = page.getByRole('table', { name: 'Parties' });
  await expect(list).toContainText('Vex · Half-elf Ranger · lv 5');

  await list.getByRole('button', { name: 'Add Vox Machina to the tracker' }).click();
  await expect(page.getByRole('tab', { name: 'Tracker' })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByLabel('Player name')).toHaveValue('Vex');
  await expect(page.getByLabel('Max HP of Vex')).toHaveValue('42');
});
