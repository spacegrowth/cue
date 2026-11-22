import { test, expect, deskAllSessions } from "./cue.mjs";

// The Desk's name on its page: over the lead's chat card (B2, #666); its sessions list (All N sessions) heads with it too.
const deskTitle = (page) => page.locator(".dk2b-chat .dk-here");

test("two named Desks share a folder, keep separate items, and rename independently", async ({ cue, page }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top .hv-menu [data-hv-desk-home]").click();
  const workspaceFilter = await page.evaluate(() => projFilter);
  async function create(name) {
    await page.locator(".dh-add").click();
    await page.locator("#nd-folder").fill("/work/cue");
    await page.locator("#nd-name").fill(name);
    await page.locator('.nd [type="submit"]').click();
    await expect(page.locator(".nd")).toHaveCount(0);
    await expect(deskTitle(page)).toHaveText(name);
    return page.evaluate(() => deskSelection());
  }
  const a = await create("Feature A");
  const qa = page.locator('.dk-qa-in[data-qa="line"]');
  await qa.fill("Only A");
  await qa.press("Enter");
  await expect(page.locator(".dk-items")).toContainText("Only A");
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top .hv-menu [data-hv-desk-home]").click();
  const b = await create("Feature B");
  expect(a).not.toBe(b);
  await expect(page.locator(".dk-items")).not.toContainText("Only A");
  await qa.fill("Only B");
  await qa.press("Enter");
  await expect(page.locator(".dk-items")).toContainText("Only B");
  // Rename it from the head of its sessions list.
  await deskAllSessions(page);
  await page.locator('[aria-label="Rename this Desk"]').click();
  await expect(page.locator(".pj-name-in")).toHaveValue("Feature B");
  await page.locator(".pj-name-in").fill("Bob's feature");
  await page.locator(".pj-name-in").press("Enter");
  await cue.called("desk_rename", { id: b, name: "Bob's feature" });
  await expect(page.locator(".dk-sh b")).toHaveText("Bob's feature");
  // Back to the lead's chat, which the sessions list stood in for: its head has the new name too.
  await page.locator(".dk-sessions .dk-back").first().click();
  await expect(deskTitle(page)).toHaveText("Bob's feature");
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top .hv-menu [data-hv-desk-home]").click();
  await page.locator(`[data-desk-project="${a}"]`).click();
  await expect(deskTitle(page)).toHaveText("Feature A");
  await expect(page.locator(".dk-items")).toContainText("Only A");
  await expect(page.locator(".dk-items")).not.toContainText("Only B");
  expect(await page.evaluate(() => projFilter)).toBe(workspaceFilter);
  expect(await cue.calls("set_setting")).toHaveLength(0);
});

test("a Desk can start in a folder with no recent project", async ({ cue, page }) => {
  await cue.open("world-empty");
  await page.locator("[data-new-menu]").click();
  await page.locator(".nw-pop [data-desk-new-desk]").click();
  await page.locator("#nd-folder").fill("/work/brand-new");
  await page.locator("#nd-name").fill("codex-cue-desk-feature");
  await page.locator('.nd [type="submit"]').click();
  await cue.called("desk_start", { folder: "/work/brand-new", name: "codex-cue-desk-feature" });
  await expect(deskTitle(page)).toHaveText("codex-cue-desk-feature");
  await deskAllSessions(page);
  const tray = page.locator(".dk-shr .dk-ric svg.dk-kic");
  await expect(tray).toBeVisible();
  await expect(tray.locator("path").first()).toHaveAttribute("d", /^M4 13l3-7/);
  await expect(page.locator(".dk-lcard .dk-ric svg path").first()).toHaveAttribute("d", "M12 3v18M9 21h6");
});

test("deleting a Desk confirms what is removed and keeps its sessions", async ({ cue, page }) => {
  await cue.open("world-empty");
  await page.locator("[data-new-menu]").click();
  await page.locator("[data-desk-new-desk]").click();
  await page.locator("#nd-folder").fill("/work/project");
  await page.locator("#nd-name").fill("My Desk");
  await page.locator('.nd [type="submit"]').click();
  await expect(deskTitle(page)).toHaveText("My Desk");
  // A Desk is deleted from Desks, not from its own header (#215).
  await deskAllSessions(page);
  await expect(page.locator(".dk-strip [data-desk-delete]")).toHaveCount(0);
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top .hv-menu [data-hv-desk-home]").click();
  await expect(page.locator(".dh-nm")).toHaveText("My Desk");
  await page.locator('[data-desk-delete="desk-1"]').click();
  await expect(page.locator(".dh-confirm")).toContainText("Close these sessions");
  await expect(page.locator('[data-desk-close-session="lead-desk-1"]')).toBeChecked();
  await page.locator("[data-desk-delete-cancel]").click();
  await expect(page.locator(".dh-confirm")).toHaveCount(0);
  await expect.poll(() => cue.calls("desk_delete")).toEqual([]);
  await page.locator('[data-desk-delete="desk-1"]').click();
  await page.locator('[data-desk-close-session="lead-desk-1"]').uncheck();
  await page.locator("[data-desk-delete-confirm]").click();
  await cue.called("desk_delete", { id: "desk-1", closeSessions: [] });
  await expect(page.locator(".dh-empty")).toHaveText("No Desks yet.");
  expect(await page.evaluate(() => state.sessions.some((s) => s.session_id === "lead-desk-1"))).toBe(true);
});


