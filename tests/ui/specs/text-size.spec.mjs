// Text size, two controls (#594). ⌘+ / ⌘− / ⌘0 and Settings → Text size zoom the whole window, as before #594
// (appearance.zoom; the app's webview zooms, so here only what's saved and said is seen). A− / A+ on a chat's head size
// that chat's words only (appearance.chat_text, styles/base.css --type-chat): its messages and the words in its box;
// the lists, labels, buttons and panels around it stay as they are. At every step, wide and at the narrowest window, in
// light and dark, the open chat stays inside the window and its buttons still hold their words.
import { test, expect, openDesk } from "./cue.mjs";
import { NARROWEST } from "./layout.mjs";

const STEPS = [13, 14, 15, 16, 18];
const setChat = (cue, t, mode = "system") => cue.change((st, [t, mode]) => { st.settings.appearance = { ...st.settings.appearance, chat_text: t, mode }; }, [t, mode]);
/** A− / A+ saved step `t`, and Cue's state has it back (the next click steps from there). */
const chatSaved = async (cue, page, t) => { await cue.called("set_setting", { key: "appearance.chat_text", value: t }); await expect.poll(() => page.evaluate(() => state.settings.appearance.chat_text)).toBe(t); };
const px = (loc) => loc.evaluate((el) => getComputedStyle(el).fontSize);
const typeBody = (loc) => loc.evaluate((el) => getComputedStyle(el).getPropertyValue("--type-body").trim());
// Its top-left corner: in a narrow window the card's middle is its Move to bottom button.
const openChat = (page) => page.locator('.nrow[data-big="n1"]').click({ position: { x: 12, y: 12 } }).then(() => page.locator(".active-pane .ap-chat").first().waitFor());

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

for (const t of STEPS) {
  for (const mode of ["light", "dark"]) {
    for (const [where, viewport] of [["", null], [" at the narrowest window", NARROWEST]]) {
      test(`chat text ${t}px, ${mode}${where}: an open chat's messages and box read at ${t}px; the list, the top bar and the box's buttons don't change`, async ({ cue, page }) => {
        if (viewport) await page.setViewportSize(viewport);
        await cue.open("board-waiting", { frozen: true });
        await setChat(cue, t, mode);
        await openChat(page);
        const chat = page.locator(".active-pane .ap-chat").first();
        expect(await typeBody(chat)).toBe(`${t}px`);
        expect(await px(page.locator(".active-pane .composer textarea").first())).toBe(`${t}px`);
        // Around the chat, the scale's sizes: a Waiting card's words, the top bar's buttons, the box's own Send.
        expect(await px(page.locator(".sec-waiting .nrow").first())).toBe("13px");
        expect(await px(page.locator(".switch-view button").first())).toBe("12px");
        expect(await px(page.locator(".active-pane .composer .send"))).toBe("13px");
        expect(await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--type-body").trim())).toBe("13px");
        // A− / A+ next to the Chat tab, the end step's one off.
        const ctl = page.locator(".active-pane .stabs .ctext button");
        await expect(ctl).toHaveText(["A−", "A+"]);
        await expect(ctl.first()).toBeEnabled({ enabled: t !== STEPS[0] });
        await expect(ctl.last()).toBeEnabled({ enabled: t !== STEPS.at(-1) });
        expect(await outOfPlace(page, ".active-pane")).toEqual([]);
      });
    }
  }
}

test("A+ and A− step the chat's words and save the step; nothing else changes size, and no hover text", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  await openChat(page);
  const sizes = () => page.evaluate(() => ({
    body: getComputedStyle(document.body).fontSize, card: getComputedStyle(document.querySelector(".nrow")).fontSize,
    tab: getComputedStyle(document.querySelector(".stab")).fontSize, send: getComputedStyle(document.querySelector(".composer .send")).fontSize,
    chat: getComputedStyle(document.querySelector(".ap-chat")).getPropertyValue("--type-body").trim(),
  }));
  const before = await sizes();
  expect(before).toEqual({ body: "13px", card: "13px", tab: "13px", send: "13px", chat: "15px" });
  const ctl = page.locator(".active-pane .stabs .ctext");
  await expect(ctl.locator("[title]")).toHaveCount(0);
  await ctl.getByRole("button", { name: "Larger chat text" }).click();
  await chatSaved(cue, page, 16);
  await expect.poll(sizes).toEqual({ ...before, chat: "16px" });
  await ctl.getByRole("button", { name: "Larger chat text" }).click();
  await chatSaved(cue, page, 18);
  await expect(ctl.getByRole("button", { name: "Larger chat text" })).toBeDisabled();
  for (const t of [16, 15, 14, 13]) {
    await ctl.getByRole("button", { name: "Smaller chat text" }).click();
    await chatSaved(cue, page, t);
  }
  await expect.poll(sizes).toEqual({ ...before, chat: "13px" });
  await expect(ctl.getByRole("button", { name: "Smaller chat text" })).toBeDisabled();
  // No zoom saved by any of it.
  expect((await cue.calls("set_setting")).filter((c) => c.args.key !== "appearance.chat_text")).toEqual([]);
});

