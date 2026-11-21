// Diffs are plain green and red (#647): added rows green, removed rows red, the same colours in light and dark, with
// text that reads on them in both. Opened for real through a kept copy's Diff (the Files changed pop-up, which the
// item page's Files tab opens too); the chat's edit steps, a teammate's Diff so far and an edit approval's preview
// are drawn beside it from their own markup, so every place a diff shows is held to the same colours.
import { test, expect, openDesk } from "./cue.mjs";

const DIFF = "diff --git a/ui/b.js b/ui/b.js\n--- a/ui/b.js\n+++ b/ui/b.js\n@@ -1,2 +1,2 @@\n-old\n+new\n same\n";
const holder = { kind: "session", sid: "host", name: "host-lead" };
const kept = (at) => ({ id: "k1", title: "Old idea, dropped", project: "cue", repo: "/r/cue", path: "/r/cue/.claude/worktrees/kept-k1", branch: "desk/k1", base: "abc1234", at,
  item: { id: "k1", project: "cue", kind: "feature", title: "Old idea, dropped", see: "", done_when: "", state: "dropped", holder, stints: [], created: Math.floor(at / 1000) - 600, updated: Math.floor(at / 1000) } });

// Each place: [name, the added row, the removed row].
const PLACES = [
  ["Files changed pop-up", ".dov .dov-lines .a", ".dov .dov-lines .d"],
  ["chat edit step", "#t647 .stx-diff .a", "#t647 .stx-diff .d"],
  ["teammate's Diff so far", "#t647 .cpm-diff > .add", "#t647 .cpm-diff > .del"],
  ["edit approval", "#t647 .diff pre:not(.del)", "#t647 .diff .del"],
];
const MARKUP = `<div id="t647">
  <div class="stx-diff mono"><div class="a"><i>+</i>new</div><div class="d"><i>-</i>old</div></div>
  <pre class="cpm-diff"><span class="add"><i class="ln" data-o="" data-n="1"></i>new</span><span class="del"><i class="ln" data-o="1" data-n=""></i>old</span></pre>
  <div class="diff"><pre class="del">− old</pre><pre>+ new</pre></div></div>`;

const rgb = (s) => s.match(/[\d.]+/g).slice(0, 3).map(Number);
const lum = (c) => { const [r, g, b] = c.map((v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }); return 0.2126 * r + 0.7152 * g + 0.0722 * b; };
const contrast = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };

test("diff rows are the same plain green and red in light and dark, with readable text, everywhere a diff shows", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.override("desk_item_diff", (args, diff) => (args.id === "k1" ? { machine: "", diff } : { machine: "", diff: "" }), DIFF);
  await cue.change((st, k) => { st.desk.projects.find((x) => x.project === "cue").kept = [k]; }, kept(Date.now() - 120000));
  await page.locator('.dk-items .dk-kept[data-kept="k1"] button.dk-kept-f').click();
  await expect(page.locator(".dov .dov-lines .a").first()).toBeVisible();
  await page.evaluate((html) => document.body.insertAdjacentHTML("beforeend", html), MARKUP);

  const seen = {};
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    for (const [name, add, del] of PLACES) {
      for (const [side, sel] of [["added", add], ["removed", del]]) {
        const { bg, fg } = await page.locator(sel).first().evaluate((el) => { const c = getComputedStyle(el); return { bg: c.backgroundColor, fg: c.color }; });
        const [r, g, b] = rgb(bg);
        if (side === "added") expect(g, `${theme} ${name}: an added row is green (${bg})`).toBeGreaterThan(Math.max(r, b));
        else expect(r, `${theme} ${name}: a removed row is red (${bg})`).toBeGreaterThan(Math.max(g, b));
        expect(contrast(rgb(bg), rgb(fg)), `${theme} ${name}: ${side} text reads on its row`).toBeGreaterThanOrEqual(4.5);
        const key = `${name} ${side}`;
        if (theme === "light") seen[key] = { bg, fg };
        else expect({ bg, fg }, `${key}: the same colours in dark as in light`).toEqual(seen[key]);
      }
    }
  }
  // One set of colours for every place, not one per screen.
  expect(new Set(PLACES.map(([n]) => seen[`${n} added`].bg)).size).toBe(1);
  expect(new Set(PLACES.map(([n]) => seen[`${n} removed`].bg)).size).toBe(1);
});