test("deleting a Desk closes selected sessions and protects shared or working sessions", async ({ cue, page }) => {
  await cue.open("world-empty");
  await page.locator("[data-new-menu]").click();
  await page.locator("[data-desk-new-desk]").click();
  await page.locator("#nd-folder").fill("/work/project");
  await page.locator("#nd-name").fill("My Desk");
  await page.locator('.nd [type="submit"]').click();
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "desk-1");
    const base = st.sessions.find((x) => x.session_id === "lead-desk-1");
    for (const [id, state] of [["keep", "idle"], ["shared", "idle"], ["busy", "working"]]) st.sessions.push({ ...base, session_id: id, name: id, state });
    d.members.push("keep", "shared", "busy");
    st.desk.projects.push({ project: "desk-other", independent: true, name: "Other", workspace_project: "project", items: [], decisions: [], members: ["shared"] });
  });
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top .hv-menu [data-hv-desk-home]").click();
  await page.locator('[data-desk-delete="desk-1"]').click();
  await expect(page.locator('[data-desk-close-session="lead-desk-1"]')).toBeChecked();
  await expect(page.locator('[data-desk-close-session="keep"]')).toBeChecked();
  await expect(page.locator('[data-desk-close-session="shared"]')).toBeDisabled();
  await expect(page.locator('[data-desk-close-session="busy"]')).toBeDisabled();
  await page.locator('[data-desk-close-session="keep"]').uncheck();
  await page.locator("[data-desk-delete-confirm]").click();
  await cue.called("desk_delete", { id: "desk-1", closeSessions: ["lead-desk-1"] });
  await expect(page.locator(".dh-nm")).toHaveText("Other");
  const ids = await page.evaluate(() => state.sessions.map((s) => s.session_id));
  expect(ids).toEqual(expect.arrayContaining(["keep", "shared", "busy"]));
  expect(ids).not.toContain("lead-desk-1");
});

test("a deleted Desk and the sessions it closed are gone as the delete comes back, before Cue's next update", async ({ cue, page }) => {
  await cue.open("world-empty");
  await page.locator("[data-new-menu]").click();
  await page.locator("[data-desk-new-desk]").click();
  await page.locator("#nd-folder").fill("/work/project");
  await page.locator("#nd-name").fill("My Desk");
  await page.locator('.nd [type="submit"]').click();
  await expect(deskTitle(page)).toHaveText("My Desk");
  await cue.change((st) => st.desk.projects.push({ project: "desk-other", independent: true, name: "Other", workspace_project: "project", items: [], decisions: [], members: [] }));
  // The hub has deleted it when the command returns; its update to the window comes later (here: never).
  await cue.override("desk_delete", ({ id, closeSessions }) => {
    const w = CueBridge.world;
    w.state.desk.projects = w.state.desk.projects.filter((d) => d.project !== id);
    w.state.sessions = w.state.sessions.filter((s) => !closeSessions.includes(s.session_id));
    return { kept: [] };
  });
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top .hv-menu [data-hv-desk-home]").click();
  const before = await page.evaluate(() => state);
  await page.locator('[data-desk-delete="desk-1"]').click();
  await page.locator("[data-desk-delete-confirm]").click();
  await expect(page.locator(".dh-nm")).toHaveText("Other");
  await expect(page.locator(".dk-hh")).toContainText("· 1");
  expect(await page.evaluate(() => state.sessions.some((s) => s.session_id === "lead-desk-1"))).toBe(false);
  // An update sent before the delete, arriving late, doesn't bring them back.
  await cue.emit("state", before);
  await expect(page.locator(".dh-nm")).toHaveText("Other");
  expect(await page.evaluate(() => state.sessions.some((s) => s.session_id === "lead-desk-1"))).toBe(false);
});
