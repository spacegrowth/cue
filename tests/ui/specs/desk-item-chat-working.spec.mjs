// After you send a note on an item's page, its chat shows its holder at work on it (its Working line) till the reply
// comes (#407), as it did before the live chat (#395), now in the holder's own chat (#418).
import { test, expect, openDesk } from "./cue.mjs";

const openItem = async (page, id) => {
  await openDesk(page);
  await page.locator(`[data-desk-item="${id}"]`).first().click();
};
const turns = (cue, list) => cue.override("session_steps", ({ sessionId }, list) => ({ version: list.length, turns: sessionId === "host" ? list : [] }), list);

test("you send a note on an item's page: a Working line shows in its chat till the reply comes", async ({ page, cue }) => {
  await cue.open("desk");
  const first = "Found it: the header centres on the wrong box.";
  // Its turn on the item ended a while ago and its reply is saved.
  const t0 = await page.evaluate(() => Date.now() - 120000);
  await turns(cue, [{ at_ms: t0, prompt: "About #d5 (x): go on", more: 0, items: [{ t: "say", text: first, at_ms: t0 + 5000 }] }]);
  await cue.change((st, [t0, first]) => {
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true, stopped: true, said: [{ at: t0 + 6000, by: "host-lead", text: first, sid: "host" }], latest: first });
    const s = st.sessions.find((x) => x.session_id === "host");
    Object.assign(s, { state: "waiting", doing: "", thread: [{ role: "you", text: "About #d5 (x): go on", at_ms: t0 }, { role: "agent", text: first, at_ms: t0 + 5000 }] });
  }, [t0, first]);
  await openItem(page, "d5");
  const ichat = page.locator(".dk-right .dk-ichat"), where = ichat.locator('[data-key="where"]');
  await expect(ichat.locator(".cv-agent").filter({ hasText: first })).toHaveCount(1);
  await expect(where).not.toContainText("Working");
  // You send a note; it goes to its session, which starts on it.
  await page.locator(".dk-right .dk-box textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  await expect(ichat.locator(".cv-you").filter({ hasText: "Check the narrow window too" })).toHaveCount(1);
  await cue.change((st) => {
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { stopped: false, now_on: true });
    Object.assign(st.sessions.find((x) => x.session_id === "host"), { state: "working", doing: "" });
  });
  // Before its new turn is read: at work, the last line of its chat, not the last reply again.
  await expect(where).toHaveCount(1);
  await expect(where).toContainText("Working");
  await expect(where.locator(".dot-live")).toHaveCount(1);
  await expect(ichat.locator(".cv-agent").filter({ hasText: first })).toHaveCount(1);
  await expect(ichat.locator(":scope > [data-key]").last()).toHaveAttribute("data-key", "where");
  // Its new turn read, nothing written yet: still at work.
  const t1 = await page.evaluate(() => Date.now());
  await turns(cue, [{ at_ms: t0, prompt: "About #d5 (x): go on", more: 0, items: [{ t: "say", text: first, at_ms: t0 + 5000 }] },
    { at_ms: t1, prompt: "About #d5 (x): Check the narrow window too", more: 0, items: [] }]);
  await cue.change(() => {});
  await expect(where).toContainText("Working");
  // Its words come: they show, and while it still works the Working line stays under them.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").thread.push({ role: "agent", text: "The narrow window is fine too.", at_ms: Date.now() }); });
  await expect(ichat.locator(".cv-agent").filter({ hasText: "The narrow window is fine too." })).toHaveCount(1);
  await expect(ichat.locator(":scope > [data-key]").last()).toHaveAttribute("data-key", "where");
  // The turn over: no Working line.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").state = "waiting"; });
  await expect(where).not.toContainText("Working");
});
