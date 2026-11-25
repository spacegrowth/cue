// Up next and Parked (#838): a click on either heading slides the room from one to the other in one motion. Nothing
// else is drawn afresh (the card's rows are the same elements after), no frame jumps back to a height outside the
// slide, a redraw that comes in mid-slide waits for it, and all three folded the green under the card shrinks with it.
import { test, expect, openDesk } from "./cue.mjs";

const open = async (page, cue, size) => {
  await page.setViewportSize(size);
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const its = st.desk.projects.find((d) => d.project === "cue").items;
    const { holder, after, ...rest } = its.find((i) => i.id === "d4");
    for (let k = 0; k < 6; k++) its.push({ ...rest, id: `q${k}`, title: `Up next ${k}` });
    for (let k = 0; k < 5; k++) its.push({ ...rest, id: `p${k}`, title: `Parked ${k}`, state: "held" });
  });
  await expect(page.locator('[data-dkn-drawer="parked"] .dkn-nx')).toHaveCount(5);
};

/** Clicks a heading and records, every frame till it settles, each drawer's and the green's height, and each time the
 *  window was drawn afresh (when, and from where). */
const slide = (page, key, redrawMidway = false) => page.evaluate(async ([key, redrawMidway]) => {
  const q = (s) => document.querySelector(s);
  const at = (k) => q(`[data-dkn-drawer="${k}"]`).getBoundingClientRect().height;
  const frames = [], calls = [];
  let t0 = 0;
  const draw = window.drawWindow;
  window.drawWindow = (...a) => { calls.push({ at: performance.now() - t0, by: new Error().stack.split("\n").slice(2, 5).join(" | ") }); return draw(...a); };
  const sample = () => frames.push([at("next"), at("parked"), q("#app .dkn-low").getBoundingClientRect().height]);
  sample();
  q(`[data-dkn-drawer="${key}"] [data-desk-c-fold]`).click();
  t0 = performance.now();
  while (performance.now() - t0 < 700) {
    await new Promise(requestAnimationFrame);
    if (redrawMidway && frames.length === 5) renderMain();
    sample();
  }
  window.drawWindow = draw;
  return { calls, frames };
}, [key, redrawMidway]);

/** Nothing is drawn afresh while it slides: a redraw that comes in meanwhile is held till it has landed. */
const drawnOnlyAfter = (r) => {
  for (const c of r.calls) expect(c.at, JSON.stringify(r.calls)).toBeGreaterThan(250);
};
/** Each height moves one way only, from its start to its end: no frame flicks back or past it. */
const oneWay = (frames) => {
  for (let c = 0; c < frames[0].length; c++) {
    const hs = frames.map((f) => f[c]), a = hs[0], b = hs.at(-1), lo = Math.min(a, b) - 1, hi = Math.max(a, b) + 1;
    for (let k = 1; k < hs.length; k++) {
      expect(hs[k], `column ${c} frame ${k}: ${hs.join(",")}`).toBeGreaterThanOrEqual(lo);
      expect(hs[k], `column ${c} frame ${k}: ${hs.join(",")}`).toBeLessThanOrEqual(hi);
      expect(Math.sign(hs[k] - hs[k - 1]) * Math.sign(b - a), `column ${c} frame ${k}: ${hs.join(",")}`).toBeGreaterThanOrEqual(0);
    }
  }
};

for (const [name, size] of [["laptop", { width: 1440, height: 900 }], ["smallest window", { width: 900, height: 600 }]]) {
  test(`Up next and Parked slide in one motion, nothing else drawn again (#838, ${name})`, async ({ page, cue }) => {
    await open(page, cue, size);
    const head = (k) => page.locator(`[data-dkn-drawer="${k}"] [data-desk-c-fold]`);
    await expect(head("next")).toHaveAttribute("aria-expanded", "true");
    // Parked opens, Up next folds.
    let r = await slide(page, "parked");
    drawnOnlyAfter(r);
    oneWay(r.frames);
    expect(r.frames.at(-1)[1]).toBeGreaterThan(r.frames[0][1] + 20);
    await expect(head("parked")).toHaveAttribute("aria-expanded", "true");
    await expect(head("next")).toHaveAttribute("aria-expanded", "false");
    await expect(page.locator('[data-dkn-drawer="parked"] .dkn-nx').first()).toBeVisible();
    await expect(page.locator('[data-dkn-drawer="next"] .dkn-nx').first()).toBeHidden();
    // Back to Up next, with a redraw asked for mid-way: it waits, the slide goes on.
    r = await slide(page, "next", true);
    oneWay(r.frames);
    await expect(head("next")).toHaveAttribute("aria-expanded", "true");
    await expect(page.locator('[data-dkn-drawer="next"] .dkn-nx').first()).toBeVisible();
    // Up next folded with Done today folded too: the green under the card shrinks to the headings in the same slide.
    if ((await head("done").getAttribute("aria-expanded")) === "true") await head("done").click();
    r = await slide(page, "next");
    drawnOnlyAfter(r);
    oneWay(r.frames);
    expect(r.frames.at(-1)[2]).toBeLessThan(r.frames[0][2] - 20);
    // And open again: it grows back the same way.
    r = await slide(page, "next");
    drawnOnlyAfter(r);
    oneWay(r.frames);
    expect(r.frames.at(-1)[2]).toBeGreaterThan(r.frames[0][2] + 20);
    // Settled, its list ends on a whole row with "↓ N more" under it, measured as it is open (in the smallest window
    // the green scrolls as a whole instead, the lists their full height).
    const list = page.locator('[data-dkn-drawer="next"] .dkn-dl');
    if ((await list.evaluate((el) => getComputedStyle(el).overflowY)) === "auto") await expect(page.locator('[data-dkn-drawer="next"] .dkn-more')).toBeVisible();
    else expect(await list.evaluate((el) => el.scrollHeight <= el.clientHeight + 1)).toBe(true);
  });
}

test("dark: Up next and Parked slide the same (#838)", async ({ page, cue }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await open(page, cue, { width: 1440, height: 900 });
  const r = await slide(page, "parked");
  drawnOnlyAfter(r);
  oneWay(r.frames);
});
