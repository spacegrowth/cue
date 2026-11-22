// Text size, one for the whole window (#684). A− / A+ on a chat's head (the Board's, the Desk's, the Inbox's), ⌘+ / ⌘− /
// ⌘0 and Settings → Text size all set the same step (appearance.zoom: Compact, Default, Large, Larger; the app's
// webview zooms, so here only what's saved and said is seen). A chat's words stay 15px on the page at every step, the
// labels around them 13px: they grow and shrink together. There is no size for a chat alone any more.
import { test, expect, openDesk } from "./cue.mjs";
import { NARROWEST } from "./layout.mjs";

const STEPS = [[0.9, "Compact"], [1, "Default"], [1.15, "Large"], [1.3, "Larger"]];
const setZoom = (cue, z, mode = "system") => cue.change((st, [z, mode]) => { st.settings.appearance = { ...st.settings.appearance, zoom: z, mode }; }, [z, mode]);
/** Step `z` was saved, and Cue's state has it back (the next click or key steps from there). */
const saved = async (cue, page, z) => { await cue.called("set_setting", { key: "appearance.zoom", value: z }); await expect.poll(() => page.evaluate(() => state.settings.appearance.zoom)).toBe(z); };
const px = (loc) => loc.evaluate((el) => getComputedStyle(el).fontSize);
const typeBody = (loc) => loc.evaluate((el) => getComputedStyle(el).getPropertyValue("--type-body").trim());
// Its top-left corner: in a narrow window the card's middle is its Move to bottom button.
const openChat = (page) => page.locator('.nrow[data-big="n1"]').click({ position: { x: 12, y: 12 } }).then(() => page.locator(".active-pane .ap-chat").first().waitFor());
const smaller = (ctl) => ctl.getByRole("button", { name: "Smaller text" }), larger = (ctl) => ctl.getByRole("button", { name: "Larger text" });

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

for (const [z, name] of STEPS) {
  for (const mode of ["light", "dark"]) {
    for (const [where, viewport] of [["", null], [" at the narrowest window", NARROWEST]]) {
      test(`text size ${name}, ${mode}${where}: A− / A+ on an open chat, the end step's one off; the chat's words 15px, the rest 13px`, async ({ cue, page }) => {
        if (viewport) await page.setViewportSize(viewport);
        await cue.open("board-waiting", { frozen: true });
        await setZoom(cue, z, mode);
        await openChat(page);
        expect(await typeBody(page.locator(".active-pane .ap-chat").first())).toBe("15px");
        expect(await px(page.locator(".active-pane .composer textarea").first())).toBe("15px");
        expect(await px(page.locator(".sec-waiting .nrow").first())).toBe("13px");
        expect(await px(page.locator(".active-pane .composer .send"))).toBe("13px");
        const ctl = page.locator(".active-pane .stabs .ctext button");
        await expect(ctl).toHaveText(["A−", "A+"]);
        await expect(ctl.first()).toBeEnabled({ enabled: z !== STEPS[0][0] });
        await expect(ctl.last()).toBeEnabled({ enabled: z !== STEPS.at(-1)[0] });
        expect(await outOfPlace(page, ".active-pane")).toEqual([]);
      });
    }
  }
}

test("A+ and A− step the whole window's text size, one step a click, with a note each time; no hover text", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  await openChat(page);
  const ctl = page.locator(".active-pane .stabs .ctext");
  await expect(ctl.locator("[title]")).toHaveCount(0);
  await larger(ctl).click();
  await saved(cue, page, 1.15);
  await expect(page.locator(".toast")).toHaveText("Text size: Large");
  await larger(ctl).click();
  await saved(cue, page, 1.3);
  await expect(page.locator(".toast")).toHaveText("Text size: Larger");
  await expect(larger(ctl)).toBeDisabled();
  for (const [z, name] of [...STEPS].reverse().slice(1)) {
    await smaller(ctl).click();
    await saved(cue, page, z);
    await expect(page.locator(".toast")).toHaveText(`Text size: ${name}`);
  }
  await expect(smaller(ctl)).toBeDisabled();
  await expect(larger(ctl)).toBeEnabled();
  // The one size: nothing else is saved, and the chat's words are where they were on the page.
  expect((await cue.calls("set_setting")).filter((c) => c.args.key !== "appearance.zoom")).toEqual([]);
  expect(await typeBody(page.locator(".active-pane .ap-chat").first())).toBe("15px");
});

