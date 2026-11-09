// A teammate or background agent gone quiet (src-tauri/src/sessions/quiet.rs): stuck says so, with Resume and Stop
// (its session or lead is asked, at once); a long tool run says what it's running and is left to it.
import { test, expect } from "./cue.mjs";

const M = 60_000;

test("a stuck background agent says so and offers Resume and Stop; a long run says what it runs", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true, flags: "many=1" });
  await page.evaluate(() => openSession("names"));
  await page.locator('[data-bg-tab="agents"]').first().click().catch(() => {});
  const stuck = page.locator('.ag-row[data-key="h:h4"]').first();
  await expect(stuck).toContainText("quiet for 16m — looks stuck");
  await expect(stuck.locator("[data-ag-wake]")).toHaveText(["Resume", "Stop"]);
  await expect(page.locator('.ag-row[data-key="h:h7"]').first()).toContainText("Running npm test for 20m");
  await expect(page.locator('.ag-row[data-key="h:h7"] [data-ag-wake]')).toHaveCount(0);
  // Its row is redrawn as its clock ticks: the click goes to the button as it is now.
  await stuck.locator('[data-ag-wake="resume"]').dispatchEvent("click");
  const sent = (await cue.called("send_to_session")).args;
  expect(sent).toMatchObject({ sessionId: "names", now: true });
  expect(sent.text).toContain('SendMessage (to: "h4")');
});

test("a stuck teammate's row says so and offers Resume and Stop; a long run is left to it", async ({ cue, page }) => {
  await cue.open("world-team", { frozen: true });
  await page.locator('.nrow[data-big="n1793984390004"]').click();
  // hub's ask answered first (as the team-open screen does), so its row is its own line.
  await page.locator('.act-card [data-act="allow"]').click();
  await page.locator("[data-mt-open]").first().click();
  await cue.change((st, ago) => {
    const lead = st.sessions.find((s) => s.session_id === "lead");
    const at = (n) => lead.team.find((t) => t.name === n);
    Object.assign(at("hub"), { quiet: "stuck", step_ms: st.now_ms - ago });
    Object.assign(at("rows"), { quiet: "tool", step_ms: st.now_ms - ago });
  }, 20 * M);
  const hub = page.locator(".mt-row", { hasText: "hub" }).first();
  await expect(hub.locator(".mt-line")).toContainText("quiet for 20m — looks stuck");
  await expect(hub.locator("[data-mt-act]")).toHaveText(["Resume", "Stop"]);
  await expect(page.locator(".mt-row", { hasText: "rows" }).first().locator(".mt-line")).toContainText("Running: node tests/team.test.js for 20m");
  await hub.locator('[data-mt-act="resume"]').click();
  expect((await cue.called("team_action", { action: "resume" })).args).toMatchObject({ sessionId: "lead" });
});
