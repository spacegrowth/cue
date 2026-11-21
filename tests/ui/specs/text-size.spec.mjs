// Text size (Settings → Text size, #594): each step only grows the words (the type scale, styles/base.css data-text);
// panels, gaps and buttons keep their place. At every step, wide and at the narrowest window, nothing runs into
// anything else, sticks out of its box or past the window's edge, and every button and chip still holds its words.
import { test, expect, openDesk } from "./cue.mjs";
import { layoutProblems, LINE, NARROWEST } from "./layout.mjs";

const SIZES = ["compact", "default", "large"];
const setSize = (cue, text) => cue.change((st, t) => { st.settings.appearance = { ...st.settings.appearance, text: t }; }, text);

/** Shown elements under `sel` past the window's right or bottom edge, or a button or chip too short or narrow for its
 *  own words (its content box overflowing, where it doesn't scroll), as sentences. */
const outOfPlace = (page, sel) => page.evaluate((sel) => {
  const out = [];
  const shown = (el) => el.getClientRects().length > 0 && getComputedStyle(el).visibility !== "hidden";
  const say = (el) => `${el.className || el.tagName} "${el.textContent.trim().replace(/\s+/g, " ").slice(0, 30)}"`;
  // Inside something that scrolls (a list, a row of tabs), past the edge is only out of view.
  const scrolls = (el) => { for (let p = el.parentElement; p; p = p.parentElement) { const o = getComputedStyle(p); if (/auto|scroll|hidden|clip/.test(o.overflowX + o.overflowY) && p !== document.body && p !== document.documentElement) return true; } return false; };
  for (const root of document.querySelectorAll(sel)) {
    for (const el of root.querySelectorAll("*")) {
      if (!shown(el) || scrolls(el)) continue;
      const r = el.getBoundingClientRect();
      if (r.width && (r.right > innerWidth + 0.5 || r.bottom > innerHeight + 0.5)) out.push(`${say(el)} runs past the window`);
    }
    for (const el of root.querySelectorAll("button, .hchip, .pill, .tag, .chip")) {
      if (!shown(el) || !el.textContent.trim() || el.querySelector("button")) continue;
      const o = getComputedStyle(el);
      if (/ellipsis/.test(o.textOverflow)) continue;
      if (el.scrollHeight > el.clientHeight + 1 || el.scrollWidth > el.clientWidth + 1) out.push(`${say(el)} is too small for its words (${el.scrollWidth}×${el.scrollHeight} in ${el.clientWidth}×${el.clientHeight})`);
    }
  }
  return out;
}, sel);

for (const text of SIZES) {
  for (const [where, viewport] of [["", null], [" at the narrowest window", NARROWEST]]) {
    test(`text size ${text}${where}: the board's header and Waiting cards keep their parts apart and inside`, async ({ cue, page }) => {
      if (viewport) await page.setViewportSize(viewport);
      await cue.open("board-waiting", { frozen: true });
      await setSize(cue, text);
      await expect(page.locator(`html[data-text="${text}"]`)).toHaveCount(1);
      expect(await layoutProblems(page, ".top", LINE(".top"))).toEqual([]);
      expect(await layoutProblems(page, ".sec-waiting .nrow-top", LINE(".nrow"))).toEqual([]);
      expect(await outOfPlace(page, "#app")).toEqual([]);
    });

    test(`text size ${text}${where}: an open chat's header keeps its chips and buttons apart`, async ({ cue, page }) => {
      if (viewport) await page.setViewportSize(viewport);
      await cue.open("board-waiting", { frozen: true });
      await setSize(cue, text);
      await page.locator('.nrow[data-big="n1"]').click();
      await page.locator(".active-pane .ap-head").first().waitFor();
      expect(await layoutProblems(page, ".ap-head", LINE(""))).toEqual([]);
      expect(await outOfPlace(page, ".active-pane")).toEqual([]);
    });

    test(`text size ${text}${where}: Settings' rows keep their words and controls apart`, async ({ cue, page }) => {
      if (viewport) await page.setViewportSize(viewport);
      await cue.open("board-waiting", { frozen: true });
      await setSize(cue, text);
      await page.keyboard.press(",");
      await page.locator(".set-row").first().waitFor();
      expect(await layoutProblems(page, ".set-row", { apart: [":scope > *"], inside: [[":scope > *", ""]] })).toEqual([]);
      expect(await layoutProblems(page, ".seg", { inside: [["button", ""]] })).toEqual([]);
      expect(await outOfPlace(page, ".sheet")).toEqual([]);
    });

    test(`text size ${text}${where}: the Desk's item rows keep their parts apart`, async ({ cue, page }) => {
      if (viewport) await page.setViewportSize(viewport);
      await cue.open("desk", { frozen: true });
      await setSize(cue, text);
      await openDesk(page);
      expect(await layoutProblems(page, ".dk-it", { apart: [":scope > *"], inside: [[":scope > *", ""]] })).toEqual([]);
      expect(await outOfPlace(page, "#app")).toEqual([]);
    });
  }
}

