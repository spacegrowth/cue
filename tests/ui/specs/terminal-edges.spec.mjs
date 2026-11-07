// A session's terminal when it doesn't go as planned: an open that fails says why and can be tried again, a
// client that ends (ssh gone) closes it, and an end from an earlier opening leaves the one open now alone.
import { test, expect } from "./cue.mjs";
import { openTerminal } from "./term.mjs";

const strip = (page) => page.locator('.tstrip[data-sid="relay"]');

test.beforeEach(async ({ cue }) => { await cue.open("terminal"); });

test("an open that fails says why, leaves no terminal, and can be tried again", async ({ cue, page }) => {
  await cue.override("term_open", () => { throw "tmux pane %1 is gone"; });
  await page.locator('.nrow[data-big="n1"]').click();
  await strip(page).click();
  await expect(page.locator(".toast")).toContainText("Couldn't open its terminal: tmux pane %1 is gone");
  await expect(page.locator(".term-host")).toHaveCount(0);
  await strip(page).click();
  await expect.poll(async () => (await cue.calls("term_open")).length).toBe(2);
});

test("its client ending (the ssh under it gone) closes it", async ({ cue, page }) => {
  await openTerminal(page);
  await cue.emit("term", { sid: "relay", gen: 1, end: true });
  await expect(page.locator(".term-host")).toHaveCount(0);
  await cue.called("term_close", { sessionId: "relay" });
});

test("an end from an earlier opening leaves the one open now", async ({ cue, page }) => {
  await openTerminal(page);
  await cue.emit("term", { sid: "relay", gen: 99, end: true });
  await page.keyboard.type("x");
  await cue.called("term_write", { sessionId: "relay", data: "x" });
  await expect(page.locator(".term-host")).toHaveCount(1);
});
