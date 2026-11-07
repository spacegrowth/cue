// Moving a session to another project (⋯ → Move to project): what the window saves, where the session then
// shows, and moving it back. The fake applies project.moves to the state as hub/mod.rs move_projects does.
import { test, expect } from "./cue.mjs";

/** The open session's ⋯ → Move to project… list. */
async function moveMenu(page, sid) {
  await page.locator(`.active-pane [data-more="${sid}"]`).click();
  await page.locator(`[data-move-ask="${sid}"]`).click();
}

/** The project switcher on `project`. */
async function showProject(page, project) {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator(`.pj-pop [data-pj="pick"][data-project="${project}"]`).click();
}

test.describe("moving a session to another project", () => {
  test.beforeEach(async ({ cue, page }) => {
    await cue.open("board-waiting");
    await page.locator('.nrow[data-big="n1"]').click();
  });

  test("picking a project saves the move and the session shows under it", async ({ cue, page }) => {
    await moveMenu(page, "relay");
    await page.locator('[data-move-proj="backtester"][data-sid="relay"]').click();
    expect((await cue.called("set_setting", { key: "project.moves" })).args.value).toEqual({ relay: "backtester" });
    await expect(page.locator(".toast")).toContainText("Moved to backtester");
    await showProject(page, "backtester");
    await expect(page.locator(".sec-waiting .nrow")).toHaveCount(2);
    await expect(page.locator('.nrow[data-big="n1"]')).toBeVisible();
  });

  test("moving it back to its folder's own project takes the move away", async ({ cue, page }) => {
    await moveMenu(page, "relay");
    await page.locator('[data-move-proj="backtester"][data-sid="relay"]').click();
    await cue.called("set_setting", { key: "project.moves", value: { relay: "backtester" } });
    await moveMenu(page, "relay");
    await expect(page.locator('[data-move-proj="agent-relay"][data-sid="relay"]')).toContainText("Its folder's own");
    await page.locator('[data-move-proj="agent-relay"][data-sid="relay"]').click();
    await cue.called("set_setting", { key: "project.moves", value: {} });
    await expect(page.locator(".toast")).toContainText("Back in agent-relay");
    await showProject(page, "agent-relay");
    await expect(page.locator('.nrow[data-big="n1"]')).toBeVisible();
  });

  test("+ New project… moves it to a new one and goes there; a name already taken is said", async ({ cue, page }) => {
    await moveMenu(page, "relay");
    await page.locator('[data-new-proj="relay"]').click();
    const box = page.locator(".pm-new-in");
    await box.fill("Backtester");
    await box.press("Enter");
    await expect(page.locator(".proj-move .pm-note").last()).toContainText("is already a project");
    expect(await cue.calls("set_setting")).toEqual([]);
    await box.fill("Ops");
    await box.press("Enter");
    expect((await cue.called("set_setting", { key: "project.moves" })).args.value).toEqual({ relay: "Ops" });
    await expect(page.locator(".pj-open").first()).toContainText("Ops");
    await expect(page.locator('.nrow[data-big="n1"]')).toBeVisible();
  });
});

// History's rows come brief, and a row opened asks for its card in full (ui/core.js fullItem): a row whose
// project then moves (hub/projects.rs, in place) is asked for again, never the card as it was.
test("a History card already had in full follows its row when its project moves", async ({ cue, page }) => {
  await cue.open("terminal");
  await cue.change((st) => { for (const h of st.history) h.brief = true; });
  const id = await page.evaluate(() => state.history[0].id);
  await page.evaluate((id) => findItem(id), id);
  await expect.poll(async () => (await cue.calls("history_item")).length).toBe(1);
  await cue.change((st, id) => { st.history.find((i) => i.id === id).project = "elsewhere"; }, id);
  await expect.poll(() => page.evaluate((id) => findItem(id).project, id)).toBe("elsewhere");
});
