// Esc, one step a press, in the order the window takes it (app/keys.js KEY_STEPS): what's over the window first (the
// pop-ups: + New Desk's sheet and menu, the diff overlay, the file viewer), then the crew's tray and page, the
// Settings chooser, the Inbox, the Desk's away sheet, an add-on's panel, the Desk, the dialogs (delete, quit, How work
// lands), then the Board's own (the "/" menu, lists, a picture, btw, a sheet, the open session). Each test opens one
// state (or two, stacked), presses Esc and checks what closed and what stayed. These write down how Esc behaved before
// the listeners became one chain (refactor step 3e), so the chain keeps every step; since #521 an Esc that closes the
// away sheet or an add-on's panel stops there, never reaching the Desk's open item.
import { test, expect, openDesk } from "./cue.mjs";

const esc = (page) => page.keyboard.press("Escape");
const get = (page, fn) => page.evaluate(fn);
/** The Desk with item d5's page open on its right. */
async function deskItem(page, cue) {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  await expect(page.locator(".dk-right [data-desk-page]")).toHaveCount(1);
}
const itemOpen = (page) => get(page, () => deskOpen?.kind === "item" && deskOpen.id === "d5");
const FILES = [{ path: "a.js", add: 1, del: 0, lines: [] }, { path: "b.js", add: 2, del: 1, lines: [] }];

// ---------- over everything: the pop-ups ----------

test("a sheet over a Desk item: the first Esc closes the sheet only, the item stays", async ({ page, cue }) => {
  await deskItem(page, cue);
  await page.evaluate(() => { sheet = "settings"; renderMain(); });
  await esc(page);
  expect(await get(page, () => sheet)).toBeNull();
  expect(await itemOpen(page)).toBe(true);
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
});

test("the diff overlay over a Desk item: Esc closes the overlay only", async ({ page, cue }) => {
  await deskItem(page, cue);
  await page.evaluate((f) => diffOverlay("Changes", "", f), FILES);
  await expect(page.locator(".dov")).toHaveCount(1);
  // Its arrows are its own.
  await page.keyboard.press("ArrowDown");
  expect(await get(page, () => dov.at)).toBe(1);
  await esc(page);
  await expect(page.locator(".dov")).toHaveCount(0);
  expect(await itemOpen(page)).toBe(true);
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
});

test("the file viewer over a Desk item: Esc closes the viewer only", async ({ page, cue }) => {
  await deskItem(page, cue);
  await page.evaluate(() => fileView("/Users/me/code/cue/a.js", "host"));
  await expect(page.locator(".fv")).toHaveCount(1);
  await esc(page);
  await expect(page.locator(".fv")).toHaveCount(0);
  expect(await itemOpen(page)).toBe(true);
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
});

test("+ New Desk's sheet: Esc closes its Project list first, then the sheet; the item under it stays", async ({ page, cue }) => {
  await deskItem(page, cue);
  await page.evaluate(() => deskNewOpen());
  await expect.poll(() => get(page, () => !!ndEl?.isConnected)).toBe(true);
  await page.evaluate(() => { ndDrop = true; ndPickDraw(); });
  await esc(page);
  expect(await get(page, () => [ndDrop, !!ndEl?.isConnected])).toEqual([false, true]);
  await esc(page);
  expect(await get(page, () => !!ndEl?.isConnected)).toBe(false);
  expect(await itemOpen(page)).toBe(true);
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
});

test("+ New's menu over the diff overlay: Esc closes the menu, then the overlay, then the Desk's turn", async ({ page, cue }) => {
  await deskItem(page, cue);
  await page.evaluate((f) => { diffOverlay("Changes", "", f); newMenuOpen = true; renderMain(); }, FILES);
  await esc(page);
  expect(await get(page, () => [newMenuOpen, !!dov])).toEqual([false, true]);
  await esc(page);
  expect(await get(page, () => !!dov)).toBe(false);
  expect(await itemOpen(page)).toBe(true);
});

// ---------- the Desk ----------

test("the Desk's away sheet: Esc closes it only; the item's holder isn't touched (#521)", async ({ page, cue }) => {
  await deskItem(page, cue);
  // Away on: its switch opens the sheet (from off, one click turns it on instead: #612).
  await page.evaluate(() => { deskOf("cue").away = { on: true, active: true, ended: "", since: 1, until: null, budget: null, spent: 0, rules: {} }; deskAwayOpen("cue"); });
  await expect.poll(() => get(page, () => !!deskAwayEdit)).toBe(true);
  await esc(page);
  expect(await get(page, () => deskAwayEdit)).toBeNull();
  expect(await itemOpen(page)).toBe(true);
  // That Esc armed nothing: one more only arms Stop, the next stops the holder.
  await esc(page);
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
  await esc(page);
  await cue.called("interrupt_session", { sessionId: "host" });
});

