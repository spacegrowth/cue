// A turn's steps in a chat (#542, option F): folded by default into one chip ("Working · 5 steps · 3 commands, …"),
// what it said between them left in view, each message ending in the count of steps that led to it (#604). While it
// works, its one running line is the line under the chat: live dot, what it's doing and the steps since its last words.
// The chip opens every step; a message's count opens only the steps that led to it; each step is one line cut with "…"
// and opens whole (command, output, diff green and red) only when you click it.
import { test, expect, openDesk } from "./cue.mjs";

const LONG = "cargo test --lib desk::hub::landing::copy_guard 2>&1 | grep -E \"test result|FAILED|panicked\" | tail -20 && git status --short";

/** host-lead at work on #d5: two steps, a message, two more steps, then a long command running now. */
const steps = (cue) => cue.override("session_steps", ({ sessionId }, long) => {
  const at = (window.__t0 ||= Date.now());
  const step = (id, kind, verb, subject, result, ago, o = {}) => ({ t: "step", id, kind, verb, subject, mono: true, result, bad: false, done: true, at_ms: at - ago * 1000, ...o });
  return sessionId !== "host" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at - 120000, prompt: "About #d5 (x): go on", more: 0, items: [
    step("s1", "read", "Read", "header.js", "40 lines", 110),
    step("s2", "run", "Ran", "npm test", "41 passed", 100),
    { t: "say", text: "Found it: the header centres on the wrong box.", at_ms: at - 90000 },
    step("s3", "edit", "Edited", "header.js", "+1 −1", 80),
    step("s4", "run", "Ran", "git diff --stat", "1 file", 70),
    step("s5", "run", "Running", long.slice(0, 79) + "…", "", 60, { done: false }),
  ] }] };
}, LONG);
const details = (cue) => cue.override("step_detail", ({ ids }, long) => Object.fromEntries(ids.map((id) => [id,
  id === "s3" ? { full: "/repo/ui/header.js", diff: [[" ", "function centre(box) {"], ["-", "  return box.parent;"], ["+", "  return box;"], [" ", "}"]], skipped: 0 }
    : id === "s5" ? { full: long } : { full: id === "s2" ? "npm test" : "git diff --stat", output: ["ok"], skipped: 0 }])), LONG);

const openWorking = async (page, cue) => {
  await cue.open("desk");
  await steps(cue);
  await details(cue);
  await cue.change((st) => {
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true, said: [] });
    Object.assign(st.sessions.find((x) => x.session_id === "host"), { state: "working", thread: [{ role: "you", text: "About #d5 (x): go on", at_ms: Date.now() - 125000 }] });
  });
  await openDesk(page);
  await page.locator('[data-desk-item="d5"]').first().click();
  return page.locator(".dk-right .dk-ichat .stx");
};

test("folded by default: one chip, its message in view with its count, no step drawn; the line under the chat is the running line, with its count", async ({ page, cue }) => {
  const turn = await openWorking(page, cue);
  const chip = turn.locator(".stx-fold");
  await expect(chip).toContainText("Working · 5 steps");
  await expect(chip).toContainText("3 commands, 1 read, 1 edit");
  await expect(chip).toHaveAttribute("aria-expanded", "false");
  // No step drawn while it works, and no second running line: the message stays, ending in how many steps led to it.
  await expect(turn.locator(".stx-line")).toHaveCount(0);
  await expect(turn.locator(".stx-last")).toHaveCount(0);
  const msg = turn.locator(".stx-say");
  await expect(msg).toContainText("Found it: the header centres on the wrong box.");
  await expect(msg.locator(".stx-cnt")).toHaveText("· 2 steps");
  // Under the pointer it darkens; never underlined.
  await msg.locator(".stx-cnt").hover();
  expect(await msg.locator(".stx-cnt").evaluate((el) => getComputedStyle(el).textDecorationLine)).toBe("none");
  await expect(msg).toBeInViewport();
  // Under the chat: its live dot, what it's doing, then the steps since its last words (s3, s4, s5).
  const status = page.locator(".dk-right .dk-ichat .cv-status");
  await expect(status.locator(".dot-live")).toHaveCount(1);
  await expect(status.locator(".stx-cnt")).toHaveText("· 3 steps");
  // Its count opens just those three steps.
  await status.locator(".stx-cnt").click();
  await expect(turn.locator(".stx-line")).toHaveText([/Edited header\.js/, /Ran git diff --stat/, new RegExp(`Running ${LONG.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`)]);
  await expect(chip).toHaveAttribute("aria-expanded", "false");
  await status.locator(".stx-cnt").click();
  await expect(turn.locator(".stx-line")).toHaveCount(0);
});

