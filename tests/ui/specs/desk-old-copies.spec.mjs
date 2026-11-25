// Old working folders (desk_copies.rs, #397): not in the item list; the lead asks once in its chat (a question card,
// tested in Rust), and How work lands has Remove old working folders: it looks, lists them with their size, and
// removes them on a second press.
import { test, expect, openDesk, deskAllSessions } from "./cue.mjs";

const old = { kb: 3 * 1024 * 1024 + 512 * 1024, copies: [
  { path: "/r/cue/.claude/worktrees/agent-a1", branch: "worktree-agent-a1", kb: 3 * 1024 * 1024 },
  { path: "/r/cue/.claude/worktrees/desk-9", branch: "desk/9", kb: 512 * 1024 },
] };
const withOld = (st, o) => { st.desk.projects.find((x) => x.project === "cue").old_copies = o; };
const row = (page) => page.locator(".ld-dialog .ld-old");

test("How work lands lists the old working folders with their size, and removes them on a second press", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change(withOld, old);
  await openDesk(page);
  await expect(page.locator(".dk-items")).not.toContainText("Old copies");
  await deskAllSessions(page);
  await page.locator(".dk-strip [data-lands-change]").click();
  // Its name is its section's heading since #447 made the screen shorter.
  await expect(page.locator('.ld-dialog [data-ld-sec="old"] .ld-h')).toHaveText("Old working folders");
  await row(page).locator('[data-ld="old"]').click();
  await expect(row(page).locator(".ld-old-list")).toContainText("agent-a1 · 3.0 GB");
  await expect(row(page).locator(".ld-old-list")).toContainText("desk-9 · 512 MB");
  expect(await cue.calls("desk_old_copies")).toHaveLength(0);
  await row(page).locator('[data-ld="old-yes"]').click();
  await cue.called("desk_old_copies", { desk: "cue", yes: true });
  await expect(page.locator(".toast")).toContainText("Removed 2 old working folders · 3.5 GB freed");
  await expect(row(page).locator('[data-ld="old"]')).toHaveText("Remove old working folders");
  await expect(page.locator(".ld-dialog [title]")).toHaveCount(0);
});

test("Keep them closes the list, removing nothing; none found says so", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change(withOld, old);
  await openDesk(page);
  await deskAllSessions(page);
  await page.locator(".dk-strip [data-lands-change]").click();
  await row(page).locator('[data-ld="old"]').click();
  await row(page).locator('[data-ld="old-no"]').click();
  await expect(row(page).locator('[data-ld="old"]')).toBeVisible();
  expect(await cue.calls("desk_old_copies")).toHaveLength(0);
  await cue.change(withOld, null);
  await row(page).locator('[data-ld="old"]').click();
  await expect(row(page)).toContainText("None: every other folder has work not on main yet, or is in use.");
});
