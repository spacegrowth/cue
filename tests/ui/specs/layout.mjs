// Layout checks the UI specs share: what's wrong with a layout, said in sentences (layout.spec.mjs, world.spec.mjs).

/** What's wrong with the layout under each of `sel`, as sentences ([] when nothing is): `checks` names what to measure,
 *  run in the page. inside: [part, box] (each `part` under it lies inside its nearest `box`, or the matched element
 *  itself when `box` is ""); apart: a selector whose matches mustn't overlap one another. Half a pixel's give for
 *  subpixel layout. */
export function layoutProblems(page, sel, checks) {
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
export const LINE = (box) => ({
  apart: [":scope > *", ":scope > * > *", ".tm-chip"],
  inside: [[":scope > *", box], [":scope > * > *", box], [".tm-chip > *", ".tm-chip"]],
});

/** The smallest the window goes (tauri.conf.json minWidth). */
export const NARROWEST = { width: 900, height: 600 };