test("the count on the line under the chat goes up as steps come in", async ({ page, cue }) => {
  await openWorking(page, cue);
  const status = page.locator(".dk-right .dk-ichat .cv-status");
  await expect(status.locator(".stx-cnt")).toHaveText("· 3 steps");
  await cue.override("session_steps", ({ sessionId }) => {
    const at = window.__t0;
    const step = (id, kind, verb, subject, ago, o = {}) => ({ t: "step", id, kind, verb, subject, mono: true, result: "ok", bad: false, done: true, at_ms: at - ago * 1000, ...o });
    return sessionId !== "host" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at - 120000, prompt: "About #d5 (x): go on", more: 0, items: [
      step("s1", "read", "Read", "header.js", 110), step("s2", "run", "Ran", "npm test", 100),
      { t: "say", text: "Found it: the header centres on the wrong box.", at_ms: at - 90000 },
      step("s3", "edit", "Edited", "header.js", 80), step("s4", "run", "Ran", "git diff --stat", 70), step("s5", "run", "Ran", "npm test", 60),
      step("s6", "run", "Running", "npm run lint", 50, { done: false }),
    ] }] };
  });
  await expect(status.locator(".stx-cnt")).toHaveText("· 4 steps", { timeout: 15000 });
  await expect(status.locator(".dot-live")).toHaveCount(1);
});

test("the chip opens and closes the whole turn, a message's count only its own steps; open, every step is one line in place between the messages, and a click opens just that one", async ({ page, cue }) => {
  const turn = await openWorking(page, cue);
  const chip = turn.locator(".stx-fold");
  await chip.click();
  await expect(chip).toHaveAttribute("aria-expanded", "true");
  await expect(turn.locator(".stx-line")).toHaveText([/Read header\.js/, /Ran npm test/, /Edited header\.js/, /Ran git diff --stat/, new RegExp(`Running ${LONG.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`)]);
  // The message sits between its steps, where it fell.
  const order = await turn.evaluate((el) => [...el.querySelectorAll(".stx-steps, .stx-say")].map((x) => (x.classList.contains("stx-say") ? "say" : x.querySelectorAll(".stx-line").length)));
  expect(order).toEqual([2, "say", 3]);
  // No Show all, and nothing open: each step one line, no output or diff until you click it.
  await expect(turn.locator(".stx-all")).toHaveCount(0);
  await expect(turn.locator(".stx-line.open")).toHaveCount(0);
  await expect(turn.locator(".stx-detail")).toHaveCount(0);
  expect((await cue.calls("step_detail")).flatMap((c) => c.args.ids).filter((id) => id !== "s5")).toEqual([]);
  // A click opens only that step, whole; a second click closes it.
  const ran = turn.locator(".stx-line").filter({ hasText: "Ran npm test" });
  await ran.click();
  await expect(ran).toHaveClass(/\bopen\b/);
  await expect(turn.locator(".stx-line.open")).toHaveCount(1);
  await expect(turn.locator(".stx-detail")).toHaveCount(1);
  await expect(turn.locator(".stx-out")).toHaveText("ok");
  const ws = await ran.locator(".stx-what").evaluate((el) => getComputedStyle(el).whiteSpace);
  expect(ws).toBe("normal");
  await ran.click();
  await expect(turn.locator(".stx-detail")).toHaveCount(0);
  await chip.click();
  await expect(turn.locator(".stx-line")).toHaveCount(0);
  // The message's count opens only the two steps that led to it, above it; the chip stays shut; a second click closes them.
  await turn.locator(".stx-say .stx-cnt").click();
  await expect(turn.locator(".stx-line")).toHaveText([/Read header\.js/, /Ran npm test/]);
  await expect(turn.locator(".stx-say .stx-cnt")).toHaveAttribute("aria-expanded", "true");
  await expect(chip).toHaveAttribute("aria-expanded", "false");
  const order2 = await turn.evaluate((el) => [...el.querySelectorAll(".stx-steps, .stx-say")].map((x) => (x.classList.contains("stx-say") ? "say" : x.querySelectorAll(".stx-line").length)));
  expect(order2).toEqual([2, "say"]);
  await turn.locator(".stx-say .stx-cnt").click();
  await expect(turn.locator(".stx-line")).toHaveCount(0);
  await expect(turn.locator(".stx-say")).toContainText("Found it");
});

