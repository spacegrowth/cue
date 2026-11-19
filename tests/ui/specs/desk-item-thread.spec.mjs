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
  // The link opens that session here, as its name on the left does, with no pop-up (#306); ‹ goes back to the item.
  await reply.locator("[data-desk-turn]").click();
  await expect(page.locator(".convo-sheet")).toHaveCount(0);
  await expect(page.locator(".dk-chat")).toBeVisible();
  await page.locator("[data-desk-chat-back]").click();
  await expect(page.locator(".dk-right .dk-ipane")).toBeVisible();
});

test("what a stop hook made it write after its reply folds under the reply, as the chat folds it (#310)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "to_check", now_on: false, latest: "", said: [{ at: Date.now() - 60000, by: "host-lead", text: "Fixed the header.", after: "1. SHAPE: the checklist answer.", sid: "host" }] });
    st.sessions.find((x) => x.session_id === "host").state = "waiting";
  });
  await openItem(page, cue, "d5");
  const reply = page.locator(".dk-right .dk-ipane .dk-tm:not(.rec)").filter({ hasText: "Fixed the header." });
  const fold = reply.locator(".follow [data-follow]");
  await expect(fold).toHaveText("After a stop hook ▾");
  await expect(reply).not.toContainText("the checklist answer");
  await fold.click();
  await expect(reply.locator(".follow-body ol li")).toHaveText("SHAPE: the checklist answer.");   // its markdown, a numbered list
  await expect(fold).toHaveText("After a stop hook ▴");
  // One with no hook has no fold.
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").said[0].after = undefined; });
  await expect(page.locator(".dk-right .dk-ipane .follow")).toHaveCount(0);
});

test("This turn in … opens its session at that reply, not at the end (#306)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const at = Date.now() - 3600000, s = st.sessions.find((x) => x.session_id === "host");
    s.thread = Array.from({ length: 40 }, (_, n) => ({ at_ms: at + n * 60000, role: n % 2 ? "agent" : "you", text: n === 21 ? "THE TURN: the fix is in." : `Message ${n}. ${"More words here. ".repeat(12)}` }));
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "to_check", now_on: false, latest: "", said: [{ at: at + 21 * 60000 + 2000, by: "host-lead", text: "THE TURN: the fix is in.", sid: "host", done: true }] });
    s.state = "waiting";
  });
  await openItem(page, cue, "d5");
  await page.locator(".dk-right .dk-ipane [data-desk-turn]").click();
  const chat = page.locator(".dk-chat"), turn = chat.locator(".cv-agent", { hasText: "THE TURN" });
  await expect(turn).toBeVisible();
  await expect.poll(async () => { const [c, t] = [await chat.boundingBox(), await turn.boundingBox()]; return t.y >= c.y - 1 && t.y < c.y + 80; }).toBe(true);
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
  await expect(page.locator(".dk-right .dk-thread .dk-took").last()).toContainText("host-lead took it");
  if (process.env.SHOT_DIR) await page.locator(".dk-right .dk-thread .dk-took").last().locator("xpath=..").screenshot({ path: `${process.env.SHOT_DIR}/took.png` });
  expect(await page.locator(".dk-right .dk-thread .dk-took").last().evaluate((e) => { const c = getComputedStyle(e); return [c.borderTopWidth, c.backgroundColor, c.cursor, e.tagName]; })).toEqual(["0px", "rgba(0, 0, 0, 0)", "auto", "DIV"]);
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

test("an item at work shows its holder's latest reply and what each of its crew at work is doing (#318)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const at = Date.now();
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true, said: [], latest: "G built. Waiting for the test helper." });
    const host = st.sessions.find((x) => x.session_id === "host");
    host.state = "waiting";
    host.team = [
      { agent_id: "a1", name: "loops-tests", state: "working", step: "Rewriting the old tests", started_ms: at - 60000, step_ms: at - 5000 },
      { agent_id: "a2", name: "loops-g", state: "done", step: "", started_ms: at - 600000, summary: "Built G" },
    ];
  });
  await openItem(page, cue, "d5");
  const now = page.locator(".dk-right .dk-pnow .dk-psay");
  await expect(now).toContainText("host-lead is on it. G built. Waiting for the test helper.");
  await expect(now.locator(".dk-crewnow")).toHaveText("Crew: loops-tests rewriting the old tests");
  const live = page.locator(".dk-right .dk-tm.live");
  await expect(live).toContainText("G built. Waiting for the test helper.");
  await expect(live.locator(".dk-crewnow")).toHaveText("Crew: loops-tests rewriting the old tests");
  // On another of its items now: neither its words nor its crew show here.
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").now_on = false; });
  await expect(page.locator(".dk-right .dk-crewnow")).toHaveCount(0);
});

