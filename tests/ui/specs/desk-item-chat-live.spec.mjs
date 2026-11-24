// An item's chat feels live (#395, option A of docs/desk-item-chat-live.md), in its holder's chat the item opens (#418,
// #744): its messages are kept across draws and only what changed is touched; it follows its end only while you're
// there; what you send shows at once; the words being written stay whole as its turn ends, till the saved reply is in.
import { test, expect, openDesk } from "./cue.mjs";

const openItem = async (page, id) => {
  await openDesk(page);
  await page.locator(`.dkn-items [data-desk-item="${id}"]`).first().click();
};
/** The item's chat: its holder's, in the session page that opened in the middle (it scrolls itself). */
const chatOf = (page, sid = "host") => page.locator(`.dk-right .dkn-sp[data-desk-page="session"] [data-desk-chat="${sid}"]`);
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
const say = (page) => chatOf(page).locator(".stx-say");

test("the reply being written grows in place: its words are the same element, and the page stays at its end", async ({ page, cue }) => {
  await cue.open("desk");
  const long = (n) => Array.from({ length: n }, (_, k) => `Line ${k + 1} of what it found.`).join("\n\n");
  await turnWith(cue, "About #d5 (x): go on", long(10));
  await atWork(cue);
  await openItem(page, "d5");
  const live = say(page), pane = chatOf(page);
  await expect(live).toContainText("Line 10 of what it found.");
  await live.evaluate((e) => { window.keptRow = e; });
  const atEnd = () => pane.evaluate((p) => p.scrollHeight - p.scrollTop - p.clientHeight < 24);
  await expect.poll(atEnd).toBe(true);
  // More words: the same element, longer, and the page still at its end.
  await turnWith(cue, "About #d5 (x): go on", long(40));
  await expect(live).toContainText("Line 40 of what it found.");
  expect(await live.evaluate((e) => e === window.keptRow)).toBe(true);
  await expect.poll(atEnd).toBe(true);
  // Scrolled up to read (a wheel of yours, not the page settling under it, #704): more words don't move it. Halfway,
  // not to the top: at the top its chat loads earlier messages (#748: an item's chat is its whole session's), which
  // keep what you read in place and so move the scroll by their height.
  const mid = await pane.evaluate((p) => { p.dispatchEvent(new WheelEvent("wheel", { bubbles: true, deltaY: -100 })); p.scrollTop = Math.floor((p.scrollHeight - p.clientHeight) / 2); return p.scrollTop; });
  await turnWith(cue, "About #d5 (x): go on", long(60));
  await expect(live).toContainText("Line 60 of what it found.");
  expect(await pane.evaluate((p) => p.scrollTop)).toBe(mid);
});

// What you send from an opened item's box shows in its session's chat at once (desk/c.js deskCItemPending, #748).
test("what you send on an item's page shows at once, before its session has it", async ({ page, cue }) => {
  await cue.open("desk");
  await atWork(cue);
  // The send never comes back while we look.
  await cue.override("desk_item_act", () => new Promise(() => {}));
  await openItem(page, "d5");
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  const mine = chatOf(page).locator(".cv-you").filter({ hasText: "Check the narrow window too" });
  await expect(mine).toHaveCount(1);
  await expect(mine).toContainText("Sending");
});

test("as a turn ends, its words stay whole till the saved reply is in, then that one shows, once", async ({ page, cue }) => {
  await cue.open("desk");
  const words = "Found it: the header centres on the wrong box.\n\nFixed it and ran the tests; all pass.";
  await turnWith(cue, "About #d5 (x): go on", words);
  await atWork(cue);
  await openItem(page, "d5");
  const ichat = chatOf(page);
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
