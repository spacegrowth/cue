// An item's page (#276), kept as its holder's own chat (#418): each reply in full, a Done as the Desk's line; the turn
// its holder is writing, live, with its steps; the box going straight to the holder, sent as about the item, with no
// copy to the lead; what a stop hook made it write folded under the reply; and Open <holder> for its whole session.
import { test, expect, openDesk } from "./cue.mjs";

const openItem = async (page, cue, id) => {
  await openDesk(page);
  await page.locator(`[data-desk-item="${id}"]`).first().click();
};
// host-lead's chat: you, about #d5, then each of `replies` in turn, a minute apart.
const replies = (cue, list, state = "waiting") => cue.change((st, [list, state]) => {
  const at = Date.now() - 600000, s = st.sessions.find((x) => x.session_id === "host");
  Object.assign(s, { state, doing: "", thread: [{ role: "you", text: "About #d5 (x): go on", at_ms: at }, ...list.map((text, n) => ({ role: "agent", text, at_ms: at + (n + 1) * 60000 }))] });
  Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: state === "working" ? "working" : "to_check", now_on: state === "working", said: [], latest: "" });
}, [list, state]);
const reply = (page, words) => page.locator(".dk-right .dk-ichat .cv-agent").filter({ hasText: words });

test("an item's replies show whole, a Done is the Desk's line, and Open host-lead opens its session", async ({ page, cue }) => {
  await cue.open("desk");
  const long = `Here is what I found.\n\n${"Every line of it matters. ".repeat(80).trim()}\n\nThe last line.\n\nDONE: #d5`;
  await replies(cue, [long]);
  await openItem(page, cue, "d5");
  const r = reply(page, "Here is what I found.");
  await expect(r).toContainText("The last line.");
  await expect(r).not.toHaveClass(/folded/);
  await expect(r.locator(".own-lines")).toContainText("Bug #d5 is done");
  await expect(r).not.toContainText("DONE:");
  // Its whole session opens here, as its name on the left does, with no pop-up (#306); ‹ goes back to the item.
  await page.locator(".dk-panel[data-side-for] .dk-sfoot").getByText("Open host-lead").click();
  await expect(page.locator(".convo-sheet")).toHaveCount(0);
  await expect(page.locator(".dk-chat")).toBeVisible();
  await page.locator("[data-desk-chat-back]").click();
  await expect(page.locator(".dk-right .dk-ipane")).toBeVisible();
});

