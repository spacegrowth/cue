// A held step's command (#670): the "wants to delete files" card showed its command in a small pill font, cut
// through a line, with a plain more. Now every held card (and an Away ask) draws it in the chat's own script block:
// the slab, its font, size, background, border and corners, "$ " and Copy, a few whole lines with a fade at the
// foot, and the same more / less. The card's title and buttons keep the app's font. Light and dark.
import { test, expect, openDesk } from "./cue.mjs";

const LONG = Array.from({ length: 12 }, (_, n) => `rm -rf build/out/part-${n} dist/cache/chunk-${n} node_modules/.cache/run-${n}`).join(" && \\\n  ");

/** The Desk, item #227's session asking to run LONG (its card at the foot of the item's page is the chat's own
 *  permission card), and two held steps: a long delete and a short push. */
async function setup(cue, page) {
  await cue.open("desk");
  await cue.change((st, command) => {
    const worker = st.sessions.find((s) => s.session_id === "host");
    worker.harness = "claude"; worker.hosted = true; worker.state = "deciding";
    const d = st.desk.projects.find((x) => x.project === "cue");
    for (const item of d.items.filter((i) => i.holder?.sid === "host")) { item.state = "working"; item.now_on = false; item.ask = null; }
    const it = d.items.find((item) => item.holder?.sid === "host");
    Object.assign(it, { id: "227", kind: "feature", title: "Give the Desk its own icon", state: "needs_you", ask: { item_id: "perm", sid: "host", text: command, choices: [], kind: "permission" } });
    st.items.push({ id: "perm", session_id: "host", harness: "claude", hosted: true, project: "cue", kind: "permission", status: "pending", tool_name: "Bash", tool_input: { command }, suggestions: [], created_ms: Date.now(), thread: [{ role: "you", text: "Desk item #227 (feature): Give the Desk its own icon" }] });
    d.held = [{ id: "held-1", sid: "site", who: "site", what: "delete files", command, at: 1 },
      { id: "held-2", sid: "site", who: "site", what: "push", command: "git push origin main", at: 2 }];
  }, LONG);
  await openDesk(page);
  await page.locator('.dk-items [data-desk-item="227"]').click();
}

/** How script block `sel` looks: its slab's font, size, background, border and corners, its Copy and "$ ", the fade
 *  at its foot, its more; and how its folded lines fall (whole lines shown, lines its clip edge cuts, and whether each
 *  line starts at a word, not mid-word). */
const look = (page, sel) => page.locator(sel).evaluate((cut) => {
  const pre = cut.querySelector(".code-wrap > pre"), cs = getComputedStyle(pre), fade = getComputedStyle(cut.querySelector(".code-wrap"), "::after");
  const more = cut.querySelector(":scope > .req-more"), ms = more && getComputedStyle(more);
  const r = pre.getBoundingClientRect(), edge = r.bottom - parseFloat(cs.borderBottomWidth);
  const lines = new Map(), rects = [];
  const w = document.createTreeWalker(pre, NodeFilter.SHOW_TEXT);
  for (let n; (n = w.nextNode());) {
    for (let k = 0; k < n.length; k++) {
      const rg = document.createRange(); rg.setStart(n, k); rg.setEnd(n, k + 1);
      const q = rg.getClientRects()[0];
      if (!q || !q.height) continue;
      rects.push(q);
      const top = Math.round(q.top);
      if (!lines.has(top)) lines.set(top, { prev: k ? n.data[k - 1] : " ", ch: n.data[k] });
    }
  }
  return {
    style: { font: cs.fontFamily, size: cs.fontSize, lh: cs.lineHeight, bg: cs.backgroundColor, color: cs.color, radius: cs.borderRadius, pad: `${cs.paddingTop} ${cs.paddingLeft} ${cs.paddingRight}`, border: `${cs.borderTopWidth} ${cs.borderLeftWidth}`, borderLeft: cs.borderLeftColor },
    ps: !!pre.querySelector(".ps"), copy: !!cut.querySelector(".code-copy"),
    fade: fade.content !== "none" && /gradient/.test(fade.backgroundImage),
    more: ms && { text: more.textContent, color: ms.color, size: ms.fontSize, weight: ms.fontWeight },
    shown: new Set(rects.filter((q) => q.bottom <= edge + 1).map((q) => Math.round(q.top))).size,
    cut: rects.filter((q) => q.top < edge - 1 && q.bottom > edge + 1).length,
    midWord: [...lines.values()].slice(1).filter((l) => /\S/.test(l.prev) && /\S/.test(l.ch) && !/[\/\-.]/.test(l.prev)).length,
  };
});

for (const scheme of ["light", "dark"]) {
  test(`a held step's command is the chat's script block: same look, a few whole lines with a fade, the same more and less (${scheme}) (#670)`, async ({ cue, page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await setup(cue, page);
    const chat = ".dk-right .act-card .req-cut", held = '.dk-items [data-held-row="held-1"] .req-cut';
    await expect(page.locator(chat)).toBeVisible();
    await expect(page.locator(held)).toBeVisible();
    const [c, h] = [await look(page, chat), await look(page, held)];
    // The same block as the chat's.
    expect(h.style).toEqual(c.style);
    expect(h.style.font).toMatch(/mono/i);
    expect(h.ps && h.copy && c.ps && c.copy).toBe(true);
    // A few whole lines, a fade at the foot, never a line cut through.
    expect(c.fade).toBe(true);
    expect(h.fade).toBe(true);
    expect(h.shown).toBe(3);
    expect(h.cut).toBe(0);
    // It wraps at spaces and after a / or -, never in the middle of a word.
    expect(h.midWord).toBe(0);
    // The same open and close control.
    expect(h.more).toEqual(c.more);
    expect(h.more.text).toBe("more");
    await page.locator(`${held} [data-req-more]`).click();
    await expect(page.locator(held)).toHaveClass(/open/);
    await expect(page.locator(`${held} [data-req-more]`)).toHaveText("less");
    const open = await look(page, held);
    expect(open.fade).toBe(false);
    expect(open.cut).toBe(0);
    await page.locator(`${held} [data-req-more]`).click();
    await expect(page.locator(`${held} [data-req-more]`)).toHaveText("more");
    // A short command shows whole: no fade, no more.
    const short = '.dk-items [data-held-row="held-2"]';
    await expect(page.locator(`${short} pre.slab`)).toHaveText("$ git push origin main");
    await expect(page.locator(`${short} [data-req-more]`)).toHaveCount(0);
    // The card's title and buttons keep the app's font, not the script's; no hover text, no stripe down its left.
    const card = await page.locator('.dk-items [data-held-row="held-1"]').evaluate((el) => {
      const f = (s) => getComputedStyle(el.querySelector(s)).fontFamily, cs = getComputedStyle(el);
      return { who: f(".dk-request-who"), btn: f('[data-allow="1"]'), body: getComputedStyle(document.body).fontFamily, left: cs.borderLeftWidth, top: cs.borderTopWidth };
    });
    expect(card.who).toBe(card.body);
    expect(card.btn).toBe(card.body);
    expect(card.left).toBe(card.top);
    await expect(page.locator(`${short} [title], [data-held-row="held-1"] [title]`)).toHaveCount(0);
  });
}
