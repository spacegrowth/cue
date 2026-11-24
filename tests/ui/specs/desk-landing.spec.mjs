// How work lands (docs/desk-landing-design.md): each item works in its own copy of its repository (a pool of three),
// and lands through one landing line, checked first. Its row says where it is: "waiting for a free working folder",
// "In the landing line · waits: … · since 8:31pm", "Checking before it lands · since …", "Check failed · since …".
// A dropped item's copy that had work in it is listed under Kept copies, with a Remove that takes two clicks.
import { test, expect, openDesk } from "./cue.mjs";
import { statusOf, openMore, listOrder } from "./desk.mjs";

const patchDesk = (cue, patch) => cue.change((st, p) => { Object.assign(st.desk.projects.find((x) => x.project === "cue"), p); }, patch);
const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
// An item opened (#744, #748): its session's chat with the item's bar over it, or (no session) its own page; Items stay
// on screen beside it.
const openItem = async (page, id) => {
  await page.locator(`.dkn-items [data-desk-item="${id}"]`).click();
  await expect(page.locator(`.dkn-items .on[data-desk-item="${id}"]`)).toHaveCount(1);
  return page.locator(".dk-right .dkn-sp");
};
const tree = (id, slot = 1) => ({ repo: "/r/cue", path: `/r/cue-copies/${slot}`, branch: `desk/${id}`, base: "abc1234", slot });

test("the landing line's steps on a finishing item: in the line with what it waits on, checking, check failed (with Send back to fix)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const at = Date.now(), since = await page.evaluate((t) => clockAt(t), at);
  await patchItem(cue, "d5", { state: "finishing", tree: tree("d5"), finish: { step: "line", wait: "The main line kept moving", at } });
  // Its row says its step.
  await expect(statusOf(page, "d5")).toHaveText(`In the landing line · waits: The main line kept moving · since ${since}`);
  await patchItem(cue, "d5", { finish: { step: "line", at } });
  await expect(statusOf(page, "d5")).toHaveText(`In the landing line · since ${since}`);
  await patchItem(cue, "d5", { finish: { step: "checking", at } });
  await expect(statusOf(page, "d5")).toHaveText(`Checking before it lands · since ${since}`);
  const pane = await openItem(page, "d5");
  // No commit wait any more: nothing to ask of another session, nothing to land without.
  await expect(pane.locator('[data-desk-act="ask_commit"], [data-desk-act="land_without"]')).toHaveCount(0);
  await patchItem(cue, "d5", { finish: { step: "check_failed", at, question: "Its check failed", log: "test x failed" } });
  const box = pane.locator(".dkn-ib .dk-wo");
  await expect(box).toContainText("Its check failed");
  await expect(box.locator('[data-desk-act="fix_back"]')).toHaveCount(1);
  await expect(box.locator('[data-desk-act="log_all"]')).toHaveText("Show the output");
  // Main may have been what broke it: run again on main as it is now (#343).
  await expect(box.locator('[data-desk-act="rerun"]')).toHaveText("Run the landing again");
  // Its row in the Items list says so too.
  await expect(statusOf(page, "d5")).toHaveText(`Check failed · since ${since}`);
});

test("Send back is only on an item to check: not on an ask, nor on a stopped landing, where Send back to fix stays (#371, #435)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Its holder asked you: you answer it; no Send back beside it.
  let pane = await openItem(page, "d1");
  await expect(pane.locator('[data-desk-act="send_back"]')).toHaveCount(0);
  // Ready to check: Send back, your words to its holder.
  pane = await openItem(page, "d2");
  await pane.locator('.dkn-ib [data-desk-act="send_back"]').click();
  await expect(pane.locator('textarea[data-text="back:d2"]')).toBeVisible();
  await pane.locator('textarea[data-text="back:d2"]').fill("Use the box's own focus step");
  await pane.locator('[data-desk-act="back_go"][data-id="d2"]').click();
  await cue.called("desk_item_act", { id: "d2", act: "send_back", text: "Use the box's own focus step" });
  // A landing stopped on something Cue can't name: Run the landing again, no Send back.
  await patchItem(cue, "d5", { state: "needs_you", tree: tree("d5"), finish: { step: "error", at: Date.now(), question: "git couldn't make its commit" } });
  pane = await openItem(page, "d5");
  const box = pane.locator(".dkn-ib .dk-wo");
  await expect(box.locator('[data-desk-act="rerun"]')).toHaveText("Run the landing again");
  await expect(box.locator('[data-desk-act="send_back"]')).toHaveCount(0);
  // A word that mustn't go in, a check failed: Send back to fix.
  await patchItem(cue, "d5", { finish: { step: "word", at: Date.now(), question: "Not landed: a word is in it" } });
  await expect(box.locator('[data-desk-act="fix_back"]')).toHaveText("Send back to fix");
});

