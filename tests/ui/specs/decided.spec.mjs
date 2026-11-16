// An agent's judgment calls (`DECIDED: what — why` lines; core.js decidedHtml): one quiet block in its message, in the
// chat and the Inbox alike, each call a line with its reason under it; a one-line preview says "Decided:".
import { test, expect } from "./cue.mjs";

const TEXT = "Done.\n\nDECIDED: Kept per-key limits — keys are what the docs promise\n- DECIDED: skipped the flaky date test -- it needs a fixed clock\nDECIDED: no reason given here\n\nInstalling now.";

test("DECIDED lines read as one block: each call, its reason under it; text around it stays as it was", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const html = await page.evaluate((t) => md(t), TEXT);
  await page.setContent(`<div class="msg">${html}</div>`);
  await expect(page.locator(".decided")).toHaveCount(1);
  await expect(page.locator(".decided .dec-head")).toHaveText("Decided on its own · 3");
  await expect(page.locator(".decided .dec-what")).toHaveText(["Kept per-key limits", "skipped the flaky date test", "no reason given here"]);
  await expect(page.locator(".decided .dec-why")).toHaveText(["keys are what the docs promise", "it needs a fixed clock"]);
  await expect(page.locator("p")).toHaveText(["Done.", "Installing now."]);
  expect(await page.evaluate(() => document.body.textContent.includes("DECIDED"))).toBe(false);
});

test("a one-line preview says Decided:, not DECIDED:", async ({ cue, page }) => {
  await cue.open("board-waiting");
  expect(await page.evaluate((t) => plain(t), TEXT)).toContain("Decided: Kept per-key limits");
});

test("in a code block it stays as written", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const html = await page.evaluate(() => md("```\nDECIDED: x — y\n```"));
  expect(html).not.toContain("decided");
});
