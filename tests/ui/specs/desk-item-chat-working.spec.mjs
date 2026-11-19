// After you send a note on an item's page, its chat shows its holder at work on it ("Working…") till the reply comes
// (#407), as it did before the live chat (#395).
import { test, expect } from "./cue.mjs";

const openItem = async (page, id) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(`[data-desk-item="${id}"]`).click();
};
const turns = (cue, list) => cue.override("session_steps", ({ sessionId }, list) => ({ version: list.length, turns: sessionId === "host" ? list : [] }), list);

test("you send a note on an item's page: a Working… line shows in its chat till the reply comes", async ({ page, cue }) => {
  await cue.open("desk");
  const t0 = Date.now() - 120000, first = "Found it: the header centres on the wrong box.";
  // Its turn on the item ended a while ago and its reply is saved.
  await turns(cue, [{ at_ms: t0, prompt: "About #d5 (x): go on", more: 0, items: [{ t: "say", text: first, at_ms: t0 + 5000 }] }]);
  await cue.change((st, [t0, first]) => {
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true, stopped: true, said: [{ at: t0 + 6000, by: "host-lead", text: first, sid: "host" }], latest: first });
    st.sessions.find((x) => x.session_id === "host").state = "waiting";
  }, [t0, first]);
  await openItem(page, "d5");
  const thread = page.locator(".dk-right .dk-thread"), live = thread.locator(".dk-tm.live");
  await expect(thread.locator(".dk-tm").filter({ hasText: first })).toHaveCount(1);
  // You send a note; it goes to its session, which starts on it.
  await page.locator(".dk-right .dk-box textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  await expect(thread.locator(".dk-tm").filter({ hasText: "Check the narrow window too" })).toHaveCount(1);
  await cue.change((st) => {
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { stopped: false, now_on: true });
    st.sessions.find((x) => x.session_id === "host").state = "working";
  });
  // Before its new turn is read: at work, not the last reply again.
  await expect(live).toHaveCount(1);
  await expect(live).toContainText("Working…");
  await expect(thread.locator(".dk-tm").filter({ hasText: first })).toHaveCount(1);
  await expect(thread.locator(".dk-tm").last()).toHaveClass(/live/);
  // Its new turn read, nothing written yet: still at work.
  await turns(cue, [{ at_ms: t0, prompt: "About #d5 (x): go on", more: 0, items: [{ t: "say", text: first, at_ms: t0 + 5000 }] },
    { at_ms: Date.now(), prompt: "About #d5 (x): Check the narrow window too", more: 0, items: [] }]);
  await cue.change(() => {});
  await expect(live).toContainText("Working…");
  // Its words come: they show as it writes them.
  await turns(cue, [{ at_ms: Date.now() - 2000, prompt: "About #d5 (x): Check the narrow window too", more: 0, items: [{ t: "say", text: "The narrow window is fine too.", at_ms: Date.now() }] }]);
  await cue.change(() => {});
  await expect(live).toContainText("The narrow window is fine too.");
  await expect(live).not.toContainText("Working…");
});
