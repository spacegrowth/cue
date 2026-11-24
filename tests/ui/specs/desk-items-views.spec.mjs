// What the Items card's Cards view (#666) tested, on the one layout left since #748 (desk/c.js): one view, no List |
// Cards switch; its own Strip | Line switch two buttons side by side, the picked one filled, flipping both ways at
// once and kept across a reload (gone in desk-live, 5ba37d4889: Strip is the one view); the items with sessions in the white card, Up next and Done today side by side on
// the green with each one's line, the calls while away when Away is on; a row opens its item lit in place, back
// returns to the lead's chat; narrow, Up next and Done today stack.
import { test, expect, openDesk, pick } from "./cue.mjs";

const items = (page) => page.locator(".dkn-items");
const card = (page) => items(page).locator(".dkn-card");
const box = (page, k) => items(page).locator(`[data-dkn-drawer="${k}"]`);
/** The Desk shown again after a reload. */
const reopen = async (page) => {
  await page.reload();
  await page.locator('#app [data-view="board"]').waitFor();
  await pick(page, "cue");
  await page.locator('.switch-view [data-view="desk"]').click();
  await card(page).waitFor();
};

// Strip is the one progress view since desk-live (5ba37d4889 "Strip is the one progress view; the Strip · Line switch
// is gone"): what the two tests of the Strip | Line switch checked is now that there is none, and Strip stays.
test("no List | Cards switch and no Strip | Line switch: Strip on every row, kept across a reload", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator("[data-desk-cards-view], .dkc-sw, .dkc-card")).toHaveCount(0);
  await expect(card(page).locator("[data-desk-c-line], .dkn-lsw")).toHaveCount(0);
  await expect(card(page).locator(".dkn-row .dkn-ks").first()).toBeVisible();
  await expect(card(page).locator(".dkn-row .dkn-tline")).toHaveCount(0);
  await reopen(page);
  await expect(card(page).locator("[data-desk-c-line], .dkn-lsw")).toHaveCount(0);
  await expect(card(page).locator(".dkn-row .dkn-ks").first()).toBeVisible();
  await expect(card(page).locator(".dkn-row .dkn-tline")).toHaveCount(0);
});

test("with no switch, the card's head over the rows reads Item, Progress, Now, and every row has its strip", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(card(page).locator(".dkn-th > span")).toHaveText(["Item", "Progress", "Now"]);
  const rows = card(page).locator(".dkn-row[data-desk-item]");
  const n = await rows.count();
  expect(n).toBeGreaterThan(0);
  await expect(card(page).locator(".dkn-row[data-desk-item] .dkn-pl .dkn-ks")).toHaveCount(n);
});

