// A permission's buttons sit at the right end of its title line (#800), where an item's buttons go: you answer without
// scrolling past the command, and nothing sits under the command. Checked in a session's chat and on the Desk (a step
// held for your OK, its title long), at laptop size and the smallest window, light and dark.
import { test, expect, openDesk } from "./cue.mjs";

const SIZES = [{ width: 1440, height: 900 }, { width: 900, height: 600 }];

/** Where a title line's text and buttons are, against the card they're on. */
const place = (head, card) => head.evaluate((h, c) => {
  const r = (e) => e.getBoundingClientRect();
  const t = r(h.querySelector(".ask-ht")), b = r(h.querySelector(".ask-hbtns")), k = r(h.closest(c)), cmd = h.closest(c).querySelector("pre");
  return { titleTop: t.top, btnTop: b.top, btnBottom: b.bottom, btnRight: b.right, btnLeft: b.left, titleRight: t.right, cardRight: k.right, cardLeft: k.left, cmdTop: cmd ? r(cmd).top : null };
}, card);

for (const scheme of ["light", "dark"]) {
  test(`a permission's No and Allow sit at the right end of its title line, above the command (${scheme})`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("board-waiting");
    await page.locator('.nrow[data-big="n4"]').click();
    const card = page.locator(".active-pane .act-card");
    const head = card.locator(".act-head");
    await expect(head.locator('[data-act="allow"]')).toHaveText("Allow");
    await expect(head.locator('[data-act="deny"]')).toBeVisible();
    await expect(card.locator(":scope > .row-btns")).toHaveCount(0);
    for (const size of SIZES) {
      await page.setViewportSize(size);
      const p = await place(head, ".act-card");
      expect(p.btnLeft).toBeGreaterThan(p.titleRight - 1);         // beside the title, not under it
      expect(p.cardRight - p.btnRight).toBeLessThan(24);           // right-aligned in the card
      expect(p.btnBottom).toBeLessThanOrEqual(p.cmdTop);           // above the command
    }
    await head.locator('[data-act="allow"]').click();
    expect((await cue.called("respond", { id: "n4" })).args.decision).toEqual({ behavior: "allow" });
  });

  test(`a long title wraps and its No and Allow stay in the card (${scheme})`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await openDesk(page);
    await cue.change((st) => {
      const d = st.desk.projects.find((x) => x.project === "cue");
      d.held = [{ id: "held-1", sid: "site", who: "cue-748-2", what: "delete files in the build folder and every cached copy of the old screenshots it made while it worked", command: "rm -rf build shots", at: 1 }];
    });
    const row = page.locator(".dk-items .dk-held").first();
    const head = row.locator(".dk-request-who");
    await expect(head.locator(".row-btns button")).toHaveText(["No", "Allow"]);
    for (const size of SIZES) {
      await page.setViewportSize(size);
      const p = await place(head, ".dk-request");
      expect(p.btnRight).toBeLessThanOrEqual(p.cardRight);
      expect(p.btnLeft).toBeGreaterThanOrEqual(p.cardLeft);
      expect(p.btnBottom).toBeLessThanOrEqual(p.cmdTop);
    }
  });
}
