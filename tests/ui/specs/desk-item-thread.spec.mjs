// An item's page as its own thread (#276): each reply in full, with a link to that turn in its session; hand-offs and
// each Done as record lines; the turn its holder is writing, live, with its steps; and the box going straight to the
// holder, sent as about the item, with no copy to the lead.
import { test, expect } from "./cue.mjs";

const openItem = async (page, cue, id) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(`[data-desk-item="${id}"]`).click();
};

test("an item's replies show whole, each with a link to its turn, and a Done is a record line", async ({ page, cue }) => {
  await cue.open("desk");
  const long = `Here is what I found.\n\n${"Every line of it matters. ".repeat(80).trim()}\n\nThe last line.`;
  await cue.change((st, long) => {
    const at = Date.now();
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "to_check", now_on: false, latest: "", said: [{ at: at - 60000, by: "host-lead", text: long, sid: "host", done: true }] });
    st.sessions.find((x) => x.session_id === "host").state = "waiting";
  }, long);
  await openItem(page, cue, "d5");
  const pane = page.locator(".dk-right .dk-ipane");
  const reply = pane.locator(".dk-tm:not(.rec)").filter({ hasText: "Here is what I found." });
  await expect(reply).toContainText("The last line.");
  await expect(reply.locator("[data-desk-turn]")).toHaveText("This turn in host-lead ›");
  await expect(pane.locator(".dk-tm.rec").last()).toHaveText(/Done · host-lead said Feature #d5 is done|Done · host-lead said .*#d5 is done/);
  // The link opens that turn in its session's conversation.
  await reply.locator("[data-desk-turn]").click();
  await expect(page.locator(".convo-sheet")).toBeVisible();
  await expect(page.locator(".convo-sheet .sheet-head")).toContainText("← Desk");
});

test("the box on an item at work goes straight to its holder, sent as the item, and says when it took it", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true, said: [] }); st.sessions.find((x) => x.session_id === "host").state = "working"; });
  await openItem(page, cue, "d5");
  await expect(page.locator(".dk-right .dk-box .dk-pre")).toHaveText("To host-lead, who has this item · sent as #d5");
  await expect(page.locator(".dk-right .dk-box textarea")).toHaveAttribute("placeholder", "Write to it here…");
  await page.locator(".dk-right .dk-box textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  const sent = await cue.called("send_to_session");
  expect(sent.args.sessionId).toBe("host");
  expect(sent.args.text).toMatch(/^About #d5 \(.+\): Check the narrow window too$/);
  expect(await cue.calls("desk_send")).toHaveLength(0);
  // Its session took it: the receipt under your words.
  await cue.change((st, text) => {
    const s = st.sessions.find((x) => x.session_id === "host"), at = Date.now();
    const e = (s.thread || []).findLast((x) => x.role === "you" && x.text.includes(text));
    if (e) e.delivery = [{ at_ms: at, what: "taken" }];
    else (s.thread ||= []).push({ at_ms: at, role: "you", text, delivery: [{ at_ms: at, what: "taken" }] });
  }, sent.args.text);
  await expect(page.locator(".dk-right .dk-thread .dk-rc").last()).toContainText("host-lead took it");
});

test("while its holder works on it, the item shows the turn it's writing, whole, with its steps in a line", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.override("session_steps", ({ sessionId }, at = Date.now()) => (sessionId !== "host" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at - 30000, prompt: "About #d5 (x): go on", more: 0, items: [
    { t: "step", id: "s1", kind: "read", verb: "Read", subject: "header.js", mono: true, result: "40 lines", bad: false, done: true, at_ms: at - 20000 },
    { t: "say", text: "Found it: the header **centres** on the wrong box.", at_ms: at - 15000 },
    { t: "step", id: "s2", kind: "run", verb: "Ran", subject: "npm test", mono: true, result: "", bad: false, done: false, at_ms: at - 5000 }] }] }));
  await cue.change((st) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true, said: [] }); st.sessions.find((x) => x.session_id === "host").state = "working"; });
  await openItem(page, cue, "d5");
  const live = page.locator(".dk-right .dk-tm.live");
  await expect(live.locator(".dk-stp")).toHaveText("read 1 file · now: Ran npm test");
  await expect(live.locator(".dk-md")).toContainText("Found it: the header centres on the wrong box.");
  await expect(live.locator(".dk-ttag")).toHaveText("Writing");
  await expect(page.locator(".dk-right .dk-pg")).toContainText("Watch host-lead");
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    if (process.env.SHOT_DIR) await page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-276-${theme}.png` });
  }
});