test("a diff you open is plain green and red, whole, in light and dark; its +N −N too", async ({ page, cue }) => {
  const turn = await openWorking(page, cue);
  await turn.locator(".stx-fold").click();
  await turn.locator(".stx-line").filter({ hasText: "Edited header.js" }).click();
  const diff = turn.locator(".stx-diff");
  await expect(diff).toHaveCount(1);
  const want = { light: { add: "rgb(218, 251, 225)", addFg: "rgb(26, 127, 55)", del: "rgb(255, 235, 233)", delFg: "rgb(207, 34, 46)" },
    dark: { add: "rgb(28, 58, 37)", addFg: "rgb(86, 211, 100)", del: "rgb(69, 32, 31)", delFg: "rgb(255, 138, 128)" } };
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    const got = await diff.evaluate((el) => {
      const cs = (q, p) => getComputedStyle(el.querySelector(q))[p];
      return { add: cs(".a", "backgroundColor"), addFg: cs(".a i", "color"), del: cs(".d", "backgroundColor"), delFg: cs(".d i", "color"),
        res: [getComputedStyle(el.closest(".stx").querySelector(".stx-res .add")).color, getComputedStyle(el.closest(".stx").querySelector(".stx-res .del")).color],
        maxH: getComputedStyle(el).maxHeight, ws: getComputedStyle(el.querySelector(".a")).whiteSpace };
    });
    expect(got).toEqual({ ...want[theme], res: [want[theme].addFg, want[theme].delFg], maxH: "none", ws: "pre-wrap" });
  }
});

/** host on #d5 with one turn, `mk(at, step, say)` → its items or { items, hooked_ms } (built in the page), and `thread`: its messages as
 *  [role, text, seconds ago] after "go on". step_detail answers each id with its own full and output. */
const openTurn = async (page, cue, mk, thread = [], working = true) => {
  await cue.open("desk");
  await cue.override("session_steps", ({ sessionId }, src) => {
    const at = (window.__t0 ||= Date.now());
    const step = (id, kind, verb, subject, ago, o = {}) => ({ t: "step", id, kind, verb, subject, mono: true, result: "ok", bad: false, done: true, at_ms: at - ago * 1000, ...o });
    const say = (text, ago) => ({ t: "say", text, at_ms: at - ago * 1000 });
    // Its items, or { items, hooked_ms } for a turn a Stop hook sent back to work.
    const turn = (r) => (Array.isArray(r) ? { items: r } : r);
    return sessionId !== "host" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at - 600000, prompt: "About #d5 (x): go on", more: 0, ...turn((0, eval)(`(${src})`)(at, step, say)) }] };
  }, mk.toString());
  await cue.override("step_detail", ({ ids }) => Object.fromEntries(ids.map((id) => [id, { full: `full ${id}`, output: [`out ${id}`], skipped: 0 }])));
  // The page's clock for the turn, so the messages fall where its steps say.
  await page.evaluate(() => { window.__t0 ||= Date.now(); });
  await cue.change((st, [thread, working]) => {
    const t0 = window.__t0;
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: working ? "working" : "review", now_on: working, said: [] });
    Object.assign(st.sessions.find((x) => x.session_id === "host"), { state: working ? "working" : "idle",
      thread: [{ role: "you", text: "About #d5 (x): go on", at_ms: t0 - 605000 }, ...thread.map(([role, text, ago]) => ({ role, text, at_ms: t0 - ago * 1000 }))] });
  }, [thread, working]);
  await openDesk(page);
  await page.locator('[data-desk-item="d5"]').first().click();
  return page.locator(".dk-right .dk-ichat");
};

