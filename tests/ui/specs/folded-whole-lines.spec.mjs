// Folded text cuts at whole lines (#609): an older reply folded to its first lines, a NOTE folded to two, a forwarded
// message's original folded to six. None shows any part of the line after its last, at each text size, light and dark.
// The folds are set by line count (-webkit-line-clamp), never a fixed height, which at some sizes ended mid-line.
import { test, expect, openDesk } from "./cue.mjs";

const LONG = "The phrases go left of Send and the box keeps its size when the window narrows; check the small window too, and the notch, and the rail, where the words wrap sooner and the fold must still land on a whole line every time.";
const push = (page, role, text) => page.evaluate(([r, t]) => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: r, text: t }); renderMain(); }, [role, text]);

/** Each folded box in the Desk's chat: whether its words run on past the fold, and the lines its clip edge cuts through
 *  (a line partly above it and partly below). The clip edge is the inside of the bottom border, where overflow stops. */
const folds = (page) => page.evaluate(() => [...document.querySelectorAll(".dk-chat :is(.cv-agent.folded .cv-text, .req-cut:not(.open) .req-t, .peer-msg .mg-full)")].map((el) => {
  const cs = getComputedStyle(el), r = el.getBoundingClientRect(), edge = r.bottom - parseFloat(cs.borderBottomWidth);
  const rects = [];
  const w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  for (let n; (n = w.nextNode());) {
    if (!n.textContent.trim()) continue;
    const rg = document.createRange(); rg.selectNodeContents(n); rects.push(...rg.getClientRects());
  }
  const cut = rects.filter((q) => q.width && q.height && q.top < edge - 1 && q.bottom > edge + 1);
  return { cls: el.className, clamp: cs.webkitLineClamp, maxHeight: cs.maxHeight, more: el.scrollHeight > el.clientHeight + 1, cut: cut.length };
}));

for (const scheme of ["light", "dark"]) for (const size of ["compact", "default", "large"]) {
  test(`folded notes, replies and forwards cut at whole lines (${size}, ${scheme}) (#609)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await openDesk(page);
    await cue.change((st, s) => { st.settings.appearance = { ...(st.settings.appearance || {}), text: s }; }, size);
    await expect(page.locator(":root")).toHaveAttribute("data-text", size);
    await push(page, "you", `keep the old one\n\n[Forwarded from the api session]\n${LONG}\n\n${LONG}\n\n${LONG}`);
    await push(page, "agent", `Sent a note.\n\n- **NOTE:** #d2 | ${LONG}`);
    // Five long replies: the older ones fold (the newest three stay open).
    for (let i = 0; i < 5; i++) await push(page, "agent", `Reply ${i}. ${LONG}\n\n- one point that runs long enough to wrap onto a second line in the chat\n- two\n\nA closing paragraph. ${LONG}`);
    await expect(page.locator(".dk-chat .cv-agent.folded")).not.toHaveCount(0);
    const got = await folds(page);
    const kinds = (re) => got.filter((f) => re.test(f.cls));
    for (const re of [/cv-text/, /req-t/, /mg-full/]) {
      expect(kinds(re).length, `a ${re.source} fold`).toBeGreaterThan(0);
      // Really folded (more under it), by line count, not a height.
      for (const f of kinds(re)) { expect(f.more, f.cls).toBe(true); expect(f.clamp, f.cls).not.toBe("none"); expect(f.maxHeight, f.cls).toBe("none"); }
    }
    expect(got.filter((f) => f.cut), "folds that cut through a line").toEqual([]);
  });
}
