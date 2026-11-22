// The header's icon buttons (Find, Terminal, Extensions, Insights, Settings) light up alike under the pointer (#680):
// the same soft rounded square as Extensions, its size, corner and colour, in light and dark, and with the header
// crowded too. Squeezed, all but Extensions (which kept its size by chance) became tall narrow pills.
import { test, expect, openDesk } from "./cue.mjs";

const look = (b) => { const s = getComputedStyle(b), r = b.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height), radius: s.borderRadius, bg: s.backgroundColor, opacity: s.opacity }; };

for (const [width, view] of [[1440, "board"], [520, "board"], [520, "desk"]]) test(`at ${width}px on the ${view}, every header icon button lights up like Extensions`, async ({ cue, page }) => {
  await page.setViewportSize({ width, height: 900 });
  await cue.open(view === "desk" ? "desk" : "many-sessions");
  if (view === "desk") await openDesk(page);
  const btns = page.locator(".top button.icon");
  const names = await btns.evaluateAll((els) => els.map((b) => b.getAttribute("aria-label")));
  for (const n of ["Find (⌘K)", "Terminal (⌘J)", "Extensions", "Insights", "Settings"]) expect(names).toContain(n);
  // No hover text on any of them.
  await expect(page.locator(".top button.icon[title]")).toHaveCount(0);
  for (const theme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: theme });
    // Pointed at (hovered again on each try, so a redraw under the pointer doesn't count), its look.
    const lit = async (label) => {
      const b = page.locator(`.top button.icon[aria-label="${label}"]`);
      await b.hover();
      return b.evaluate(look);
    };
    let want;
    await expect.poll(async () => (want = await lit("Extensions")).opacity).toBe("1");
    expect(want, `${theme}: Extensions is a 32px square`).toMatchObject({ w: 32, h: 32 });
    expect(want.radius).not.toBe("0px");
    expect(want.bg).not.toBe("rgba(0, 0, 0, 0)");
    for (const label of names) await expect.poll(() => lit(label), { message: `${theme}: ${label} lit like Extensions` }).toEqual(want);
    await page.mouse.move(1, 899);
  }
});
