// Settings → Chat → "Bullets with a clock" (#931), off unless you turn it on: a turn's words as bullets, each with
// when it started on the left; the live one pulses with its running step under it; "N steps ›"
// opens a bullet's steps as rows. The turn's count sits in a bar at the top while it works, a "Worked …" chip after.
import { test, expect, openDesk } from "./cue.mjs";

/** host-lead at work on #d5: two steps (one failed), what it found, two steps, then one running now. */
const open = async (page, cue, { bullets = true, working = true, extra = 0, mid = false } = {}) => {
  await cue.open("desk");
  await cue.override("session_steps", ({ sessionId }, working) => {
    const at = (window.__t0 ||= Date.now());
    const step = (id, kind, verb, subject, result, ago, o = {}) => ({ t: "step", id, kind, verb, subject, mono: true, result, bad: false, done: true, at_ms: at - ago * 1000, ...o });
    const items = [
      step("s1", "read", "Read", "header.js", "40 lines", 110),
      window.__mid ? step("s2", "run", "Running", "npm test", "", 100, { done: false }) : step("s2", "run", "Ran", "npm test", "exit 1", 100, { bad: true }),
      { t: "say", text: "Found it: the header centres on the wrong box.", at_ms: at - 90000 },
      step("s3", "edit", "Edited", "header.js", "+1 −1", 80),
      step("s4", "run", "Ran", "git diff --stat", "1 file", 70),
      ...Array.from({ length: window.__extra || 0 }, (_, n) => step(`x${n}`, "read", "Read", `file${n}.js`, "10 lines", 69 - n * 0.1)),
      working ? step("s5", "run", "Running", "cargo test --lib", "", 60, { done: false }) : step("s5", "run", "Ran", "cargo test --lib", "ok", 60),
      ...(working ? [] : [{ t: "say", text: "Fixed: the header centres on its own box now.", at_ms: at - 30000 }]),
    ];
    return sessionId !== "host" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at - 120000, prompt: "About #d5 (x): go on", more: 0, items }] };
  }, working);
  await cue.override("step_detail", ({ ids }) => Object.fromEntries(ids.map((id) => [id, id === "s3"
    ? { full: "/repo/ui/header.js", diff: [["-", "  return box.parent;"], ["+", "  return box;"]], skipped: 0 }
    : { full: `full ${id}`, output: [`out ${id}`], skipped: 0 }])));
  await page.evaluate(([e, m]) => { window.__t0 ||= Date.now(); window.__extra = e; window.__mid = m; }, [extra, mid]);
  await cue.change((st, [bullets, working, mid]) => {
    st.settings = { ...(st.settings || {}), steps: { ...(st.settings?.steps || {}), mode: "line", bullets } };
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: working ? "working" : "review", now_on: working, said: [] });
    Object.assign(st.sessions.find((x) => x.session_id === "host"), { state: working ? "working" : "idle",
      thread: [{ role: "you", text: "About #d5 (x): go on", at_ms: window.__t0 - 125000 }, ...(mid ? [{ role: "you", text: "also check the footer", at_ms: window.__t0 - 95000 }] : []), ...(working ? [] : [{ role: "agent", text: "Fixed: the header centres on its own box now.", at_ms: window.__t0 - 30000 }])] });
  }, [bullets, working, mid]);
  await openDesk(page);
  await page.locator('[data-desk-item="d5"]').first().click();
  return page.locator(".dk-right .dkn-sp[data-desk-page='session'] .stx");
};

test("off by default: the steps as rows under Working · N steps, no bullets", async ({ page, cue }) => {
  const turn = await open(page, cue, { bullets: false });
  await expect(turn.locator(".stx-fold")).toContainText("Working · 5 steps");
  await expect(turn.locator(".stx-line")).toHaveCount(5);
  await expect(turn.locator(".bul-row")).toHaveCount(0);
});