test("a reply with the stop hook's checklist in its own words, saved before the hub kept it apart, folds it too (#325)", async ({ page, cue }) => {
  await cue.open("desk");
  const text = "Loops tests pass (42/42); the other helper is still on the crew-page tests.\n\n1. SHAPE: Edits this turn:\n   - `addon.js`: a line.\n2. ASSUMPTIONS:\n   - the fake's count, UNVERIFIED.\n3. VERIFY E2E: loops.spec passes.\n4, 6, 7, 11, 12 — N/A.\n10. TESTS: each failure root-caused.";
  const plain = "1. Read the header.\n2. Fixed its width.";
  await cue.change((st, [text, plain]) => {
    const at = Date.now();
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "to_check", now_on: false, latest: "", said: [
      { at: at - 120000, by: "host-lead", text: plain, sid: "host" },
      { at: at - 60000, by: "host-lead", text, sid: "host" }] });
  }, [text, plain]);
  await openItem(page, cue, "d5");
  const pane = page.locator(".dk-right .dk-ipane");
  const reply = pane.locator(".dk-tm:not(.rec)").filter({ hasText: "Loops tests pass (42/42)" });
  await expect(reply.locator(".dk-md").first()).toHaveText("Loops tests pass (42/42); the other helper is still on the crew-page tests.");
  await expect(reply).not.toContainText("SHAPE");
  await reply.locator("[data-follow]").click();
  await expect(reply.locator(".follow-body")).toContainText("SHAPE: Edits this turn");
  await expect(reply.locator(".follow-body")).toContainText("TESTS: each failure root-caused.");
  // A numbered list that isn't the checklist stays as it is.
  const other = pane.locator(".dk-tm:not(.rec)").filter({ hasText: "Read the header." });
  await expect(other.locator("[data-follow]")).toHaveCount(0);
});

test("a reply's stop-hook answer folds however its holder wrote it: Shape, **Shape:**, 1 — Shape (#353)", async ({ page, cue }) => {
  await cue.open("desk");
  const forms = [
    "Fixed the header.\n\n1. Shape: one CSS rule in `ui/styles/inbox.css`.\n2. Assumptions:\n   - none UNVERIFIED.\n4, 5, 6: N/A",
    "Wrote the doc.\n\n1. **Shape:** one new file, `docs/x.md`.\n2. **Assumptions:** checked against the code.",
    "Drew the page.\n\n1 — Shape was set before any edit: a ne…",
  ];
  await cue.change((st, forms) => {
    const at = Date.now();
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "to_check", now_on: false, latest: "",
      said: forms.map((text, n) => ({ at: at - 300000 + n * 60000, by: "host-lead", text, sid: "host" })) });
  }, forms);
  await openItem(page, cue, "d5");
  const pane = page.locator(".dk-right .dk-ipane");
  for (const words of ["Fixed the header.", "Wrote the doc.", "Drew the page."]) {
    const reply = pane.locator(".dk-tm:not(.rec)").filter({ hasText: words });
    await expect(reply.locator(".dk-md").first()).toHaveText(words);
    await expect(reply).not.toContainText(/shape/i);
    await expect(reply.locator("[data-follow]")).toHaveText("After a stop hook ▾");
  }
  // Revealed with a click, and folded again.
  const first = pane.locator(".dk-tm:not(.rec)").filter({ hasText: "Fixed the header." });
  await first.locator("[data-follow]").click();
  await expect(first.locator(".follow-body")).toContainText("Shape: one CSS rule");
  await first.locator("[data-follow]").click();
  await expect(first.locator(".follow-body")).toHaveCount(0);
});

test("an item's latest words that are only the stop-hook answer stay folded (#353)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "to_check", now_on: false, said: [],
      latest: "Header fixed.\n\n1. Shape: one rule.\n2. Assumptions: none." });
  });
  await openItem(page, cue, "d5");
  const reply = page.locator(".dk-right .dk-ipane .dk-tm:not(.rec)").filter({ hasText: "Header fixed." });
  await expect(reply).not.toContainText("one rule");
  await expect(reply.locator("[data-follow]")).toHaveText("After a stop hook ▾");
});
