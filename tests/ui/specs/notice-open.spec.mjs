// A notification clicked (the app's "select" event with a card's id, or "c:{id, sid}" naming its session) opens its
// session where it lives (#317): a Desk's session on that Desk (the lead's chat for its lead; for another, the item it's
// on, else its chat, #388), any other session on the Board, as before. A card gone by the time it's clicked still opens
// its own session, never another's (#388). Scenario: desk-inbox (cue-lead runs the cue Desk, web holds item d7k2 on it,
// notes is on none).
import { test, expect } from "./cue.mjs";

const cardOf = (page, sid) => page.evaluate((s) => state.items.find((i) => i.session_id === s).id, sid);

test("a notification for the Desk lead opens the Desk on its chat, not the Board", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await cue.emit("select", await cardOf(page, "cue-lead"));
  await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/\bon\b/);
  expect(await page.evaluate(() => [deskSelection(), deskOpen])).toEqual(["cue", null]);
});

test("a notification for a session on a Desk opens the item it's on, not its page", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await cue.emit("select", await cardOf(page, "web"));
  await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/\bon\b/);
  expect(await page.evaluate(() => [deskSelection(), deskOpen?.kind, deskOpen?.id])).toEqual(["cue", "item", "d7k2"]);
  await expect(page.locator('.dk-pane[data-scroll-key="holder:web"]')).toHaveCount(0);
});

test("a notification for a Desk session on no open item opens its chat", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await cue.change((st) => { const d = st.desk.projects.find((p) => p.project === "cue"); d.items.find((i) => i.id === "d7k2").state = "done"; d.members = [...(d.members || []), "web"]; });
  await cue.emit("select", await cardOf(page, "web"));
  expect(await page.evaluate(() => [deskSelection(), deskOpen?.kind, deskOpen?.sid])).toEqual(["cue", "chat", "web"]);
});

test("a card gone by the time its notification is clicked still opens its own session, not the top of Waiting", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  // On a Desk: the item that session is on.
  await cue.emit("select", `c:${JSON.stringify({ id: "n-gone-1", sid: "web" })}`);
  expect(await page.evaluate(() => [view, deskOpen?.kind, deskOpen?.id])).toEqual(["desk", "item", "d7k2"]);
  // On no Desk (notes; a "stuck-" or "task-failed-" notice is the same): that session on the Board.
  await cue.emit("select", `c:${JSON.stringify({ id: "stuck-notes", sid: "notes" })}`);
  await expect(page.locator('.switch-view [data-view="board"]')).toHaveClass(/\bon\b/);
  expect(await page.evaluate(() => [active.sid, findItem(active.id)?.session_id ?? "notes"])).toEqual(["notes", "notes"]);
});

test("a Desk session's page says what it's doing now, not a reply from hours ago", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await page.evaluate(() => { openInDesk("cue"); deskShow({ kind: "holder", sid: "web" }); });
  const latest = page.locator('.dk-pane[data-scroll-key="holder:web"] .dk-quiet');
  // Idle: its latest reply, and how long ago it came.
  await expect(latest).toContainText("The box loses focus after Send");
  await expect(latest).toContainText("ago");
  // At work again: the step it's on now, not the reply from before this turn.
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "web"); Object.assign(s, { state: "working", doing: "Editing ui/reply.js", doing_ms: Date.now() }); st.items = st.items.filter((i) => i.session_id !== "web"); });
  await expect(latest).toContainText("Editing ui/reply.js");
  await expect(latest).not.toContainText("The box loses focus");
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
