// Enter and the cursor in every message box (refactor step 3f, one Enter and one focus). Per box: Enter while an
// input method is composing (isComposing, or WebKit's keyCode 229) never sends, ⇧↵ makes a new line, ↵ sends where
// that box sends. Then where the cursor lands after the main actions: opening a session or a Desk item, Send back
// opening and closing, the mini window opening. These write down how each box
// behaved before Enter and focus went through one path each, so moving a box keeps it.
import { test, expect, openDesk } from "./cue.mjs";

const get = (page, fn, arg) => page.evaluate(fn, arg);
/** A keydown of Enter dispatched on `box` as an input method sends it mid-composition: `how` "composing"
 *  (isComposing) or "229" (keyCode 229, isComposing false: WebKit's IME Enter). */
const imeEnter = (box, how) => box.evaluate((t, how) => {
  const e = new KeyboardEvent("keydown", { key: "Enter", code: "Enter", isComposing: how === "composing", bubbles: true, cancelable: true });
  if (how === "229") Object.defineProperty(e, "keyCode", { get: () => 229 });
  t.dispatchEvent(e);
}, how);
/** All calls of `cmd` so far. */
const count = async (cue, cmd) => (await cue.calls(cmd)).length;

/** The Enter keys in `box`: `text` typed in, Enter while composing (both kinds) sends nothing and changes nothing, ⇧↵
 *  adds a new line and sends nothing, then ↵ sends: a call of `cmd` with `args`. `ime` the kinds of composing Enter. */
async function enterKeys(page, cue, box, cmd, args, { text = "keep the old one", ime = ["composing", "229"] } = {}) {
  await box.focus();
  await box.fill(text);
  const before = await count(cue, cmd);
  for (const how of ime) await imeEnter(box, how);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
  expect(await count(cue, cmd), "Enter while composing sends nothing").toBe(before);
  await expect(box).toHaveValue(text);
  await expect(box).toBeFocused();
  await box.press("Shift+Enter");
  await expect(box).toHaveValue(`${text}\n`);
  expect(await count(cue, cmd), "⇧↵ sends nothing").toBe(before);
  await box.press("Enter");
  await cue.called(cmd, args);
}

// ---------- the Board ----------

test("the Board's box: composing Enter never sends, ⇧↵ a new line, ↵ sends to the session", async ({ page, cue }) => {
  await cue.open("board-waiting");
  await page.locator('.working[data-session="alerts"]').first().click();
  const box = page.locator('.active-pane textarea[data-text="s:alerts"]');
  // Opening a session: its box takes the cursor.
  await expect(box).toBeFocused();
  await enterKeys(page, cue, box, "send_to_session", { sessionId: "alerts", text: "keep the old one" });
});

test("btw's box: composing Enter never asks, ⇧↵ a new line, ↵ asks", async ({ page, cue }) => {
  await cue.open("board-waiting");
  await page.locator('.working[data-session="alerts"]').first().click();
  await page.evaluate(() => { btwFor = "alerts"; renderMain(); });
  const box = page.locator('textarea[data-text="btw:alerts"]');
  await enterKeys(page, cue, box, "btw", { sessionId: "alerts", question: "keep the old one" });
});

test("the forward note: composing Enter never sends, ⇧↵ a new line, ↵ sends with the message", async ({ page, cue }) => {
  await cue.open("board-waiting");
  await page.locator('.nrow[data-big="n1"]').click();
  await page.locator(".active-pane [data-fwd]").first().click();
  await page.locator('.fwd-pop [data-fwd-to="alerts"]').click();
  const box = page.locator('textarea[data-text="fwd"]');
  // Picking who it goes to: the note takes the cursor.
  await expect(box).toBeFocused();
  await enterKeys(page, cue, box, "send_to_session", { sessionId: "alerts" });
  const sent = (await cue.calls("send_to_session")).pop();
  expect(sent.args.text).toMatch(/^keep the old one\n\n\[Forwarded from the /);
});

// ---------- the Desk ----------

test("the Desk lead's box: composing Enter never sends, ⇧↵ a new line, ↵ sends to the lead", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const box = page.locator(".dk-right .foot:not(.held) .composer textarea");
  await enterKeys(page, cue, box, "desk_send", { project: "cue", text: "keep the old one" });
});

test("a Desk item's box: opening it puts the cursor there; composing Enter never sends, ⇧↵ a new line, ↵ sends", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  const box = page.locator(".dk-right .dk-box textarea");
  await expect(box).toBeFocused();
  await enterKeys(page, cue, box, "desk_send", { project: "cue", text: "About #d6 (Tidy the Settings copy): keep the old one" });
});

