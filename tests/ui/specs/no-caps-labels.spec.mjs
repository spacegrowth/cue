// No label shouts (#622): tags, heads and chips are set apart by weight, size and colour, never by capitals. On
// each main screen nothing visible is uppercased by its style, and no label Cue writes itself is in all caps.
import { test, expect, openDesk } from "./cue.mjs";

/** Every visible element uppercased by CSS, and every visible bit of Cue's own text written in caps: a run of
 *  three or more capital letters with no small letter in the same text. What people or agents wrote (a chat's
 *  words, code, a terminal) is theirs, not a label, and is left out, as are short acronyms Cue names (PR, CI). */
const shouting = (page) => page.evaluate(() => {
  const theirs = "pre, code, kbd, textarea, input, .xterm, .cv-text, .mg-body, .dk-ttext, .md, .read-body, [contenteditable]";
  const seen = (el) => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== "hidden"; };
  const out = [];
  for (const el of document.body.querySelectorAll("*")) {
    if (el.closest(theirs) || !seen(el)) continue;
    const own = [...el.childNodes].filter((n) => n.nodeType === 3).map((n) => n.textContent).join(" ").trim();
    if (!own) continue;
    const cs = getComputedStyle(el);
    if (cs.textTransform === "uppercase" || cs.fontVariantCaps === "all-small-caps" || cs.fontVariantCaps === "small-caps") {
      out.push(`${el.className || el.tagName}: styled caps "${own}"`);
      continue;
    }
    const words = own.replace(/#\w+|\b(PR|CI|OK|ID|UI|AI|CPU|MCP|API|URL|JSON|CLI|SSH|README|MD)\b/g, "");
    if (/[A-Z]{3,}/.test(words) && !/[a-z]/.test(words)) out.push(`${el.className || el.tagName}: "${own}"`);
  }
  return out;
});

const SCREENS = {
  "the board": { scenario: "board-waiting" },
  "an empty board": { scenario: "world-empty" },
  "Parked": { scenario: "parked-ended", setup: (p) => p.locator('[data-shelf="parked"]').click() },
  "a chat": { scenario: "board-waiting", setup: (p) => p.locator('.nrow[data-big="n1"]').click() },
  "a crew": { scenario: "world-team", setup: (p) => p.locator('.nrow[data-big="n1793984390004"]').click() },
  "Settings": { scenario: "board-waiting", setup: (p) => p.keyboard.press(",") },
  "Insights": { scenario: "board-waiting", setup: (p) => p.locator('[data-view="insights"]').click() },
  "the Desk": { scenario: "desk", setup: openDesk },
  "the Desk's inbox": { scenario: "desk-inbox" },
};

for (const [name, { scenario, setup }] of Object.entries(SCREENS)) {
  for (const theme of ["light", "dark"]) {
    test(`no label in caps: ${name}, ${theme}`, async ({ page, cue }) => {
      await page.emulateMedia({ colorScheme: theme });
      await cue.open(scenario);
      if (setup) await setup(page);
      await page.waitForTimeout(300);
      expect(await shouting(page)).toEqual([]);
    });
  }
}
