// Each row in the Desk's item list has a ⋯ at its right end (#839): the item page's own menu (#669: Start now, Park
// and the others that fit), without opening the item. It shows as you point at the row and always on the open row;
// Esc or a click outside shuts it. In Waiting on you, Working, Up next and Parked; light and dark, laptop and the
// smallest window.
import { test, expect, openDesk } from "./cue.mjs";
import { openMore } from "./desk.mjs";

const LAPTOP = { width: 1440, height: 900 }, SMALLEST = { width: 900, height: 600 };
const row = (page, id) => page.locator(`.dkn-items [data-desk-item="${id}"]`);
const dots = (page, id) => row(page, id).locator(":scope > .dkn-rwrap > .dkn-rmore");
const menu = (page) => page.locator(".dkn-items .dkn-rwrap .dk-moremenu");
const shown = (loc) => loc.evaluate((e) => getComputedStyle(e.closest(".dkn-rwrap")).opacity);

/** The Desk with a parked item (p1) and a new one (n1) added, d1 (asks you) and d3 (working) stopped mid-turn so
 *  Resume fits them; Up next and then Parked open. */
async function open(page, cue, size = LAPTOP) {
  await page.setViewportSize(size);
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.items.push({ id: "p1", project: "cue", kind: "task", title: "Tidy the header", state: "held", created: Date.now() / 1000, updated: Date.now() / 1000, holder: null, from: "you" });
    for (const id of ["d1", "d3"]) d.items.find((i) => i.id === id).stopped = true;
  });
  await expect(row(page, "p1")).toHaveCount(1);
}

