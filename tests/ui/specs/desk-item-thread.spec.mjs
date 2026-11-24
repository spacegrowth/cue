// An item opens its holder's own chat (#276, #418, #744): each reply in full; the turn its holder is writing, live,
// with its steps; the box going straight to the holder, sent as about the item, with no copy to the lead; what a stop
// hook made it write folded under the reply.
import { test, expect, openDesk } from "./cue.mjs";

const openItem = async (page, cue, id) => {
  await openDesk(page);
  await page.locator(`.dkn-items [data-desk-item="${id}"]`).first().click();
};
/** The item's chat: its holder's, in the session page that opened in the middle. */
const chatOf = (page, sid = "host") => page.locator(`.dk-right .dkn-sp[data-desk-page="session"] [data-desk-chat="${sid}"]`);
// host-lead's chat: you, about #d5, then each of `replies` in turn, a minute apart.
const replies = (cue, list, state = "waiting") => cue.change((st, [list, state]) => {
  const at = Date.now() - 600000, s = st.sessions.find((x) => x.session_id === "host");
  Object.assign(s, { state, doing: "", thread: [{ role: "you", text: "About #d5 (x): go on", at_ms: at }, ...list.map((text, n) => ({ role: "agent", text, at_ms: at + (n + 1) * 60000 }))] });
  Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: state === "working" ? "working" : "to_check", now_on: state === "working", said: [], latest: "" });
}, [list, state]);
const reply = (page, words) => chatOf(page).locator(".cv-agent").filter({ hasText: words });

test("an item's replies show whole, and its Done is in Activity", async ({ page, cue }) => {
  await cue.open("desk");
  const long = `Here is what I found.\n\n${"Every line of it matters. ".repeat(80).trim()}\n\nThe last line.\n\nDONE: #d5`;
  await replies(cue, [long]);
  await openItem(page, cue, "d5");
  const r = reply(page, "Here is what I found.");
  await expect(r).toContainText("The last line.");
  await expect(r).not.toHaveClass(/folded/);
  // (Its DONE line is plain text in a session's chat since #744, as desk-layout.spec checks for IF PICKED and CHECK.)
  await page.evaluate(() => deskOpenActivity());
  await expect(page.locator(".dk-moves .dk-mv-ln .own-lines")).toContainText("Bug #d5 is done");
});

test("what a stop hook made it write after its reply folds under the reply, as the chat folds it (#310)", async ({ page, cue }) => {
  await cue.open("desk");
  // The turn's time fixed once: a turn that starts at another time each fetch is a new turn, folded again.
  const at = await page.evaluate(() => Date.now() - 600000);
  const hooked = (cue, on) => cue.override("session_steps", ({ sessionId }, [on, at]) => {
    return sessionId !== "host" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at, prompt: "About #d5 (x): go on", more: 0, ...(on ? { hooked_ms: at + 70000 } : {}), items: [
      { t: "step", id: "s1", kind: "read", verb: "Read", subject: "header.js", mono: true, result: "40 lines", bad: false, done: true, at_ms: at + 5000 },
      { t: "say", text: "Fixed the header.", at_ms: at + 60000 },
      ...(on ? [{ t: "say", text: "1. SHAPE: the checklist answer.", at_ms: at + 80000 }] : [])] }] };
  }, [on, at]);
  await hooked(cue, true);
  await replies(cue, ["Fixed the header."]);
  await openItem(page, cue, "d5");
  const ichat = chatOf(page), fold = ichat.locator(".stx-hook [data-hook-fold]");
  await expect(reply(page, "Fixed the header.")).toHaveCount(1);
  await expect(fold).toHaveText("After a stop hook ▾");
  await expect(ichat).not.toContainText("the checklist answer");
  await fold.click();
  await expect(ichat.locator(".stx-hook .follow-body ol li")).toHaveText("SHAPE: the checklist answer.");   // its markdown, a numbered list
  await expect(fold).toHaveText("After a stop hook ▴");
  // One with no hook has no fold.
  await hooked(cue, false);
  await cue.change(() => {});
  await expect(ichat.locator(".stx-hook")).toHaveCount(0);
});

