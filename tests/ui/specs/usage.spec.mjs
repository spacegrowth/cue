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

test("both providers have labels and separate popovers, and settings can hide Codex", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await usage(cue);
  await expect(page.locator('.usage-provider')).toHaveText(['Claude', 'Codex']);
  await expect(page.locator('[data-usage="meter"]')).toHaveText('Claude5h32%');
  await expect(page.locator('[data-usage="codex"]')).toHaveText('Codex5h64%');
  await page.getByRole('button', { name: 'Codex usage', exact: true }).click();
  await expect(page.locator('.usage-pop')).toHaveCount(1);
  await expect(page.locator('.usage-pop')).toContainText('64%');
  await expect(page.locator('.usage-pop')).toContainText('resets');
  await expect(page.locator('.usage-pop')).toContainText('Weekly');
  await expect(page.locator('.usage-pop')).toContainText('28%');
  await page.getByRole('button', { name: 'Claude Code usage', exact: true }).click();
  await expect(page.locator('.usage-pop')).toHaveCount(1);
  await expect(page.locator('.usage-pop')).toContainText('32%');
  await expect(page.locator('.usage-pop')).toContainText('Weekly');
  await expect(page.locator('.usage-pop')).toContainText('18%');
  await page.locator('.top').click({ position: { x: 2, y: 2 } });
  await expect(page.locator('.usage-pop')).toHaveCount(0);
  await cue.change((s) => { s.settings.usage = { ...s.settings.usage, codex: false }; });
  await expect(page.locator('[data-usage="codex"]')).toHaveCount(0);
  await expect(page.locator('.usage-provider')).toHaveCount(0);
  await expect(page.locator('[data-usage="meter"]')).toHaveText('5h32%wk18%');
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

test("two meters and their details fit the smallest window", async ({ cue, page }) => {
  await page.setViewportSize({ width: 900, height: 600 });
  await cue.open("board-waiting");
  await usage(cue);
  await page.emulateMedia({ colorScheme: "dark" });
  const settings = page.getByRole('button', { name: 'Settings', exact: true });
  await expect(settings).toBeVisible();
  expect((await settings.boundingBox()).x + (await settings.boundingBox()).width).toBeLessThanOrEqual(901);
  expect(await page.locator('.top > :last-child').evaluate((el) => el.getBoundingClientRect().right)).toBeLessThanOrEqual(901);
  await page.getByRole('button', { name: 'Codex usage', exact: true }).click();
  const pop = await page.locator('.usage-pop').boundingBox();
  expect(pop.x).toBeGreaterThanOrEqual(0);
  expect(pop.x + pop.width).toBeLessThanOrEqual(901);
  await page.screenshot({ path: '/tmp/cue-codex-usage-narrow.png' });
});