for (const [where, viewport] of [["", null], [" at the narrowest window", NARROWEST]]) {
  test(`the Desk lead's chat has A− / A+ too${where}: its chat and box follow them, the item list doesn't`, async ({ cue, page }) => {
    if (viewport) await page.setViewportSize(viewport);
    await cue.open("desk", { frozen: true });
    await openDesk(page);
    const chat = page.locator(".dk-chat").first();
    await chat.waitFor();
    // Polled: the Desk redraws as it opens, and a replaced (detached) node has no styles.
    await expect.poll(() => typeBody(chat)).toBe("15px");
    await page.locator(".dk-ptabs .ctext").getByRole("button", { name: "Larger chat text" }).click();
    await chatSaved(cue, page, 16);
    await expect.poll(() => typeBody(page.locator(".dk-chat").first())).toBe("16px");
    await expect.poll(() => px(page.locator(".dk-box textarea").first())).toBe("16px");
    await expect.poll(() => px(page.locator(".dk-it .dk-t").first())).toBe("13px");
    expect(await outOfPlace(page, "#app")).toEqual([]);
  });
}

test("an item's chat has A− / A+ on its Chat tab row; the other tabs don't", async ({ cue, page }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d5"]').first().click();
  const head = page.locator(".dk-right .dk-itabs");
  await expect(head.locator(".ctext button")).toHaveText(["A−", "A+"]);
  await head.locator(".ctext").getByRole("button", { name: "Smaller chat text" }).click();
  await chatSaved(cue, page, 14);
  await expect.poll(() => typeBody(page.locator(".dk-right .dk-ichat").first())).toBe("14px");
  await page.locator(".dk-right .dk-itab", { hasText: "Record" }).click();
  await expect(page.locator(".dk-right .dk-itabs .ctext")).toHaveCount(0);
});

test("⌘+ / ⌘− / ⌘0 zoom the whole window in Settings → Text size's four steps, as before; the chat's own size stays", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  await page.keyboard.press(",");
  const seg = page.locator('.seg:has([data-set="appearance.zoom"])');
  await expect(seg.locator("button")).toHaveText(["Compact", "Default", "Large", "Larger"]);
  await expect(seg.locator("button.on")).toHaveText("Default");
  await page.keyboard.press("Escape");
  // Each step from the zoom Cue's state has; the next key waits for the saved one to come back in it.
  const saved = async (z) => { await cue.called("set_setting", { key: "appearance.zoom", value: z }); await expect.poll(() => page.evaluate(() => state.settings.appearance.zoom)).toBe(z); };
  await page.keyboard.press("Meta+=");
  await saved(1.15);
  await expect(page.locator(".toast")).toHaveText("Text size: Large");
  await page.keyboard.press("Meta+=");
  await saved(1.3);
  await page.keyboard.press("Meta+=");
  await expect(page.locator(".toast")).toHaveText("Text size: Larger (the largest)");
  await page.keyboard.press("Meta+0");
  await saved(1);
  await page.keyboard.press("Meta+-");
  await saved(0.9);
  await expect(page.locator(".toast")).toHaveText("Text size: Compact");
  expect((await cue.calls("set_setting")).filter((c) => c.args.key === "appearance.chat_text")).toEqual([]);
  await openChat(page);
  expect(await typeBody(page.locator(".active-pane .ap-chat").first())).toBe("15px");
});
