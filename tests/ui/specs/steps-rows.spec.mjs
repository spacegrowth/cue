// A turn's steps in a chat, as in release 0.1.86 (#897): each step its own row (what it did, the file, how it went,
// a failed one in red), the newest turn open and its steps coming in as they run under "Working · N steps"; older
// turns fold to one chip with a summary, what it said left in view. "Show all" opens every step's output or diff; a
// step opens on its own when you click it. No "· N steps" counts anywhere.
import { test, expect, openDesk } from "./cue.mjs";

const LONG = "cargo test --lib desk::hub::landing::copy_guard 2>&1 | grep -E \"test result|FAILED|panicked\" | tail -20 && git status --short";

/** host-lead at work on #d5: two steps, a message, two more steps, then a long command running now. */
const steps = (cue) => cue.override("session_steps", ({ sessionId }, long) => {
  const at = (window.__t0 ||= Date.now());
  const step = (id, kind, verb, subject, result, ago, o = {}) => ({ t: "step", id, kind, verb, subject, mono: true, result, bad: false, done: true, at_ms: at - ago * 1000, ...o });
  return sessionId !== "host" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at - 120000, prompt: "About #d5 (x): go on", more: 0, items: [
    step("s1", "read", "Read", "header.js", "40 lines", 110),
    step("s2", "run", "Ran", "npm test", "exit 1", 100, { bad: true }),
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
  return page.locator(".dk-right .dkn-sp[data-desk-page='session'] .stx");
};

test("while it works: every step a row in place, the running one last with its live dot, a failed one in red; no counts", async ({ page, cue }) => {
  const turn = await openWorking(page, cue);
  const chip = turn.locator(".stx-fold");
  await expect(chip).toContainText("Working · 5 steps");
  await expect(chip).toHaveAttribute("aria-expanded", "true");
  await expect(chip.locator(".dot-live")).toHaveCount(1);
  await expect(chip.locator(".stx-sum")).toHaveCount(0);
  await expect(turn.locator(".stx-line")).toHaveText([/Read header\.js\s*40 lines/, /Ran npm test\s*exit 1/, /Edited header\.js\s*\+1 −1/, /Ran git diff --stat/, /Running cargo test/]);
  // The message sits between its steps, where it fell.
  const order = await turn.evaluate((el) => [...el.querySelectorAll(".stx-steps, .stx-say")].map((x) => (x.classList.contains("stx-say") ? "say" : x.querySelectorAll(".stx-line").length)));
  expect(order).toEqual([2, "say", 3]);
  // The running step: a live dot, no time.
  const run = turn.locator(".stx-line").last();
  await expect(run.locator(".stx-res .dot-live")).toHaveCount(1);
  await expect(run.locator(".stx-res [data-ago]")).toHaveCount(0);
  // The failed command in red, without opening it.
  const bad = turn.locator(".stx-line.bad");
  await expect(bad).toHaveCount(1);
  const deny = await page.evaluate(() => { const d = document.createElement("i"); d.style.color = "var(--deny)"; document.body.append(d); const c = getComputedStyle(d).color; d.remove(); return c; });
  expect(await bad.locator(".stx-res").evaluate((el) => getComputedStyle(el).color)).toBe(deny);
  // Each step one line; nothing read until you click one.
  await expect(turn.locator(".stx-detail")).toHaveCount(0);
  await expect(page.locator(".stx-cnt, .stx-now, .stx-last")).toHaveCount(0);
  // Under the chat: what it's doing, as 0.1.86 said it.
  const status = page.locator(".dk-right .dkn-sp[data-desk-page='session'] .cv-status");
  await expect(status).toContainText("Testing the fix");
});

test("new steps come in as rows while it works", async ({ page, cue }) => {
  const turn = await openWorking(page, cue);
  await expect(turn.locator(".stx-line")).toHaveCount(5);
  await cue.override("session_steps", ({ sessionId }) => {
    const at = window.__t0;
    const step = (id, kind, verb, subject, ago, o = {}) => ({ t: "step", id, kind, verb, subject, mono: true, result: "ok", bad: false, done: true, at_ms: at - ago * 1000, ...o });
    return sessionId !== "host" ? { version: 2, turns: [] } : { version: 2, turns: [{ at_ms: at - 120000, prompt: "About #d5 (x): go on", more: 0, items: [
      step("s1", "read", "Read", "header.js", 110), step("s2", "run", "Ran", "npm test", 100),
      { t: "say", text: "Found it: the header centres on the wrong box.", at_ms: at - 90000 },
      step("s3", "edit", "Edited", "header.js", 80), step("s4", "run", "Ran", "git diff --stat", 70), step("s5", "run", "Ran", "npm test", 60),
      step("s6", "run", "Running", "npm run lint", 50, { done: false }),
    ] }] };
  });
  await expect(turn.locator(".stx-line")).toHaveCount(6, { timeout: 15000 });
  await expect(turn.locator(".stx-line").last()).toHaveAttribute("data-key", "s6");
  await expect(turn.locator(".stx-fold")).toContainText("Working · 6 steps");
});

test("the chip folds the turn to a summary, its words left in view; Show all opens every step, a click closes one", async ({ page, cue }) => {
  const turn = await openWorking(page, cue);
  const chip = turn.locator(".stx-fold");
  await chip.click();
  await expect(chip).toHaveAttribute("aria-expanded", "false");
  await expect(chip).toContainText("read 1 file · 1 edit · 3 commands · 1 failed");
  await expect(turn.locator(".stx-line")).toHaveCount(0);
  await expect(turn.locator(".stx-say")).toContainText("Found it");
  await expect(turn.locator(".stx-all")).toHaveCount(0);
  await chip.click();
  await expect(turn.locator(".stx-line")).toHaveCount(5);
  // Show all: every finished step open, with its output or diff.
  await turn.locator(".stx-all").click();
  await expect(turn.locator(".stx-all")).toHaveText("Show less");
  await expect(turn.locator(".stx-line.open")).toHaveCount(4);
  await expect(turn.locator(".stx-diff")).toHaveCount(1);
  const ran = turn.locator(".stx-line").filter({ hasText: "Ran npm test" });
  await ran.click();
  await expect(turn.locator(".stx-line.open")).toHaveCount(3);
  // Show less: as in 0.1.86, a step you flipped by hand flips back the other way.
  await turn.locator(".stx-all").click();
  await expect(turn.locator(".stx-line.open")).toHaveCount(1);
  await expect(turn.locator(".stx-out")).toHaveText("ok");
  await ran.click();
  await expect(turn.locator(".stx-detail")).toHaveCount(0);
});

test("a diff you open is plain green and red, whole, in light and dark; the row's +N −N in the accent and red", async ({ page, cue }) => {
  const turn = await openWorking(page, cue);
  await turn.locator(".stx-line").filter({ hasText: "Edited header.js" }).click();
  const diff = turn.locator(".stx-diff");
  await expect(diff).toHaveCount(1);
  // One fixed set in light and dark alike (#647): diffs aren't theme-aware.
  const one = { add: "rgb(218, 251, 225)", addFg: "rgb(26, 127, 55)", del: "rgb(255, 235, 233)", delFg: "rgb(207, 34, 46)" };
  const want = { light: one, dark: one };
  for (const theme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: theme });   // as the Mac switches it: a redraw puts Settings → Appearance (system) back over a data-theme set by hand
    const got = await diff.evaluate((el) => {
      const cs = (q, p) => getComputedStyle(el.querySelector(q))[p];
      return { add: cs(".a", "backgroundColor"), addFg: cs(".a i", "color"), del: cs(".d", "backgroundColor"), delFg: cs(".d i", "color"),
        res: [getComputedStyle(el.closest(".stx").querySelector(".stx-res .add")).color, getComputedStyle(el.closest(".stx").querySelector(".stx-res .del")).color],
        maxH: getComputedStyle(el).maxHeight, ws: getComputedStyle(el.querySelector(".a")).whiteSpace };
    });
    // The row's own +N −N as 0.1.86 had them: the theme's accent and its red.
    const [accent, deny] = await page.evaluate(() => ["--accent", "--deny"].map((v) => { const d = document.createElement("i"); d.style.color = `var(${v})`; document.body.append(d); const c = getComputedStyle(d).color; d.remove(); return c; }));
    expect(got).toEqual({ ...want[theme], res: [accent, deny], maxH: "none", ws: "pre-wrap" });
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
  return page.locator(".dk-right .dkn-sp[data-desk-page='session']");
};

test("a finished newest turn stays open, every step a row; its reply drawn once, with no counts", async ({ page, cue }) => {
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
  await expect(chip).toHaveAttribute("aria-expanded", "true");
  await expect(chat.locator(".stx-line")).toHaveCount(5);
  await expect(chat.getByText(REPLY)).toHaveCount(1);
  await expect(chat.locator(".stx .stx-say")).toHaveText(["Found it: the wrong box."]);
  await expect(chat.locator(".stx-cnt")).toHaveCount(0);
});

test("a turn you wrote into mid-turn: one chip, your message where you sent it, steps as rows on both sides", async ({ page, cue }) => {
  const chat = await openTurn(page, cue, (at, step, say) => [
    step("m1", "read", "Read", "header.js", 300), step("m2", "run", "Ran", "npm test", 290),
    step("m3", "edit", "Edited", "header.js", 270), say("Done the header; the footer next.", 260),
    step("m4", "run", "Running", "npm test", 250, { done: false }),
  ], [["you", "About #d5 (x): check the footer too", 280]]);
  await expect(chat.locator(".stx-fold")).toHaveCount(1);
  await expect(chat.locator(".stx-fold")).toContainText("Working · 4 steps");
  const order = await chat.evaluate((el) => [...el.querySelectorAll(".stx-fold, .cv-you, .stx-say, .stx-steps")].filter((x) => !x.classList.contains("cv-you") || x.textContent.includes("#d5")).map((x) => x.classList.contains("stx-fold") ? "chip" : x.classList.contains("cv-you") ? (x.textContent.includes("footer too") ? "you" : "go on") : x.classList.contains("stx-say") ? "say" : x.querySelectorAll(".stx-line").length));
  expect(order).toEqual(["go on", "chip", 2, "you", 1, "say", 1]);
});

test("your fold stays across polls and redraws; a running step's output isn't asked for", async ({ page, cue }) => {
  const turn = await openWorking(page, cue);
  const chip = turn.locator(".stx-fold");
  await chip.click();
  await expect(turn.locator(".stx-line")).toHaveCount(0);
  const polls = async () => (await cue.calls("session_steps")).length;
  const p0 = await polls();
  await expect.poll(polls, { timeout: 15000 }).toBeGreaterThan(p0 + 2);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").doing_ms = Date.now(); });
  await expect(turn.locator(".stx-line")).toHaveCount(0);
  await expect(chip).toHaveAttribute("aria-expanded", "false");
  await chip.click();
  const p1 = await polls();
  await expect.poll(polls, { timeout: 15000 }).toBeGreaterThan(p1 + 1);
  await expect(turn.locator(".stx-line")).toHaveCount(5);
  const asked = (await cue.calls("step_detail")).flatMap((c) => c.args.ids).filter((id) => id === "s5");
  expect(asked).toEqual([]);
});

test("an open turn of 75 steps lists them one line each and reads nothing until you click one; then just that one", async ({ page, cue }) => {
  const chat = await openTurn(page, cue, (at, step, say) => Array.from({ length: 75 }, (_, k) => step(`b${k}`, "run", "Ran", `job ${k}`, 400 - k)).concat([say("All 75 ran.", 300)]),
    [["agent", "All 75 ran.", 300]], false);
  await expect(chat.locator(".stx-line")).toHaveCount(75);
  await expect(chat.locator(".stx-detail")).toHaveCount(0);
  const asked = async () => (await cue.calls("step_detail")).flatMap((c) => c.args.ids).filter((id) => id !== "b74");
  expect(await asked()).toEqual([]);
  await chat.locator(".stx-line").first().click();
  await expect(chat.locator(".stx-out")).toHaveCount(1);
  await expect(chat.locator(".stx-out")).toHaveText("out b0");
  expect(await asked()).toEqual(["b0"]);
});

test("a Stop hook's steps stay in its own fold, which shows a live dot while one runs", async ({ page, cue }) => {
  const chat = await openTurn(page, cue, (at, step, say) => ({ hooked_ms: at - 200000, items: [
    step("h1", "run", "Ran", "npm test", 300), say("Done: tests pass.", 290),
    step("h2", "read", "Read", "CHECKLIST.md", 190), step("h3", "run", "Running", "npm run lint", 180, { done: false }),
  ] }), [["agent", "Done: tests pass.", 290]]);
  await expect(chat.locator(".stx-line")).toHaveText([/Ran npm test/]);
  await expect(chat.locator(".stx-hook .dot-live")).toHaveCount(1);
  await chat.locator(".stx-hook [data-hook-fold]").click();
  await expect(chat.locator(".stx-hook .stx-line")).toHaveCount(2);
});
