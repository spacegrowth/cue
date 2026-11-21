// The Desk's chats update in place (#624): an item page's chat, the lead's and a session's. Each draw used to take the
// chat out of the window and put it back (WebKit has no moveBefore, so it left the page each time): the row under the
// pointer lost its hover, its scroll box was a new one, and the chat blinked several times a second while its session
// worked. Now the window is drawn around it: the chat and everything it sits in stay in the page, and only messages
// that are new or changed are touched. Here updates stream in (steps, new messages, its state) and the chat is watched
// the whole time: never out of the page, its earlier messages the same elements with the same words, where you'd
// scrolled and a message you'd opened as they were, the row under the pointer still under it.
import { test, expect, openDesk } from "./cue.mjs";

const LONG = (n) => `Reply ${n}: ` + "what it found and what it changed, in some detail. ".repeat(12);
const NOT_DONE = "Desk item #d5 (x) isn't done: the header still jumps\n\nThe end of its log:\nline one\nline two";
// The holder's live turn: `n` steps, words after every third.
const steps = (cue, n) => cue.override("session_steps", ({ sessionId }, n) => {
  const T = (globalThis.__t0 ??= Date.now() - 30000), items = [];
  for (let k = 0; k < n; k++) {
    items.push({ t: "step", id: `s${k}`, kind: "read", verb: "Read", subject: `f${k}.js`, mono: true, result: "40 lines", bad: false, done: k < n - 1, at_ms: T + 1000 + k * 10 });
    if (k % 3 === 2) items.push({ t: "say", text: `Words after step ${k}`, at_ms: T + 1005 + k * 10 });
  }
  return sessionId !== "host" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: T, prompt: "go on", more: 0, items }] };
}, n);

/** Watches chat `sel` from now on: whether it (or anything it sits in) leaves the page, and its rows as they are now. */
const watch = (page, sel) => page.evaluate((sel) => {
  const chat = document.querySelector(sel);
  window.__w = { chat, pane: chat.parentElement, left: [], rows: [...chat.querySelectorAll(":scope > [data-key]")].map((el) => {
    // A message's words (its "9m ago" may tick over: that is a real change).
    const words = el.querySelector(".mg-text") || el;
    return { el, key: el.dataset.key, words, text: words.textContent };
  }) };
  new MutationObserver((ms) => { for (const m of ms) for (const n of m.removedNodes) if (n === chat || n.contains?.(chat)) window.__w.left.push(n.className || n.nodeName); })
    .observe(document, { childList: true, subtree: true });
}, sel);
/** What changed since watch(): what took the chat out of the page, and its rows gone or redrawn. */
const changed = (page, sel, keep) => page.evaluate(([sel, keep]) => {
  const w = window.__w;
  return {
    left: w.left,
    same: document.querySelector(sel) === w.chat && w.chat.isConnected,
    gone: w.rows.filter((r) => !r.el.isConnected || r.el.parentElement !== w.chat).map((r) => r.key),
    reworded: w.rows.filter((r) => new RegExp(keep).test(r.key) && (!r.words.isConnected || !r.el.contains(r.words) || r.words.textContent !== r.text)).map((r) => r.key),
  };
}, [sel, keep]);

