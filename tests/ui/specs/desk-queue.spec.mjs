// The Desk's Queue (#576): waiting items sit in a "Queue" group at the top of the list, in your order, with no
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

test("the Queue sits at the top of the list and keeps its order and drag hint", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect.poll(async () => (await listOrder(page))[0]).toBe("# Queue");
  const order = await listOrder(page);
  // Its rows right under it, then the other groups.
  expect(order.slice(0, 4)).toEqual(["# Queue", "d4", "d6", "# Waiting on you"]);
  await expect(page.locator(".dk-items .dk-gh").first()).toContainText("drag to reorder");
  await expect(page.locator(".dk-items .dk-gh").first()).toHaveAttribute("data-desk-group", "queued");
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
  await expect(who(page, "d6")).toHaveText("web-lead · after #d3");
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
