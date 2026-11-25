// What the "new Desk page" (#628) tested, on the one layout left since #748 (desk/c.js): it's the Desk with no switch
// to turn it on or off, an old switch's key changing nothing; the Desks from the header's menu, with Desk settings as
// a dialog of its sections; the lead and the Desk's sessions in the column at the left, each opening its chat with a
// way back to the lead's; progress as Strip or Line; the groups in their order, landing ones in Working before Up
// next; an item opened leads with its steps, the one it's at, who is on it and its buttons, and goes back to the lead.
import { test, expect, openDesk } from "./cue.mjs";
import { listOrder } from "./desk.mjs";

const open = async (page, cue) => {
  await page.setViewportSize({ width: 1800, height: 1095 });
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-board.dk-c")).toHaveCount(1);
};

test("The Desk opens on its one layout: the sessions, the lead's chat, Items; no other page", async ({ page, cue }) => {
  await open(page, cue);
  await expect(page.locator(".dk-board.dk-v2, .dk-board.dk-b2:not(.dk-c)")).toHaveCount(0);
  const [s, c, l] = await Promise.all([".dk-c > .dkn-side", ".dk-c > .dk2b-chat", ".dk-c > .dkn-items"].map((q) => page.locator(q).boundingBox()));
  expect(c.x).toBeGreaterThan(s.x + s.width - 1);
  expect(l.x).toBeGreaterThan(c.x + c.width - 1);
  await expect(page.locator(".dk-c > .dk2b-chat [data-desk-page=\"dispatcher\"], .dk-c > .dk2b-chat .dk-chat").first()).toBeVisible();
  // No pills of Desks, drawer or List | Progress of the old page.
  await expect(page.locator(".dk2-head, [data-desk2-set], [data-desk2-chats], [data-desk2-mode]")).toHaveCount(0);
});

test("No switch in Desk settings, and the old switch's key changes nothing across a reload", async ({ page, cue }) => {
  await open(page, cue);
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator(".top .hv-menu [data-hv-desk-set]").click();
  const dlg = page.locator(".ld-dialog");
  await expect(dlg).toBeVisible();
  await expect(dlg.locator('[data-desk2-try], [data-ld-sec="try"], .dk2-sw')).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(dlg).toHaveCount(0);
  for (const v of ["1", "0"]) {
    await page.evaluate((v) => localStorage.setItem("cue.deskV2", v), v);
    await page.reload();
    await page.locator('#app [data-view="board"]').waitFor();
    await openDesk(page);
    await expect(page.locator(".dk-board.dk-c")).toHaveCount(1);
    await expect(page.locator(".dk-board.dk-v2")).toHaveCount(0);
  }
});

