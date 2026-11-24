// #791: checking an item moves nothing but that item. Opened to check, Looks right and Send back change only the
// item's own row and bar: the other rows in Items, Done today, and the chat under the bar stay where they are.
import { test, expect, openDesk } from "./cue.mjs";

/** Where everything round item `id` sits: every other row in Items and Done today, the item's bar's height, and the
 *  chat under it (its top edge and its last message). */
const around = (page, id) => page.evaluate((id) => {
  const top = (el) => Math.round(el.getBoundingClientRect().top);
  const rows = [...document.querySelectorAll(".dkn-items .dk-it[data-desk-item], .dkn-items .dkn-nx[data-desk-item]")].filter((el) => el.dataset.deskItem !== id).map((el) => `${el.dataset.deskItem}@${top(el)}`);
  const bar = document.querySelector(".dk-right .dkn-sp .dkn-ib"), chat = document.querySelector(".dk-right .dkn-sp .dk-chat");
  return { rows, bar: bar && Math.round(bar.getBoundingClientRect().height), chat: chat && top(chat), last: chat?.lastElementChild && top(chat.lastElementChild) };
}, id);

test("Looks right on an opened item: its row turns done where it is, its bar keeps its height, nothing round it moves; it goes to Done today when you open something else", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.override("desk_looks_right", ({ id }) => {
    const it = CueBridge.world.state.desk.projects[0].items.find((i) => i.id === id);
    Object.assign(it, { state: "done", done_at: Math.floor(Date.now() / 1000) });
    CueBridge.emit("state", CueBridge.world.snapshot());
    return { commit: "", files: 0, waiting: [] };
  });
  // d2 is the last row in Items: scroll to it, as you would, then open it.
  await page.locator('[data-dkn-list="items"]').evaluate((el) => { el.scrollTop = el.scrollHeight; });
  const row = page.locator('.dkn-items .dkn-row[data-desk-item="d2"]');
  await row.locator(".dkn-tt").click();
  const bar = page.locator(".dk-right .dkn-sp .dkn-ib");
  await expect(bar).toHaveClass(/\bcheck\b/);
  const opened = await around(page, "d2"), at = await row.boundingBox();

  await bar.locator('[data-desk-act="looks_right"]').click();
  await expect(bar.locator(".dk-soon")).toHaveText("Done · Undo · back to check");
  expect(await around(page, "d2")).toEqual(opened);
  // Sent after its moment: Looks right and Send back don't come back meanwhile, and once done its row is still where
  // it was, ticked, and the bar shows how it went in the line where what to try was.
  await page.clock.runFor(3100);
  await cue.called("desk_looks_right", { id: "d2" });
  await expect(bar).not.toHaveClass(/\bcheck\b/);
  await expect(bar.locator('[data-desk-act="looks_right"]')).toHaveCount(0);
  await expect(bar.locator(".dkn-ibsee")).toHaveText("Looked right · Undo · back to check");
  await expect(row.locator('.dk-soon [data-desk-act="reopen"]')).toHaveText("Undo");
  expect(await row.boundingBox()).toEqual(at);
  expect(await around(page, "d2")).toEqual(opened);
  await expect(page.locator('[data-dkn-drawer="done"] [data-desk-item="d2"]')).toHaveCount(0);

  // Closing it lets it go: out of Items, into Done today.
  await page.evaluate(() => deskShow(null));
  await expect(row).toHaveCount(0);
  await expect(page.locator('[data-dkn-drawer="done"] [data-desk-item="d2"]')).toHaveCount(1);
});

test("Send back on an opened item moves no row in Items or Done today", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dkn-items .dkn-row[data-desk-item="d2"] .dkn-tt').click();
  const bar = page.locator(".dk-right .dkn-sp .dkn-ib");
  await expect(bar).toHaveClass(/\bcheck\b/);
  const { rows } = await around(page, "d2");
  await bar.locator('[data-desk-act="send_back"]').click();
  await expect(page.locator(".dk-right .dkn-ibback textarea")).toHaveCount(1);
  expect((await around(page, "d2")).rows).toEqual(rows);
  await page.locator(".dk-right .dkn-ibback textarea").fill("The arrows still skip one");
  await page.locator('.dk-board [data-desk-act="back_go"]').click();
  await cue.called("desk_item_act", { id: "d2", act: "send_back", text: "The arrows still skip one" });
  expect((await around(page, "d2")).rows).toEqual(rows);
});
