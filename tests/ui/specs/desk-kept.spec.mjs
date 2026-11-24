// A dropped item's kept copy (#608): its row opens the item (with the chat of the session that held it, which
// you can still write to), shows what it changed with a Diff, and offers Pick up again (the item back on the Desk with
// that copy, for the lead to route) beside Remove, which still takes two clicks.
import { test, expect, openDesk } from "./cue.mjs";

const DIFF = "diff --git a/ui/a.js b/ui/a.js\nnew file mode 100644\n--- /dev/null\n+++ b/ui/a.js\n@@ -0,0 +1,3 @@\n+a\n+b\n+c\n"
  + "diff --git a/ui/b.js b/ui/b.js\n--- a/ui/b.js\n+++ b/ui/b.js\n@@ -1,2 +1,2 @@\n-old\n+new\n same\n";
const holder = { kind: "session", sid: "host", name: "host-lead" };
const kept = (at) => ({ id: "k1", title: "Old idea, dropped", project: "cue", repo: "/r/cue", path: "/r/cue/.claude/worktrees/kept-k1", branch: "desk/k1", base: "abc1234", at,
  item: { id: "k1", project: "cue", kind: "feature", title: "Old idea, dropped", see: "The idea shows", done_when: "", state: "dropped", holder, stints: [], created: Math.floor(at / 1000) - 600, updated: Math.floor(at / 1000) } });
const setKept = (cue) => cue.change((st, k) => { st.desk.projects.find((x) => x.project === "cue").kept = [k]; }, kept(Date.now() - 120000));

test.beforeEach(async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.override("desk_item_diff", (args, diff) => (args.id === "k1" ? { machine: "", diff } : { machine: "", diff: "" }), DIFF);
  await setKept(cue);
});

test("a kept copy's row shows what it changed, and its Diff opens the files over the Desk", async ({ page, cue }) => {
  const row = page.locator('.dk-items .dk-kept[data-kept="k1"]');
  await expect(row.locator(".dk-kept-t")).toContainText("#k1 Old idea, dropped");
  await cue.called("desk_item_diff", { id: "k1" });
  const diff = row.locator("button.dk-kept-f");
  await expect(diff).toContainText("2 files");
  await expect(diff.locator(".dv-n")).toHaveText(["+4", "−1"]);
  await expect(diff).toContainText("Diff");
  // Remove still at its end; Pick up again beside it.
  await expect(row.locator("button")).toHaveText([/#k1/, /2 files/, "Pick up again", "Remove"]);
  await diff.click();
  await expect(page.locator(".dov .dov-lines .a").first()).toBeVisible();
  await expect(page.locator(".dov")).toContainText("ui/a.js");
  await page.keyboard.press("Escape");
  await expect(page.locator(".dov")).toHaveCount(0);
  // No hover text on any of it, and no colored left edge.
  await expect(row.locator("[title]")).toHaveCount(0);
  await expect(row).toHaveCSS("border-left-width", "0px");
});

test("clicking a kept copy opens its item: the chat of the session that held it, a box straight to it, Pick up again and Remove", async ({ page, cue }) => {
  await page.locator('.dkn-items .dk-kept[data-kept="k1"] .dk-kept-t').click();
  const right = page.locator(".dk-c > .dk-right");
  await expect(right.locator(".dkn-sp")).toHaveCount(1);
  await expect(right).toContainText("Old idea, dropped");
  await expect(right).toContainText("/r/cue/.claude/worktrees/kept-k1 on desk/k1");
  // Its bar: Pick up again and Remove.
  const bar = right.locator(".dkn-ib .dkn-ibacts");
  await expect(bar.locator('[data-desk-act="pickup_kept"]')).toHaveText("Pick up again");
  // The box goes to the session that held it, not as an item message.
  await expect(right.locator(".foot:not(.held) .composer .field-to1")).toHaveText("To host-lead");
  await expect(right.locator(".foot:not(.held) .composer .field-to2")).toHaveText("Held #k1");
  await right.locator(".dkn-spm textarea").fill("What did you get done on it?");
  await page.keyboard.press("Enter");
  await expect.poll(async () => (await cue.calls("send_to_session")).map((c) => [c.args.sessionId, c.args.text])).toContainEqual(["host", "What did you get done on it?"]);
  // Remove asks twice, here as on its row.
  const remove = bar.locator('[data-desk-act="remove_kept"]');
  await expect(remove).toHaveText("Remove");
  await remove.click();
  await expect(remove).toHaveText("Remove it and its work");
  await expect(right.locator(".dkn-ib [title]")).toHaveCount(0);
});

test("Pick up again puts the item back on the Desk, new, with its kept copy, and off Kept copies", async ({ page, cue }) => {
  await page.locator('.dkn-items .dk-kept[data-kept="k1"] .dk-kept-t').click();
  await page.locator('.dk-c > .dk-right .dkn-ib [data-desk-act="pickup_kept"]').click();
  await cue.called("desk_item_act", { id: "k1", act: "pickup_kept" });
  await expect(page.locator(".dkn-items .dk-kept")).toHaveCount(0);
  await expect(page.locator('.dkn-items .dkn-gh[data-desk2b-group="kept"]')).toHaveCount(0);
  await page.locator(".dk-c > .dk-right .dk-back").first().click();
  await expect(page.locator('.dkn-items [data-desk-item="k1"]')).toBeVisible();
});

test("Pick up again from the row; Remove there still takes two clicks", async ({ page, cue }) => {
  const row = page.locator('.dk-items .dk-kept[data-kept="k1"]');
  const remove = row.locator('[data-desk-act="remove_kept"]');
  await remove.click();
  await expect(remove).toHaveText("Remove it and its work");
  await row.locator('[data-desk-act="pickup_kept"]').click();
  // Another button disarms Remove; nothing was removed.
  expect((await cue.calls("desk_item_act")).filter((c) => c.args.act === "remove_kept")).toHaveLength(0);
  await cue.called("desk_item_act", { id: "k1", act: "pickup_kept" });
  await expect(page.locator('.dkn-items [data-desk-item="k1"]')).toBeVisible();
});