/** More screens, swept whole: name → [scenario, open it, what's on screen once it's drawn]. */
const SWEEP = {
  "many sessions": ["many-sessions", null, (p) => p.locator('.nrow[data-big="mi0"]').waitFor()],
  "a long chat": ["board-waiting", (p) => p.locator('.nrow[data-big="n1"]').click(), (p) => p.locator(".active-pane .ap-head").first().waitFor()],
  "Insights": ["board-waiting", (p) => p.locator('[data-view="insights"]').click(), (p) => p.waitForFunction(() => CueBridge.called("insights_summary").some((c) => "result" in c))],
  "a lead's crew": ["world-team", (p) => p.locator('.nrow[data-big="n1793984390004"]').click(), (p) => p.locator("[data-mt-open]").first().waitFor()],
  "Settings → Agents": ["board-waiting", async (p) => { await p.keyboard.press(","); await p.locator('[data-act="set-tab"][data-tab="agents"]').click(); }, (p) => p.locator(".sheet .set-row").first().waitFor()],
  "Settings → Chat": ["board-waiting", async (p) => { await p.keyboard.press(","); await p.locator('[data-act="set-tab"][data-tab="chat"]').click(); }, (p) => p.locator(".sheet .set-row").first().waitFor()],
};
for (const text of SIZES) {
  for (const [where, viewport] of [["", null], [" at the narrowest window", NARROWEST]]) {
    for (const [name, [scenario, open, done]] of Object.entries(SWEEP)) {
      test(`text size ${text}${where}: ${name} fits the window and every button holds its words`, async ({ cue, page }) => {
        if (viewport) await page.setViewportSize(viewport);
        await cue.open(scenario, { frozen: true });
        await setSize(cue, text);
        if (open) await open(page);
        await done(page);
        expect(await outOfPlace(page, "#app")).toEqual([]);
      });
    }
  }
}

test("Settings → Text size: three steps, Default picked until you pick another, and a pick resizes only the words", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  await page.keyboard.press(",");
  const seg = page.locator('.seg:has([data-set="appearance.text"])');
  await expect(seg.locator("button")).toHaveText(["Compact", "Default", "Large"]);
  await expect(seg.locator("button.on")).toHaveText("Default");
  const sizes = () => page.evaluate(() => ({ body: getComputedStyle(document.body).fontSize, pad: getComputedStyle(document.querySelector(".app")).padding, zoom: getComputedStyle(document.documentElement).zoom }));
  const before = await sizes();
  expect(before.body).toBe("15px");
  await seg.locator("button", { hasText: "Compact" }).click();
  await cue.called("set_setting", { key: "appearance.text", value: "compact" });
  await expect(page.locator('html[data-text="compact"]')).toHaveCount(1);
  const after = await sizes();
  expect(after).toEqual({ ...before, body: "13px" });
  // ⌘+ steps up again, past Default to Large, and stops there; ⌘0 is Default.
  await page.locator("body").click({ position: { x: 5, y: 5 } }).catch(() => {});
  await page.keyboard.press("Meta+=");
  await expect(page.locator('html[data-text="default"]')).toHaveCount(1);
  await page.keyboard.press("Meta+=");
  await expect(page.locator('html[data-text="large"]')).toHaveCount(1);
  expect((await sizes()).body).toBe("16px");
  await page.keyboard.press("Meta+0");
  await expect(page.locator('html[data-text="default"]')).toHaveCount(1);
});
