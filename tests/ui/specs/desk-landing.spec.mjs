// How work lands (docs/desk-landing-design.md): each item works in its own copy of its repository (a pool of three),
// and lands through one landing line, checked first. Its row and page say where it is: "waiting for a free working folder",
// "in its own copy desk/<id>", "In the landing line · waits: … · since 8:31pm", "Checking before it lands · since …", "Check failed · since …", "landed, not checked".
// A dropped item's copy that had work in it is listed under Kept copies, with a Remove that takes two clicks.
import { test, expect, openDesk } from "./cue.mjs";
import { statusOf, openMore } from "./desk.mjs";

const patchDesk = (cue, patch) => cue.change((st, p) => { Object.assign(st.desk.projects.find((x) => x.project === "cue"), p); }, patch);
const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
// An item's page (#418): its holder's chat in the middle, the item itself in the column at the right, in the Items
// list's place, so another item opens from the list after "‹ Items".
const backToItems = async (page) => {
  if (await page.locator(".dk-panel[data-side-for]").count()) await page.locator(".dk-right .dk-back").click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
const openItem = async (page, id) => {
  await backToItems(page);
  await page.locator(`.dk-items [data-desk-item="${id}"]`).click();
  await page.locator(`.dk-panel[data-side-for="${id}"]`).waitFor();
  return page.locator(".dk-board");
};
const tree = (id, slot = 1) => ({ repo: "/r/cue", path: `/r/cue-copies/${slot}`, branch: `desk/${id}`, base: "abc1234", slot });

test("the landing line's steps on a finishing item: in the line with what it waits on, checking, check failed (with Send back to fix)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const row = page.locator('.dk-items [data-desk-item="d5"]');
  const at = Date.now(), since = await page.evaluate((t) => clockAt(t), at);
  await patchItem(cue, "d5", { state: "finishing", tree: tree("d5"), finish: { step: "line", wait: "The main line kept moving", at } });
  // Its row's one line says its step (B2, #666); its page has the step in full.
  await expect(statusOf(page, "d5")).toHaveText(`In the landing line · waits: The main line kept moving · since ${since}`);
  await patchItem(cue, "d5", { finish: { step: "line", at } });
  await expect(statusOf(page, "d5")).toHaveText(`In the landing line · since ${since}`);
  await patchItem(cue, "d5", { finish: { step: "checking", at } });
  await expect(statusOf(page, "d5")).toHaveText(`Checking before it lands · since ${since}`);
  const pane = await openItem(page, "d5");
  await expect(pane).toContainText(`Checking before it lands · since ${since}`);
  // No commit wait any more: nothing to ask of another session, nothing to land without.
  await expect(pane.locator('[data-desk-act="ask_commit"], [data-desk-act="land_without"]')).toHaveCount(0);
  await patchItem(cue, "d5", { finish: { step: "check_failed", at, question: "Its check failed", log: "test x failed" } });
  const box = pane.locator(".dk-wo");
  await expect(box).toContainText("Its check failed");
  await expect(box.locator('[data-desk-act="fix_back"]')).toHaveCount(1);
  await expect(box.locator('[data-desk-act="log_all"]')).toHaveText("Show the output");
  // Main may have been what broke it: run again on main as it is now (#343).
  await expect(box.locator('[data-desk-act="rerun"]')).toHaveText("Run the landing again");
  // Its row in the Items list says so too.
  await backToItems(page);
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
  await pane.locator('.dk-panel[data-side-for] [data-desk-act="send_back"]').click();
  await expect(pane.locator('textarea[data-text="back:d2"]')).toBeVisible();
  await pane.locator('textarea[data-text="back:d2"]').fill("Use the box's own focus step");
  await pane.locator('[data-desk-act="back_go"][data-id="d2"]').click();
  await cue.called("desk_item_act", { id: "d2", act: "send_back", text: "Use the box's own focus step" });
  // A landing stopped on something Cue can't name: Run the landing again, no Send back.
  await patchItem(cue, "d5", { state: "needs_you", tree: tree("d5"), finish: { step: "error", at: Date.now(), question: "git couldn't make its commit" } });
  pane = await openItem(page, "d5");
  const box = pane.locator(".dk-wo");
  await expect(box.locator('[data-desk-act="rerun"]')).toHaveText("Run the landing again");
  await expect(box.locator('[data-desk-act="send_back"]')).toHaveCount(0);
  // A word that mustn't go in, a check failed: Send back to fix.
  await patchItem(cue, "d5", { finish: { step: "word", at: Date.now(), question: "Not landed: a word is in it" } });
  await expect(box.locator('[data-desk-act="fix_back"]')).toHaveText("Send back to fix");
});

test("a status line an item stopped on isn't labelled Answer on its page; a question is (#435)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await patchItem(cue, "d5", { state: "needs_you", ask: null, finish: null, latest: "A re-run of the tests is going now." });
  const pane = await openItem(page, "d5");
  await expect(pane).toContainText("A re-run of the tests is going now.");
  await expect(pane).not.toContainText("Answer:");
  await patchItem(cue, "d5", { latest: "Keep the old group?" });
  await expect(pane).toContainText("Answer: Keep the old group?");
});

