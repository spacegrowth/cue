// Old copies (desk_copies.rs, #376): copies of the project whose work is already in the app and that nothing uses are
// listed on its Desk with their size. Remove asks once more before it removes them; Not now hides the list.
import { test, expect } from "./cue.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
const old = { kb: 3 * 1024 * 1024 + 512 * 1024, copies: [
  { path: "/r/cue/.claude/worktrees/agent-a1", branch: "worktree-agent-a1", kb: 3 * 1024 * 1024 },
  { path: "/r/cue/.claude/worktrees/desk-9", branch: "desk/9", kb: 512 * 1024 },
] };
const withOld = (st, o) => { const d = st.desk.projects.find((x) => x.project === "cue"); d.old_copies = o; };

test("old copies are listed with their size, and removed only on a second click", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change(withOld, old);
  await openDesk(page);
  const sec = page.locator(".dk-old");
  await expect(sec).toContainText("already in the app");
  await expect(sec.locator(".dk-old-list")).toContainText("desk-9");
  await expect(sec.locator(".dk-old-list")).toContainText("agent-a1");
  await expect(sec.locator("[title]")).toHaveCount(0);
  const remove = sec.locator('[data-desk-old="remove"]');
  await remove.click();
  await expect(remove).toContainText("Remove 2");
  expect(await cue.calls("desk_old_copies")).toHaveLength(0);
  await remove.click();
  await cue.called("desk_old_copies", { desk: "cue", yes: true });
});

test("Not now answers no and removes nothing", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change(withOld, old);
  await openDesk(page);
  await page.locator('.dk-old [data-desk-old="later"]').click();
  await cue.called("desk_old_copies", { desk: "cue", yes: false });
});
