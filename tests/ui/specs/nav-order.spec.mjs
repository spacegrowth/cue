// The header's switch: Board first, then Desk, then the rest. No Inbox. The terminal isn't in it: its
// button opens the full view on a click (held pressed), and ⌘J keeps to the small peek.
import { test, expect } from "./cue.mjs";

test("the switch reads Board, Desk, Read, and no Inbox, Terminal or Insights", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const views = await page.locator(".switch-view button").evaluateAll((bs) => bs.map((b) => b.dataset.view));
  expect(views.slice(0, 2)).toEqual(["board", "desk"]);
  expect(views).toContain("read");
  expect(views).not.toContain("inbox");
  expect(views).not.toContain("terminal");
  expect(views).not.toContain("insights");
});

test("Insights is the small icon just left of Settings: it opens Insights, held pressed", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const next = await page.locator(".insights-btn").evaluate((b) => b.nextElementSibling?.dataset.act);
  expect(next).toBe("open-settings");
  await expect(page.locator(".insights-btn")).not.toHaveAttribute("title", /./);
  await page.locator(".insights-btn").click();
  await expect.poll(() => page.evaluate(() => view)).toBe("insights");
  await expect(page.locator(".insights-btn")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".switch-view button.on")).toHaveCount(0);
});

test("a click on the terminal button opens the full view, pressed; again goes back", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await page.locator(".switch-view [data-view=read]").click();
  await page.locator(".shells-btn").click();
  await expect.poll(() => page.evaluate(() => view)).toBe("terminal");
  await expect(page.locator(".shells-btn")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".switch-view button.on")).toHaveCount(0);
  await page.locator(".shells-btn").click();
  await expect.poll(() => page.evaluate(() => view)).toBe("read");
});

test("⌘J keeps to the peek", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await page.locator(".shells-btn").click();   // a first shell
  await page.locator(".shells-btn").click();   // back to the board
  await page.keyboard.press("Meta+j");
  await expect.poll(() => page.evaluate(() => view)).toBe("board");
  await expect(page.locator(".pip")).toBeVisible();
  await expect(page.locator(".pip")).not.toHaveClass(/in-view/);
});