test("a finished turn folds with no foot row; its reply is drawn once, as the chat's, with the steps that led to it", async ({ page, cue }) => {
  const REPLY = "Fixed: the header centres on its own box now.";
  const chat = await openTurn(page, cue, (at, step, say) => [
    step("f1", "read", "Read", "header.js", 300), step("f2", "run", "Ran", "npm test", 290),
    say("Found it: the wrong box.", 280),
    step("f3", "edit", "Edited", "header.js", 270), step("f4", "run", "Ran", "npm test", 260), step("f5", "run", "Ran", "git diff", 255),
    say("Fixed: the header centres on its own box now.", 250),
  ], [["agent", REPLY, 250]], false);
  const chip = chat.locator(".stx-fold");
  await expect(chip).toContainText("5 steps");
  await expect(chip).not.toContainText("Working");
  await expect(chat.locator(".stx-last")).toHaveCount(0);
  await expect(chat.locator(".stx-line")).toHaveCount(0);
  // The reply once: the chat's row, not again inside the turn.
  await expect(chat.getByText(REPLY)).toHaveCount(1);
  await expect(chat.locator(".stx .stx-say")).toHaveText([/Found it: the wrong box\.\s*· 2 steps/]);
  const reply = chat.locator(".cv-agent").filter({ hasText: REPLY });
  await expect(reply.locator(".stx-cnt")).toHaveText("· 3 steps");
  // The counts add up to the chip's; the reply's count opens only its three steps, and closes them.
  await reply.locator(".stx-cnt").click();
  await expect(chat.locator(".stx-line")).toHaveText([/Edited/, /Ran npm test/, /Ran git diff/]);
  await expect(chip).toHaveAttribute("aria-expanded", "false");
  await reply.locator(".stx-cnt").click();
  await expect(chat.locator(".stx-line")).toHaveCount(0);
});

test("a turn you wrote into mid-turn: one chip, your message where you sent it, the words after it with their count", async ({ page, cue }) => {
  const chat = await openTurn(page, cue, (at, step, say) => [
    step("m1", "read", "Read", "header.js", 300), step("m2", "run", "Ran", "npm test", 290),
    step("m3", "edit", "Edited", "header.js", 270), say("Done the header; the footer next.", 260),
    step("m4", "run", "Running", "npm test", 250, { done: false }),
  ], [["you", "About #d5 (x): check the footer too", 280]]);
  await expect(chat.locator(".stx-fold")).toHaveCount(1);
  await expect(chat.locator(".stx-fold")).toContainText("Working · 4 steps");
  // In time order: the chip, your message, then the words with the steps since the previous ones (all three).
  const order = await chat.evaluate((el) => [...el.querySelectorAll(".stx-fold, .cv-you, .stx-say, .stx-last")].map((x) => x.classList.contains("stx-fold") ? "chip" : x.classList.contains("cv-you") ? (x.textContent.includes("footer too") ? "you" : "go on") : x.classList.contains("stx-say") ? "say" : "last"));
  expect(order).toEqual(["go on", "chip", "you", "say"]);
  await expect(chat.locator(".stx-say .stx-cnt")).toHaveText("· 3 steps");
  // Opened from the chip: every step back, on both sides of your message.
  await chat.locator(".stx-fold").click();
  await expect(chat.locator(".stx-line")).toHaveCount(4);
});

