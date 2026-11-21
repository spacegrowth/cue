// Text size (Settings → Text size, #594): the chat's words only (styles/base.css --type-chat). A chat's messages and
// the words in its box grow a step; the lists, labels, buttons and panels around them stay as they were. At every step,
// wide and at the narrowest window, the open chat stays inside the window and its buttons still hold their words.
import { test, expect, openDesk } from "./cue.mjs";
import { NARROWEST } from "./layout.mjs";

const SIZES = { compact: "13px", default: "15px", large: "16px" };
const setSize = (cue, text) => cue.change((st, t) => { st.settings.appearance = { ...st.settings.appearance, text: t }; }, text);
const px = (loc) => loc.evaluate((el) => getComputedStyle(el).fontSize);

/** Shown elements under `sel` past the window's right or bottom edge, or a button or chip too small for its own words. */
const outOfPlace = (page, sel) => page.evaluate((sel) => {
  const out = [];
  const shown = (el) => el.getClientRects().length > 0 && getComputedStyle(el).visibility !== "hidden";
  const say = (el) => `${el.className || el.tagName} "${el.textContent.trim().replace(/\s+/g, " ").slice(0, 30)}"`;
  const scrolls = (el) => { for (let p = el.parentElement; p; p = p.parentElement) { const o = getComputedStyle(p); if (/auto|scroll|hidden|clip/.test(o.overflowX + o.overflowY) && p !== document.body && p !== document.documentElement) return true; } return false; };
  for (const root of document.querySelectorAll(sel)) {
    for (const el of root.querySelectorAll("*")) {
      if (!shown(el) || scrolls(el)) continue;
      const r = el.getBoundingClientRect();
      if (r.width && (r.right > innerWidth + 0.5 || r.bottom > innerHeight + 0.5)) out.push(`${say(el)} runs past the window`);
    }
    for (const el of root.querySelectorAll("button, .hchip, .pill, .tag, .chip")) {
      if (!shown(el) || !el.textContent.trim() || el.querySelector("button")) continue;
      if (/ellipsis/.test(getComputedStyle(el).textOverflow)) continue;
      if (el.scrollHeight > el.clientHeight + 1 || el.scrollWidth > el.clientWidth + 1) out.push(`${say(el)} is too small for its words`);
    }
  }
  return out;
}, sel);

for (const [text, size] of Object.entries(SIZES)) {
  for (const [where, viewport] of [["", null], [" at the narrowest window", NARROWEST]]) {
    test(`text size ${text}${where}: an open chat's messages and box read at ${size}; the list and the top bar don't change`, async ({ cue, page }) => {
      if (viewport) await page.setViewportSize(viewport);
      await cue.open("board-waiting", { frozen: true });
      await setSize(cue, text);
      await expect(page.locator(`html[data-text="${text}"]`)).toHaveCount(1);
      // Its top-left corner: in a narrow window the card's middle is its Move to bottom button.
      await page.locator('.nrow[data-big="n1"]').click({ position: { x: 12, y: 12 } });
      const chat = page.locator(".active-pane .ap-chat").first();
      await chat.waitFor();
      expect(await chat.evaluate((el) => getComputedStyle(el).getPropertyValue("--type-body").trim())).toBe(size);
      expect(await px(page.locator(".active-pane .composer textarea").first())).toBe(size);
      // Around the chat, main's sizes: a Waiting card's words, the top bar's buttons, the box's own Send.
      expect(await px(page.locator(".sec-waiting .nrow").first())).toBe("13px");
      expect(await px(page.locator(".switch-view button").first())).toBe("12px");
      expect(await px(page.locator(".active-pane .composer .send"))).toBe("13px");
      expect(await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--type-body").trim())).toBe("13px");
      expect(await outOfPlace(page, ".active-pane")).toEqual([]);
    });

    test(`text size ${text}${where}: the Desk lead's chat and box read at ${size}; the item list doesn't change`, async ({ cue, page }) => {
      if (viewport) await page.setViewportSize(viewport);
      await cue.open("desk", { frozen: true });
      await setSize(cue, text);
      await openDesk(page);
      const chat = page.locator(".dk-chat").first();
      await chat.waitFor();
      // Polled: the Desk redraws as it opens, and a replaced (detached) node has no styles.
      await expect.poll(() => chat.evaluate((el) => getComputedStyle(el).getPropertyValue("--type-body").trim())).toBe(size);
      await expect.poll(() => px(page.locator(".dk-box textarea").first())).toBe(size);
      await expect.poll(() => px(page.locator(".dk-it .dk-t").first())).toBe("13px");
      expect(await outOfPlace(page, "#app")).toEqual([]);
    });
  }
}

test("Settings → Text size: three steps, Default picked until you pick another; a pick sizes only the chat", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  const sizes = () => page.evaluate(() => ({
    body: getComputedStyle(document.body).fontSize, pad: getComputedStyle(document.querySelector(".app")).padding,
    zoom: getComputedStyle(document.documentElement).zoom, card: getComputedStyle(document.querySelector(".nrow")).fontSize,
    chat: getComputedStyle(document.querySelector(".ap-chat")).getPropertyValue("--type-body").trim(),
  }));
  const openChat = () => page.locator('.nrow[data-big="n1"]').click({ position: { x: 12, y: 12 } }).then(() => page.locator(".ap-chat").first().waitFor());
  await openChat();
  const before = await sizes();
  expect(before).toMatchObject({ body: "13px", card: "13px", chat: "15px" });
  await page.locator(".active-pane .composer textarea").blur();
  await page.keyboard.press(",");
  const seg = page.locator('.seg:has([data-set="appearance.text"])');
  await expect(seg.locator("button")).toHaveText(["Compact", "Default", "Large"]);
  await expect(seg.locator("button.on")).toHaveText("Default");
  await seg.locator("button", { hasText: "Compact" }).click();
  await cue.called("set_setting", { key: "appearance.text", value: "compact" });
  await expect(page.locator('html[data-text="compact"]')).toHaveCount(1);
  await page.keyboard.press("Escape");
  await openChat();
  expect(await sizes()).toEqual({ ...before, chat: "13px" });
  // ⌘+ steps up again, past Default to Large, and stops there; ⌘0 is Default.
  await page.locator(".active-pane .composer textarea").blur();
  await page.keyboard.press("Meta+=");
  await expect(page.locator('html[data-text="default"]')).toHaveCount(1);
  await page.keyboard.press("Meta+=");
  await page.keyboard.press("Meta+=");
  await expect(page.locator('html[data-text="large"]')).toHaveCount(1);
  expect(await sizes()).toEqual({ ...before, chat: "16px" });
  await page.keyboard.press("Meta+0");
  await expect(page.locator('html[data-text="default"]')).toHaveCount(1);
});