for (const scheme of ["light", "dark"]) {
  test(`an item page's chat updates in place as steps, messages and its state stream in (${scheme}) (#624)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1400, height: 800 });
    await cue.open("desk");
    await cue.change((st, [NOT_DONE, long]) => {
      const LONG = (0, eval)(`(${long})`);
      const t0 = Date.now() - 600000, s = st.sessions.find((x) => x.session_id === "host");
      s.thread = [{ role: "you", text: "About #d5 (x): go on", at_ms: t0 }];
      for (let k = 0; k < 6; k++) s.thread.push({ role: "agent", text: LONG(k), at_ms: t0 + 1000 * (k + 1) }, { role: "you", text: `Next ${k}`, at_ms: t0 + 1000 * (k + 1) + 500 });
      s.thread.push({ role: "you", text: NOT_DONE, at_ms: t0 + 9000 }, { role: "you", text: "go on", at_ms: Date.now() - 30000 });
      Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true });
      s.state = "working";
    }, [NOT_DONE, LONG.toString()]);
    await steps(cue, 1);
    await openDesk(page);
    await page.locator('[data-desk-item="d5"]').first().click();
    const chat = page.locator(".dk-right .dk-ichat"), pane = page.locator(".dk-right .dk-ipane");
    await expect(chat).toContainText("Reply 5");
    // Cue's "not done" message opened, a reply under the pointer, and the page scrolled up to read.
    const cueMsg = chat.locator(".cue-msg").filter({ hasText: "the header still jumps" });
    await cueMsg.locator(".mg-lk").click();
    await expect(cueMsg.locator(".mg-full")).toContainText("line two");
    await pane.evaluate((p) => { p.scrollTop = 300; });
    const read = chat.locator(".cv-agent").filter({ hasText: "Reply 2" });
    await read.hover();
    await watch(page, ".dk-right .dk-ichat");
    const top = await pane.evaluate((p) => p.scrollTop);

    // Steps land, a status changes, a message comes, the session stops: several draws each.
    for (let n = 2; n <= 7; n++) { await steps(cue, n); await expect(chat).toContainText(`${n} steps`); }
    await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").doing = "Running the tests"; });
    await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "host"); s.thread.push({ role: "agent", text: "Reply 6: the tests pass.", at_ms: Date.now() }); s.state = "waiting"; });
    await expect(chat).toContainText("Reply 6: the tests pass.");

    const c = await changed(page, ".dk-right .dk-ichat", "^[ay]:");
    expect(c.left, "the chat never left the page").toEqual([]);
    expect(c.same).toBe(true);
    expect(c.gone, "earlier messages are the same elements").toEqual([]);
    expect(c.reworded, "earlier messages keep their words").toEqual([]);
    // What you'd opened, where you were and the row under the pointer, as they were.
    await expect(cueMsg.locator(".mg-full")).toContainText("line two");
    expect(await pane.evaluate((p) => p.scrollTop)).toBe(top);
    expect(await read.evaluate((el) => el.matches(":hover"))).toBe(true);
    expect(await page.evaluate(() => document.querySelector(".dk-right .dk-ipane") === window.__w.pane.closest(".dk-ipane"))).toBe(true);
  });
}

test("the lead's chat and a session's chat on the Desk update in place too (#624)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    for (let k = 0; k < 4; k++) s.thread.push({ role: k % 2 ? "you" : "agent", text: `Lead line ${k}`, at_ms: Date.now() - 50000 + k * 1000 });
  });
  const lead = page.locator(".dk-right .dk-chat");
  await expect(lead).toContainText("Lead line 3");
  await watch(page, ".dk-right .dk-chat");
  for (let k = 4; k < 7; k++) {
    await cue.change((st, k) => { const s = st.sessions.find((x) => x.session_id === "lead"); s.thread.push({ role: "agent", text: `Lead line ${k}`, at_ms: Date.now() }); s.state = k % 2 ? "working" : "waiting"; }, k);
    await expect(lead).toContainText(`Lead line ${k}`);
  }
  let c = await changed(page, ".dk-right .dk-chat", "^[ay]:");
  expect(c.left).toEqual([]);
  expect(c.same).toBe(true);
  expect(c.gone).toEqual([]);
  expect(c.reworded).toEqual([]);

  // A session's chat opened on the Desk.
  await page.evaluate(() => { deskShow({ kind: "chat", sid: "host" }); renderMain(); });
  const sess = page.locator('.dk-right .dk-chat[data-desk-chat="host"]');
  await expect(sess).toHaveCount(1);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").thread.push({ role: "agent", text: "Session line A", at_ms: Date.now() - 1000 }); });
  await expect(sess).toContainText("Session line A");
  await watch(page, '.dk-right .dk-chat[data-desk-chat="host"]');
  for (let k = 0; k < 3; k++) {
    await cue.change((st, k) => { const s = st.sessions.find((x) => x.session_id === "host"); s.thread.push({ role: "agent", text: `Session line ${k}`, at_ms: Date.now() }); s.state = k % 2 ? "working" : "waiting"; }, k);
    await expect(sess).toContainText(`Session line ${k}`);
  }
  c = await changed(page, '.dk-right .dk-chat[data-desk-chat="host"]', "^[ay]:");
  expect(c.left).toEqual([]);
  expect(c.same).toBe(true);
  expect(c.gone).toEqual([]);
  expect(c.reworded).toEqual([]);
});
