// Text size: ⌘+ / ⌘− step the whole window's zoom through Settings → Text size's four steps, ⌘0 goes back to Default
// (appearance.zoom; the app's webview zooms, so here only what's saved, said and shown in Settings is seen). There is
// no separate size for a chat: no A− / A+ anywhere.
import { test, expect, openDesk } from "./cue.mjs";

/** Step `z` was saved, and Cue's state has it back (the next key steps from there). */
const saved = async (cue, page, z) => { await cue.called("set_setting", { key: "appearance.zoom", value: z }); await expect.poll(() => page.evaluate(() => state.settings.appearance.zoom)).toBe(z); };

test("⌘+ / ⌘− step the text size with a note, Settings shows the step, ⌘0 goes back to Default", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  await page.keyboard.press("Meta+=");
  await saved(cue, page, 1.15);
  await expect(page.locator(".toast")).toHaveText("Text size: Large");
  await page.keyboard.press("Meta+=");
  await saved(cue, page, 1.3);
  await page.keyboard.press("Meta+=");
  await expect(page.locator(".toast")).toHaveText("Text size: Larger (the largest)");
  await page.keyboard.press(",");
  const seg = page.locator('.seg:has([data-set="appearance.zoom"])');
  await expect(seg.locator("button")).toHaveText(["Compact", "Default", "Large", "Larger"]);
  await expect(seg.locator("button.on")).toHaveText("Larger");
  await page.keyboard.press("Meta+0");
  await saved(cue, page, 1);
  await expect(page.locator(".toast")).toHaveText("Text size: Default");
  await expect(seg.locator("button.on")).toHaveText("Default");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Meta+-");
  await saved(cue, page, 0.9);
  await expect(page.locator(".toast")).toHaveText("Text size: Compact");
  await page.keyboard.press("Meta+-");
  await expect(page.locator(".toast")).toHaveText("Text size: Compact (the smallest)");
  expect((await cue.calls("set_setting")).filter((c) => c.args.key !== "appearance.zoom")).toEqual([]);
});

test("no A− / A+ on a chat, the Desk lead's chat or an item's chat", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  await page.locator('.nrow[data-big="n1"]').click({ position: { x: 12, y: 12 } });
  await page.locator(".active-pane .ap-chat").first().waitFor();
  await expect(page.locator(".active-pane .stabs")).not.toContainText("A+");
  await cue.open("desk");
  await openDesk(page);
  await page.locator(".dk-chat").first().waitFor();
  await expect(page.getByRole("button", { name: /text/i })).toHaveCount(0);
  await page.locator('[data-desk-item="d5"]').first().click();
  await page.locator(".dk-right .dkn-sp .dkn-stabs").waitFor();
  await expect(page.locator(".dk-right .dkn-sp")).not.toContainText("A+");
});