test("the items with sessions in the card; Up next, Done today and the calls while away beside it, side by side", async ({ page, cue }) => {
  await page.setViewportSize({ width: 2600, height: 1100 });
  await cue.open("desk");
  await openDesk(page);
  // #d1 asks, #d2 is ready to check, #d3 and #d5 are at work; #d4 waits on #d3 and #d6 couldn't be given out; #d7–#d9
  // done today.
  expect((await card(page).locator(".dkn-row[data-desk-item]").evaluateAll((els) => els.map((e) => e.dataset.deskItem))).sort()).toEqual(["d1", "d2", "d3", "d5"]);
  const d5 = card(page).locator('.dkn-row[data-desk-item="d5"]');
  await expect(d5.locator(".dkn-sub")).toContainText("#d5");
  await expect(d5.locator(".dkn-sub .dkn-k")).toHaveAttribute("aria-label", "bug");
  await expect(d5.locator(".dkn-sub")).toContainText("host-lead");
  await expect(d5.locator(".dkn-ks .dkn-kp")).toHaveCount(6);
  await expect(d5.locator(".dkn-ks")).toHaveAttribute("aria-label", /^Step \d of 6, /);
  await expect(box(page, "next").locator(".dkn-dh")).toHaveText(/^▾UP NEXT 2/);
  await expect(box(page, "next").locator('[data-desk-item="d4"] .dkn-a2 > span:last-child')).toHaveText("After #d3");
  await expect(box(page, "next").locator('[data-desk-item="d6"] .dkn-a2 > span:last-child')).toHaveText("Couldn't give it out · No session in this project could take it.");
  await expect(box(page, "done").locator(".dkn-dh")).toHaveText(/^▾DONE TODAY 3$/);
  // Each done one says when it landed, by the app's clock.
  const first = box(page, "done").locator(".dkn-nx").first();
  const id = await first.getAttribute("data-desk-item");
  const at = await page.evaluate((id) => { const i = deskItem("cue", id); return clockAt(deskMs(i.done_at || i.updated)); }, id);
  await expect(first.locator(".dkn-a2 > span:last-child")).toHaveText(new RegExp(`(^|· )landed ${at.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`));
  // No calls while Away is off; on, the calls since it went on.
  await expect(card(page).locator('[data-desk2b-group="away"]')).toHaveCount(0);
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    const since = Math.min(...d.decisions.filter((c) => c.call).map((c) => c.at)) * 1000 - 1000;
    d.away = { on: true, active: true, ended: "", since, until: since + 9 * 3600000, budget: 10, spent: 1 };
  });
  const n = await page.evaluate(() => deskAwayCalls(deskOf("cue")).length);
  expect(n).toBeGreaterThan(0);
  // Calls while away is a band of its own (desk-live, ac3d10347c), its heading folding it.
  const fold = card(page).locator('.dkn-gh[data-desk2b-group="away"]');
  await expect(fold).toHaveText(new RegExp(`^CALLS WHILE AWAY ${n}`));
  if ((await fold.getAttribute("aria-expanded")) !== "true") await fold.click();
  await expect(card(page)).toContainText(`${n} call${n === 1 ? "" : "s"} while away`);
  await expect(card(page)).toContainText(/since \d\d:\d\d/);
  // Away has no end time (#790): the line says what it spent of its budget, and an end time saved earlier isn't shown.
  await expect(card(page).locator(".dk-aw-fold .dk-aw-w:not(.dk-aw-since)")).toHaveText("$1 of $10");
  await expect(card(page)).not.toContainText(/till \d\d:\d\d/);
  // Up next and Done today side by side, under the card.
  const [c, nx, dn] = await Promise.all([card(page).boundingBox(), box(page, "next").boundingBox(), box(page, "done").boundingBox()]);
  expect(nx.y).toBeGreaterThan(c.y + c.height - 1);
  expect(dn.x).toBeGreaterThan(nx.x + nx.width - 1);
  expect(Math.abs(dn.y - nx.y)).toBeLessThan(2);
});

test("a row opens its item, lit in place, and back goes to the lead's chat", async ({ page, cue }) => {
  await page.setViewportSize({ width: 2600, height: 1100 });
  await cue.open("desk");
  await openDesk(page);
  const row = card(page).locator('.dkn-row[data-desk-item="d3"]');
  const before = await row.boundingBox();
  await row.locator(".dkn-tt").click();
  const sp = page.locator(".dk-c > .dk-right .dkn-sp");
  await expect(sp.locator(".dkn-smeta")).toContainText("#d3");
  // In the middle, left of Items, which stays where it is; the row lit in the same place.
  expect((await sp.boundingBox()).x).toBeLessThan((await items(page).boundingBox()).x);
  await expect(row).toHaveClass(/\bon\b/);
  expect(await row.boundingBox()).toEqual(before);
  // A row in Up next opens its item too.
  await box(page, "next").locator('[data-desk-item="d4"]').click();
  await expect(page.locator(".dk-c > .dk-right .dkn-sp .dkn-smeta")).toContainText("#d4");
  await expect(box(page, "next").locator('[data-desk-item="d4"]')).toHaveClass(/\bon\b/);
  await expect(row).not.toHaveClass(/\bon\b/);
  await page.locator(".dk-c > .dk-right .dkn-sback .dk-back").click();
  await expect(page.locator(".dk-c > .dk-right .dkn-sp")).toHaveCount(0);
  await expect(page.locator(".dk-c > .dk2b-chat")).toBeVisible();
  await expect(items(page).locator(".dkn-row.on, .dkn-nx.on")).toHaveCount(0);
});

test("narrow, Up next and Done today stack into one column", async ({ page, cue }) => {
  await page.setViewportSize({ width: 900, height: 600 });
  await cue.open("desk");
  await openDesk(page);
  expect(await items(page).evaluate((el) => el.getBoundingClientRect().width)).toBeLessThanOrEqual(460);
  const [n, d] = await Promise.all([box(page, "next").boundingBox(), box(page, "done").boundingBox()]);
  expect(d.y).toBeGreaterThan(n.y + n.height - 1);
  expect(Math.abs(d.x - n.x)).toBeLessThan(2);
  expect(Math.abs(d.width - n.width)).toBeLessThan(2);
});