test("what a stop hook made it write after its reply folds under the reply, as the chat folds it (#310)", async ({ page, cue }) => {
  await cue.open("desk");
  const hooked = (cue, on) => cue.override("session_steps", ({ sessionId }, on) => {
    const at = Date.now() - 600000;
    return sessionId !== "host" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at, prompt: "About #d5 (x): go on", more: 0, ...(on ? { hooked_ms: at + 70000 } : {}), items: [
      { t: "step", id: "s1", kind: "read", verb: "Read", subject: "header.js", mono: true, result: "40 lines", bad: false, done: true, at_ms: at + 5000 },
      { t: "say", text: "Fixed the header.", at_ms: at + 60000 },
      ...(on ? [{ t: "say", text: "1. SHAPE: the checklist answer.", at_ms: at + 80000 }] : [])] }] };
  }, on);
  await hooked(cue, true);
  await replies(cue, ["Fixed the header."]);
  await openItem(page, cue, "d5");
  const ichat = page.locator(".dk-right .dk-ichat"), fold = ichat.locator(".stx-hook [data-hook-fold]");
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
  await expect(page.locator(".dk-right .dk-box .dk-pre")).toHaveText("To host-lead, who has this item · sent as #d5");
  await expect(page.locator(".dk-right .dk-box textarea")).toHaveAttribute("placeholder", "Write to it here…");
  await page.locator(".dk-right .dk-box textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  await cue.called("desk_item_act", { id: "d5", act: "note", text: "Check the narrow window too" });
  const sent = { args: { text: await page.evaluate(() => deskOf("cue").decisions.find((c) => c.item_id === "d5" && c.delivery)?.delivery.text) } };
  expect(await page.evaluate(() => deskOf("cue").decisions.find((c) => c.item_id === "d5" && c.delivery)?.delivery.session_id)).toBe("host");
  expect(sent.args.text).toMatch(/^About #d5 \(.+\): Check the narrow window too$/);
  expect(await cue.calls("desk_send")).toHaveLength(0);
  // In its chat, not taken yet while its holder works: queued, it goes in when it's free (#531); taken, that goes.
  await cue.change((st, text) => {
    const s = st.sessions.find((x) => x.session_id === "host");
    s.held = null;
    (s.thread ||= []).push({ at_ms: Date.now(), role: "you", text, pending: true });
  }, sent.args.text);
  const mine = page.locator(".dk-right .dk-ichat .cv-you").filter({ hasText: "Check the narrow window too" });
  await expect(mine).toHaveCount(1);
  await expect(mine.locator(".msg-line")).toHaveText("◌ Queued · goes in when it's free");
  await expect(mine).toHaveClass(/msg-queued/);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").thread.at(-1).pending = false; });
  await expect(mine).toHaveCount(1);
  // The same line as under any message (#422): delivered, to whom, when; its bubble filled.
  const line = mine.locator(".msg-line");
  await expect(line).toContainText("✓ Delivered to host-lead ·");
  if (process.env.SHOT_DIR) await line.locator("xpath=..").screenshot({ path: `${process.env.SHOT_DIR}/took.png` });
  expect(await line.evaluate((e) => { const c = getComputedStyle(e); return [c.borderTopWidth, c.backgroundColor, c.cursor, e.tagName]; })).toEqual(["0px", "rgba(0, 0, 0, 0)", "auto", "DIV"]);
  await expect(mine).toHaveClass(/msg-delivered/);
});