test("a row's ⋯ holds only Start now, Park and Drop, and opening it doesn't open the item; its page keeps its own menu", async ({ page, cue }) => {
  await open(page, cue);
  // Up next: d4 waits on d3 and has a session. No Put it before… here: that stays on its page.
  await dots(page, "d4").click();
  await expect(menu(page)).toHaveAttribute("role", "menu");
  await expect(menu(page).locator('[role="menuitem"]')).toHaveText(["Start now", "Park", "Drop this item"]);
  // Nothing opened: no row lit, the lead's chat still in the middle.
  await expect(page.locator(".dkn-items .on[data-desk-item]")).toHaveCount(0);
  await expect(page.locator(".dk-right .dkn-ib")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(menu(page)).toHaveCount(0);
  // Its page's menu is as it was, Put it before… in it.
  await row(page, "d4").click();
  await expect(row(page, "d4")).toHaveClass(/\bon\b/);
  const side = page.locator(".dk-right");
  await openMore(side);
  await expect(side.locator(".dk-moremenu [role='menuitem']")).toHaveText(["Start now", "Put it before Search stays centred", "Park"]);
  // One menu at a time: the row's ⋯ shuts the page's.
  await dots(page, "d4").click();
  await expect(page.locator(".dk-moremenu")).toHaveCount(1);
  await expect(menu(page)).toHaveCount(1);
});

test("Waiting on you, Working, Up next and Parked: Start now and Park wherever nothing is at work yet, Drop on all but done", async ({ page, cue }) => {
  await open(page, cue);
  const cases = [
    ["d1", ["Start now", "Drop this item"]],   // asks you, its session stopped: Start now wakes it
    ["d3", ["Start now", "Drop this item"]],   // working, stopped
    ["d5", ["Drop this item"]],                // at work: nothing to start or park
    ["d2", ["Drop this item"]],                // ready to check
    ["d6", ["Start now", "Park", "Drop this item"]],   // new, couldn't be given out
  ];
  for (const [id, want] of cases) {
    await dots(page, id).click();
    await expect(menu(page).locator('[role="menuitem"]')).toHaveText(want);
    await page.keyboard.press("Escape");
    await expect(menu(page)).toHaveCount(0);
  }
  // Park does it and shuts the menu.
  await dots(page, "d6").click();
  await menu(page).locator('[data-desk-act="park"]').click();
  await cue.called("desk_item_act", { id: "d6", act: "park" });
  await expect(menu(page)).toHaveCount(0);
  // Drop takes two presses: the first arms it and keeps the menu open.
  await dots(page, "d5").click();
  await menu(page).locator('[data-desk-act="drop"]').click();
  await expect(menu(page).locator('[data-desk-act="drop"]')).toHaveText("Drop it");
  await menu(page).locator('[data-desk-act="drop"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "drop" });
  // Parked: open its drawer, Start.
  await page.locator('.dkn-items [data-desk-c-fold="parked"]').click();
  await dots(page, "p1").click();
  await expect(menu(page).locator('[role="menuitem"]')).toHaveText(["Start", "Drop this item"]);
  await menu(page).locator('[data-desk-act="start_now"]').click();
  await cue.called("desk_item_act", { id: "p1", act: "start_now" });
  await expect(menu(page)).toHaveCount(0);
  // Done rows have no ⋯.
  await expect(row(page, "d7").locator(".dkn-rwrap")).toHaveCount(0);
});

test("the ⋯ shows as you point at a row and stays on the open one, and showing it moves nothing", async ({ page, cue }) => {
  await open(page, cue);
  await page.mouse.move(2, 2);
  const now = row(page, "d3").locator(".dkn-now"), title = row(page, "d3").locator(".dkn-tt");
  const before = [await now.boundingBox(), await title.boundingBox()];
  await expect.poll(() => shown(dots(page, "d3"))).toBe("0");
  await row(page, "d3").hover();
  await expect.poll(() => shown(dots(page, "d3"))).toBe("1");
  expect([await now.boundingBox(), await title.boundingBox()]).toEqual(before);
  // At the row's right end, past its Now column.
  const b = await dots(page, "d3").boundingBox(), r = await row(page, "d3").boundingBox();
  expect(b.x).toBeGreaterThanOrEqual(before[0].x + before[0].width);
  expect(b.x + b.width).toBeLessThanOrEqual(r.x + r.width);
  // Up next keeps room for it beside the drag grip.
  await row(page, "d4").hover();
  const g = await row(page, "d4").locator(".dk-grip").boundingBox(), u = await dots(page, "d4").boundingBox();
  expect(u.x + u.width).toBeLessThanOrEqual(g.x);
  // Open a row: its ⋯ stays with the pointer gone.
  await row(page, "d1").click();
  await page.mouse.move(2, 2);
  await expect.poll(() => shown(dots(page, "d1"))).toBe("1");
  await expect.poll(() => shown(dots(page, "d3"))).toBe("0");
  // A click outside shuts its menu.
  await dots(page, "d1").click();
  await expect(menu(page)).toHaveCount(1);
  await page.locator(".dkn-items .dkn-ih").click({ position: { x: 4, y: 4 } });
  await expect(menu(page)).toHaveCount(0);
  await expect(page.locator(".dkn-items [title]")).toHaveCount(0);
});

for (const [name, size] of [["laptop", LAPTOP], ["the smallest window", SMALLEST]]) {
  for (const dark of [false, true]) {
    test(`${name}${dark ? ", dark" : ""}: the ⋯ inside its row and its menu inside the window`, async ({ page, cue }) => {
      if (dark) await page.emulateMedia({ colorScheme: "dark" });
      await open(page, cue, size);
      for (const id of ["d1", "d4"]) {
        await row(page, id).hover();
        const b = await dots(page, id).boundingBox(), r = await row(page, id).boundingBox();
        expect(b.width).toBeGreaterThan(10);
        expect(b.x).toBeGreaterThanOrEqual(r.x);
        expect(b.x + b.width).toBeLessThanOrEqual(r.x + r.width + 0.5);
        expect(b.y).toBeGreaterThanOrEqual(r.y);
        expect(b.y + b.height).toBeLessThanOrEqual(r.y + r.height + 0.5);
      }
      // In the card, a row's open menu lies over no Now word, its own or another row's, and each still says what it did
      // ("Ready to check" never reads as "Start now").
      const words = () => page.locator(".dkn-card .dkn-row[data-desk-item] .dkn-now").allTextContents();
      const said = await words();
      expect(said.join(" ")).toContain("Ready to check");
      for (const id of ["d1", "d2", "d3", "d5"]) {
        await row(page, id).hover();
        await dots(page, id).click();
        const mb = await menu(page).boundingBox();
        const over = await page.locator(".dkn-card .dkn-row .dkn-now > *").evaluateAll((els, b) => els.map((e) => e.getBoundingClientRect()).filter((r) => r.width && r.left < b.x + b.width && r.right > b.x && r.top < b.y + b.height && r.bottom > b.y).length, mb);
        expect(over, `${id}'s menu lies over a Now word`).toBe(0);
        // Under its ⋯ when there's room, else over it.
        const bb = await dots(page, id).boundingBox(), vh = page.viewportSize().height;
        if (vh - (bb.y + bb.height) - 12 >= mb.height) expect(mb.y).toBeGreaterThanOrEqual(bb.y + bb.height);
        else expect(mb.y + mb.height).toBeLessThanOrEqual(bb.y + 0.5);
        expect(await words()).toEqual(said);
        await page.keyboard.press("Escape");
      }
      await row(page, "d4").hover();
      await dots(page, "d4").click();
      const m = await menu(page).boundingBox();
      expect(m.x).toBeGreaterThanOrEqual(0);
      expect(m.y).toBeGreaterThanOrEqual(0);
      expect(m.x + m.width).toBeLessThanOrEqual(size.width);
      expect(m.y + m.height).toBeLessThanOrEqual(size.height);
      if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/row-more-${size.width}${dark ? "-dark" : ""}.png` });
    });
  }
}
