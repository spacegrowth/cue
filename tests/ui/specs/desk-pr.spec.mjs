// PR landing (desk/hub/landing/pr.rs): an item whose project lands by pull request shows "In review" once its pull request
// is open, with the link and Open the PR (no other buttons); one whose branch couldn't be pushed or pull request
// opened says why and offers Run the landing again; one closed unmerged says so. After each landing, the item's
// line says it was pushed.
import { test, expect, openDesk } from "./cue.mjs";

const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
// An item's page (#418): its holder's chat in the middle, the item itself in the column at the right, in the Items
// list's place, so another item opens from the list after "‹ Items".
const backToItems = async (page) => {
  if (await page.locator(".dk-panel[data-side-for]").count()) await page.locator(".dk-right .dk-back").click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
const openItem = async (page, id) => {
  await backToItems(page);
  await page.locator(`.dk-items [data-desk-item="${id}"] .dk-t`).click();
  await page.locator(`.dk-panel[data-side-for="${id}"]`).waitFor();
  return page.locator(".dk-right .dk-pg");
};
const tree = (id) => ({ repo: "/r/cue", path: "/r/cue-copies/1", branch: `desk/${id}`, base: "abc1234", slot: 1 });
const PR = "https://example.test/o/r/pull/7";

test("an item in review shows its pull request's link and Open the PR, and nothing else to press", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const at = Date.now(), since = await page.evaluate((t) => clockAt(t), at);
  await patchItem(cue, "d5", { state: "finishing", tree: tree("d5"), finish: { step: "review", pr: PR, at } });
  const row = page.locator('.dk-items [data-desk-item="d5"]');
  await expect(row.locator(".dk-who")).toHaveText(`In review · since ${since}`);
  const pane = await openItem(page, "d5");
  const box = pane.locator(".dk-wo");
  await expect(box.locator(".dk-wo-t")).toHaveText("A review of its pull request · example.test/o/r/pull/7");
  await expect(box.locator(`.dk-wo-t a[data-href="${PR}"]`)).toHaveCount(1);
  await expect(box.locator(`a.btn[data-href="${PR}"]`)).toHaveText("Open the PR");
  await expect(box.locator("button")).toHaveCount(0);
  await expect(pane.locator('[data-desk-act="looks_right"]')).toHaveCount(0);
  await expect(box.locator("[title]")).toHaveCount(0);
  // Merged, not seen on main yet: its page and its row say so.
  await patchItem(cue, "d5", { finish: { step: "review", pr: PR, at, wait: "Merged; waiting to see it on main" } });
  await expect(box.locator(".dk-wo-s")).toHaveText("Merged; waiting to see it on main");
  await backToItems(page);
  await expect(row.locator(".dk-who")).toHaveText("In review · Merged; waiting to see it on main" + ` · since ${since}`);
});

test("a pull request that couldn't be opened says why; one closed unmerged offers a new one", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await patchItem(cue, "d5", { state: "finishing", tree: tree("d5"), finish: { step: "pr_failed", at: Date.now(), question: "This project has no remote to push to, so no pull request was opened" } });
  const pane = await openItem(page, "d5");
  const box = pane.locator(".dk-wo");
  await expect(box.locator(".dk-wo-t")).toHaveText("This project has no remote to push to, so no pull request was opened");
  await expect(box.locator('[data-desk-act="rerun"]')).toHaveText("Run the landing again");
  await expect(box).not.toContainText("A plain retry");
  await patchItem(cue, "d5", { finish: { step: "pr_closed", at: Date.now(), question: "Its pull request was closed without merging" } });
  await expect(box.locator(".dk-wo-t")).toHaveText("Its pull request was closed without merging");
  await expect(box.locator('[data-desk-act="rerun"]')).toHaveText("Open a new pull request");
});

test("pushed after its landing: its line says so", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await patchItem(cue, "d2", { tree: null, commit: "def5678", finish: { step: "checked", commit: "def5678", at: Date.now(), pushed: "Pushed to origin" } });
  const pane = await openItem(page, "d2");
  await expect(pane.locator(".dk-panel[data-side-for] .dk-pmeta")).toContainText("committed def5678 · pushed to origin");
});