test("one queued for a copy waits for a free one, and Start now starts it at once", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Queued for a working folder, all three in use (#401): its row says so plainly, naming who uses them.
  await patchItem(cue, "d4", { state: "queued", after: "", tree_wait: true, copy_users: ["d3", "d5", "d7"] });
  const row = page.locator('.dkn-items [data-desk-item="d4"]');
  await expect(statusOf(page, "d4")).toHaveText("Waiting for a free working folder on This Mac (in use by #d3, #d5, #d7)");
  // Sent back while none is free: it says so; Start now starts it at once, and the line goes.
  await patchItem(cue, "d4", { back_waiting: "Desk item #d4 isn't done: the dot is too big" });
  await expect(statusOf(page, "d4")).toHaveText("Sent back · waiting for a free working folder on This Mac (in use by #d3, #d5, #d7)");
  // Start now is in a ⋯ menu, not on show on its row: the row's only button is its own ⋯ (#839).
  await expect(row.locator("button")).toHaveCount(1);
  await expect(row.locator("button.dkn-rmore")).toHaveCount(1);
  await expect(row.locator('[data-desk-act="start_now"]')).toHaveCount(0);
  const pane = await openItem(page, "d4");
  // Its bar says what it waits for, with Start now, and the ⋯ menu has Start now too.
  const card = pane.locator(".dkn-ib .dk-ns");
  await expect(card).toContainText("Sent back. Waiting for one of 3 items on This Mac to finish.");
  await expect(card).toContainText("It starts by itself when one is free.");
  await expect(card.locator('[data-desk-act="start_now"]')).toHaveText("Start now");
  await openMore(pane.locator(".dkn-ib"));
  await expect(pane.locator('.dkn-ib [role="menuitem"][data-desk-act="start_now"]')).toHaveText("Start now");
  await pane.locator('.dkn-ib [role="menuitem"][data-desk-act="start_now"]').click();
  await cue.called("desk_item_act", { id: "d4", act: "start_now" });
  await expect(statusOf(page, "d4")).not.toContainText("working folder");
  await expect(pane).not.toContainText("free working folder");
});

test("Kept copies lists a dropped item's copy with its work; Remove takes two clicks, and anything else disarms it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await patchDesk(cue, { kept: [{ id: "d9", title: "Old idea, dropped", project: "cue", repo: "/r/cue", path: "/r/cue-kept/d9", branch: "desk/d9", at: Date.now() - 120000 }] });
  const list = page.locator(".dkn-items");
  // A band of its own, its heading with its plain line (desk-live, ac3d10347c).
  await expect(list.locator('.dkn-gh[data-desk2b-group="kept"]')).toHaveText(/^KEPT COPIES 1\s*copies of dropped work$/);
  const kept = list.locator('.dk-kept[data-kept="d9"]');
  await expect(kept).toContainText("#d9 Old idea, dropped");
  await expect(kept).toContainText("/r/cue-kept/d9 · desk/d9 · dropped");
  // Above Done.
  const heads = await listOrder(page);
  expect(heads.indexOf("# KEPT COPIES")).toBeGreaterThan(-1);
  expect(heads.indexOf("# KEPT COPIES")).toBeLessThan(heads.indexOf("# DONE TODAY"));
  await cue.override("desk_item_act", () => null);
  const remove = kept.locator('[data-desk-act="remove_kept"]');
  await expect(remove).toHaveText("Remove");
  // First click arms it; a click elsewhere disarms.
  await remove.click();
  await expect(remove).toHaveText("Remove it and its work");
  await list.locator(".dkn-ih b").click();
  await expect(remove).toHaveText("Remove");
  // Two clicks remove it.
  await remove.click();
  await expect(remove).toHaveText("Remove it and its work");
  await remove.click();
  await cue.called("desk_item_act", { id: "d9", act: "remove_kept" });
  // No colored left edge on the row.
  await expect(kept).toHaveCSS("border-left-width", "0px");
});

test("a landing stopped on leftover edits already on its branch says so and offers Clear and land, one OK (#659)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const row = page.locator('.dkn-items [data-desk-item="d5"]');
  await patchItem(cue, "d5", { state: "finishing", stopped: true, tree: tree("d5"), finish: { step: "conflict", at: Date.now(), lead: true, question: "Not merged: the main checkout has edits of its own in a.txt, ui/b.js, c.txt that aren't committed", clear: ["a.txt", "ui/b.js"], clear_keep: [["c.txt", "tidier"]], clear_in: "/r/cue", clear_sha: "abc1234" } });
  // Its row: what stopped it in plain words; the button is in its bar.
  await expect(statusOf(page, "d5")).toHaveText("Stopped · leftover edits in the main folder");
  await expect(row.locator('[data-desk-act]')).toHaveCount(0);
  const pane = await openItem(page, "d5");
  const box = pane.locator(".dkn-ib .dk-wo");
  await expect(box.locator(".dk-wo-t")).toHaveText("Leftover edits in the main folder are in its way. a.txt and ui/b.js already have this item's changes.");
  await expect(box).toContainText("c.txt also has tidier's edits: left alone.");
  await expect(box).toContainText("Clear and land saves each file's diff to work/, sets them back to main, and lands it.");
  const btn = box.locator('[data-desk-act="clear_land"]');
  await expect(btn).toHaveText("Clear and land");
  await expect(btn).toHaveClass(/primary/);
  // No hover text on it.
  await expect(btn).not.toHaveAttribute("title", /./);
  if (process.env.SHOT_DIR) for (const c of ["light", "dark"]) { await page.emulateMedia({ colorScheme: c }); await page.waitForTimeout(150); await page.screenshot({ clip: await box.boundingBox(), path: `${process.env.SHOT_DIR}/cue-clear-land-${c}.png` }); }
  await btn.click();
  await cue.called("desk_item_act", { id: "d5", act: "clear_land" });
});