test("while it works: a bar with the count, a bullet per thing it said, times and marks, the live one with its running step", async ({ page, cue }) => {
  const turn = await open(page, cue);
  const bar = turn.locator(".bul-pin");
  await expect(bar).toContainText(/Working · \d+:\d\d/);
  await expect(bar).toContainText("5 steps · read 1 file · 1 edit · 3 commands");
  await expect(bar.locator(".bul-bad")).toHaveText("· 1 failed");
  await expect(bar.locator(".stx-all")).toHaveText("Show all steps");
  const rows = turn.locator(".bul-row");
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(0)).toContainText("First steps");
  await expect(rows.nth(0)).toHaveClass(/done/);
  await expect(rows.nth(0)).toHaveClass(/fail/);
  await expect(rows.nth(0).locator(".bul-t")).toHaveText("0:10");
  // No marks per step: the time alone on the left; the red ring and the count say what failed.
  await expect(rows.nth(0).locator(".bul-c")).toHaveText("0:10");
  await expect(rows.nth(0).locator(".bul-n")).toHaveText("1 failed · 2 steps ›");
  const live = rows.nth(1);
  await expect(live).toHaveClass(/now/);
  await expect(live.locator(".bul-x")).toHaveText("Found it: the header centres on the wrong box.");
  await expect(live.locator(".bul-t")).toHaveText(/^2:0\d$/);
  await expect(live.locator(".bul-run")).toContainText("Running cargo test --lib");
  await expect(live.locator(".bul-run [data-ago]")).toHaveCount(1);
  // No step rows until you open a bullet's steps.
  await expect(turn.locator(".stx-line")).toHaveCount(0);
});

test("a click anywhere on a bullet's line opens its steps and closes them again; an open bullet has no panel of its own", async ({ page, cue }) => {
  const turn = await open(page, cue, { working: false });
  const first = turn.locator(".bul-row").first();
  await expect(first.locator(".bul-x")).toHaveText("First steps");
  // Closed, its line shows a soft ground under the mouse; open, none.
  await first.locator(".bul-x").hover();
  expect(await first.evaluate((e) => getComputedStyle(e).backgroundColor)).not.toBe("rgba(0, 0, 0, 0)");
  await first.locator(".bul-x").click();
  await expect(first).toHaveClass(/open/);
  await expect(first.locator(".stx-line")).toHaveText([/Read header\.js/, /Ran npm test/]);
  // The chat's own ground, as steps had before bullets: no tinted box around the open bullet.
  expect(await first.evaluate((e) => getComputedStyle(e).backgroundColor)).toBe("rgba(0, 0, 0, 0)");
  await first.locator(".bul-x").click();
  await expect(first.locator(".stx-line")).toHaveCount(0);
});

test("a bullet's steps open under it, a step opens its diff, and close again; Show all steps opens every bullet", async ({ page, cue }) => {
  const turn = await open(page, cue);
  const live = turn.locator(".bul-row").nth(1);
  await live.locator(".bul-n").click();
  await expect(live).toHaveClass(/open/);
  await expect(live.locator(".stx-line")).toHaveText([/Edited header\.js/, /Ran git diff --stat/, /Running cargo test/]);
  await expect(live.locator(".bul-run")).toHaveCount(0);
  await expect(live.locator(".bul-n")).toHaveText("3 steps ⌄");
  await live.locator(".stx-line").filter({ hasText: "Edited header.js" }).click();
  await expect(live.locator(".stx-diff")).toHaveCount(1);
  await live.locator(".bul-n").click();
  await expect(live.locator(".stx-line")).toHaveCount(0);
  await expect(live.locator(".bul-run")).toHaveCount(1);
  await turn.locator(".bul-pin .stx-all").click();
  await expect(turn.locator(".bul-pin .stx-all")).toHaveText("Hide steps");
  await expect(turn.locator(".stx-line")).toHaveCount(5);
  // Rows only: the one step you opened stays open, the rest show no output until clicked.
  await expect(turn.locator(".stx-detail")).toHaveCount(1);
  await expect(turn.locator(".stx-diff")).toHaveCount(1);
});

test("once it's done: a Worked chip with its steps and updates; the reply once; folding leaves only the chip", async ({ page, cue }) => {
  const turn = await open(page, cue, { working: false });
  const chip = turn.locator(".stx-fold");
  await expect(chip).toContainText(/Worked 1m 30s · 5 steps · 1 update/);
  await expect(chip.locator(".bul-bad")).toHaveText("· 1 failed");
  await expect(turn.locator(".bul-pin")).toHaveCount(0);
  await expect(turn.locator(".bul-row")).toHaveCount(2);
  await expect(turn.locator(".bul-row.now")).toHaveCount(0);
  await expect(page.getByText("Fixed: the header centres on its own box now.")).toHaveCount(1);
  await chip.click();
  await expect(turn.locator(".bul-row")).toHaveCount(0);
  await expect(chip).toHaveAttribute("aria-expanded", "false");
});

