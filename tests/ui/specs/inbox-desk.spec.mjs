// The Inbox and the Desk (ui/inbox_desk.js): a project lead's ask about its Desk sits among the other asks under Needs
// you, tagged "<PROJECT> · Desk"; a session holding a Desk item shows "<PROJECT> · for <title>" on its row and, opened,
// a bar with Open in Desk; a session's own ask can be added to its project's Desk. Scenario: desk-inbox.
import { test, expect } from "./cue.mjs";

const openInbox = async (page) => {
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator(".inbox .ib-row").first().waitFor();
};
/** Section titles and each one's rows, in order. */
const layout = (page) => page.evaluate(() => {
  const out = [];
  for (const el of document.querySelectorAll(".ib-scroll > *")) {
    if (el.matches(".ib-sect")) out.push([el.textContent.trim(), []]);
    else if (el.matches(".ib-row")) out.at(-1)[1].push(el.dataset.inboxOpen);
  }
  return out;
});

test("a lead's Desk ask is one of the asks under Needs you, tagged '<PROJECT> · Desk'; no From Dispatch", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await openInbox(page);
  const groups = await layout(page);
  expect(groups.map(([t]) => t)).toEqual(["Needs you"]);
  expect([...groups[0][1]].sort()).toEqual(["cue-lead", "notes", "web"]);
  const lead = page.locator('.ib-row[data-inbox-open="cue-lead"]');
  await expect(lead.locator(".ib-name")).toHaveText("cue-lead");
  await expect(lead.locator(".ib-for")).toHaveText("cue · Desk");
  await expect(lead.locator(".ib-pj")).toHaveCSS("text-transform", "uppercase");
  await expect(page.locator(".ib-dmark")).toHaveCount(0);
  await expect(page.locator(".ib-counts")).toHaveText("3 need you");
  // Opened: the bar says what it's about, with Open in Desk; no Add to Desk (it's already the Desk's).
  await lead.click();
  await expect(page.locator(".ib-forbar")).toContainText("Desk");
  await expect(page.locator(".ib-forbar .ib-desk")).toHaveText("Open in Desk ›");
  await expect(page.locator("[data-inbox-add-desk]")).toHaveCount(0);
});

test("a session holding a Desk item: '<PROJECT> · for <title>' on its row; a plain one has none; no hover text", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await openInbox(page);
  await expect(page.locator('.ib-row[data-inbox-open="web"] .ib-for')).toHaveText("cue · for Reply box loses focus after Send");
  await expect(page.locator('.ib-row[data-inbox-open="web"] .ib-pj')).toHaveCSS("text-transform", "uppercase");
  // notes held an item that's done: a plain session now.
  await expect(page.locator('.ib-row[data-inbox-open="notes"] .ib-for')).toHaveCount(0);
  await page.locator('.ib-row[data-inbox-open="web"]').click();
  await expect(page.locator(".ib-forbar")).toContainText("for Reply box loses focus after Send");
  await expect(page.locator(".ib-forbar .ib-desk")).toHaveText("Open in Desk ›");
  // It holds one already: no Add to Desk.
  await expect(page.locator("[data-inbox-add-desk]")).toHaveCount(0);
  expect(await page.locator(".inbox [title], .inbox-page [title]").count()).toBe(0);
});

test("Add to Desk on a session's ask: an item on its project's Desk, held by it", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await openInbox(page);
  await page.locator('.ib-row[data-inbox-open="notes"]').click();
  const add = page.locator('.ib-head [data-inbox-add-desk="notes"]');
  await expect(add).toHaveText("Add to Desk");
  await add.click();
  await cue.called("desk_add", { project: "website", kind: "task", title: "Publish the draft now, or wait for review?", holder: "notes" });
  await expect(page.locator(".toast")).toContainText("Added to website's Desk");
  // Now it holds it: the row says so, and the button's gone.
  await expect(page.locator('.ib-row[data-inbox-open="notes"] .ib-for')).toHaveText("website · for Publish the draft now, or wait for review?");
  await expect(page.locator("[data-inbox-add-desk]")).toHaveCount(0);
  expect(await page.locator(".inbox [title], .inbox-page [title]").count()).toBe(0);
});

