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
  await expect(button).toHaveText('5hClaude 32%Codex 64%');
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
  const settings = page.getByRole('button', { name: 'Settings', exact: true });
  await expect(settings).toBeVisible();
  expect((await settings.boundingBox()).x + (await settings.boundingBox()).width).toBeLessThanOrEqual(901);
  expect(await page.locator('.top > :last-child').evaluate((el) => el.getBoundingClientRect().right)).toBeLessThanOrEqual(901);
  await page.getByRole('button', { name: 'Usage', exact: true }).click();
  const pop = await page.locator('.usage-pop').boundingBox();
  expect(pop.x).toBeGreaterThanOrEqual(0);
  expect(pop.x + pop.width).toBeLessThanOrEqual(901);
  await page.screenshot({ path: '/tmp/cue-codex-usage-narrow.png' });
});
