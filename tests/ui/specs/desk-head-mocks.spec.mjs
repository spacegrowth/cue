// #928: a finished (picked) mocks item's "Open the mocks ↗" sits in its header's row like the buttons beside it:
// the same height, centred on the row with "Done", inside the header. Light and dark, laptop and smallest window.
import { test, expect, openDesk } from "./cue.mjs";
import { openDone } from "./desk.mjs";

const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);

for (const scheme of ["light", "dark"]) {
  for (const [size, viewport] of [["laptop", { width: 1440, height: 900 }], ["smallest", { width: 900, height: 600 }]]) {
    test(`a finished mocks item's Open the mocks sits centred in its header's row (${scheme}, ${size}) (#928)`, async ({ page, cue }) => {
      await page.setViewportSize(viewport);
      await page.emulateMedia({ colorScheme: scheme });
      await cue.open("desk");
      await openDesk(page);
      await patchItem(cue, "d5", { state: "done", picked: "B", title: "Mocks: working items", designs: "https://claude.ai/artifact/MOCKS", next: [], updated: Date.now() });
      await openDone(page);
      await page.locator('.dkn-items [data-desk-item="d5"]').getByText("Mocks: working items").click();
      const row = page.locator(".dk-right .dkh-r"), btn = row.locator(".dkh-acts .dk-dlink"), st = row.locator(".dkh-st");
      await expect(btn).toHaveText("Open the mocks ↗");
      await expect(st).toHaveText("Done");
      const box = (l) => l.evaluate((el) => { const r = el.getBoundingClientRect(); return { top: r.top, bottom: r.bottom, mid: (r.top + r.bottom) / 2, h: r.height }; });
      const [r, b, s, head] = [await box(row), await box(btn), await box(st), await box(page.locator(".dk-right .dkh"))];
      // A button's height, the row's own; inside the header; its words centred in it and on the row with "Done".
      expect(b.h).toBe(28);
      expect(b.top).toBeGreaterThanOrEqual(head.top);
      expect(b.top).toBeGreaterThanOrEqual(r.top);
      expect(b.bottom).toBeLessThanOrEqual(r.bottom);
      expect(Math.abs(b.mid - r.mid)).toBeLessThanOrEqual(1);
      expect(Math.abs(b.mid - s.mid)).toBeLessThanOrEqual(1.5);
      const text = await btn.evaluate((el) => { const rg = document.createRange(); rg.selectNodeContents(el); const t = rg.getBoundingClientRect(), r = el.getBoundingClientRect(); return { above: t.top - r.top, below: r.bottom - t.bottom, size: getComputedStyle(el).fontSize, small: getComputedStyle(document.querySelector(".dk-right .dkh-st")).fontSize }; });
      expect(Math.abs(text.above - text.below)).toBeLessThanOrEqual(1.5);
      await expect(page.locator(".dk-right [title]")).toHaveCount(0);
      if (process.env.SHOT_DIR) await page.locator(".dk-right .dkh").screenshot({ path: `${process.env.SHOT_DIR}/928-${size}-${scheme}.png` });
    });
  }
}
