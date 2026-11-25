// One line at the tabs in every Desk chat (#827), and since option A of the calmer tabs (#935) it is the one under them,
// where the chat starts: no line over the tabs. Whatever sits right above them (an item's bar with Looks right and Send
// back) drops its bottom line, as the lead's heading does.
import { test, expect, openDesk } from "./cue.mjs";

/** The visible horizontal lines from the 60px above the tabs down to their top edge, in the right side. */
const linesOver = (page, sel) => page.evaluate((sel) => {
  const tr = document.querySelector(sel).getBoundingClientRect(), out = [];
  for (const el of document.querySelectorAll(".dk-right, .dk-right *")) {
    const r = el.getBoundingClientRect(), cs = getComputedStyle(el);
    if (r.width < 100 || cs.visibility === "hidden" || !el.getClientRects().length) continue;
    const edge = (w, s, y) => parseFloat(w) > 0 && s !== "none" && y <= tr.top + 1 && y > tr.top - 60;
    if (edge(cs.borderBottomWidth, cs.borderBottomStyle, r.bottom)) out.push(Math.round(r.bottom));
    if (edge(cs.borderTopWidth, cs.borderTopStyle, r.top)) out.push(Math.round(r.top));
  }
  return out;
}, sel);
/** The tab row's own line under it. */
const lineUnder = (page, sel) => page.evaluate((sel) => getComputedStyle(document.querySelector(sel)).borderBottomStyle, sel);

for (const scheme of ["light", "dark"]) for (const size of [{ width: 1440, height: 900 }, { width: 800, height: 560 }]) {
  test(`no line over the tabs, one under them: lead, an item's chat, a session's chat, a crew member (${scheme}, ${size.width}px)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").team = st.sessions.find((x) => x.session_id === "lead").team; });
    await openDesk(page);
    await expect.poll(() => linesOver(page, ".dk-right .dk-ptabs")).toHaveLength(0);
    expect(await lineUnder(page, ".dk-right .dk-ptabs")).toBe("solid");
    // An item with its bar (Looks right, Send back) over its holder's chat.
    await page.locator('.dk-items .dk-it[data-desk-item="d2"]').first().click();
    await expect(page.locator(".dk-right .dkn-sp > .dkn-ib")).toBeVisible();
    await page.setViewportSize(size);
    await expect.poll(() => linesOver(page, ".dk-right .dkn-stabs")).toHaveLength(0);
    expect(await lineUnder(page, ".dk-right .dkn-stabs")).toBe("solid");
    await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
    await expect.poll(() => linesOver(page, ".dk-right .dkn-stabs")).toHaveLength(0);
    expect(await lineUnder(page, ".dk-right .dkn-stabs")).toBe("solid");
    const crew = await page.evaluate(() => (sessionOf("host").team || []).map((t) => t.agent_id || t.call));
    await page.evaluate(([lead, agent]) => deskShow({ kind: "crew", lead, agent }), ["host", crew[0]]);
    await expect.poll(() => linesOver(page, ".dk-right .dkn-crew .dkn-stabs")).toHaveLength(0);
    expect(await lineUnder(page, ".dk-right .dkn-crew .dkn-stabs")).toBe("solid");
  });
}