// The item opens in the lead chat's place (B2, #666) and the list stays, its row lit: the keys stay on that row, not
// the page's box.
test("a Desk item opened from the keyboard keeps the keys out of its box; one opened by a click puts them there", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const row = page.locator('.dk-items [data-desk-item="d6"]');
  await row.focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".dk-right .dk-pg h1")).toContainText("Tidy the Settings copy");
  await expect(row).toBeFocused();
  await expect(page.locator(".dk-right .dk-box textarea")).not.toBeFocused();
  await page.locator(".dk-right .dk-ph [data-desk-back]").click();
  await expect(page.locator(".dk-right .dk-pg[data-desk-page=\"item\"]")).toHaveCount(0);
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  await expect(page.locator(".dk-right .dk-box textarea")).toBeFocused();
});

test("Send back: opening it puts the cursor in its box; composing Enter never sends, ⇧↵ a new line, ↵ sends it back", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dk-items [data-desk-item="d2"]').click();
  const pane = page.locator(".dk-right");
  await pane.locator('.dk-panel[data-side-for] [data-desk-act="send_back"]').click();
  const box = pane.locator('textarea[data-text="back:d2"]');
  await expect(box).toBeFocused();
  // Send with nothing in it: says why, the cursor stays in the box.
  await pane.locator('[data-desk-act="back_go"][data-id="d2"]').click();
  await expect(page.locator(".toast").last()).toHaveText("Say why it goes back first");
  await expect(box).toBeFocused();
  // Send back again closes it.
  await pane.locator('.dk-panel[data-side-for] [data-desk-act="send_back"]').click();
  await expect(box).toHaveCount(0);
  await pane.locator('.dk-panel[data-side-for] [data-desk-act="send_back"]').click();
  await expect(box).toBeFocused();
  await enterKeys(page, cue, box, "desk_item_act", { id: "d2", act: "send_back", text: "keep the old one" });
  await expect(box).toHaveCount(0);
});

test("Go back on ticked decisions: the box takes the cursor; composing Enter never sends, ⇧↵ a new line, ↵ sends it (#522)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-tab="decisions"]').click();
  const right = page.locator(".dk-right");
  await right.locator('[data-desk-tick="c1"]').click();
  await right.locator('[data-desk-go-back="desk:cue"]').click();
  const box = right.locator('textarea[data-text="goback"]');
  await expect(box).toBeFocused();
  // Enter with nothing in it: says what's missing, sends nothing, the cursor stays.
  await box.press("Enter");
  await expect(page.locator(".toast").last()).toHaveText("Say what should happen instead first");
  expect(await count(cue, "desk_undo")).toBe(0);
  await expect(box).toBeFocused();
  await enterKeys(page, cue, box, "desk_undo", { decisionId: "c1", answer: "keep the old one" });
  await expect(box).toHaveCount(0);
});

// ---------- the mini window ----------


test("mini: a Desk row's answer box takes the cursor; composing Enter never sends, ⇧↵ a new line, ↵ answers", async ({ page, cue }) => {
  await cue.openMini("desk", "rail");
  await page.locator("[data-out]").click();
  await page.locator(".m-chat .m-dk").waitFor();
  const row = page.locator('.m-dk-it[data-mdesk-item="d1"]');
  await row.locator("[data-mdk-answer]").click();
  const box = row.locator("textarea[data-mdk-box]");
  await expect(box).toBeFocused();
  await enterKeys(page, cue, box, "respond", {});
});

test("mini: Send back's box in a Desk row: composing Enter never sends, ⇧↵ a new line, ↵ sends it back", async ({ page, cue }) => {
  await cue.openMini("desk", "rail");
  await page.locator("[data-out]").click();
  await page.locator(".m-chat .m-dk").waitFor();
  const row = page.locator('.m-dk-it[data-mdesk-item="d2"]');
  await row.locator('[data-desk-act="send_back"]').click();
  const box = row.locator('textarea[data-text="back:d2"]');
  await expect(box).toBeFocused();
  await enterKeys(page, cue, box, "desk_item_act", { id: "d2", act: "send_back", text: "keep the old one" });
});

test("mini: a session's chat opens with the cursor in its box; composing Enter never sends, ↵ sends", async ({ page, cue }) => {
  await cue.openMini("world-team", "rail");
  await page.locator("[data-out]").click();
  const box = page.locator('.m-chat textarea[data-text="s:lead"]');
  await expect(box).toBeFocused();
  // Its crew member's ask answered first (the lead's box sends nothing while it waits on you).
  const crew = page.locator(".m-chat [data-mt-open]").first();
  if ((await crew.getAttribute("aria-expanded")) !== "true") await crew.click();
  await page.locator('.m-chat .mt-row.need [data-act="allow"]').click();
  await cue.called("respond", { id: "n1793984390004" });
  await enterKeys(page, cue, box, "send_to_session", { sessionId: "lead", text: "keep the old one" });
});
