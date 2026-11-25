// The lead's chat and the Board's chat have the same tab row as a Desk session's (#898): the lead's first tab reads
// "Chat" (its name is in the header above), and the Board's chat has the band of tabs (Chat, Activity, then what the
// session has), as on the Desk. Every row has option A's look (#935): the tabs as equal parts of one rounded track,
// the picked one raised (the panel's colour, ink words), one line under the row and none over it.
import { test, expect, openDesk } from "./cue.mjs";

/** How a tab row and its picked tab look: their lines, fill, text and height. */
const look = (page, row, on) => page.evaluate(([row, on]) => {
  const r = getComputedStyle(document.querySelector(row)), t = document.querySelector(on), c = getComputedStyle(t);
  const trk = t.closest(".tabtrk"), tabs = [...trk.children], ws = tabs.map((b) => b.getBoundingClientRect().width);
  return { top: r.borderTopStyle, bottom: r.borderBottomStyle, track: getComputedStyle(trk).backgroundColor, fill: c.backgroundColor, ink: c.color, size: c.fontSize, weight: c.fontWeight, h: Math.round(t.getBoundingClientRect().height),
    equal: Math.max(...ws) - Math.min(...ws) < 2 };
}, [row, on]);

for (const scheme of ["light", "dark"]) for (const size of [{ width: 1440, height: 900 }, { width: 800, height: 560 }]) {
  test(`the lead's first tab is Chat; the Board's chat has the Desk's tab row (${scheme}, ${size.width}px)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize(size);
    await cue.open("desk");
    await openDesk(page);
    const leadTabs = page.locator(".dk-right .dk-ptabs .dk-tab");
    await expect(leadTabs).toHaveText(["Chat", "Decisions", "Activity", "Away"].map((t) => new RegExp(`^${t}`)));
    const desk = await look(page, ".dk-right .dk-ptabs", ".dk-right .dk-ptabs .dk-tab.on");
    const ink = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--ink").trim());
    expect(desk).toMatchObject({ top: "none", bottom: "solid", weight: "600", equal: true });
    expect(desk.fill).not.toBe(desk.track);
    expect(await page.evaluate(([a, b]) => { const el = document.createElement("i"); el.style.color = a; document.body.append(el); const x = getComputedStyle(el).color; el.style.color = b; const y = getComputedStyle(el).color; el.remove(); return x === y; }, [desk.ink, ink])).toBe(true);
    await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
    const sess = await look(page, ".dk-right .dkn-stabs", ".dk-right .dkn-stab.on");
    expect(sess).toEqual(desk);
    // The Board: its open session's chat, Chat picked and Activity beside it even before it has any.
    await page.evaluate(() => { active = { sid: "host" }; view = "board"; renderMain(); });
    const row = page.locator(".active-pane .stabs");
    await expect(row.locator(".stab").first()).toHaveText("Chat");
    await expect(row.locator('.stab[data-bg-tab="activity"]')).toHaveText("Activity");
    expect(await look(page, ".active-pane .stabs", ".active-pane .stab.on")).toEqual(desk);
    await row.locator('.stab[data-bg-tab="activity"]').click();
    await expect(row.locator(".stab.on")).toHaveText("Activity");
    await expect(row.locator("[title]")).toHaveCount(0);
  });
}
