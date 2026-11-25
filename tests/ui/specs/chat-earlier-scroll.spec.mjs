// A session's chat loads earlier messages when you move it to its top yourself (a scroll, a drag of its scrollbar),
// never when it moves on its own (a redraw): that loads none, or a chat left open would page in its whole history,
// and every draw would lay all of it out again.
import { test, expect } from "./cue.mjs";

test.beforeEach(async ({ cue }) => { await cue.open("hosted-relay"); });

/** agent-relay's chat open. */
const openChat = (page) => page.locator('.nrow[data-big="n1"]').click();
const chatAtTop = (page) => page.evaluate(() => { const c = document.querySelector("[data-chat]"); c.scrollTop = 0; c.dispatchEvent(new Event("scroll")); });
const pagesAsked = async (cue) => (await cue.calls("session_log_page")).length;

test("scrolling up to the chat's top yourself loads earlier messages", async ({ cue, page }) => {
  await openChat(page);
  // The chat just opened is put at its end by code, and that pin's scroll events aren't you for 400 ms (ui/box/end.js):
  // a person's scroll comes after it.
  await page.waitForTimeout(500);
  await page.locator("[data-chat]").hover();
  await page.mouse.wheel(0, -400);
  // The wheel has moved the chat (it lands a moment after the call returns).
  await page.waitForFunction(() => { const c = document.querySelector("[data-chat]"); return c.scrollTop < c.scrollHeight - c.clientHeight - 1; });
  await chatAtTop(page);
  await expect.poll(() => pagesAsked(cue)).toBeGreaterThan(0);
});

test("a slow drag of the chat's scrollbar to its top loads earlier messages", async ({ cue, page }) => {
  await openChat(page);
  const chat = page.locator("[data-chat]");
  await chat.dispatchEvent("pointerdown");
  // Held longer than a press counts as moving the chat, still dragging.
  await page.waitForTimeout(1300);
  await chatAtTop(page);
  await expect.poll(() => pagesAsked(cue)).toBeGreaterThan(0);
  await chat.dispatchEvent("pointerup");
});

test("once a press on the chat is let go, the chat moving on its own loads nothing", async ({ cue, page }) => {
  await openChat(page);
  const chat = page.locator("[data-chat]");
  await chat.dispatchEvent("pointerdown");
  await chat.dispatchEvent("pointerup");
  await page.waitForTimeout(1300);
  await chatAtTop(page);
  expect(await pagesAsked(cue)).toBe(0);
});
