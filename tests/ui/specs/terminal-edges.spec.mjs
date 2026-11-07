// A session's terminal when it doesn't go as planned: an open that fails says why and can be tried again, a
// client that ends (ssh gone) closes it, an end from an earlier opening leaves the one open now alone, and
// the terminal (one, kept from opening to opening) says nothing of the last session over the next.
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

/** Another session (det) in tmux, its chat and terminal opened as its strip's click opens them, after `before`
 *  runs in the window in the same turn. */
const switchTo = (page, before) => page.evaluate((src) => {
  (0, eval)(`(${src})`)();
  sessionOf("det").tmux_pane = "%2";
  setActive(null, "det");
  openTerm("det");
}, before.toString());

test("typing refused in the last session's terminal says nothing over the next one's", async ({ cue, page }) => {
  await openTerminal(page);
  await cue.override("term_write", () => new Promise((_, refuse) => setTimeout(() => refuse("its terminal isn't responding"), 300)));
  await page.keyboard.type("a");
  await switchTo(page, () => {});
  await expect.poll(async () => (await cue.calls("term_write")).some((c) => "error" in c)).toBe(true);
  await page.waitForFunction(() => termSid === "det");
  await expect(page.locator(".term-stalled")).toHaveCount(0);
});
