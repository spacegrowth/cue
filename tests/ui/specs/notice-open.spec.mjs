// A notification clicked (the app's "select" event with a card's id) opens its session where it lives (#317): a
// Desk's session on that Desk (the lead's chat for its lead, that session's page for another), any other session on
// the Board, as before. Scenario: desk-inbox (cue-lead runs the cue Desk, web holds an item on it, notes is on none).
import { test, expect } from "./cue.mjs";

const cardOf = (page, sid) => page.evaluate((s) => state.items.find((i) => i.session_id === s).id, sid);

test("a notification for the Desk lead opens the Desk on its chat, not the Board", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await cue.emit("select", await cardOf(page, "cue-lead"));
  await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/\bon\b/);
  expect(await page.evaluate(() => [deskSelection(), deskOpen])).toEqual(["cue", null]);
});

test("a notification for a session on a Desk opens its page on that Desk", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await cue.emit("select", await cardOf(page, "web"));
  await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/\bon\b/);
  await expect(page.locator('.dk-pane[data-scroll-key="holder:web"]')).toBeVisible();
});

test("a notification for a session on no Desk opens it on the Board, as before", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  const id = await cardOf(page, "notes");
  await cue.emit("select", id);
  await expect(page.locator('.switch-view [data-view="board"]')).toHaveClass(/\bon\b/);
  expect(await page.evaluate(() => active.sid)).toBe("notes");
});

test("the add-on can take a notification: its openNotice true leaves the window where the add-on put it", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await page.evaluate(() => { addOn.openNotice = (id, sid) => { window.taken = [id, sid]; return sid === "web"; }; });
  const id = await cardOf(page, "web");
  await cue.emit("select", id);
  expect(await page.evaluate(() => [window.taken, view])).toEqual([[id, "web"], "board"]);
});