test("Open in Desk picks the project, goes to the Desk and opens the item (when the Desk is there)", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await openInbox(page);
  // A stand-in for the Desk view, as ui/desk.js gives it.
  await page.evaluate(() => { window.__opened = []; window.deskOpenItem = (id) => window.__opened.push(id); });
  await page.locator('.ib-row[data-inbox-open="web"]').click();
  await page.locator(".ib-forbar .ib-desk").click();
  expect(await page.evaluate(() => [view, deskSelection(), window.__opened])).toEqual(["desk", "cue", ["d7k2"]]);
});

test("Open in Desk from the real Desk view opens the item on the right", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await openInbox(page);
  await page.locator('.ib-row[data-inbox-open="web"]').click();
  await page.locator(".ib-forbar .ib-desk").click();
  await expect(page.locator(".dk-right .dk-pg h1")).toHaveText("Bug #d7k2 · Reply box loses focus after Send");
  await expect(page.locator('[data-desk-item="d7k2"]')).toHaveClass(/\bon\b/);
});

test("without a Desk view, Open in Desk says so and stays in the Inbox", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await openInbox(page);
  // The Desk stubbed away (a top-level function can't be deleted, only overwritten), so this runs with ui/desk.js too.
  await page.evaluate(() => { window.deskOpenItem = undefined; });
  await page.locator('.ib-row[data-inbox-open="web"]').click();
  await page.locator(".ib-forbar .ib-desk").click();
  expect(await page.evaluate(() => view)).toBe("inbox");
  await expect(page.locator(".inbox-page")).toHaveAttribute("data-inbox-page", "web");
});

test("an item held in a project with no name: its 'for' line without a project, and no Open in Desk", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await cue.change((st) => { st.desk.projects[0].items[0].project = ""; st.desk.projects[0].project = ""; });
  await openInbox(page);
  await expect(page.locator('.ib-row[data-inbox-open="web"] .ib-for')).toHaveText("for Reply box loses focus after Send");
  await expect(page.locator('.ib-row[data-inbox-open="web"] .ib-pj')).toHaveCount(0);
  await page.locator('.ib-row[data-inbox-open="web"]').click();
  await expect(page.locator(".ib-forbar")).toHaveText("for Reply box loses focus after Send");
  await expect(page.locator(".ib-forbar .ib-desk")).toHaveCount(0);
});

for (const [what, desk] of [["no state.desk at all", undefined], ["an empty desk.projects", { projects: [] }]]) {
  test(`${what}: the lead's Desk ask is still tagged (by its own project); no 'for' lines on the others`, async ({ cue, page }) => {
    await cue.open("desk-inbox");
    await cue.change((st, d) => { if (d) st.desk = d; else delete st.desk; }, desk);
    await openInbox(page);
    expect((await layout(page)).map(([t]) => t)).toEqual(["Needs you"]);
    await expect(page.locator('.ib-row[data-inbox-open="cue-lead"] .ib-for')).toHaveText("cue · Desk");
    await expect(page.locator('.ib-row[data-inbox-open="web"] .ib-for, .ib-row[data-inbox-open="notes"] .ib-for')).toHaveCount(0);
  });
}

test("a lead's Desk ask is answered as any other: its choice goes through respond", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await openInbox(page);
  await page.locator('.ib-row[data-inbox-open="cue-lead"]').click();
  await expect(page.locator(".inbox-page")).toHaveAttribute("data-inbox-page", "cue-lead");
  await page.locator(".inbox-page button", { hasText: "Wait for web-lead" }).first().click();
  const call = await cue.called("respond");
  expect(call.args.id).toMatch(/1$/);
});

test("the shot: a lead's Desk ask among the others", async ({ cue, page }) => {
  await cue.open("desk-inbox", { frozen: true });
  await openInbox(page);
  await page.locator('.ib-row[data-inbox-open="cue-lead"]').click();
  await page.locator(".ib-forbar").waitFor();
  await page.screenshot({ path: `${process.env.TMPDIR || "/tmp"}/cue-inbox-desk.png` });
  await page.locator('.ib-row[data-inbox-open="notes"]').click();
  await page.locator(".ib-head [data-inbox-add-desk]").waitFor();
  await page.screenshot({ path: `${process.env.TMPDIR || "/tmp"}/cue-inbox-desk-add.png` });
});
