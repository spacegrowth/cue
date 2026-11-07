// Layout: nothing on screen sits on top of something else or is cut off by its box. The screenshots catch a
// change; these say what's wrong in words, and keep saying it if a baseline is ever rewritten over a bug.
// Each check measures in the page in one go, so a redraw (they come as steps land) can't fall between two reads.
import { test, expect } from "./cue.mjs";

// The smallest the window goes (tauri.conf.json minWidth).
const NARROWEST = { width: 900, height: 600 };
const openCard = (page, id) => page.locator(`.nrow[data-big="${id}"]`).click();
const openLead = async (page) => { await openCard(page, "n1"); await page.locator(".active-pane .rv-row").first().waitFor(); };

/** What's wrong with the layout under each of `sel`, as sentences ([] when nothing is): `checks` names what to measure,
 *  run in the page. inside: [part, box] (each `part` under it lies inside its nearest `box`, or the matched element
 *  itself when `box` is ""); apart: a selector whose matches mustn't overlap one another. Half a pixel's give for
 *  subpixel layout. */
function layoutProblems(page, sel, checks) {
  return page.evaluate(([sel, checks]) => {
    const out = [];
    const shown = (el) => el.getClientRects().length > 0 && getComputedStyle(el).visibility !== "hidden";
    const say = (el) => `${el.className || el.tagName} "${el.textContent.trim().slice(0, 30)}"`;
    const rect = (el) => el.getBoundingClientRect();
    for (const root of document.querySelectorAll(sel)) {
      if (!shown(root)) continue;
      for (const [part, box] of checks.inside || []) {
        for (const el of root.querySelectorAll(part)) {
          if (!shown(el)) continue;
          const b = box ? el.parentElement.closest(box) : root, r = rect(el), o = rect(b);
          if (r.left < o.left - 0.5 || r.top < o.top - 0.5 || r.right > o.right + 0.5 || r.bottom > o.bottom + 0.5) out.push(`${say(el)} sticks out of ${say(b)}`);
        }
      }
      for (const part of checks.apart || []) {
        const els = [...root.querySelectorAll(part)].filter(shown);
        for (let i = 0; i < els.length; i++) for (let j = i + 1; j < els.length; j++) {
          const a = rect(els[i]), b = rect(els[j]);
          if (Math.min(a.right, b.right) - Math.max(a.left, b.left) > 0.5 && Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 0.5) out.push(`${say(els[i])} overlaps ${say(els[j])}`);
        }
      }
    }
    return out;
  }, [sel, checks]);
}
/** A line of chips, tags and buttons: its parts (and their parts: a card's top line groups who it is and its time and
 *  buttons) apart, each inside `box` (a card's × reaches into the card's padding, so a card's top line is measured
 *  against the card), and each chip's words inside the chip. */
const LINE = (box) => ({
  apart: [":scope > *", ":scope > * > *", ".tm-chip"],
  inside: [[":scope > *", box], [":scope > * > *", box], [".tm-chip > *", ".tm-chip"]],
});
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