test("your choice stays across polls and redraws; a running step's command is asked for once, not every poll", async ({ page, cue }) => {
  const turn = await openWorking(page, cue);
  const chip = turn.locator(".stx-fold");
  await chip.click();
  await expect(turn.locator(".stx-line")).toHaveCount(5);
  // A few polls of its steps and a state change later, still open.
  const polls = async () => (await cue.calls("session_steps")).length;
  const p0 = await polls();
  await expect.poll(polls, { timeout: 15000 }).toBeGreaterThan(p0 + 2);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").doing_ms = Date.now(); });
  await expect(turn.locator(".stx-line")).toHaveCount(5);
  await expect(chip).toHaveAttribute("aria-expanded", "true");
  // Closed, it stays closed the same way.
  await chip.click();
  const p1 = await polls();
  await expect.poll(polls, { timeout: 15000 }).toBeGreaterThan(p1 + 1);
  await expect(turn.locator(".stx-line")).toHaveCount(0);
  // The running step (s5) was asked for once, though every poll redraws it.
  const asked = (await cue.calls("step_detail")).flatMap((c) => c.args.ids).filter((id) => id === "s5");
  expect(asked).toEqual(["s5"]);
});

test("the line under the chat says what it's doing whether the turn is folded or open", async ({ page, cue }) => {
  const turn = await openWorking(page, cue);
  await cue.change((st, long) => Object.assign(st.sessions.find((x) => x.session_id === "host"), { doing: `Running: ${long.slice(0, 40)}…`, doing_ms: Date.now() - 60000 }), LONG);
  const chat = page.locator(".dk-right .dk-ichat");
  await expect(turn.locator(".stx-last")).toHaveCount(0);
  await expect(chat.locator(".cv-status")).toContainText("Running: cargo test");
  await expect(chat.locator(".cv-status .stx-cnt")).toHaveText("· 3 steps");
  await turn.locator(".stx-fold").click();
  await expect(turn.locator(".stx-last")).toHaveCount(0);
  await expect(chat.locator(".cv-status")).toContainText("Running: cargo test");
});

test("an open turn of 75 steps lists them one line each and reads nothing until you click one; then just that one", async ({ page, cue }) => {
  const chat = await openTurn(page, cue, (at, step, say) => Array.from({ length: 75 }, (_, k) => step(`b${k}`, "run", "Ran", `job ${k}`, 400 - k)).concat([say("All 75 ran.", 300)]),
    [["agent", "All 75 ran.", 300]], false);
  await chat.locator(".stx-fold").click();
  await expect(chat.locator(".stx-line")).toHaveCount(75);
  await expect(chat.locator(".stx-detail")).toHaveCount(0);
  const asked = async () => (await cue.calls("step_detail")).flatMap((c) => c.args.ids).filter((id) => id !== "b74");
  expect(await asked()).toEqual([]);
  await chat.locator(".stx-line").first().click();
  await expect(chat.locator(".stx-out")).toHaveCount(1);
  await expect(chat.locator(".stx-out")).toHaveText("out b0");
  expect(await asked()).toEqual(["b0"]);
});

test("an item's page whose holder moved on to another item: the turn folds under that item's line, so the line under the chat says what it's doing", async ({ page, cue }) => {
  const chat = await openTurn(page, cue, (at, step) => [
    step("o1", "read", "Read", "rail.js", 300), step("o2", "run", "Running", "npm run rail", 290, { done: false }),
  ], [["you", "About #d4 (y): now the rail dot", 320]]);
  await cue.change((st) => Object.assign(st.sessions.find((x) => x.session_id === "host"), { doing: "Running: npm run rail", doing_ms: Date.now() - 30000 }));
  await expect(chat.locator(".dk-fold")).toContainText("Worked on #d4");
  await expect(chat.locator(".stx-last")).toHaveCount(0);
  await expect(chat.locator(".cv-status")).toContainText("Running: npm run rail");
});

