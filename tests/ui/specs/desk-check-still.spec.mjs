// #791: checking an item moves nothing but that item. Opened to check, Looks right and Send back change only the
// item's own row and bar: the other rows in Items, Done today, and the chat under the bar stay where they are; after
// Looks right's moment its row folds shut and the rows under it slide up, never jump.
import { test, expect, openDesk } from "./cue.mjs";

/** Where everything round item `id` sits: every other row in Items and Done today, the item's bar's height, and the
 *  chat under it (its top edge and its last message). */
const around = (page, id) => page.evaluate((id) => {
  const top = (el) => Math.round(el.getBoundingClientRect().top);
  const rows = [...document.querySelectorAll(".dkn-items .dk-it[data-desk-item], .dkn-items .dkn-nx[data-desk-item]")].filter((el) => el.dataset.deskItem !== id).map((el) => `${el.dataset.deskItem}@${top(el)}`);
  const bar = document.querySelector(".dk-right .dkn-sp .dkn-ib"), chat = document.querySelector(".dk-right .dkn-sp .dk-chat");
  return { rows, bar: bar && Math.round(bar.getBoundingClientRect().height), chat: chat && top(chat), last: chat?.lastElementChild && top(chat.lastElementChild) };
}, id);

/** The page's clock stopped where it is, so the 1.2 s run only as runFor moves them (a slow run's own seconds don't). */
const pause = async (page) => page.clock.pauseAt(await page.evaluate(() => Date.now() + 500));
/** The tests' window has animations off (load.js ?still=1): back on, to see the fold as you would. */
const motionOn = (page) => page.evaluate(() => { for (const st of document.querySelectorAll("style")) if (st.textContent.includes("animation:none!important")) st.remove(); });
/** Rows' tops and heights with their margins about every 25 ms for `ms`, as the fold runs. */
const frames = async (page, ms) => {
  const out = [];
  for (const end = Date.now() + ms; Date.now() < end;) {
    out.push(await page.evaluate(() => Object.fromEntries([...document.querySelectorAll(".dkn-items .dkn-row[data-desk-item]")].map((el) => [el.dataset.deskItem, [el.getBoundingClientRect().top, el.getBoundingClientRect().height + parseFloat(getComputedStyle(el).marginBottom)]]))));
    await page.waitForTimeout(25);
  }
  return out;
};

test("Looks right on an opened item: Done · Undo for 1.2 s with nothing round it moving and its bar one height; then its row folds shut, the row under it sliding up with no jump; Undo works till it's shut", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1800, height: 1095 });   // a list with room for all its rows: nothing scrolls
  await cue.open("desk");
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1").state = "to_check"; });
  await openDesk(page);
  await cue.override("desk_looks_right", ({ id }) => {
    const it = CueBridge.world.state.desk.projects[0].items.find((i) => i.id === id);
    Object.assign(it, { state: "done", done_at: Math.floor(Date.now() / 1000) });
    CueBridge.emit("state", CueBridge.world.snapshot());
    return { commit: "", files: 0, waiting: [] };
  });
  await pause(page);
  const row = page.locator('.dkn-items .dkn-row[data-desk-item="d2"]'), under = page.locator('.dkn-items .dkn-row[data-desk-item="d1"]');
  await row.locator(".dkn-tt").click();
  const bar = page.locator(".dk-right .dkn-sp .dkn-ib");
  await expect(bar).toHaveClass(/\bcheck\b/);
  const opened = await around(page, "d2"), h = (await row.boundingBox()).height, below = (await under.boundingBox()).y;
  const tall = below - (await row.boundingBox()).y;   // the row with what's between it and the next

  await bar.locator('[data-desk-act="looks_right"]').click();
  await expect(bar.locator(".dk-soon")).toHaveText("Done · Undo · back to check");
  await page.clock.runFor(1100);
  expect(await around(page, "d2")).toEqual(opened);
  await expect(row).not.toHaveClass(/\bdk-leave\b/);

  // The fold: the row under it goes up by the row's height, a little each frame, never in one jump.
  await motionOn(page);
  await page.clock.runFor(150);
  await expect(row).toHaveClass(/\bdk-leave\b/);
  // It folds as an animation of its height, and the row under it sits right under it all the way: it rises exactly as
  // far as the fold has shut, never further at once.
  expect(await row.evaluate((el) => el.getAnimations().map((a) => a.animationName))).toEqual(["dkn-leave"]);
  const seen = await frames(page, 450);
  for (const f of seen) {
    expect(f.d1[0]).toBeCloseTo(f.d2[0] + f.d2[1], 0);
    expect(below - f.d1[0]).toBeCloseTo(tall - f.d2[1], 0);
  }
  expect(seen.at(-1).d1[0]).toBeCloseTo(below - tall, 0);
  // Undo while it folds would still take it back; left alone, it's sent once the fold is over, and goes.
  expect(await cue.calls("desk_looks_right")).toEqual([]);
  await page.clock.runFor(300);
  await cue.called("desk_looks_right", { id: "d2" });
  await expect(row).toHaveCount(0);
  expect((await under.boundingBox()).y).toBeCloseTo(below - tall, 0);
  // The opened item's bar kept its height as it turned done; how it went is where what to try was.
  await expect(bar.locator(".dkn-ibsee")).toHaveText("Looked right · Undo · back to check");
  expect((await around(page, "d2")).bar).toBe(opened.bar);
});

