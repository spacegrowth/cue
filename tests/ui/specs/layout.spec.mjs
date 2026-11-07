// Layout: nothing on screen sits on top of something else or is cut off by its box. The screenshots catch a
// change; these say what's wrong in words, and keep saying it if a baseline is ever rewritten over a bug.
// Each check measures in the page in one go, so a redraw (they come as steps land) can't fall between two reads.
import { test, expect } from "./cue.mjs";
import { layoutProblems, LINE } from "./layout.mjs";

// The smallest the window goes (tauri.conf.json minWidth).
const NARROWEST = { width: 900, height: 600 };
const openCard = (page, id) => page.locator(`.nrow[data-big="${id}"]`).click();
const openLead = async (page) => { await openCard(page, "n1"); await page.locator(".active-pane .rv-row").first().waitFor(); };

/** The words of what `sel` shows that are split over two lines, by where each word's letters land. */
const splitWords = (page, sel) => page.evaluate((sel) => {
  const out = [];
  for (const el of document.querySelectorAll(sel)) {
    const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    for (let t = walk.nextNode(); t; t = walk.nextNode()) {
      for (const m of t.data.matchAll(/\S+/g)) {
        const r = document.createRange();
        r.setStart(t, m.index);
        r.setEnd(t, m.index + m[0].length);
        if (new Set([...r.getClientRects()].map((x) => Math.round(x.top))).size > 1) out.push(m[0]);
      }
    }
  }
  return out;
}, sel);

test("a lead's chat opens with its first review row in full view", async ({ cue, page }) => {
  await cue.open("relay-review", { frozen: true });
  await openLead(page);
  expect(await layoutProblems(page, ".rv-rows", { inside: [[".rv-row:first-child", ""]] })).toEqual([]);
});

for (const [size, viewport] of [["", null], [" at the narrowest window", NARROWEST]]) {
  test(`a lead's chat header: its chips and buttons never run into each other${size}`, async ({ cue, page }) => {
    if (viewport) await page.setViewportSize(viewport);
    await cue.open("relay-review", { frozen: true });
    await openLead(page);
    expect(await layoutProblems(page, ".ap-head", LINE(""))).toEqual([]);
  });
}

for (const [scenario, size, viewport] of [["board-waiting", "", null], ["relay-review", "", null], ["board-waiting", " at the narrowest window", NARROWEST]]) {
  test(`${scenario}: every Waiting card's top line keeps its name, chips, time and buttons apart and inside the card${size}`, async ({ cue, page }) => {
    if (viewport) await page.setViewportSize(viewport);
    await cue.open(scenario, { frozen: true });
    expect(await layoutProblems(page, ".sec-waiting .nrow-top", LINE(".nrow"))).toEqual([]);
  });
}

test("at the narrowest window a command on its card wraps between its words, never inside one", async ({ cue, page }) => {
  await page.setViewportSize(NARROWEST);
  await cue.open("board-waiting", { frozen: true });
  await expect(page.locator(".nrow-text.mono")).not.toHaveCount(0);
  expect(await splitWords(page, ".nrow-text.mono")).toEqual([]);
});

test("a waiting stretch too short for its stripes is drawn solid, not as an outlined sliver", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  // Every session's last half hour: waiting stretches of 20 s to 5 min between stretches of work.
  await cue.change((st) => {
    const t = Date.now() - 1_800_000, s = 1000, m = 60 * s;
    const waits = [20 * s, 40 * s, 60 * s, 2 * m, 5 * m];
    for (const x of st.sessions) {
      let at = t;
      x.segments = waits.flatMap((w) => { const g = [{ kind: "working", start_ms: at, end_ms: at + 3 * m }, { kind: "waiting", start_ms: at + 3 * m, end_ms: at + 3 * m + w }]; at += 3 * m + w; return g; });
    }
  });
  // Each striped layer spans its whole bar, masked to its stretches: their widths are the mask's opaque spans.
  // One stripe of the pattern runs 6 / sin 45° ≈ 8.5 px along the bar; a stretch narrower than that is drawn solid
  // (its layer `z thin`), never striped.
  const striped = await page.evaluate(() => [...document.querySelectorAll(".bar i.z.layer:not(.thin)")].flatMap((el) => {
    const width = el.getBoundingClientRect().width;
    const at = [...(el.style.maskImage || el.style.webkitMaskImage).matchAll(/(?:#000|rgb\(0, 0, 0\)) ([\d.]+)%/g)].map((m) => +m[1]);
    return at.filter((_, i) => i % 2 === 0).map((a, i) => ((at[2 * i + 1] - a) / 100) * width);
  }));
  expect(await page.locator(".bar i.z.layer.thin").count(), "the short waiting stretches, drawn solid").toBeGreaterThan(0);
  expect(striped.length, "and its wide ones, striped").toBeGreaterThan(0);
  expect(striped.filter((px) => px < 8.5)).toEqual([]);
});

test("the open status list keeps its switch and what it costs in full view, under steps that scroll", async ({ cue, page }) => {
  await cue.open("status-list", { frozen: true });
  await openCard(page, "n1");
  await page.locator('.active-pane .st-bar[data-sid="relay"]').click();
  await page.locator(".active-pane .st-drawer").waitFor();
  expect(await layoutProblems(page, ".st-drawer", { inside: [[".st-toggle", ""], [".st-note", ""]] })).toEqual([]);
});

test("Settings → Agents: a command in a setting's note stays whole", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  await page.keyboard.press(",");
  await page.locator('[data-act="set-tab"][data-tab="agents"]').click();
  await page.locator(".sheet .set-sub code").first().waitFor();
  expect(await splitWords(page, ".sheet .set-sub")).toEqual([]);
});

test("the board's column headings line up", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  const tops = await page.evaluate(() => [".col.main", ".col-waiting", ".col-sessions"].map((c) => {
    const r = document.createRange();
    r.selectNodeContents(document.querySelector(`${c} .col-head`));
    return Math.round(r.getClientRects()[0].top);
  }));
  expect(new Set(tops).size, `heading tops ${tops}`).toBe(1);
});

test("a lead's plan alone in its card has no rule above it", async ({ cue, page }) => {
  await cue.open("relay-review", { frozen: true });
  await openLead(page);
  const foot = page.locator(".active-pane .team-card > .tc-foot:first-child");
  await expect(foot).toHaveCount(1);
  expect(await foot.evaluate((el) => getComputedStyle(el).borderTopStyle)).toBe("none");
});
