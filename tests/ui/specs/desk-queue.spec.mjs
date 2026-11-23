// The Desk's Queue (#576): waiting items sit in UP NEXT (C, #748: Waiting on you, Working, then Up next and Done today), in your order, with no
// session yet; each row says in words what it waits for: the session it's marked for (and when it goes to a fresh
// one instead), the item before it, or any free session. The item's own page says the same.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";
import { listOrder, statusOf, groupOf, openDone } from "./desk.mjs";

/** Queued item `id` on the cue Desk set to `fields`, with no holder (as the engine queues it now). */
const queue = (cue, id, fields) => cue.change((st, [id, f]) => {
  const i = st.desk.projects.find((d) => d.project === "cue").items.find((x) => x.id === id);
  delete i.holder; delete i.next_for; delete i.fresh_at;
  Object.assign(i, { state: "queued", after: null, queued_at: Date.now() }, f);
}, [id, fields]);
const who = (page, id) => statusOf(page, id);
const HOST = { kind: "session", sid: "host", name: "host-lead" };

/** The order of Items' group heads and its drawers on the green, by key ("you", "working", "next", "done", …). */
const heads = (page) => page.locator(".dk-items [data-desk2b-group], .dk-items [data-dkn-drawer]").evaluateAll((els) => els.map((el) => el.dataset.desk2bGroup || el.dataset.dknDrawer));
/** One item parked on the cue Desk, so every section shows. */
const park = (cue) => cue.change((st) => {
  st.desk.projects.find((d) => d.project === "cue").items.push({ id: "p1", project: "cue", kind: "task", title: "Tidy the footer", state: "held", holder: null, after: null,
    created: Date.now() / 1000, updated: Date.now() / 1000, from: "you", held: [Math.floor(Date.now() / 1000), "You", ""] });
});
/** C's order, top to bottom (#748): what waits on you (questions, then what's ready to check), what's at work, what's
 *  up next (queued, then parked), and Done today. */
const ORDER = ["you", "working", "next", "done"];

test("the list's groups in order: Waiting on you, Working, Up next, Done today; light and dark (#602, #748)", async ({ page, cue }) => {
  for (const colorScheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme });
    await cue.open("desk");
    await openDesk(page);
    await park(cue);
    await expect.poll(() => heads(page)).toEqual(ORDER);
    // Each one's rows under it: the question before the finished one; the queue in your order, the parked one last.
    await expect.poll(async () => (await listOrder(page)).slice(0, 11)).toEqual(["# WAITING ON YOU", "d1", "d2", "# WORKING", "d3", "d5", "# UP NEXT", "d4", "d6", "p1", "# DONE TODAY"]);
    await expect(statusOf(page, "p1")).toHaveText("Parked: starts when you press Start");
    // Done today starts open, its rows after its head.
    await expect(page.locator('.dk-items [data-desk-c-fold="done"]')).toHaveAttribute("aria-expanded", "true");
    expect((await listOrder(page)).slice(11)).toContain("d7");
  }
});

test("the order holds with Done today opened, groups emptied, and after a restart (#602, #748)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await park(cue);
  await expect.poll(() => heads(page)).toEqual(ORDER);
  // Done today opened: the same order, and so after a restart, as you left it.
  await openDone(page);
  await expect(page.locator('.dk-items [data-desk-item="d7"]')).toBeVisible();
  await expect.poll(() => heads(page)).toEqual(ORDER);
  await cue.open("desk");
  await openDesk(page);
  await park(cue);
  await expect.poll(() => heads(page)).toEqual(ORDER);
  await expect(page.locator('.dk-items [data-desk-c-fold="done"]')).toHaveAttribute("aria-expanded", "true");

  // Empty groups go, and the rest keep their order.
  await cue.change((st) => { for (const i of st.desk.projects.find((d) => d.project === "cue").items) if (i.id === "d1") { i.state = "working"; delete i.ask; } });
  await expect(page.locator('.dk-items [data-desk2b-group="you"] > span:not(.dkn-fa)')).toHaveText("1");
  await expect.poll(() => heads(page)).toEqual(ORDER);
  await cue.change((st) => { for (const i of st.desk.projects.find((d) => d.project === "cue").items) if (i.id === "d2") { i.state = "working"; delete i.finish; } });
  await expect.poll(() => heads(page)).toEqual(["working", "next", "done"]);
  await cue.change((st) => { for (const i of st.desk.projects.find((d) => d.project === "cue").items) if (i.state === "working") i.state = "done"; });
  await expect.poll(() => heads(page)).toEqual(["next", "done"]);
  await cue.change((st) => { for (const i of st.desk.projects.find((d) => d.project === "cue").items) if (i.state === "held") i.state = "queued"; });
  await expect.poll(() => heads(page)).toEqual(["next", "done"]);
  expect(await groupOf(page, "p1")).toBe("UP NEXT");
});

test("each queued row says what it waits for, in words; the item's page says the same", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Marked for a session, going to a fresh one at a time (17:00 now, UTC).
  await queue(cue, "d4", { next_for: HOST, fresh_at: NOW + 20 * 60000 });
  await expect(who(page, "d4")).toHaveText("Next for host-lead · goes to a fresh session at 17:20");
  // Marked for it, no fresh time.
  await queue(cue, "d4", { next_for: HOST });
  await expect(who(page, "d4")).toHaveText("Next for host-lead");
  // After another item, then for a session.
  await queue(cue, "d4", { after: "d3", next_for: HOST });
  await expect(who(page, "d4")).toHaveText("After #d3 · then host-lead");
  // After another item only.
  await queue(cue, "d4", { after: "d3" });
  await expect(who(page, "d4")).toHaveText("After #d3");
  // Neither: any free session, no Give to….
  await queue(cue, "d4", {});
  await expect(who(page, "d4")).toHaveText("Any free session");
  await expect(page.locator('.dk-items [data-desk-item="d4"] [data-desk-give]')).toHaveCount(0);
  // An older one still queued with a holder reads as before.
  await queue(cue, "d6", { holder: { kind: "lead", sid: "lead", name: "web-lead" }, after: "d3" });
  await expect(who(page, "d6")).toHaveText("After #d3");
  // Nothing in the list has hover text.
  await expect(page.locator(".dk-items [title]")).toHaveCount(0);

  // Its page (no session has it: desk/c.js deskCItemBare): the line under its title says the same.
  await queue(cue, "d4", { next_for: HOST, fresh_at: NOW + 20 * 60000 });
  await page.locator('.dk-items [data-desk-item="d4"]').click();
  const meta = page.locator('.dk-right .dkn-sp[data-desk-page="item"] .dkn-ihead .dkn-smeta');
  await expect(meta).toHaveText("Next for host-lead · goes to a fresh session at 17:20");
  await queue(cue, "d4", {});
  await expect(meta).toHaveText("Any free session");
});

test("the Queue's look, light and dark", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await queue(cue, "d4", { next_for: HOST, fresh_at: NOW + 20 * 60000 });
  await queue(cue, "d6", { after: "d4", next_for: HOST });
  await expect(who(page, "d6")).toHaveText("After #d4 · then host-lead");
  for (const colorScheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme });
    await page.waitForTimeout(150);
    // The list may be drawn anew as the colors change: its box looked up again till it's there.
    await expect(async () => {
      const clip = await page.locator(".dk-items").boundingBox();
      expect(clip).toBeTruthy();
      await page.screenshot({ clip, path: test.info().outputPath(`desk-queue-${colorScheme}.png`) });
    }).toPass({ timeout: 5000 });
  }
});