test("A− / A+ and ⌘+ / ⌘− are one size: each steps from where the other left it, Settings shows it, ⌘0 goes back to Default", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  await openChat(page);
  const ctl = page.locator(".active-pane .stabs .ctext");
  await larger(ctl).click();
  await saved(cue, page, 1.15);
  await page.keyboard.press("Meta+=");
  await saved(cue, page, 1.3);
  await expect(larger(ctl)).toBeDisabled();
  await page.keyboard.press("Meta+=");
  await expect(page.locator(".toast")).toHaveText("Text size: Larger (the largest)");
  await page.keyboard.press("Meta+-");
  await saved(cue, page, 1.15);
  await expect(larger(ctl)).toBeEnabled();
  await page.keyboard.press(",");
  const seg = page.locator('.seg:has([data-set="appearance.zoom"])');
  await expect(seg.locator("button")).toHaveText(["Compact", "Default", "Large", "Larger"]);
  await expect(seg.locator("button.on")).toHaveText("Large");
  // One row for it, and it says the mini window keeps its own size.
  const row = page.locator(".set-row", { has: seg });
  await expect(row).toContainText("A− and A+ on a chat, or ⌘+ and ⌘−");
  await expect(row).toContainText("The mini window keeps its own size.");
  await expect(page.locator(".sheet-body").getByText("Text size", { exact: true })).toHaveCount(1);
  await page.keyboard.press("Meta+0");
  await saved(cue, page, 1);
  await expect(page.locator(".toast")).toHaveText("Text size: Default");
  await expect(seg.locator("button.on")).toHaveText("Default");
  await page.keyboard.press("Escape");
  await smaller(ctl).click();
  await saved(cue, page, 0.9);
  await expect(smaller(ctl)).toBeDisabled();
  await page.keyboard.press("Meta+0");
  await saved(cue, page, 1);
  await expect(smaller(ctl)).toBeEnabled();
});

test("a chat-only size saved by an older Cue changes nothing: the chat's words stay 15px", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  await cue.change((st) => { st.settings.appearance = { ...st.settings.appearance, chat_text: 18 }; });
  await openChat(page);
  expect(await typeBody(page.locator(".active-pane .ap-chat").first())).toBe("15px");
  expect(await px(page.locator(".active-pane .composer textarea").first())).toBe("15px");
});

for (const [where, viewport] of [["", null], [" at the narrowest window", NARROWEST]]) {
  test(`the Desk lead's chat has A− / A+ too${where}: they step the same text size`, async ({ cue, page }) => {
    if (viewport) await page.setViewportSize(viewport);
    await cue.open("desk", { frozen: true });
    await openDesk(page);
    const chat = page.locator(".dk-chat").first();
    await chat.waitFor();
    // Polled: the Desk redraws as it opens, and a replaced (detached) node has no styles.
    await expect.poll(() => typeBody(chat)).toBe("15px");
    await larger(page.locator(".dk-ptabs .ctext")).click();
    await saved(cue, page, 1.15);
    await expect.poll(() => typeBody(page.locator(".dk-chat").first())).toBe("15px");
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
  await smaller(head.locator(".ctext")).click();
  await saved(cue, page, 0.9);
  await expect(smaller(page.locator(".dk-right .dk-itabs .ctext"))).toBeDisabled();
  await page.locator(".dk-right .dk-itab", { hasText: "Record" }).click();
  await expect(page.locator(".dk-right .dk-itabs .ctext")).toHaveCount(0);
});

for (const [where, viewport] of [["", null], [" at the narrowest window", NARROWEST]]) {
  test(`the Inbox has A− / A+ in an open session's top row${where}, greyed out at the ends, light and dark`, async ({ cue, page }) => {
    if (viewport) await page.setViewportSize(viewport);
    for (const mode of ["light", "dark"]) {
      await cue.open("board-waiting", { frozen: true });
      await setZoom(cue, 1, mode);
      await page.locator('.switch-view [data-view="inbox"]').click();
      await page.locator(".ib-row").first().click();
      const ctl = page.locator(".inbox-page .ib-head .ctext");
      await expect(ctl.locator("button")).toHaveText(["A−", "A+"]);
      await expect(ctl.locator("[title]")).toHaveCount(0);
      await larger(ctl).click();
      await saved(cue, page, 1.15);
      await larger(ctl).click();
      await saved(cue, page, 1.3);
      await expect(larger(ctl)).toBeDisabled();
      await expect(page.locator(".toast")).toHaveText("Text size: Larger");
      expect(await outOfPlace(page, ".inbox-page .ib-head")).toEqual([]);
    }
  });
}