test("The Desks from the header, Desk settings' sections, the sessions' chats, Strip or Line, the groups' order", async ({ page, cue }) => {
  await open(page, cue);
  // A row per Desk in the header's Desks menu, the open one lit (no tick, #885), and New Desk….
  await page.locator('.top [data-hv-menu="desks"]').click();
  const menu = page.locator(".top .hv-menu");
  await expect(menu.locator(".hv-row[data-hv-desk]")).toHaveCount(2);
  await expect(menu.locator('.hv-row.on[data-hv-desk="cue"]')).toHaveAttribute("aria-current", "true");
  await expect(menu.locator(".hv-tick")).toHaveCount(0);
  await expect(menu.locator("[data-hv-new-desk]")).toHaveText("New Desk…");
  // Desk settings: a dialog with its sections; Esc closes it.
  await menu.locator("[data-hv-desk-set]").click();
  const dlg = page.locator(".ld-dialog");
  for (const s of ["lands", "old"]) await expect(dlg.locator(`[data-ld-sec="${s}"]`)).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(dlg).toHaveCount(0);
  // The groups: Working, Waiting on you (desk-live, 670449c234: Working above Waiting on you), then Up next and Done
  // today, in that order.
  const heads = (await listOrder(page)).filter((x) => x.startsWith("# "));
  expect(heads).toEqual(["# WORKING", "# WAITING ON YOU", "# UP NEXT", "# DONE TODAY"]);
  // The sessions at the left: the lead and the Desk's session; one opens its chat, its back goes to the lead's.
  const side = page.locator(".dkn-side");
  await expect(side.locator(".dkn-lead")).toContainText("web-lead");
  const host = side.locator('[data-desk-chat-of="host"]');
  await host.click();
  await expect(page.locator('.dk-c > .dk-right [data-desk-page="session"] .ap-head')).toContainText("host-lead");
  await side.locator(".dkn-lead").click();
  await expect(page.locator(".dk-c > .dk2b-chat")).toHaveCount(1);
  await expect(page.locator('.dk-c [data-desk-page="session"]')).toHaveCount(0);
  // Progress as the strip, the one view (desk-live, 5ba37d4889: the Strip · Line switch is gone).
  const items = page.locator(".dkn-card");
  await expect(items.locator("[data-desk-c-line], .dkn-lsw")).toHaveCount(0);
  await expect(items.locator(".dkn-row .dkn-ks").first()).toBeVisible();
  await expect(items.locator(".dkn-row .dkn-tline")).toHaveCount(0);
  // Another Desk from the header's menu.
  await page.locator('.top [data-hv-menu="desks"]').click();
  await menu.locator('.hv-row[data-hv-desk="website"]').click();
  await page.locator('.top [data-hv-menu="desks"]').click();
  await expect(menu.locator('.hv-row.on[data-hv-desk="website"]')).toHaveCount(1);
});

test("Landing items sit in Working with the ones at work, before Up next", async ({ page, cue }) => {
  await open(page, cue);
  const at = await page.evaluate(() => now());
  await cue.change((st, t) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "finishing", finish: { step: "line", at: t } }); }, at);
  expect(await listOrder(page)).toEqual(["# WORKING", "d5", "d3", "# WAITING ON YOU", "d1", "d2", "# UP NEXT", "d4", "d6", "# DONE TODAY", "d7", "d8", "d9"]);
});

test("An item opened leads with its steps, the one it's at, who is on it and its buttons; back goes to the lead", async ({ page, cue }) => {
  await open(page, cue);
  const now = Date.now();
  await cue.change((st, now) => {
    const d5 = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5");
    d5.steps = [now - 600000, now - 480000, now - 60000, 0, 0, 0];
    d5.act = { now: "Running tests", was: "", at_ms: now - 30000 };
  }, now);
  await page.locator('.dkn-card .dkn-row[data-desk-item="d5"] .dkn-tt').click();
  const sp = page.locator('.dk-c > .dk-right .dkn-sp[data-desk-page="session"]');
  // Who is on it: its session's own top bar, and the item it's on under it.
  await expect(sp.locator(".ap-head")).toContainText("host-lead");
  await expect(sp.locator(".dkn-smeta")).toContainText("#d5");
  // Its steps, two done and Tests the one it's at.
  const bar = sp.locator(".dkn-ib");
  await expect(bar.locator(".dkn-kp")).toHaveCount(6);
  await expect(bar.locator(".dkn-kp.done")).toHaveCount(2);
  await expect(bar.locator(".dkn-kp.cur")).toHaveCount(1);
  await expect(bar.locator(".dkn-kpop b")).toHaveText(["Picked up", "Building", "Tests", "Commit", "Landing", "To check"]);
  await expect(bar.locator(".dkn-ks")).toHaveAttribute("aria-label", "Step 3 of 6, Tests");
  // Its buttons beside the steps; the old tabs and ⋯ of Record, Decisions and Files are gone.
  await expect(bar.locator(".dkn-ibacts")).toHaveCount(1);
  await expect(page.locator(".dk-right .dk-itabs, .dk-right .dk2-top, [data-desk-itab]")).toHaveCount(0);
  // Back goes to the lead's chat.
  await sp.locator(".dkn-sback .dk-back").click();
  await expect(page.locator(".dk-c > .dk2b-chat")).toHaveCount(1);
  await expect(page.locator('.dkn-card .dkn-row.on[data-desk-item="d5"]')).toHaveCount(0);
});
