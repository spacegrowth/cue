// A parked item's page (#847): Start out as its main button, Drop this item inside its ⋯, the ⋯ the quiet icon button
// other menus have, and its menu whole inside the pane it opens in. Light and dark, laptop and the smallest window.
import { test, expect, openDesk } from "./cue.mjs";

const LAPTOP = { width: 1440, height: 900 }, SMALLEST = { width: 900, height: 600 };

async function openParked(page, cue, size) {
  await page.setViewportSize(size);
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.items.push({ id: "p1", project: "cue", kind: "task", title: "Tidy the header", state: "held", created: Date.now() / 1000, updated: Date.now() / 1000, holder: null, after: null, from: "you", held: [Math.floor(Date.now() / 1000), "You", ""] });
  });
  await page.locator('.dkn-items [data-desk-c-fold="parked"]').click();
  await page.locator('.dkn-items [data-desk-item="p1"]').click();
  return page.locator('.dk-right .dkn-sp[data-desk-page="item"]');
}

for (const [name, size] of [["laptop", LAPTOP], ["the smallest window", SMALLEST]]) {
  for (const dark of [false, true]) {
    test(`${name}${dark ? ", dark" : ""}: a parked item shows Start, and its ⋯ opens Drop inside the pane`, async ({ page, cue }) => {
      if (dark) await page.emulateMedia({ colorScheme: "dark" });
      const pane = await openParked(page, cue, size);
      const bar = pane.locator(".dkn-ib");
      await expect(bar.locator('.btn.primary[data-desk-act="start_now"]')).toHaveText("Start");
      await expect(bar.locator('.dk-acts > [data-desk-act="drop"]')).toHaveCount(0);
      // The quiet ⋯: no border, no fill till pointed at.
      const more = bar.locator(".dk-more");
      const look = await more.evaluate((e) => { const c = getComputedStyle(e); return [c.borderTopWidth, c.backgroundColor]; });
      expect(look[0]).toBe("0px");
      expect(look[1]).toMatch(/rgba\(0, 0, 0, 0\)|transparent/);
      await more.click();
      const menu = bar.locator(".dk-moremenu");
      await expect(menu.locator('[role="menuitem"]')).toHaveText(["Drop this item"]);
      const m = await menu.boundingBox(), p = await page.locator(".dk-c > .dk-right").boundingBox(), s = await bar.locator('[data-desk-act="start_now"]').boundingBox();
      expect(m.x).toBeGreaterThanOrEqual(p.x);
      expect(m.x + m.width).toBeLessThanOrEqual(p.x + p.width + 0.5);
      expect(m.y + m.height).toBeLessThanOrEqual(size.height);
      // Start isn't under it.
      const over = !(m.x + m.width <= s.x || s.x + s.width <= m.x || m.y + m.height <= s.y || s.y + s.height <= m.y);
      expect(over).toBe(false);
      if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/parked-more-${size.width}${dark ? "-dark" : ""}.png` });
      // Two presses drop it: the first arms it and leaves the menu open.
      await menu.locator('[data-desk-act="drop"]').click();
      await expect(menu.locator('[data-desk-act="drop"]')).toHaveText("Drop it");
      expect(await cue.calls("desk_item_act")).toEqual([]);
      await menu.locator('[data-desk-act="drop"]').click();
      await cue.called("desk_item_act", { id: "p1", act: "drop" });
      await expect(page.locator("[title]")).toHaveCount(0);
    });
  }
}

test("Start on a parked item's page starts it", async ({ page, cue }) => {
  const pane = await openParked(page, cue, LAPTOP);
  await pane.locator('.dkn-ib [data-desk-act="start_now"]').click();
  await cue.called("desk_item_act", { id: "p1", act: "start_now" });
});
