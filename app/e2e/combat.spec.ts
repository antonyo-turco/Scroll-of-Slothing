import { expect, test, type Page } from '@playwright/test';

// Combat tracker against the in-memory mock backend (mock-combat.ts).

/** Syncs the mock bestiary in Settings → Combat. Hash navigation keeps the in-memory mock. */
async function syncBestiary(page: Page) {
  await page.goto('/#/settings');
  await page.getByRole('tab', { name: 'Combat' }).click();
  const status = page.getByTestId('bestiary-status');
  await expect(status).toContainText('not synced');
  await page.getByRole('button', { name: 'Sync from mirror' }).click();
  await expect(status).toContainText('5 creatures');
  await page.evaluate(() => (location.hash = '#/combat'));
}

async function openWithGoblins(page: Page) {
  await syncBestiary(page);

  // a new player row opens ready for its name
  await page.getByRole('button', { name: '+ Player' }).click();
  await page.getByLabel('Player name').fill('Vex');
  await page.getByLabel('Player name').press('Tab');
  await page.getByRole('button', { name: 'Edit Vex' }).click();

  await page.getByRole('button', { name: '+ Creature…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Add creature' });
  await dialog.getByLabel('Search the bestiary').fill('gob');
  await dialog.getByRole('option', { name: /^Goblin MM/ }).click();
  await dialog.getByRole('spinbutton', { name: 'Count' }).fill('2');
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
  await expect(page.getByRole('button', { name: 'Remove Prone' })).toHaveText('Prone (0/1)');

  const hp = page.getByLabel('Current HP of Goblin (1)');
  await hp.fill('-3');
  await hp.press('Enter');
  await expect(hp).toHaveValue('4');
  await expect(page.getByLabel('Current HP of Goblin (1)').locator('xpath=ancestor::div[contains(@class, "hp-part")]')).toHaveAttribute('title', 'Hurt');

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
  // columns are set up in Settings → Combat
  await page.evaluate(() => (location.hash = '#/settings'));
  await page.getByRole('tab', { name: 'Combat' }).click();
  await page.getByLabel('Add a column').selectOption({ label: 'Armor Class' });
  await expect(page.getByRole('button', { name: 'Remove column AC' })).toBeVisible();
  await page.evaluate(() => (location.hash = '#/combat'));
  await expect(page.getByRole('columnheader', { name: 'AC', exact: true })).toBeVisible();
  // a value is text until the row's pen is clicked
  await expect(page.getByLabel('AC for Goblin (1)')).toHaveText('15');
  await page.getByRole('button', { name: 'Edit Goblin (1)' }).click();
  await expect(page.getByLabel('AC for Goblin (1)')).toHaveValue('15');
});

test('HP buttons appear on click and step the current HP', async ({ page }) => {
  await openWithGoblins(page);
  const hp = page.getByLabel('Current HP of Goblin (1)');
  const steps = page.getByRole('group', { name: 'Change the HP of Goblin (1)' });
  await expect(steps).toBeHidden();
  await hp.click();
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
  await page.getByRole('button', { name: 'Import…' }).click();
  const imp = page.getByRole('dialog', { name: 'Import encounters' });
  await imp.getByLabel(/encounter link/).fill(JSON.stringify({ name: 'Bridge', items: [{ h: 'goblin_mm', c: 4 }], fileType: 'encounter' }));
  await imp.getByRole('button', { name: 'Import', exact: true }).click();
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
  await dlg.getByLabel('Class of hero 1', { exact: true }).selectOption('Ranger');
  await dlg.getByLabel('Level of hero 1').fill('5');
  await dlg.getByLabel('Hit points of hero 1').fill('42');
  await dlg.getByLabel('Hit points of hero 1').press('Tab');
  await dlg.getByRole('button', { name: 'Save party' }).click();
  const list = page.getByRole('table', { name: 'Parties' });
  await expect(list).toContainText('Vex · Half-elf Ranger · lv 5');

  await list.getByRole('button', { name: 'Add Vox Machina to the tracker' }).click();
  await expect(page.getByRole('tab', { name: 'Tracker' })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('button', { name: 'Edit Vex' })).toBeVisible();
  // max HP is text until the row's pen is clicked
  await expect(page.getByLabel('Max HP of Vex')).toHaveText('42');
});
