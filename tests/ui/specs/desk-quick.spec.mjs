// Quick phrases per kind of chat (#840, chat/compose.js QUICK_BY_CHAT): an item's page has none over its box, and the row
// takes no space; the lead's chat has only "Go ahead", sent at once as its own message.
import { test, expect, openDesk } from "./cue.mjs";

const composer = ".dk-right .foot:not(.held) .composer";

async function openItem(cue, page, kind = "task") {
  await cue.open("desk");
  await cue.change((st, type) => {
    st.settings.quick = { phrases: ["Go ahead", "show me"] };
    st.desk.projects[0].items.find((i) => i.id === "d2").kind = type;
    st.items = [];
  }, kind);
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
}

for (const [kind, scheme, size] of [["task", "light", [1440, 900]], ["feature", "dark", [1440, 900]], ["bug", "light", [900, 600]], ["task", "dark", [900, 600]]]) {
  test(`a ${kind} page has no quick phrases and no empty row over its box (${scheme}, ${size.join("×")})`, async ({ cue, page }) => {
    await page.setViewportSize({ width: size[0], height: size[1] });
    await page.emulateMedia({ colorScheme: scheme });
    await openItem(cue, page, kind);
    await expect(page.locator(`${composer} textarea`)).toBeVisible();
    await expect(page.locator(`${composer} .quick`)).toHaveCount(0);
    // Nothing between the box's top and its text row: no space kept for an empty row.
    const gap = await page.evaluate((c) => {
      const box = document.querySelector(c), row = box?.querySelector(".field-row");
      return row ? row.getBoundingClientRect().top - box.getBoundingClientRect().top : null;
    }, composer);
    expect(gap).not.toBeNull();
    expect(gap).toBeLessThan(24);
  });
}

test("the lead's chat shows only Go ahead, readable, and sends it at once", async ({ cue, page }) => {
  for (const scheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await cue.change((st) => { st.settings.quick = { phrases: ["Commit", "Go ahead", "Release"] }; st.items = []; });
    await openDesk(page);
    const qp = page.locator(`${composer} .quick .qp`);
    await expect(qp).toHaveText(["Go ahead"]);
    const [fg, bg] = await page.evaluate((c) => { const s = getComputedStyle(document.querySelector(`${c} .quick .qp`)); return [s.color, s.backgroundColor]; }, composer);
    expect(fg).not.toBe(bg);
  }
  await page.locator(`${composer} .quick .qp`).click();
  await cue.called("desk_send", { project: "cue", text: "Go ahead" });
});
