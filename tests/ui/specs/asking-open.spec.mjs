// What's asking you, opened from any list of it (#492), goes where that session lives, as a notification does
// (notice-open.spec.mjs): a Desk's session on that Desk (the lead's chat for its lead; for another, the item it's on,
// else its chat), any other session on the Board. The lists: the header's "asking you" chip (and ‹ › stepping on from
// it), Waiting's list under its chip, and the mini window's ⤢ In Cue ("s:<session>"). Scenario: desk-inbox (cue-lead
// runs the cue Desk, web holds item d7k2 on it, notes is on none; each asks a question, notes the oldest).
import { test, expect } from "./cue.mjs";

const where = (page) => page.evaluate(() => ({ view, desk: view === "desk" ? deskSelection() : null, open: view === "desk" ? (deskOpen ? `${deskOpen.kind}:${deskOpen.id || deskOpen.sid}` : deskTab) : active?.sid }));

test("the asking chip opens a Board session on the Board, a Desk item's session at its item on the Desk, the lead's ask in the lead's chat; ‹ › step through them", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  const chip = page.locator('.top [data-chip="asking"]');
  await expect(chip).toContainText("3 asking you");
  // Oldest first: notes (on no Desk) on the Board.
  await chip.click();
  await expect.poll(() => where(page)).toEqual({ view: "board", desk: null, open: "notes" });
  await expect(page.locator(".top .hchip.nav")).toContainText("1 of 3 asking you");
  // Then web: on the cue Desk, its item d7k2 open, not the Board.
  await page.locator('.top [data-chip-step="asking:1"]').click();
  await expect.poll(() => where(page)).toEqual({ view: "desk", desk: "cue", open: "item:d7k2" });
  await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/\bon\b/);
  await expect(page.locator(".top .hchip.nav")).toContainText("2 of 3 asking you");
  // Then the Desk lead's ask: the lead's chat on its Desk.
  await page.locator('.top [data-chip-step="asking:1"]').click();
  await expect.poll(() => where(page)).toEqual({ view: "desk", desk: "cue", open: "lead" });
  await expect(page.locator(".top .hchip.nav")).toContainText("3 of 3 asking you");
  // And back round to notes, on the Board again.
  await page.locator('.top [data-chip-step="asking:1"]').click();
  await expect.poll(() => where(page)).toEqual({ view: "board", desk: null, open: "notes" });
});

test("a Desk session on no open item opens its chat on the Desk from the asking chip", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await cue.change((st) => { const d = st.desk.projects.find((p) => p.project === "cue"); d.items.find((i) => i.id === "d7k2").state = "done"; d.members = [...(d.members || []), "web"]; });
  await page.locator('.top [data-chip="asking"]').click();
  await page.locator('.top [data-chip-step="asking:1"]').click();
  await expect.poll(() => where(page)).toEqual({ view: "desk", desk: "cue", open: "chat:web" });
});

test("Waiting's list under its chip opens a Desk session on its Desk, a Board one on the Board", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await cue.change((st) => { for (const i of st.items) if (i.session_id !== "cue-lead") i.kind = "waiting"; });
  await page.locator('.top [data-chip-list="waiting"]').click();
  await page.locator(".wt-pop .wt-row", { hasText: "web" }).click();
  await expect.poll(() => where(page)).toEqual({ view: "desk", desk: "cue", open: "item:d7k2" });
  await page.locator('.top [data-chip-list="waiting"]').click();
  await expect(page.locator(".wt-pop .wt-row.on")).toContainText("web");
  await page.locator(".wt-pop .wt-row", { hasText: "notes" }).click();
  await expect.poll(() => where(page)).toEqual({ view: "board", desk: null, open: "notes" });
});

test("the mini window's ⤢ In Cue opens a Desk session on its Desk and the lead in its chat; a Board session on the Board", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await cue.emit("select", "s:web");
  await expect.poll(() => where(page)).toEqual({ view: "desk", desk: "cue", open: "item:d7k2" });
  await cue.emit("select", "s:notes");
  await expect.poll(() => where(page)).toEqual({ view: "board", desk: null, open: "notes" });
  await cue.emit("select", "s:cue-lead");
  await expect.poll(() => where(page)).toEqual({ view: "desk", desk: "cue", open: "lead" });
});
