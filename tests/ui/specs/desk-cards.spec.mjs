// The Items card's Cards view (desk/cards.js): a List / Cards switch in the Items head (List by default, the pick kept
// across a reload); With sessions as filled cards at the left, Up next, Done today and the calls while away as dashed
// boxes at the right; a card opens its item on the left as a List row does, ‹ Items going back; one column when narrow.
import { test, expect, openDesk, pick } from "./cue.mjs";

const items = (page) => page.locator(".dk-card.dk-items");
const sw = (page, k) => items(page).locator(`[data-desk-cards-view="${k}"]`);
const cards = (page) => items(page).locator(".dkc-card");
const box = (page, k) => items(page).locator(`[data-desk-cards-box="${k}"]`);
/** The Desk shown again after a reload, on whichever view its Items card was left in. */
const reopen = async (page) => {
  await page.reload();
  await page.locator('#app [data-view="board"]').waitFor();
  await pick(page, "cue");
  await page.locator('.switch-view [data-view="desk"]').click();
  await items(page).waitFor();
};

test("the switch shows List first and keeps Cards across a reload", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(sw(page, "list")).toHaveAttribute("aria-pressed", "true");
  await expect(items(page).locator(".dk2b-row").first()).toBeVisible();
  await expect(cards(page)).toHaveCount(0);
  await sw(page, "cards").click();
  await expect(sw(page, "cards")).toHaveAttribute("aria-pressed", "true");
  await expect(cards(page).first()).toBeVisible();
  await expect(items(page).locator(".dk2b-row, .dk2b-tabs")).toHaveCount(0);
  await reopen(page);
  await expect(sw(page, "cards")).toHaveAttribute("aria-pressed", "true");
  await expect(cards(page).first()).toBeVisible();
  // And back to List, kept too.
  await sw(page, "list").click();
  await reopen(page);
  await expect(sw(page, "list")).toHaveAttribute("aria-pressed", "true");
  await expect(cards(page)).toHaveCount(0);
});

test("Cards: the items with sessions at the left, Up next, Done today and the calls at the right, side by side", async ({ page, cue }) => {
  await page.setViewportSize({ width: 2600, height: 1100 });
  await cue.open("desk");
  await openDesk(page);
  await sw(page, "cards").click();
  // #d1 asks, #d2 is ready to check, #d3 and #d5 are at work; #d4 waits on #d3 and #d6 is new; #d7–#d9 done today.
  await expect(items(page).locator(".dkc-cards > .dkc-gh")).toHaveText(/^WITH SESSIONS · 4\s*NEWEST STEP FIRST$/);
  await expect(cards(page)).toHaveCount(4);
  expect((await cards(page).evaluateAll((els) => els.map((e) => e.dataset.deskItem))).sort()).toEqual(["d1", "d2", "d3", "d5"]);
  const d5 = items(page).locator('.dkc-card[data-desk-item="d5"]');
  await expect(d5.locator(".dkc-h")).toContainText("#d5");
  await expect(d5.locator(".dkc-k")).toHaveText("Bug");
  await expect(d5.locator(".dkc-who")).toContainText("host-lead");
  await expect(d5.locator(".dk2b-trk i")).toHaveCount(6);
  await expect(d5.locator(".dkc-st")).toContainText(/step \d of 6 · /);
  await expect(d5.locator(".dkc-run")).toHaveText(/^(Running: .+|Thinking…|Compacting…) · /);
  await expect(box(page, "next").locator(".dkc-bh")).toHaveText(/^Up next\s*2$/);
  await expect(box(page, "next").locator('[data-desk-item="d4"] .dkc-rr')).toHaveText("after #d3");
  await expect(box(page, "next").locator('[data-desk-item="d6"] .dkc-rr')).toHaveText("new");
  await expect(box(page, "done").locator(".dkc-bh")).toHaveText(/^Done today\s*3$/);
  await expect(box(page, "done").locator(".dkc-rr").first()).toHaveText(/^landed \d\d:\d\d$/);
  // No calls box while Away is off; on, the calls since it went on with Undo.
  await expect(box(page, "calls")).toHaveCount(0);
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    const since = Math.min(...d.decisions.filter((c) => c.call).map((c) => c.at)) * 1000 - 1000;
    d.away = { on: true, active: true, ended: "", since, until: since + 9 * 3600000, budget: 10, spent: 1 };
  });
  const n = await page.evaluate(() => deskAwayCalls(deskOf("cue")).length);
  expect(n).toBeGreaterThan(0);
  await expect(box(page, "calls").locator(".dkc-bh b")).toHaveText(`${n} call${n === 1 ? "" : "s"} while away`);
  await expect(box(page, "calls").locator(".dkc-bh")).toContainText(/since \d\d:\d\d · till \d\d:\d\d/);
  await expect(box(page, "calls").locator(".dkc-call")).toHaveCount(Math.min(n, 5));
  // Two columns, side by side.
  const [l, r] = await Promise.all([items(page).locator(".dkc-cards").boundingBox(), items(page).locator(".dkc-side").boundingBox()]);
  expect(r.x).toBeGreaterThan(l.x + l.width - 1);
  expect(Math.abs(r.y - l.y)).toBeLessThan(2);
});

test("a card opens its item on the left, lit in place, and ‹ Items goes back", async ({ page, cue }) => {
  await page.setViewportSize({ width: 2600, height: 1100 });
  await cue.open("desk");
  await openDesk(page);
  await sw(page, "cards").click();
  const card = items(page).locator('.dkc-card[data-desk-item="d3"]');
  const before = await card.boundingBox();
  await card.click();
  const pg = page.locator(".dk2b-ipage");
  await expect(pg.locator(".dk2b-in")).toHaveText("#d3");
  // On the left, the Items card staying where it is, the card lit in the same place.
  expect((await pg.boundingBox()).x).toBeLessThan((await items(page).boundingBox()).x);
  await expect(card).toHaveClass(/\bon\b/);
  expect(await card.boundingBox()).toEqual(before);
  // A row in a box opens its item too.
  await box(page, "next").locator('[data-desk-item="d4"]').click();
  await expect(pg.locator(".dk2b-in")).toHaveText("#d4");
  await expect(box(page, "next").locator('[data-desk-item="d4"]')).toHaveClass(/\bon\b/);
  await expect(card).not.toHaveClass(/\bon\b/);
  await page.locator(".dk-back.dk2b-items").click();
  await expect(page.locator(".dk2b-ipage")).toHaveCount(0);
  await expect(page.locator(".dk-card.dk2b-chat")).toBeVisible();
  await expect(items(page).locator(".dkc-card.on, .dkc-row.on")).toHaveCount(0);
});

test("narrow, Cards stacks into one column: the cards first, then the boxes", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await cue.open("desk");
  await openDesk(page);
  await sw(page, "cards").click();
  expect((await items(page).boundingBox()).width).toBeLessThan(1000);
  const [l, r] = await Promise.all([items(page).locator(".dkc-cards").boundingBox(), items(page).locator(".dkc-side").boundingBox()]);
  expect(r.y).toBeGreaterThan(l.y + l.height - 1);
  expect(Math.abs(r.x - l.x)).toBeLessThan(2);
  expect(Math.abs(r.width - l.width)).toBeLessThan(2);
});
