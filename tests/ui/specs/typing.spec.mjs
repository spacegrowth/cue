// Typing to sessions: a reply to a finished turn, a message to a working one (kept in Cue until its turn ends,
// sent now with ⌘↵, taken back with Esc), quick phrases, "!" lines, and drafts.
import { test, expect } from "./cue.mjs";

/** A session's message box in the Active pane. */
const box = (page, sid) => page.locator(`.active-pane textarea[data-text="s:${sid}"]`);
const openCard = (page, id) => page.locator(`.nrow[data-big="${id}"]`).click();
/** A working session's chat, from its card under Sessions. */
const openWorking = (page, sid) => page.locator(`.working[data-session="${sid}"]`).first().click();

test.beforeEach(async ({ cue }) => { await cue.open("board-waiting"); });

test("Enter in a finished turn's box replies with what's typed", async ({ cue, page }) => {
  await openCard(page, "n1");
  await box(page, "relay").pressSequentially("carry it over to the successor");
  await box(page, "relay").press("Enter");
  const c = await cue.called("reply", { id: "n1" });
  expect(c.args.text).toBe("carry it over to the successor");
  await expect(page.locator('.nrow[data-big="n1"]')).toHaveCount(0);
  await expect(box(page, "relay")).toHaveValue("");
});

test("Send replies, and Shift+Enter keeps the lines together", async ({ cue, page }) => {
  await openCard(page, "n2");
  await box(page, "det").pressSequentially("tune the stop");
  await box(page, "det").press("Shift+Enter");
  await box(page, "det").pressSequentially("then rerun");
  expect(await cue.calls("reply")).toEqual([]);
  await page.locator(".active-pane .btn.send").click();
  expect((await cue.called("reply", { id: "n2" })).args.text).toBe("tune the stop\nthen rerun");
});

test("Nothing typed: Enter sends nothing", async ({ cue, page }) => {
  await openCard(page, "n1");
  await box(page, "relay").press("Enter");
  await expect(page.locator(".active-pane .btn.send")).toBeDisabled();
  expect(await cue.calls("reply")).toEqual([]);
});

test("A quick phrase replies with its words", async ({ cue, page }) => {
  await openCard(page, "n1");
  await page.locator('.active-pane .qp[data-phrase="Go ahead"]').click();
  expect((await cue.called("reply", { id: "n1" })).args.text).toBe("Go ahead");
});

test("To a working session: kept in Cue until its turn ends", async ({ cue, page }) => {
  await openWorking(page, "eng");
  await box(page, "eng").pressSequentially("also keep the old key valid for a day");
  await box(page, "eng").press("Enter");
  const c = await cue.called("send_to_session", { sessionId: "eng" });
  expect(c.args).toMatchObject({ text: "also keep the old key valid for a day", now: false });
  expect(c.result).toMatch(/^kept in Cue/);
  await expect(page.locator(".active-pane")).toContainText("also keep the old key valid for a day");
});

test("⌘↵ sends to a working session now", async ({ cue, page }) => {
  await openWorking(page, "alerts");
  await box(page, "alerts").pressSequentially("stop, check the retry count first");
  await box(page, "alerts").press("Meta+Enter");
  expect((await cue.called("send_to_session", { sessionId: "alerts" })).args).toMatchObject({ text: "stop, check the retry count first", now: true });
});

test("Esc takes a kept message back into the box", async ({ cue, page }) => {
  await openWorking(page, "eng");
  await box(page, "eng").pressSequentially("use the new pool");
  await box(page, "eng").press("Enter");
  await cue.called("send_to_session", { sessionId: "eng" });
  await expect(page.locator(".active-pane")).toContainText("use the new pool");
  await page.keyboard.press("Escape");
  await cue.called("unhold_message", { sessionId: "eng" });
  await expect(box(page, "eng")).toHaveValue("use the new pool");
});

test("A \"! \" line runs in the session's folder, not sent to it", async ({ cue, page }) => {
  await openCard(page, "n1");
  await box(page, "relay").pressSequentially("! git status");
  await box(page, "relay").press("Enter");
  expect((await cue.called("run_line", { sessionId: "relay" })).args.line).toBe("git status");
  await expect(page.locator(".active-pane .ran-out")).toContainText("nothing to commit, working tree clean");
  expect(await cue.calls("reply")).toEqual([]);
});

test("\"!\" with no space after it is a message", async ({ cue, page }) => {
  await openCard(page, "n1");
  await box(page, "relay").pressSequentially("!important: keep the footer");
  await box(page, "relay").press("Enter");
  expect((await cue.called("reply", { id: "n1" })).args.text).toBe("!important: keep the footer");
  expect(await cue.calls("run_line")).toEqual([]);
});

test("What's typed is kept as the box's draft", async ({ cue, page }) => {
  await openCard(page, "n1");
  await box(page, "relay").pressSequentially("half-written");
  await cue.called("set_draft", { key: "s:relay", text: "half-written" });
});

test("Typing in a box doesn't trigger the board's keys", async ({ cue, page }) => {
  await openCard(page, "n4");
  await page.keyboard.press("r");
  await page.locator('.active-pane textarea[data-text="n4"]').pressSequentially("a d h");
  expect(await cue.calls("respond")).toEqual([]);
  await expect(page.locator('[data-view="board"]')).toHaveClass(/on/);
});
