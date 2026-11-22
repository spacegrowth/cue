import { test, expect } from "./cue.mjs";

async function usage(cue, { claude = true, codex = true } = {}) {
  await cue.change((s, enabled) => {
    const now = Date.now();
    s.usage = enabled.claude ? { five_hour: { pct: 32, resets_ms: now + 3600000 }, seven_day: { pct: 18, resets_ms: now + 86400000 }, models: [], at_ms: now } : null;
    s.codex_usage = enabled.codex ? { windows: [{ pct: 64, minutes: 300, resets_ms: now + 7200000 }, { pct: 28, minutes: 10080, resets_ms: now + 86400000 }], at_ms: now } : null;
  }, { claude, codex });
}

test("a single provider keeps the existing meter layout", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await usage(cue, { codex: false });
  await expect(page.locator('.usage-chip')).toHaveCount(1);
  await expect(page.locator('.usage-provider')).toHaveCount(0);
  await expect(page.locator('.usage-chip')).toHaveText('5h32%wk18%');
  await page.getByRole('button', { name: 'Claude Code usage', exact: true }).click();
  await expect(page.locator('.usage-pop')).toContainText('Claude Code usage');
  await usage(cue, { claude: false });
  await expect(page.locator('.usage-chip')).toHaveCount(1);
  await expect(page.locator('.usage-provider')).toHaveCount(0);
  await page.getByRole('button', { name: 'Codex usage', exact: true }).click();
  await expect(page.locator('.usage-pop')).toContainText('Codex usage');
  await expect(page.locator('.usage-pop')).toContainText('64%');
});

test("one usage button opens both providers and their weekly limits", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await usage(cue);
  await expect(page.locator('.usage-chip')).toHaveCount(1);
  const button = page.getByRole('button', { name: 'Usage', exact: true });
  await expect(button).toHaveText('Claude32%Codex64%5h');
  await expect(button.locator('.um')).toHaveCount(2);
  await expect(button).not.toContainText('wk');
  await expect(button).not.toContainText('18%');
  await expect(button).not.toContainText('28%');
  await button.click();
  const popup = page.locator('.usage-pop');
  await expect(popup).toHaveCount(1);
  await expect(popup).toContainText('Claude Code usage');
  await expect(popup).toContainText('Codex usage');
  await expect(popup).toContainText('32%');
  await expect(popup).toContainText('64%');
  await expect(popup).toContainText('18%');
  await expect(popup).toContainText('28%');
  await expect(popup).toContainText('Weekly');
  await expect(popup).toContainText('resets');
  await button.click();
  await expect(popup).toHaveCount(0);
  await button.click();
  await page.locator('.top').click({ position: { x: 2, y: 2 } });
  await expect(popup).toHaveCount(0);
  await cue.change((s) => { s.settings.usage = { ...s.settings.usage, codex: false }; });
  await expect(page.locator('.usage-chip')).toHaveText('5h32%wk18%');
  await page.getByRole('button', { name: 'Claude Code usage', exact: true }).click();
  await expect(popup).not.toContainText('Codex usage');
});

test("expired Codex windows disappear and alternate durations keep their labels", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await usage(cue, { claude: false });
  await cue.change((s) => {
    s.codex_usage.windows[0].resets_ms = Date.now() - 1000;
    s.codex_usage.windows[1].minutes = 1440;
  });
  await expect(page.locator('[data-usage="codex"]')).toHaveText('1d28%');
  await page.locator('[data-usage="codex"]').click();
  await expect(page.locator('.usage-pop')).toContainText('1 day');
  await cue.change((s) => { s.codex_usage.windows[1].resets_ms = Date.now() - 1000; });
  await expect(page.locator('[data-usage="codex"]')).toHaveCount(0);
});

test("merged usage and its details fit the smallest window", async ({ cue, page }) => {
  await page.setViewportSize({ width: 900, height: 600 });
  await cue.open("board-waiting");
  await usage(cue);
  await page.emulateMedia({ colorScheme: "dark" });
  // The bar redraws as the usage lands, so each box is read in one go from the element there then (a boundingBox()
  // taken between redraws is null), once the merged button is in.
  const rect = (loc) => loc.evaluate((el) => { const r = el.getBoundingClientRect(); return { x: r.x, right: r.right }; });
  await expect(page.getByRole('button', { name: 'Usage', exact: true })).toHaveText('Claude32%Codex64%5h');
  const settings = page.getByRole('button', { name: 'Settings', exact: true });
  await expect(settings).toBeVisible();
  expect((await rect(settings)).right).toBeLessThanOrEqual(901);
  expect((await rect(page.locator('.top > :last-child'))).right).toBeLessThanOrEqual(901);
  await page.getByRole('button', { name: 'Usage', exact: true }).click();
  await expect(page.locator('.usage-pop')).toBeVisible();
  const pop = await rect(page.locator('.usage-pop'));
  expect(pop.x).toBeGreaterThanOrEqual(0);
  expect(pop.right).toBeLessThanOrEqual(901);
});

/** The usage as each plan reports it: Pro and Max with both windows, Ultra with the weekly one only. */
async function plan(cue, plan, { codex = false } = {}) {
  await cue.change((s, a) => {
    const now = Date.now();
    const w = (pct, h) => ({ pct, resets_ms: now + h * 3600000 });
    s.usage = a.plan === "Ultra"
      ? { seven_day: w(4, 40), models: [], at_ms: now, no_limit: ["five_hour"], plan: "Ultra" }
      : { five_hour: w(32, 1), seven_day: w(18, 24), models: [], at_ms: now, no_limit: [], plan: a.plan };
    s.codex_usage = a.codex ? { windows: [{ pct: 64, minutes: 300, resets_ms: now + 7200000 }], at_ms: now } : null;
  }, { plan, codex });
}

for (const name of ["Pro", "Max 5x", "Max 20x"]) {
  test(`${name}: both limits with their numbers, and the plan named`, async ({ cue, page }) => {
    await cue.open("board-waiting");
    await plan(cue, name);
    await expect(page.locator('.usage-chip')).toHaveText('5h32%wk18%');
    await page.getByRole('button', { name: 'Claude Code usage', exact: true }).click();
    const pop = page.locator('.usage-pop');
    await expect(pop.locator('.up-head')).toContainText(`Claude Code usage · ${name}`);
    await expect(pop).toContainText('32%');
    await expect(pop).not.toContainText('No 5-hour limit');
  });
}

test("Ultra: no 5-hour limit is said plainly, with no bar or old number", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await plan(cue, "Ultra");
  const chip = page.locator('.usage-chip');
  await expect(chip).toHaveText('5hno limitwk4%');
  await expect(chip.locator('.um')).toHaveCount(1);
  await chip.click();
  const pop = page.locator('.usage-pop');
  await expect(pop.locator('.up-head')).toContainText('Claude Code usage · Ultra');
  await expect(pop).toContainText('No 5-hour limit on your plan');
  await expect(pop.locator('.up-bar')).toHaveCount(1);
  await expect(pop).toContainText('4%');
  // Beside Codex, the shared 5h chip says the same.
  await chip.click();
  await plan(cue, "Ultra", { codex: true });
  await expect(page.getByRole('button', { name: 'Usage', exact: true })).toHaveText('Claudeno limitCodex64%5h');
});