test("a list scrolled to its end: as the row folds shut the rows over it come down with it, a little each frame, never in a jump", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1").state = "to_check"; });
  await openDesk(page);
  await pause(page);
  const list = page.locator('[data-dkn-list="items"]');
  await list.evaluate((el) => { el.scrollTop = el.scrollHeight; });
  const row = page.locator('.dkn-items .dkn-row[data-desk-item="d2"]'), h = (await row.boundingBox()).height;
  const start0 = await page.evaluate(() => Object.fromEntries([...document.querySelectorAll(".dkn-items .dkn-row[data-desk-item]")].map((el) => [el.dataset.deskItem, el.getBoundingClientRect().top])));
  await row.locator(".dk-ok").click();
  await motionOn(page);
  await page.clock.runFor(1250);
  await expect(row).toHaveClass(/\bdk-leave\b/);
  const tall = start0.d1 - start0.d2;
  const start = await page.evaluate(() => Object.fromEntries([...document.querySelectorAll(".dkn-items .dkn-row[data-desk-item]")].map((el) => [el.dataset.deskItem, el.getBoundingClientRect().top])));
  const seen = await frames(page, 450);
  // Every row moves by no more than the fold has shut so far (never ahead of it, never in a jump), and the row under
  // it sits right under it all the way.
  for (const f of seen) {
    for (const id of ["d3", "d5"]) expect(Math.abs(f[id][0] - start[id]), id).toBeLessThanOrEqual(tall - f.d2[1] + 1);
    expect(f.d1[0]).toBeCloseTo(f.d2[0] + f.d2[1], 0);
  }
  // Shut, then gone once it's sent and done: nothing moves as it leaves.
  const shut = await page.evaluate(() => Object.fromEntries([...document.querySelectorAll(".dkn-items .dkn-row[data-desk-item]")].map((el) => [el.dataset.deskItem, Math.round(el.getBoundingClientRect().top)])));
  await page.clock.runFor(300);
  await cue.called("desk_looks_right", { id: "d2" });
  await expect(row).toHaveCount(0);
  delete shut.d2;
  expect(await page.evaluate(() => Object.fromEntries([...document.querySelectorAll(".dkn-items .dkn-row[data-desk-item]")].map((el) => [el.dataset.deskItem, Math.round(el.getBoundingClientRect().top)])))).toEqual(shut);
});

test("Undo while the row folds shut: it opens again where it was, still to check, nothing sent", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await pause(page);
  const row = page.locator('.dkn-items .dkn-row[data-desk-item="d2"]');
  await row.scrollIntoViewIfNeeded();
  const at = await row.boundingBox();
  await row.locator(".dk-ok").click();
  await page.clock.runFor(1300);
  await expect(row).toHaveClass(/\bdk-leave\b/);
  await row.locator('[data-desk-act="looks_undo"]').click();
  await expect(row).not.toHaveClass(/\b(dk-leave|soon)\b/);
  expect(await row.boundingBox()).toEqual(at);
  await page.clock.runFor(2000);
  expect(await cue.calls("desk_looks_right")).toEqual([]);
});

test("with reduced motion the row goes at once after its 1.2 s, no fold", async ({ page, cue }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await cue.open("desk");
  await openDesk(page);
  await pause(page);
  const row = page.locator('.dkn-items .dkn-row[data-desk-item="d2"]');
  await row.locator(".dk-ok").click();
  await page.clock.runFor(1300);
  await expect(row).toBeHidden();
  await cue.called("desk_looks_right", { id: "d2" });
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
