// The header's switch: Inbox first, then Board, then Desk, then the rest.
import { test, expect } from "./cue.mjs";

test("the switch reads Inbox, Board, Desk, Read, Insights, Terminal", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const views = await page.locator(".switch-view button").evaluateAll((bs) => bs.map((b) => b.dataset.view));
  expect(views.slice(0, 3)).toEqual(["inbox", "board", "desk"]);
  expect(views).toEqual(expect.arrayContaining(["read", "insights", "terminal"]));
});
