// An item's chat feels live (#395, option A of docs/desk-item-chat-live.md), kept in its holder's chat (#418): its
// messages are kept across draws and only what changed is touched; it follows its end only while you're there; what
// you send shows at once; the words being written stay whole as its turn ends, till the saved reply is in; a turn typed
// with no item number stays on the item its session holds; and the live words leave out the Desk's own lines, as the
// saved reply does.
import { test, expect } from "./cue.mjs";

const openItem = async (page, id) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(`[data-desk-item="${id}"]`).first().click();
};
// The holder's newest turn: you wrote `prompt` (in its chat), and it read a file and has written `words` so far (its
// steps feed). The turn's times are kept across calls, so it's the same turn growing.
const turnWith = async (cue, prompt, words) => {
  await cue.override("session_steps", ({ sessionId }, [prompt, words]) => {
    const t0 = (globalThis.__turnAt ??= Date.now() - 30000);
    return sessionId !== "host" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: t0, prompt, more: 0, items: [
      { t: "step", id: "s1", kind: "read", verb: "Read", subject: "header.js", mono: true, result: "40 lines", bad: false, done: true, at_ms: t0 + 1000 },
      { t: "say", text: words, at_ms: t0 + 2000 }] }] };
  }, [prompt, words]);
  await cue.change((st, prompt) => {
    const t0 = (globalThis.__turnAt ??= Date.now() - 30000);
    st.sessions.find((x) => x.session_id === "host").thread = [{ role: "you", text: prompt, at_ms: t0 }];
  }, prompt);
};
const atWork = (cue) => cue.change((st) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true, said: [], latest: "" }); st.sessions.find((x) => x.session_id === "host").state = "working"; });
const say = (page) => page.locator(".dk-right .dk-ichat .stx-say");

test("the reply being written grows in place: its words are the same element, and the page stays at its end", async ({ page, cue }) => {
  await cue.open("desk");
  const long = (n) => Array.from({ length: n }, (_, k) => `Line ${k + 1} of what it found.`).join("\n\n");
  await turnWith(cue, "About #d5 (x): go on", long(10));
  await atWork(cue);
  await openItem(page, "d5");
  const live = say(page), pane = page.locator(".dk-right .dk-ipane");
  await expect(live).toContainText("Line 10 of what it found.");
  await live.evaluate((e) => { window.keptRow = e; });
  const atEnd = () => pane.evaluate((p) => p.scrollHeight - p.scrollTop - p.clientHeight < 24);
  await expect.poll(atEnd).toBe(true);
  // More words: the same element, longer, and the page still at its end.
  await turnWith(cue, "About #d5 (x): go on", long(40));
  await expect(live).toContainText("Line 40 of what it found.");
  expect(await live.evaluate((e) => e === window.keptRow)).toBe(true);
  await expect.poll(atEnd).toBe(true);
  // Scrolled up to read: more words don't move it.
  await pane.evaluate((p) => { p.scrollTop = 0; });
  await turnWith(cue, "About #d5 (x): go on", long(60));
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
  const mine = page.locator(".dk-right .dk-ichat .cv-you").filter({ hasText: "Check the narrow window too" });
  await expect(mine).toHaveCount(1);
  await expect(mine).toContainText("Sending");
});

test("as a turn ends, its words stay whole till the saved reply is in, then that one shows, once", async ({ page, cue }) => {
  await cue.open("desk");
  const words = "Found it: the header centres on the wrong box.\n\nFixed it and ran the tests; all pass.";
  await turnWith(cue, "About #d5 (x): go on", words);
  await atWork(cue);
  await openItem(page, "d5");
  const ichat = page.locator(".dk-right .dk-ichat");
  const shown = ichat.locator(".stx-say, .cv-agent").filter({ hasText: "all pass." });
  await expect(say(page)).toContainText("all pass.");
  // Stopped, its reply not saved yet: its last line only would show; the whole of it stays.
  await cue.change((st) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { now_on: false, stopped: true, latest: "Not committed: ui/header.js" }); st.sessions.find((x) => x.session_id === "host").state = "waiting"; });
  await expect(ichat).not.toContainText("Not committed");
  await expect(shown).toHaveCount(1);
  await expect(shown).toContainText("Found it: the header centres on the wrong box.");
  // The saved reply in: shown once, as the chat's reply.
  await cue.change((st, words) => {
    const s = st.sessions.find((x) => x.session_id === "host");
    s.thread.push({ role: "agent", text: words, at_ms: globalThis.__turnAt + 2000 });
    st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").said = [{ at: Date.now(), by: "host-lead", text: words, sid: "host" }];
  }, words);
  await expect(ichat.locator(".cv-agent").filter({ hasText: "all pass." })).toHaveCount(1);
  await expect(shown).toHaveCount(1);
});

test("a turn typed in the session's own chat, naming no item, stays in the one item's chat it holds; one naming another folds", async ({ page, cue }) => {
  await cue.open("desk");
  await turnWith(cue, "also check the footer", "The footer is fine.");
  await atWork(cue);
  await openItem(page, "d5");
  const ichat = page.locator(".dk-right .dk-ichat");
  await expect(ichat).toContainText("The footer is fine.");
  await expect(ichat.locator(".dk-fold")).toHaveCount(0);
  // One naming another item is that one's: folded to its line.
  await turnWith(cue, "look at #d4 instead", "Looking at the other one.");
  await expect(ichat.locator(".dk-fold")).toContainText("Worked on #d4");
  await expect(ichat).not.toContainText("Looking at the other one.");
});

test("the words being written leave out the Desk's own lines, as the saved reply does", async ({ page, cue }) => {
  await cue.open("desk");
  await turnWith(cue, "About #d5 (x): go on", "All done here.\n\nDONE: #d5\n**DECIDED:** kept the old colour");
  await atWork(cue);
  await openItem(page, "d5");
  const live = say(page);
  await expect(live).toContainText("All done here.");
  await expect(live).not.toContainText("DONE:");
  await expect(live).not.toContainText("DECIDED");
});