test("opening a bullet's steps low in the chat glides them up into view", async ({ page, cue }) => {
  const turn = await open(page, cue);
  await page.setViewportSize({ width: 1280, height: 520 });
  const live = turn.locator(".bul-row").nth(1);
  // Its line at the chat's bottom, as when the reply's newest bullet is what you see last.
  await live.evaluate((row) => { const c = scrollerOf(row); c.scrollTop += row.getBoundingClientRect().bottom - c.getBoundingClientRect().bottom + 4; });
  const chat = page.locator(".dk-right .dkn-sp[data-desk-page='session'] .dk-chat");
  await live.locator(".bul-n").click();
  const last = live.locator(".stx-line").last();
  await expect(last).toHaveText(/Running cargo test/);
  await expect.poll(async () => {
    const [r, b] = await Promise.all([last.boundingBox(), chat.boundingBox()]);
    return r.y + r.height <= b.y + b.height;
  }).toBe(true);
  await expect.poll(async () => (await live.boundingBox()).y >= (await chat.boundingBox()).y).toBe(true);
});

test("a long bullet's steps opened at the chat's end: its own line stays in view, under the bar", async ({ page, cue }) => {
  const turn = await open(page, cue, { extra: 30 });
  await page.setViewportSize({ width: 1280, height: 520 });
  const live = turn.locator(".bul-row").nth(1);
  await live.evaluate((row) => { const c = scrollerOf(row); c.scrollTop = c.scrollHeight; });
  const chat = page.locator(".dk-right .dkn-sp[data-desk-page='session'] .dk-chat");
  await live.locator(".bul-n").click();
  await expect(live.locator(".stx-line")).toHaveCount(33);
  await page.waitForTimeout(400);   // past the glide and the chat's own settling
  const [r, bar] = await Promise.all([live.boundingBox(), turn.locator(".bul-pin").boundingBox()]);
  expect(r.y).toBeGreaterThanOrEqual(bar.y + bar.height - 1);
  expect(r.y).toBeLessThan((await chat.boundingBox()).y + 120);
});

test("a reply you wrote into while it worked: only the bullet at work now pulses and shows its running step", async ({ page, cue }) => {
  const turn = await open(page, cue, { mid: true });
  const msgs = page.locator(".dk-right .dkn-sp[data-desk-page='session']");
  await expect(msgs.locator(".bul-row")).toHaveCount(2);
  await expect(msgs.locator(".bul-row.now")).toHaveCount(1);
  await expect(msgs.locator(".bul-run")).toHaveCount(1);
  await expect(msgs.locator(".bul-row.now .bul-run")).toContainText("Running cargo test --lib");
  await expect(turn.first().locator(".bul-row").first()).toContainText("First steps");
  await expect(turn.first().locator(".bul-row.now")).toHaveCount(0);
});

test("closing a bullet's steps folds them up, then they're gone and the chat stays in place", async ({ page, cue }) => {
  const turn = await open(page, cue, { extra: 6 });
  await page.setViewportSize({ width: 1280, height: 520 });
  const live = turn.locator(".bul-row").nth(1);
  await live.locator(".bul-n").click();
  await expect(live.locator(".stx-line")).toHaveCount(9);
  await page.waitForTimeout(400);
  const full = (await live.locator(".stx-steps").boundingBox()).height;
  await live.locator(".bul-n").click();
  // Part-way: still there, shorter than it was.
  await page.waitForTimeout(90);
  const mid = await live.locator(".stx-steps").evaluate((el) => el.getBoundingClientRect().height).catch(() => null);
  expect(mid).not.toBeNull();
  expect(mid).toBeGreaterThan(0);
  expect(mid).toBeLessThan(full);
  await expect(live.locator(".stx-steps")).toHaveCount(0);
  await expect(live.locator(".bul-n")).toHaveText("9 steps ›");
  await expect(live.locator(".bul-run")).toHaveCount(1);
});
