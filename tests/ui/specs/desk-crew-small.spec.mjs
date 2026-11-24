// A crew member's page at the smallest window (#862): the Desk is one column there, and the page keeps room for its
// heading, tabs and whole box with Send; its chat is what gives way. Items take what's left and never push the window.
import { test, expect, openDesk } from "./cue.mjs";

/** The open page's card, its box and Send, and how far the window itself scrolled. */
const fit = (page) => page.evaluate(() => {
  const r = (el) => { const b = el.getBoundingClientRect(); return { top: Math.round(b.top), bottom: Math.round(b.bottom) }; };
  const card = document.querySelector(".dk-right"), box = card.querySelector(".dkn-crew .composer");
  const send = [...box.querySelectorAll("button")].find((b) => b.textContent.trim() === "Send");
  return { card: r(card), box: r(box), send: r(send), scrolled: document.documentElement.scrollTop };
});

for (const scheme of ["light", "dark"]) {
  test(`a crew member's page shows its whole box with Send at 760×560 (${scheme})`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").team = st.sessions.find((x) => x.session_id === "lead").team; });
    await openDesk(page);
    await page.setViewportSize({ width: 760, height: 560 });
    const crew = await page.evaluate(() => (sessionOf("host").team || []).map((t) => t.agent_id || t.call));
    await page.evaluate(([lead, agent]) => deskShow({ kind: "crew", lead, agent }), ["host", crew[0]]);
    await expect(page.locator(".dk-right .dkn-crew .composer")).toBeVisible();
    await expect.poll(async () => { const f = await fit(page); return f.box.bottom <= f.card.bottom && f.send.bottom <= f.box.bottom && f.box.top >= f.card.top; }).toBe(true);
    const f = await fit(page);
    expect(f.scrolled).toBe(0);
    expect(f.box.bottom).toBeLessThanOrEqual(560);
    // Its ‹ back sits on the page's edge, as a session's page starts: no old-layout gap over it.
    const back = await page.locator(".dk-right .dkn-crew .dkn-sback").boundingBox();
    expect(back.height).toBeLessThan(42);
  });
}