test("the Desk's progress panel closes before the item; then the item's holder (Esc twice stops it)", async ({ page, cue }) => {
  await deskItem(page, cue);
  await page.evaluate(() => { deskProgShown = true; renderMain(); });
  await esc(page);
  expect(await get(page, () => deskProgShown)).toBe(false);
  expect(await itemOpen(page)).toBe(true);
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
  await esc(page);
  await esc(page);
  await cue.called("interrupt_session", { sessionId: "host" });
});

test("a Desk item with nothing at work: Esc closes its page", async ({ page, cue }) => {
  await deskItem(page, cue);
  await cue.change((st) => { st.sessions.find((s) => s.session_id === "host").state = "waiting"; });
  await expect.poll(() => get(page, () => sessionOf("host").state)).toBe("waiting");
  await page.locator("body").click({ position: { x: 1, y: 1 } }).catch(() => {});
  await page.evaluate(() => document.activeElement?.blur());
  await esc(page);
  expect(await itemOpen(page)).toBe(false);
});

test("Esc while composing (IME) in the Desk item's box: the Desk doesn't take it, the Board's Esc blurs the box", async ({ page, cue }) => {
  await deskItem(page, cue);
  const box = page.locator(".dk-right .dk-box textarea");
  await box.focus();
  await box.evaluate((t) => t.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", isComposing: true, bubbles: true, cancelable: true })));
  expect(await itemOpen(page)).toBe(true);
  await expect(box).not.toBeFocused();
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
  // No Stop armed either: one real Esc now only arms it.
  await esc(page);
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
});

// ---------- dialogs and an add-on's panel ----------

test("the delete dialog over a Desk item: Esc says no, the item stays", async ({ page, cue }) => {
  await deskItem(page, cue);
  await page.evaluate(() => { deleteAsk = "host"; renderMain(); });
  await esc(page);
  expect(await get(page, () => deleteAsk)).toBeNull();
  expect(await itemOpen(page)).toBe(true);
  expect(await cue.calls("delete_session")).toHaveLength(0);
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
});

test("the quit dialog: Esc keeps Cue open; the sheet under it stays", async ({ page, cue }) => {
  await cue.open("board-waiting");
  await page.evaluate(() => { sheet = "settings"; quitAsk = { why: "quit", list: [] }; renderMain(); });
  await esc(page);
  expect(await get(page, () => [quitAsk, sheet])).toEqual([null, "settings"]);
});

test("an add-on's panel on the Board: Esc closes it, and the Board's own Esc doesn't run (the live list stays)", async ({ page, cue }) => {
  await cue.open("board-waiting");
  await page.evaluate(() => { liveOpen = true; renderMain(); const el = document.createElement("div"); document.body.append(el); extPanel = { name: "x", el }; });
  await esc(page);
  expect(await get(page, () => [extPanel, liveOpen])).toEqual([null, true]);
  await esc(page);
  expect(await get(page, () => liveOpen)).toBe(false);
});

test("an add-on's panel on a Desk item: Esc closes the panel only; the item and its holder stay as they were (#521)", async ({ page, cue }) => {
  await deskItem(page, cue);
  await page.evaluate(() => { const el = document.createElement("div"); document.body.append(el); extPanel = { name: "x", el }; });
  await esc(page);
  expect(await get(page, () => extPanel)).toBeNull();
  expect(await itemOpen(page)).toBe(true);
  // That Esc armed nothing: one more only arms Stop, the next stops the holder.
  await esc(page);
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
  await esc(page);
  await cue.called("interrupt_session", { sessionId: "host" });
});

test("Settings → Insights' chooser: Esc closes the chooser, not the sheet; then the sheet", async ({ page, cue }) => {
  await cue.open("board-waiting");
  await page.evaluate(() => { sheet = "settings"; insChoosing = "mac|claude"; renderMain(); });
  await esc(page);
  expect(await get(page, () => [insChoosing, sheet])).toEqual(["", "settings"]);
  await esc(page);
  expect(await get(page, () => sheet)).toBeNull();
});

// ---------- the crew ----------

test("the crew's tray: a member's menu closes first, then the tray; the lead's chat stays", async ({ page, cue }) => {
  await cue.open("world-team");
  await page.locator('.nrow[data-big="n1793984390004"]').click();
  await expect(page.locator(".crew-tray.open")).toHaveCount(1);
  await page.evaluate(() => document.activeElement?.blur());
  await page.evaluate(() => { mateMenu = "lead|x"; renderMain(); });
  await esc(page);
  expect(await get(page, () => [mateMenu, mateOpenLeads.has("lead")])).toEqual([null, true]);
  await esc(page);
  expect(await get(page, () => [mateOpenLeads.has("lead"), active?.sid])).toEqual([false, "lead"]);
});

test("the crew's tray under a sheet: Esc closes the sheet, the tray stays", async ({ page, cue }) => {
  await cue.open("world-team");
  await page.locator('.nrow[data-big="n1793984390004"]').click();
  await page.evaluate(() => { sheet = "settings"; renderMain(); });
  await esc(page);
  expect(await get(page, () => [sheet, mateOpenLeads.has("lead")])).toEqual([null, true]);
});

test("the crew's page: Esc closes btw first, then the page; a sheet over it closes before either", async ({ page, cue }) => {
  await cue.open("world-team");
  await page.evaluate(() => crewPageOpen("lead"));
  await expect(page.locator(".crew-page")).toHaveCount(1);
  await page.evaluate(() => { document.activeElement?.blur(); sheet = "settings"; renderMain(); });
  await esc(page);
  expect(await get(page, () => [sheet, crewPageSid])).toEqual([null, "lead"]);
  await page.evaluate(() => { document.activeElement?.blur(); btwFor = "lead"; renderMain(); });
  await esc(page);
  expect(await get(page, () => [btwFor, crewPageSid])).toEqual([null, "lead"]);
  await page.evaluate(() => document.activeElement?.blur());
  await esc(page);
  await expect(page.locator(".crew-page")).toHaveCount(0);
});

// ---------- the Inbox ----------

test("the Inbox: Esc closes the Snooze menu, and then nothing else that press", async ({ page, cue }) => {
  await cue.open("board-waiting");
  await page.evaluate(() => { view = "inbox"; renderMain(); inboxOpen("relay"); inboxSnoozeAt = "relay"; renderMain(); document.activeElement?.blur(); });
  await esc(page);
  expect(await get(page, () => [inboxSnoozeAt, inboxSid, view])).toEqual(["", "relay", "inbox"]);
});

// ---------- the Board's own ----------

test("a picture full size over a sheet: Esc closes the picture, then the sheet", async ({ page, cue }) => {
  await cue.open("board-waiting");
  await page.evaluate(() => { sheet = "settings"; lightbox = { srcs: ["data:image/png;base64,AA=="], i: 0 }; renderMain(); });
  await esc(page);
  expect(await get(page, () => [lightbox, sheet])).toEqual([null, "settings"]);
  await esc(page);
  expect(await get(page, () => sheet)).toBeNull();
});

test("the Board: the live list closes before a sheet's Esc; btw closes before the open session's", async ({ page, cue }) => {
  await cue.open("board-waiting");
  await page.locator('.nrow[data-big="n1"]').click();
  await page.evaluate(() => { document.activeElement?.blur(); liveOpen = true; btwFor = "relay"; renderMain(); });
  await esc(page);
  expect(await get(page, () => [liveOpen, btwFor])).toEqual([false, "relay"]);
  await esc(page);
  expect(await get(page, () => btwFor)).toBeNull();
  expect(await get(page, () => active?.sid)).toBe("relay");
});

// ---------- the mini window ----------

test("mini: a Desk row's answer box closes on Esc, the Desk tab stays (an Esc with nothing focused doesn't reach the chat)", async ({ page, cue }) => {
  await cue.openMini("desk", "rail");
  await page.locator("[data-out]").click();
  await page.locator(".m-chat .m-dk").waitFor();
  const row = page.locator('.m-dk-it[data-mdesk-item="d1"]');
  await row.locator("[data-mdk-answer]").click();
  await row.locator("textarea[data-mdk-box]").focus();
  await esc(page);
  await expect(row.locator("textarea[data-mdk-box]")).toHaveCount(0);
  await expect(page.locator(".m-chat .m-dk")).toHaveCount(1);
  expect(await get(page, () => document.activeElement === document.body)).toBe(true);
  await esc(page);
  await expect(page.locator(".m-chat .m-dk")).toHaveCount(1);
  // From inside the chat (a row's button focused), Esc folds it.
  await page.locator(".m-chat .m-dk button").first().focus();
  await esc(page);
  await page.locator("#mini .m-tuck").waitFor();
});

test("mini: the crew's tray takes the first Esc, then the chat folds", async ({ page, cue }) => {
  await cue.openMini("world-team", "rail");
  await page.locator("[data-out]").click();
  await page.locator('.m-tab.on[data-mopen="lead"]').waitFor();
  await page.evaluate(() => { mateOpenLeads.add("lead"); renderMain(); });
  await expect(page.locator(".m-chat .crew-tray.open")).toHaveCount(1);
  await page.locator('.m-chat textarea[data-text="s:lead"]').focus();
  await esc(page);
  expect(await get(page, () => mateOpenLeads.has("lead"))).toBe(false);
  await expect(page.locator(".m-chat")).toHaveCount(1);
  await esc(page);
  await page.locator("#mini .m-tuck").waitFor();
});
