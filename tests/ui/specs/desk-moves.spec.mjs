// What the lead moved (#374): the Dispatcher's "Dispatcher Actions" tab (named so in #390) lists every move the lead made on its own, newest first, each
// with its item, why and time; older ones load from Cue as you scroll; Clear asks once, then empties it.
import { test, expect } from "./cue.mjs";
import { NOW } from "./clock.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
const s = (ms) => Math.floor(ms / 1000);
const act = (id, item, act, text, why, at) => ({ id, project: "cue", item_id: item, by: "web-lead", sid: "lead", text, why, at: s(at), undo: "none", act });
const DAY = 24 * 60 * 60 * 1000;

test("the lead's own moves stay in a list, newest first, older ones load as you scroll, until you clear it", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1728, height: 900 });
  await cue.open("desk");
  await openDesk(page);
  // Today's in the Desk's state; 70 older ones only in Cue's database.
  const today = [
    act("t1", "d3", "send_back", "Sent #d3 to web again", "the header still jumps", NOW - 60 * 1000),
    act("t2", "d4", "start", "Started #d4 now, without waiting for a copy", "the user needs it now", NOW - 5 * 60 * 1000),
  ];
  const older = Array.from({ length: 70 }, (_, k) => act(`o${String(k).padStart(2, "0")}`, "d8", "close", `Closed #d8: its work is on main (old ${k})`, "it landed by hand", NOW - (k + 2) * DAY));
  await cue.change((st, { today, older }) => {
    const d = st.desk.projects.find((p) => p.project === "cue");
    d.decisions.push(...today);
    d.moves = older;
  }, { today, older });
  // Its tab and the list's title say "Dispatcher Actions" (#390), the link under the panel's Just did "More" (#392), never "Moves".
  await expect(page.locator('.dk-right .dk-tab[data-desk-tab="moves"]')).toHaveText("Dispatcher Actions");
  await expect(page.locator('.dk-right .dk-pall[data-desk-tab="moves"]')).toHaveText("More");
  await page.locator('.dk-right .dk-pall[data-desk-tab="moves"]').click();
  const list = page.locator(".dk-moves");
  await expect(list.locator(".dk-mv-t")).toHaveText("Dispatcher Actions");
  expect(await page.locator(".dk-right").innerText()).not.toMatch(/\bmoves\b/i);
  const rows = list.locator(".dk-mv");
  await expect(rows.first()).toContainText("Sent #d3 to web again · the header still jumps");
  await expect(rows.nth(1)).toContainText("Started #d4 now");
  await expect(rows.nth(2)).toContainText("Closed #d8: its work is on main (old 0) · it landed by hand");
  await expect(rows.nth(2).locator(".dk-mv-at")).toContainText(",");   // an older day says its date
  await expect(rows).toHaveCount(30);
  // Scrolled to its end: more load.
  await list.locator("[data-desk-moves-more]").scrollIntoViewIfNeeded();
  await expect(rows).toHaveCount(60);
  await list.locator("[data-desk-moves-more]").scrollIntoViewIfNeeded();
  await expect(rows).toHaveCount(72);
  await expect(rows.last()).toContainText("(old 69)");
  // Its item's number opens the item.
  await rows.first().locator('[data-desk-chk-item="d3"]').click();
  await expect(page.locator('.dk-right .dk-pg[data-desk-page="item"]')).toBeVisible();
  await page.locator(".dk-right .dk-back").first().click();
  await page.locator('.dk-right .dk-tab[data-desk-tab="moves"]').click();
  // Clear asks once; Keep leaves it; Clear again empties it.
  await list.locator("[data-desk-moves-clear]").click();
  await expect(list.locator(".dk-mv-ask")).toHaveText("Clear the whole list?");
  await list.locator("[data-desk-moves-keep]").click();
  await expect(list.locator(".dk-mv-ask")).toHaveCount(0);
  await list.locator("[data-desk-moves-clear]").click();
  await list.locator("[data-desk-moves-clear]").click();
  await expect(rows).toHaveCount(0);
  await expect(list.locator(".dk-empty")).toContainText("stays until you clear it");
  expect((await cue.calls("desk_moves_clear")).length).toBe(1);
});
