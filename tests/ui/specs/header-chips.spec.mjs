// The header's chips (ui/board_motion.js headerChips): no "N live" chip; "N waiting" opens its list, newest first,
// a click there opens that session, and ‹ › on the chip step on from it.
import { test, expect } from "./cue.mjs";

test("no live chip; waiting opens its sessions newest first, a click opens one, and ‹ › step on from it", async ({ cue, page }) => {
  await cue.open("many-sessions");
  await expect(page.locator(".top .lv-chip")).toHaveCount(0);
  await expect(page.locator(".top .hchips")).not.toContainText(" live");
  const chip = page.locator('.top [data-chip-list="waiting"]');
  await expect(chip).toContainText("6 waiting");
  await chip.click();
  const rows = page.locator(".wt-pop .wt-row");
  // Newest first: by when each started waiting on you.
  const want = await page.evaluate(() => state.items.filter((i) => i.kind === "waiting").sort((a, b) => b.created_ms - a.created_ms).map((i) => i.id));
  expect(want.length).toBe(6);
  expect(await rows.evaluateAll((els) => els.map((el) => el.dataset.chipPick))).toEqual(want);
  // One click opens that one, and the list closes.
  await rows.nth(2).click();
  await expect(page.locator(".wt-pop")).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => active?.id)).toBe(want[2]);
  await expect(page.locator(".top .hchip.nav")).toContainText("3 of 6 waiting");
  await page.locator('.top [data-chip-step="waiting:1"]').click();
  await expect.poll(() => page.evaluate(() => active?.id)).toBe(want[3]);
  await page.locator('.top [data-chip-step="waiting:-1"]').click();
  await page.locator('.top [data-chip-step="waiting:-1"]').click();
  await expect.poll(() => page.evaluate(() => active?.id)).toBe(want[1]);
  // From the stepping chip, its middle opens the list again, the one shown marked; Esc closes it, as does a click away.
  await page.locator('.top .hnav[data-chip-list="waiting"]').click();
  await expect(page.locator(".wt-pop .wt-row.on")).toHaveAttribute("data-chip-pick", want[1]);
  await page.keyboard.press("Escape");
  await expect(page.locator(".wt-pop")).toHaveCount(0);
  await page.locator('.top [data-chip-list="waiting"]').click();
  await page.locator(".col.main").click({ position: { x: 5, y: 5 } });
  await expect(page.locator(".wt-pop")).toHaveCount(0);
});

test("⌘L still opens every live session, now in the middle of the window", async ({ cue, page }) => {
  await cue.open("many-sessions");
  await page.keyboard.press("Meta+l");
  await expect(page.locator(".spot-scrim .lv-pop")).toBeVisible();
  await page.keyboard.press("Meta+l");
  await expect(page.locator(".lv-pop")).toHaveCount(0);
});

test("waiting says no 'oldest'; its dot blinks only while a session you wrote to has come back and waits unopened", async ({ cue, page }) => {
  await cue.open("many-sessions");
  const chip = page.locator('.top [data-chip-list="waiting"]');
  await expect(chip).toHaveText("6 waiting");
  await expect(chip.locator(".hdot.back")).toHaveCount(0);
  // You wrote to one 2 minutes ago and it came back a minute later: its reply waits on you.
  const id = await page.evaluate(() => state.items.filter((i) => i.kind === "waiting")[0].id);
  await cue.change((st, id) => {
    const it = st.items.find((i) => i.id === id), s = st.sessions.find((x) => x.session_id === it.session_id);
    const t = Date.now();
    it.created_ms = t - 60000;
    s.thread = [...(s.thread || []), { role: "you", text: "and the footer", at_ms: t - 120000 }];
   }, id);
  await expect(chip.locator(".hdot.back")).toHaveCount(1);
  // The test page stills every animation: that taken off, it's the blink.
  await page.evaluate(() => [...document.querySelectorAll("style")].filter((s) => s.textContent.includes("animation:none!important")).forEach((s) => s.remove()));
  expect(await chip.locator(".hdot").evaluate((el) => getComputedStyle(el).animationName)).toBe("pulse");
  // Opened, it's read: steady again.
  await chip.click();
  await page.locator(`.wt-pop [data-chip-pick="${id}"]`).click();
  await expect(page.locator(".top .hdot.back")).toHaveCount(0);
});