test("the box on an item at work goes straight to its holder, sent as the item, and its chat says when it took it", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true, said: [] }); st.sessions.find((x) => x.session_id === "host").state = "working"; });
  await openItem(page, cue, "d5");
  await expect(page.locator(".dk-right .foot:not(.held) .composer .field-to1")).toHaveText("To host-lead");
  await expect(page.locator(".dk-right .foot:not(.held) .composer .field-to2")).toHaveText("#d5");
  await expect(page.locator(".dk-right .foot:not(.held) .composer textarea")).toHaveAttribute("placeholder", "Write to it here…");
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  await cue.called("desk_item_act", { id: "d5", act: "note", text: "Check the narrow window too" });
  // The window hears of it shortly after the call, as from the hub.
  const delivery = () => page.evaluate(() => deskOf("cue").decisions.find((c) => c.item_id === "d5" && c.delivery)?.delivery);
  await expect.poll(async () => (await delivery())?.session_id).toBe("host");
  const sent = { args: { text: (await delivery()).text } };
  expect(sent.args.text).toMatch(/^About #d5 \(.+\): Check the narrow window too$/);
  expect(await cue.calls("desk_send")).toHaveLength(0);
  // In its chat, not taken yet while its holder works: queued, it goes in when it's free (#531); taken, that goes.
  await cue.change((st, text) => {
    const s = st.sessions.find((x) => x.session_id === "host");
    s.held = null;
    (s.thread ||= []).push({ at_ms: Date.now(), role: "you", text, pending: true });
  }, sent.args.text);
  const mine = chatOf(page).locator(".cv-you").filter({ hasText: "Check the narrow window too" });
  await expect(mine).toHaveCount(1);
  await expect(mine.locator(".msg-line")).toHaveText("◌ Queued · goes in when it's free");
  await expect(mine).toHaveClass(/msg-queued/);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").thread.at(-1).pending = false; });
  await expect(mine).toHaveCount(1);
  // The same line as under any message (#422): delivered says who and how long ago (#744); its bubble filled.
  const line = mine.locator(".msg-line");
  await expect(line).toHaveText(/^You · .+ ago$/);
  if (process.env.SHOT_DIR) await line.locator("xpath=..").screenshot({ path: `${process.env.SHOT_DIR}/took.png` });
  // (Polled: a redraw between finding the line and reading it would leave a detached one, with no style.)
  await expect.poll(() => line.evaluate((e) => { const c = getComputedStyle(e); return [c.borderTopWidth, c.backgroundColor, c.cursor, e.tagName]; })).toEqual(["0px", "rgba(0, 0, 0, 0)", "auto", "DIV"]);
  await expect(mine).toHaveClass(/msg-delivered/);
});

test("while its holder works on it, the item's chat shows the turn it's writing, whole, with its steps folded", async ({ page, cue }) => {
  await cue.open("desk");
  // The turn's time fixed once: a turn that starts at another time each fetch is a new turn, folded again.
  await cue.override("session_steps", ({ sessionId }, at) => (sessionId !== "host" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at - 30000, prompt: "About #d5 (x): go on", more: 0, items: [
    { t: "step", id: "s1", kind: "read", verb: "Read", subject: "header.js", mono: true, result: "40 lines", bad: false, done: true, at_ms: at - 20000 },
    { t: "say", text: "Found it: the header **centres** on the wrong box.", at_ms: at - 15000 },
    { t: "step", id: "s2", kind: "run", verb: "Ran", subject: "npm test", mono: true, result: "", bad: false, done: false, at_ms: at - 5000 }] }] }), await page.evaluate(() => Date.now()));
  await cue.override("step_detail", ({ ids }) => Object.fromEntries(ids.map((id) => [id, id === "s2" ? { full: "npm test" } : { full: "/repo/header.js" }])));
  await cue.change((st) => {
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true, said: [] });
    Object.assign(st.sessions.find((x) => x.session_id === "host"), { state: "working", thread: [{ role: "you", text: "About #d5 (x): go on", at_ms: Date.now() - 30000 }] });
  });
  await openItem(page, cue, "d5");
  const live = chatOf(page).locator(".stx");
  await expect(live.locator(".stx-fold")).toContainText("Working · 2 steps");
  await expect(live.locator(".stx-fold .dot-live")).toHaveCount(1);
  await expect(live.locator(".stx-say")).toContainText("Found it: the header centres on the wrong box.");
  // Folded: no step drawn while it works (the line under the chat is its running line); the chip opens every step in place.
  await expect(live.locator(".stx-line")).toHaveCount(0);
  await live.locator(".stx-fold").click();
  await expect(live.locator(".stx-line")).toHaveText([/Read header\.js/, /Ran npm test/]);
  for (const theme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: theme });   // as the Mac switches it: a redraw puts Settings → Appearance (system) back over a data-theme set by hand
    if (process.env.SHOT_DIR) await page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-276-${theme}.png` });
  }
});