test("while its holder works on it, the item's chat shows the turn it's writing, whole, with its steps folded", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.override("session_steps", ({ sessionId }, at = Date.now()) => (sessionId !== "host" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at - 30000, prompt: "About #d5 (x): go on", more: 0, items: [
    { t: "step", id: "s1", kind: "read", verb: "Read", subject: "header.js", mono: true, result: "40 lines", bad: false, done: true, at_ms: at - 20000 },
    { t: "say", text: "Found it: the header **centres** on the wrong box.", at_ms: at - 15000 },
    { t: "step", id: "s2", kind: "run", verb: "Ran", subject: "npm test", mono: true, result: "", bad: false, done: false, at_ms: at - 5000 }] }] }));
  await cue.override("step_detail", ({ ids }) => Object.fromEntries(ids.map((id) => [id, id === "s2" ? { full: "npm test" } : { full: "/repo/header.js" }])));
  await cue.change((st) => {
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true, said: [] });
    Object.assign(st.sessions.find((x) => x.session_id === "host"), { state: "working", thread: [{ role: "you", text: "About #d5 (x): go on", at_ms: Date.now() - 30000 }] });
  });
  await openItem(page, cue, "d5");
  const live = page.locator(".dk-right .dk-ichat .stx");
  await expect(live.locator(".stx-fold")).toContainText("Working · 2 steps");
  await expect(live.locator(".stx-fold .dot-live")).toHaveCount(1);
  await expect(live.locator(".stx-say")).toContainText("Found it: the header centres on the wrong box.");
  // Folded: no step drawn while it works (the line under the chat is its running line); the chip opens every step in place.
  await expect(live.locator(".stx-line")).toHaveCount(0);
  await live.locator(".stx-fold").click();
  await expect(live.locator(".stx-line")).toHaveText([/Read header\.js/, /Ran npm test/]);
  await expect(page.locator(".dk-panel[data-side-for] .dk-sfoot")).toContainText("Open host-lead");
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    if (process.env.SHOT_DIR) await page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-276-${theme}.png` });
  }
});

test("an item at work shows its holder's latest reply and what each of its crew at work is doing (#318)", async ({ page, cue }) => {
  await cue.open("desk");
  await replies(cue, ["G built. Waiting for the test helper."]);
  await cue.change((st) => {
    const at = Date.now();
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true, said: [], latest: "G built. Waiting for the test helper." });
    st.sessions.find((x) => x.session_id === "host").team = [
      { agent_id: "a1", name: "loops-tests", state: "working", step: "Rewriting the old tests", started_ms: at - 60000, step_ms: at - 5000 },
      { agent_id: "a2", name: "loops-g", state: "done", step: "", started_ms: at - 600000, summary: "Built G" },
    ];
  });
  await openItem(page, cue, "d5");
  const now = page.locator(".dk-panel[data-side-for] .dk-pnow .dk-psay");
  await expect(now).toContainText("host-lead is on it. G built. Waiting for the test helper.");
  await expect(now.locator(".dk-crewnow")).toHaveText("Crew: loops-tests rewriting the old tests");
  await expect(reply(page, "G built. Waiting for the test helper.")).toHaveCount(1);
  // On another of its items now: its crew doesn't show here.
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").now_on = false; });
  await expect(page.locator(".dk-panel[data-side-for] .dk-crewnow")).toHaveCount(0);
});

test("a reply with the stop hook's checklist in its own words, saved before the hub kept it apart, folds it too (#325)", async ({ page, cue }) => {
  await cue.open("desk");
  const text = "Loops tests pass (42/42); the other helper is still on the crew-page tests.\n\n1. SHAPE: Edits this turn:\n   - `addon.js`: a line.\n2. ASSUMPTIONS:\n   - the fake's count, UNVERIFIED.\n3. VERIFY E2E: loops.spec passes.\n4, 6, 7, 11, 12 — N/A.\n10. TESTS: each failure root-caused.";
  const plain = "1. Read the header.\n2. Fixed its width.";
  await replies(cue, [plain, text]);
  await openItem(page, cue, "d5");
  const r = reply(page, "Loops tests pass (42/42)");
  await expect(r.locator(".cv-text > p").first()).toHaveText("Loops tests pass (42/42); the other helper is still on the crew-page tests.");
  await expect(r.locator(".cv-text")).not.toContainText("SHAPE");
  await r.getByText("After a stop hook ▾").click();
  await expect(r.locator(".follow-body")).toContainText("SHAPE: Edits this turn");
  await expect(r.locator(".follow-body")).toContainText("TESTS: each failure root-caused.");
  // A numbered list that isn't the checklist stays as it is.
  await expect(reply(page, "Read the header.").getByText(/After a stop hook/)).toHaveCount(0);
});

test("a reply's stop-hook answer folds however its holder wrote it: Shape, **Shape:**, 1 — Shape (#353)", async ({ page, cue }) => {
  await cue.open("desk");
  const forms = [
    "Fixed the header.\n\n1. Shape: one CSS rule in `ui/styles/inbox.css`.\n2. Assumptions:\n   - none UNVERIFIED.\n4, 5, 6: N/A",
    "Wrote the doc.\n\n1. **Shape:** one new file, `docs/x.md`.\n2. **Assumptions:** checked against the code.",
    "Drew the page.\n\n1 — Shape was set before any edit: a ne…",
  ];
  await replies(cue, forms);
  await openItem(page, cue, "d5");
  for (const words of ["Fixed the header.", "Wrote the doc.", "Drew the page."]) {
    const r = reply(page, words);
    await expect(r.locator(".cv-text")).toHaveText(words);
    await expect(r.getByText("After a stop hook ▾")).toHaveCount(1);
  }
  // Revealed with a click, and folded again.
  const first = reply(page, "Fixed the header.");
  await first.getByText("After a stop hook ▾").click();
  await expect(first.locator(".follow-body")).toContainText("Shape: one CSS rule");
  await first.getByText("After a stop hook ▴").click();
  await expect(first.locator(".follow-body")).toHaveCount(0);
});
