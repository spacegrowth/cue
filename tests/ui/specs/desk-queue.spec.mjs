// The Desk's Queue (#576): waiting items sit in a "Queue" group under In progress (#602: Waiting on you, Ready to check, In progress, Queue, Parked, Done), in your order, with no
// session yet; each row says in words what it waits for: the session it's marked for (and when it goes to a fresh
// one instead), the item before it, or any free session. The item's own page says the same.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";
import { listOrder } from "./desk.mjs";

/** Queued item `id` on the cue Desk set to `fields`, with no holder (as the engine queues it now). */
const queue = (cue, id, fields) => cue.change((st, [id, f]) => {
  const i = st.desk.projects.find((d) => d.project === "cue").items.find((x) => x.id === id);
  delete i.holder; delete i.next_for; delete i.fresh_at;
  Object.assign(i, { state: "queued", after: null, queued_at: Date.now() }, f);
}, [id, fields]);
const who = (page, id) => page.locator(`.dk-items [data-desk-item="${id}"] .dk-who`);
const HOST = { kind: "session", sid: "host", name: "host-lead" };

/** The order of the list's group heads, by key ("you", "check", "working", …). */
const heads = (page) => page.locator(".dk-items [data-desk-group]").evaluateAll((els) => els.map((el) => el.dataset.deskGroup));
const head = (page, key) => page.locator(`.dk-items [data-desk-group="${key}"]`);
/** One item parked on the cue Desk, so every section shows. */
const park = (cue) => cue.change((st) => {
  st.desk.projects.find((d) => d.project === "cue").items.push({ id: "p1", project: "cue", kind: "task", title: "Tidy the footer", state: "held", holder: null, after: null,
    created: Date.now() / 1000, updated: Date.now() / 1000, from: "you", held: [Math.floor(Date.now() / 1000), "You", ""] });
});
/** Robin's order, top to bottom (#602). */
const ORDER = ["you", "check", "working", "queued", "held", "done"];

test("the list's sections in Robin's order: Waiting on you, Ready to check, In progress, Queue, Parked, Done; Parked and Done folded; light and dark (#602)", async ({ page, cue }) => {
  for (const colorScheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme });
    await cue.open("desk");
    await openDesk(page);
    await park(cue);
    await expect.poll(() => heads(page)).toEqual(ORDER);
    // Each one's rows under it: Waiting on you only the question, Ready to check the finished one, the Queue in your
    // order with the drag hint.
    expect(await listOrder(page)).toEqual(["# Waiting on you", "d1", "# Ready to check", "d2", "# In progress", "d3", "d5", "# Queue", "d4", "d6", "# Parked", "# Done", "d7", "d8", "d9"]);
    await expect(head(page, "queued")).toContainText("drag to reorder");
    await expect(page.locator('.dk-items [data-desk-gbody="held"] [data-desk-item="p1"]')).toHaveCount(1);
    // Parked and Done start folded; the rest open.
    for (const k of ORDER) await expect(head(page, k)).toHaveAttribute("aria-expanded", String(!["held", "done"].includes(k)));
    await expect(page.locator('.dk-items [data-desk-item="p1"]')).toBeHidden();
    await expect(page.locator('.dk-items [data-desk-item="d7"]')).toBeHidden();
  }
});

test("the order holds with sections folded, opened, empty, and after a restart (#602)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await park(cue);
  await expect.poll(() => heads(page)).toEqual(ORDER);
  // In progress folded, Parked and Done opened: the same order, and so after a restart, each as you left it.
  for (const k of ["working", "held", "done"]) await head(page, k).click();
  await expect(head(page, "working")).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator('.dk-items [data-desk-item="p1"]')).toBeVisible();
  await expect.poll(() => heads(page)).toEqual(ORDER);
  await cue.open("desk");
  await openDesk(page);
  await park(cue);
  await expect.poll(() => heads(page)).toEqual(ORDER);
  for (const k of ORDER) await expect(head(page, k)).toHaveAttribute("aria-expanded", String(k !== "working"));

  // Empty sections go, and the rest keep their order; Waiting on you stays, at 0.
  await cue.change((st) => { for (const i of st.desk.projects.find((d) => d.project === "cue").items) if (i.id === "d1") { i.state = "working"; delete i.ask; } });
  await expect(head(page, "you").locator(".n")).toHaveText("0");
  await expect.poll(() => heads(page)).toEqual(ORDER);
  await cue.change((st) => { for (const i of st.desk.projects.find((d) => d.project === "cue").items) if (i.id === "d2") { i.state = "working"; delete i.finish; } });
  await expect.poll(() => heads(page)).toEqual(["you", "working", "queued", "held", "done"]);
  await cue.change((st) => { for (const i of st.desk.projects.find((d) => d.project === "cue").items) if (i.state === "working") i.state = "done"; });
  await expect.poll(() => heads(page)).toEqual(["you", "queued", "held", "done"]);
  await cue.change((st) => { for (const i of st.desk.projects.find((d) => d.project === "cue").items) if (i.state === "held") i.state = "queued"; });
  await expect.poll(() => heads(page)).toEqual(["you", "queued", "done"]);
});

test("each queued row says what it waits for, in words; the item's page says the same", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Marked for a session, going to a fresh one at a time (17:00 now, UTC).
  await queue(cue, "d4", { next_for: HOST, fresh_at: NOW + 20 * 60000 });
  await expect(who(page, "d4")).toHaveText("next for host-lead · goes to a fresh session at 17:20");
  // Marked for it, no fresh time.
  await queue(cue, "d4", { next_for: HOST });
  await expect(who(page, "d4")).toHaveText("next for host-lead");
  // After another item, then for a session.
  await queue(cue, "d4", { after: "d3", next_for: HOST });
  await expect(who(page, "d4")).toHaveText("after #d3 · then host-lead");
  // After another item only.
  await queue(cue, "d4", { after: "d3" });
  await expect(who(page, "d4")).toHaveText("after #d3");
  // Neither: any free session, no Give to….
  await queue(cue, "d4", {});
  await expect(who(page, "d4")).toHaveText("any free session");
  await expect(who(page, "d4").locator("[data-desk-give]")).toHaveCount(0);
  // An older one still queued with a holder reads as before.
  await queue(cue, "d6", { holder: { kind: "lead", sid: "lead", name: "web-lead" }, after: "d3" });
  await expect(who(page, "d6")).toHaveText("after #d3");
  // Nothing in the list has hover text.
  await expect(page.locator(".dk-items [title]")).toHaveCount(0);

  // Its page: the panel's Now and the line under it.
  await queue(cue, "d4", { next_for: HOST, fresh_at: NOW + 20 * 60000 });
  await page.locator('.dk-items [data-desk-item="d4"]').click();
  const now = page.locator(".dk-right .dk-pnow");
  await expect(now.locator(".dk-psay")).toHaveText("Next for host-lead · goes to a fresh session at 17:20.");
  await expect(now.locator(".dk-pmeta")).toContainText("next for host-lead");
  await queue(cue, "d4", {});
  await expect(now.locator(".dk-psay")).toHaveText("Any free session.");
  await expect(now.locator(".dk-pmeta")).not.toContainText("next for");
});

test("the Queue's look, light and dark", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await queue(cue, "d4", { next_for: HOST, fresh_at: NOW + 20 * 60000 });
  await queue(cue, "d6", { after: "d4", next_for: HOST });
  await expect(who(page, "d6")).toHaveText("after #d4 · then host-lead");
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