test("a Stop hook's running step: not drawn while its fold is closed; the count under the chat opens the fold and it shows there once", async ({ page, cue }) => {
  const chat = await openTurn(page, cue, (at, step, say) => ({ hooked_ms: at - 200000, items: [
    step("h1", "run", "Ran", "npm test", 300), say("Done: tests pass.", 290),
    step("h2", "read", "Read", "CHECKLIST.md", 190), step("h3", "run", "Running", "npm run lint", 180, { done: false }),
  ] }), [["agent", "Done: tests pass.", 290]]);
  await expect(chat.locator(".stx-last")).toHaveCount(0);
  await expect(chat.locator(".stx-line")).toHaveCount(0);
  const cnt = chat.locator(".cv-status .stx-cnt");
  await expect(cnt).toHaveText("· 2 steps");
  await cnt.click();
  await expect(chat.locator(".stx-hook .follow-body")).toBeVisible();
  await expect(chat.locator(".stx-line").filter({ hasText: "full h3" })).toHaveCount(1);
});

test("a command that finishes keeps its full words on its line: never the cut subject while its output loads", async ({ page, cue }) => {
  const turn = await openWorking(page, cue);
  await turn.locator(".stx-fold").click();
  const foot = turn.locator(".stx-line").last();
  await expect(foot).toContainText(`Running ${LONG}`);
  // Watch every redraw for the cut subject from here on.
  await page.evaluate(() => {
    window.__cut = false;
    const look = () => { if ([...document.querySelectorAll(".dk-ichat .stx-line")].some((l) => l.textContent.includes("…"))) window.__cut = true; };
    new MutationObserver(look).observe(document.body, { subtree: true, childList: true, characterData: true });
  });
  // Its output takes a moment; meanwhile the step comes in finished.
  await cue.override("step_detail", ({ ids }, long) => new Promise((r) => setTimeout(() => r(Object.fromEntries(ids.map((id) => [id, { full: id === "s5" ? long : `full ${id}`, output: ["ok"], skipped: 0 }]))), 1500)), LONG);
  await cue.override("session_steps", ({ sessionId }, long) => {
    const at = window.__t0;
    const step = (id, kind, verb, subject, result, ago, o = {}) => ({ t: "step", id, kind, verb, subject, mono: true, result, bad: false, done: true, at_ms: at - ago * 1000, ...o });
    return sessionId !== "host" ? { version: 2, turns: [] } : { version: 2, turns: [{ at_ms: at - 120000, prompt: "About #d5 (x): go on", more: 0, items: [
      step("s1", "read", "Read", "header.js", "40 lines", 110), step("s2", "run", "Ran", "npm test", "41 passed", 100),
      { t: "say", text: "Found it: the header centres on the wrong box.", at_ms: at - 90000 },
      step("s3", "edit", "Edited", "header.js", "+1 −1", 80), step("s4", "run", "Ran", "git diff --stat", "1 file", 70),
      step("s5", "run", "Ran", long.slice(0, 79) + "…", "3 passed", 60),
    ] }] };
  }, LONG);
  await expect(foot).toContainText(`Ran ${LONG}`, { timeout: 10000 });
  await expect(foot.locator(".stx-res")).toContainText("3 passed");
  // Its output in: opened, the same line, whole.
  // Asked twice in all: running (its command), then finished (its output), which lands 1.5 s later.
  await expect.poll(async () => (await cue.calls("step_detail")).filter((c) => c.args.ids.includes("s5")).length, { timeout: 10000 }).toBe(2);
  await page.waitForTimeout(2000);
  await expect(foot).toContainText(`Ran ${LONG}`);
  expect(await page.evaluate(() => window.__cut)).toBe(false);
});

test("a turn cut off mid-step shows that step stopped: no mark, no live dot, no running time", async ({ page, cue }) => {
  const chat = await openTurn(page, cue, (at, step) => [
    step("c1", "read", "Read", "header.js", 300), step("c2", "run", "Running", "npm test", 290, { done: false }),
  ], [], false);
  const foot = chat.locator(".stx-last");
  await expect(foot).toContainText("Running full c2");
  await expect(foot.locator(".dot-live")).toHaveCount(0);
  await expect(foot.locator("[data-ago]")).toHaveCount(0);
  // No mark by default: just the words.
  await expect(foot.locator(".stx-ic")).toHaveCount(0);
  await expect(chat.locator(".stx-fold")).not.toContainText("Working");
});
