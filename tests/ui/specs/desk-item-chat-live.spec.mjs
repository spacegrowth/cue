// An item's chat feels live (#395, option A of docs/desk-item-chat-live.md): its messages are kept across draws and
// only what changed is touched; it follows its end only while you're there; what you send shows at once; the reply
// being written stays whole as its turn ends, till the saved one is in; a turn typed with no item number counts when
// its session holds only this item; and the live words leave out the Desk's own lines, as the saved reply does.
import { test, expect } from "./cue.mjs";

const openItem = async (page, id) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(`[data-desk-item="${id}"]`).click();
};
// The holder's newest turn, as its steps feed has it: started on `prompt`, having written `words`.
const turnWith = (cue, prompt, words) => cue.override("session_steps", ({ sessionId }, [prompt, words]) => (sessionId !== "host" ? { version: 1, turns: [] }
  : { version: 1, turns: [{ at_ms: Date.now() - 30000, prompt, more: 0, items: [{ t: "say", text: words, at_ms: Date.now() - 1000 }] }] }), [prompt, words]);
const atWork = (cue) => cue.change((st) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true, said: [], latest: "" }); st.sessions.find((x) => x.session_id === "host").state = "working"; });

test("the reply being written grows in place: its message is the same element, and the page stays at its end", async ({ page, cue }) => {
  await cue.open("desk");
  const long = (n) => Array.from({ length: n }, (_, k) => `Line ${k + 1} of what it found.`).join("\n\n");
  await turnWith(cue, "About #d5 (x): go on", long(10));
  await atWork(cue);
  await openItem(page, "d5");
  const live = page.locator(".dk-right .dk-tm.live"), pane = page.locator(".dk-right .dk-ipane");
  await expect(live).toContainText("Line 10 of what it found.");
  await live.evaluate((e) => { window.keptRow = e; });
  const atEnd = () => pane.evaluate((p) => p.scrollHeight - p.scrollTop - p.clientHeight < 24);
  await expect.poll(atEnd).toBe(true);
  // More words: the same message, longer, and the page still at its end.
  await turnWith(cue, "About #d5 (x): go on", long(40));
  await cue.change(() => {});
  await expect(live).toContainText("Line 40 of what it found.");
  expect(await live.evaluate((e) => e === window.keptRow)).toBe(true);
  await expect(page.locator(".dk-right .dk-thread .dk-tm").first()).toHaveAttribute("data-key", "first");
  await expect.poll(atEnd).toBe(true);
  // Scrolled up to read: more words don't move it.
  await pane.evaluate((p) => { p.scrollTop = 0; });
  await turnWith(cue, "About #d5 (x): go on", long(60));
  await cue.change(() => {});
  await expect(live).toContainText("Line 60 of what it found.");
  expect(await pane.evaluate((p) => p.scrollTop)).toBe(0);
});

test("what you send on an item's page shows at once, before its session has it", async ({ page, cue }) => {
  await cue.open("desk");
  await atWork(cue);
  // The send never comes back while we look.
  await cue.override("send_to_session", () => new Promise(() => {}));
  await openItem(page, "d5");
  await page.locator(".dk-right .dk-box textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  await expect(page.locator(".dk-right .dk-thread .dk-tm").filter({ hasText: "Check the narrow window too" })).toHaveCount(1);
});

test("as a turn ends, its reply stays whole till the saved one is in, then that one shows", async ({ page, cue }) => {
  await cue.open("desk");
  const words = "Found it: the header centres on the wrong box.\n\nFixed it and ran the tests; all pass.";
  await turnWith(cue, "About #d5 (x): go on", words);
  await atWork(cue);
  await openItem(page, "d5");
  const thread = page.locator(".dk-right .dk-thread");
  await expect(thread.locator(".dk-tm.live")).toContainText("all pass.");
  // Stopped, its reply not saved yet: its last line only would show; the whole reply stays.
  await cue.change((st) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { now_on: false, stopped: true, latest: "Not committed: ui/header.js" }); st.sessions.find((x) => x.session_id === "host").state = "waiting"; });
  await expect(thread).not.toContainText("Not committed");
  await expect(thread.locator(".dk-tm").last()).toContainText("Found it: the header centres on the wrong box.");
  await expect(thread.locator(".dk-tm").last()).toContainText("all pass.");
  await expect(thread.locator(".dk-tm.live")).toHaveCount(0);
  // The saved reply in: shown once.
  await cue.change((st, words) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").said = [{ at: Date.now(), by: "host-lead", text: words, sid: "host" }]; }, words);
  await expect(thread.locator("[data-desk-turn]")).toHaveCount(1);
  await expect(thread.locator(".dk-tm").filter({ hasText: "all pass." })).toHaveCount(1);
});

test("a turn typed in the session's own chat, naming no item, shows on the one item it holds", async ({ page, cue }) => {
  await cue.open("desk");
  await turnWith(cue, "also check the footer", "The footer is fine.");
  await atWork(cue);
  await openItem(page, "d5");
  await expect(page.locator(".dk-right .dk-tm.live")).toContainText("The footer is fine.");
  // One naming another item is that one's.
  await turnWith(cue, "look at #d4 instead", "Looking at the other one.");
  await cue.change(() => {});
  await expect(page.locator(".dk-right .dk-tm.live")).not.toContainText("Looking at the other one.");
});

test("the words being written leave out the Desk's own lines, as the saved reply does", async ({ page, cue }) => {
  await cue.open("desk");
  await turnWith(cue, "About #d5 (x): go on", "All done here.\n\nDONE: #d5\n**DECIDED:** kept the old colour");
  await atWork(cue);
  await openItem(page, "d5");
  const live = page.locator(".dk-right .dk-tm.live");
  await expect(live).toContainText("All done here.");
  await expect(live).not.toContainText("DONE:");
  await expect(live).not.toContainText("DECIDED");
});