test("an item in its own copy says so with its branch; one queued for a copy waits for a free one; one landed with no check says so", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // At work in its own copy: its page names the branch.
  await patchItem(cue, "d3", { tree: tree("d3", 2) });
  let pane = await openItem(page, "d3");
  await expect(pane.locator(".dk-panel[data-side-for] .dk-pmeta")).toContainText("in its own copy desk/d3");

  // Queued for a working folder, all three in use (#401): its row and page say so plainly, naming who uses them.
  await patchItem(cue, "d4", { state: "queued", after: "", tree_wait: true, copy_users: ["d3", "d5", "d7"] });
  const row = page.locator('.dk-items [data-desk-item="d4"]');
  await backToItems(page);
  await expect(statusOf(page, "d4")).toHaveText("Waiting for a free working folder (in use by #d3, #d5, #d7)");
  pane = await openItem(page, "d4");
  await expect(pane).toContainText("Waiting for a free working folder (in use by #d3, #d5, #d7). It starts by itself when one is free, or now with Start now.");
  await expect(pane.locator(".dk-panel[data-side-for] .dk-pmeta")).not.toContainText("in its own copy");
  // Sent back while none is free: it says so; Start now starts it at once, and the line goes.
  await patchItem(cue, "d4", { back_waiting: "Desk item #d4 isn't done: the dot is too big" });
  await openMore(pane.locator(".dk-panel[data-side-for]"));
  await expect(pane.locator('.dk-panel[data-side-for] [data-desk-act="start_now"]')).toHaveText("Start now");
  await backToItems(page);
  await expect(statusOf(page, "d4")).toHaveText("Sent back · waiting for a free working folder (in use by #d3, #d5, #d7)");
  // Start now is on its page, not its row (B2, #666).
  await expect(row.locator("button")).toHaveCount(0);
  pane = await openItem(page, "d4");
  await openMore(pane.locator(".dk-panel[data-side-for]"));
  await pane.locator('.dk-panel[data-side-for] [data-desk-act="start_now"]').click();
  await cue.called("desk_item_act", { id: "d4", act: "start_now" });
  await backToItems(page);
  await expect(statusOf(page, "d4")).not.toContainText("working folder");
  pane = await openItem(page, "d4");
  await expect(pane).not.toContainText("free working folder");

  // Landed with no check to run: to check, "landed, not checked" by its commit; its copy is gone.
  await patchItem(cue, "d2", { tree: null, commit: "def5678", finish: { step: "checked", commit: "def5678", at: Date.now(), unchecked: true } });
  pane = await openItem(page, "d2");
  await expect(pane.locator(".dk-panel[data-side-for] .dk-pmeta")).toContainText("landed, not checked");
  await patchItem(cue, "d2", { finish: { step: "checked", commit: "def5678", at: Date.now() } });
  await expect(pane.locator(".dk-panel[data-side-for] .dk-pmeta")).not.toContainText("not checked");
});

test("Kept copies lists a dropped item's copy with its work; Remove takes two clicks, and anything else disarms it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await patchDesk(cue, { kept: [{ id: "d9", title: "Old idea, dropped", project: "cue", repo: "/r/cue", path: "/r/cue-kept/d9", branch: "desk/d9", at: Date.now() - 120000 }] });
  const list = page.locator(".dk-items");
  await expect(list.locator(".dk2b-gh", { hasText: "KEPT COPIES" })).toHaveText("KEPT COPIES 1");
  const kept = list.locator('.dk-kept[data-kept="d9"]');
  await expect(kept).toContainText("#d9 Old idea, dropped");
  await expect(kept).toContainText("/r/cue-kept/d9 · desk/d9 · dropped");
  // Above Done.
  const heads = await list.locator(".dk2b-gh, .dk2b-fold").allTextContents();
  expect(heads.findIndex((h) => h.startsWith("KEPT COPIES"))).toBeGreaterThan(-1);
  expect(heads.findIndex((h) => h.startsWith("KEPT COPIES"))).toBeLessThan(heads.findIndex((h) => h.startsWith("DONE TODAY")));
  await cue.override("desk_item_act", () => null);
  const remove = kept.locator('[data-desk-act="remove_kept"]');
  await expect(remove).toHaveText("Remove");
  // First click arms it; a click elsewhere disarms.
  await remove.click();
  await expect(remove).toHaveText("Remove it and its work");
  await list.locator(".dk2b-ih b").click();
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
  const row = page.locator('.dk-items [data-desk-item="d5"]');
  await patchItem(cue, "d5", { state: "finishing", stopped: true, tree: tree("d5"), finish: { step: "conflict", at: Date.now(), lead: true, question: "Not merged: the main checkout has edits of its own in a.txt, ui/b.js, c.txt that aren't committed", clear: ["a.txt", "ui/b.js"], clear_keep: [["c.txt", "tidier"]], clear_in: "/r/cue", clear_sha: "abc1234" } });
  // Its row: what stopped it in plain words; the button is on its page (B2, #666).
  await expect(statusOf(page, "d5")).toHaveText("Stopped · leftover edits in the main folder");
  await expect(row.locator('[data-desk-act]')).toHaveCount(0);
  const pane = await openItem(page, "d5");
  const box = pane.locator(".dk-wo");
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
