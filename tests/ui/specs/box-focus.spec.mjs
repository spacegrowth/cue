// The message box while you type in it (#845): every chat's box gets the same quiet focus edge, a 1px line a little
// stronger than at rest (--line-focus), never the old 2px accent frame. Light and dark, laptop and the smallest window.
import { test, expect, openDesk } from "./cue.mjs";

const SHOTS = process.env.BOX_FOCUS_SHOTS;
/** The focused box's edge: [box-shadow, border-top-color, the --line-focus colour] of the box around `box`. */
const edge = (box) => box.evaluate((t) => {
  const f = t.closest(".field, .dk-box, .m-dk-box"), cs = getComputedStyle(f);
  const probe = document.createElement("i");
  probe.style.color = "var(--line-focus)";
  f.append(probe);
  const want = getComputedStyle(probe).color;
  probe.remove();
  return { shadow: cs.boxShadow, border: cs.borderTopColor, want, cls: f.className };
});
async function check(box, page, name) {
  await box.focus();
  await expect(box).toBeFocused();
  const e = await edge(box);
  if (e.cls.includes("field")) {
    expect(e.shadow, `${name}: 1px inset line`).toMatch(/inset/);
    expect(e.shadow, `${name}: 1px, not 2px`).toMatch(/0px 0px 0px 1(\.5)?px/);
    expect(e.shadow.startsWith(e.want) || e.shadow.includes(e.want), `${name}: the --line-focus colour (${e.shadow} vs ${e.want})`).toBe(true);
  } else {
    expect(e.border, `${name}: --line-focus border`).toBe(e.want);
  }
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/${name}.png` });
}

for (const scheme of ["light", "dark"]) {
  for (const [size, vp] of [["laptop", { width: 1440, height: 900 }], ["narrowest", { width: 760, height: 560 }]]) {
    test(`message boxes focus quietly: ${scheme}, ${size}`, async ({ page, cue }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await page.setViewportSize(vp);
      await cue.open("board-waiting");
      await page.locator('.working[data-session="alerts"]').first().click();
      await check(page.locator('.active-pane textarea[data-text="s:alerts"]'), page, `board-${scheme}-${size}`);
      await cue.open("desk");
      await openDesk(page);
      await check(page.locator(".dk-right .foot:not(.held) .composer textarea"), page, `lead-${scheme}-${size}`);
      await page.locator('[data-desk-item="d6"]').click();
      await check(page.locator(".dk-right .foot:not(.held) .composer textarea"), page, `item-${scheme}-${size}`);
    });
  }
}
