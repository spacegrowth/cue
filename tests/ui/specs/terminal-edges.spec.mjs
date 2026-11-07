// A session's terminal when it doesn't go as planned: an open that fails says why and can be tried again, a
// client that ends (ssh gone) closes it, an end from an earlier opening leaves the one open now alone, and
// the terminal (one, kept from opening to opening) shows nothing of the last session in the next.
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

/** The open terminal's screen and scrollback, as text. */
const screenText = (page) => page.evaluate(() => {
  const b = termObj?.buffer.active;
  return b ? Array.from({ length: b.length }, (_, i) => b.getLine(i)?.translateToString()).join("\n") : "";
});

/** Another session (det) in tmux, its chat and terminal opened as its strip's click opens them, after `before`
 *  runs in the window in the same turn. */
const switchTo = (page, before) => page.evaluate((src) => {
  (0, eval)(`(${src})`)();
  sessionOf("det").tmux_pane = "%2";
  setActive(null, "det");
  openTerm("det");
}, before.toString());

test("the next session's terminal never shows output the last one's hadn't drawn yet", async ({ page }) => {
  await openTerminal(page);
  await switchTo(page, () => termObj.write("LEFTOVER-FROM-RELAY\r\n"));
  await page.waitForFunction(() => termSid === "det" && termGen > 0);
  await expect.poll(() => screenText(page)).toContain("Do you want to proceed?");
  expect(await screenText(page)).not.toContain("LEFTOVER-FROM-RELAY");
});

test("typing refused in the last session's terminal says nothing over the next one's", async ({ cue, page }) => {
  await openTerminal(page);
  await cue.override("term_write", () => new Promise((_, refuse) => setTimeout(() => refuse("its terminal isn't responding"), 300)));
  await page.keyboard.type("a");
  await switchTo(page, () => {});
  await expect.poll(async () => (await cue.calls("term_write")).some((c) => "error" in c)).toBe(true);
  await page.waitForFunction(() => termSid === "det");
  await expect(page.locator(".term-stalled")).toHaveCount(0);
});

test("typing refused in an earlier opening of the same terminal says nothing over the one open now", async ({ cue, page }) => {
  await openTerminal(page);
  await cue.override("term_write", () => new Promise((_, refuse) => setTimeout(() => refuse("its terminal isn't responding"), 300)));
  await page.keyboard.type("a");
  await page.evaluate(() => { closeTerm(true); openTerm("relay"); });
  await expect.poll(async () => (await cue.calls("term_write")).some((c) => "error" in c)).toBe(true);
  await page.waitForFunction(() => termSid === "relay");
  await expect(page.locator(".term-stalled")).toHaveCount(0);
});

// Earlier messages come in a page at a time as you scroll up to the chat's top (ui/app/bind.js onChatScroll).
// The chat also moves on its own (its terminal opening above the box shrinks it, a redraw): that loads none,
// or a chat left open would page in its whole history, and every draw would lay all of it out again.
const chatAtTop = (page) => page.evaluate(() => { const c = document.querySelector("[data-chat]"); c.scrollTop = 0; c.dispatchEvent(new Event("scroll")); });
const pagesAsked = async (cue) => (await cue.calls("session_log_page")).length;

test("the chat moving on its own as its terminal opens loads no earlier messages", async ({ cue, page }) => {
  await openTerminal(page);
  await chatAtTop(page);
  expect(await pagesAsked(cue)).toBe(0);
});

test("scrolling up to the chat's top yourself loads earlier messages", async ({ cue, page }) => {
  await openTerminal(page);
  await page.locator("[data-chat]").hover();
  await page.mouse.wheel(0, -400);
  await chatAtTop(page);
  await expect.poll(() => pagesAsked(cue)).toBeGreaterThan(0);
});

test("a slow drag of the chat's scrollbar to its top loads earlier messages", async ({ cue, page }) => {
  await openTerminal(page);
  const chat = page.locator("[data-chat]");
  await chat.dispatchEvent("pointerdown");
  // Held longer than a press counts as moving the chat, still dragging.
  await page.waitForTimeout(1300);
  await chatAtTop(page);
  await expect.poll(() => pagesAsked(cue)).toBeGreaterThan(0);
  await chat.dispatchEvent("pointerup");
});

test("once a press on the chat is let go, the chat moving on its own loads nothing", async ({ cue, page }) => {
  await openTerminal(page);
  const chat = page.locator("[data-chat]");
  await chat.dispatchEvent("pointerdown");
  await chat.dispatchEvent("pointerup");
  await page.waitForTimeout(1300);
  await chatAtTop(page);
  expect(await pagesAsked(cue)).toBe(0);
});
