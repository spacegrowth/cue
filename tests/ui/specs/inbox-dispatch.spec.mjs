// The Inbox and the Desk (ui/inbox.js): asks from a project's Dispatch under "From Dispatch" on top (a dark "D" mark,
// "<project> Dispatch"), the rest under "From sessions"; a session holding a Desk item shows "<PROJECT> · for <title>"
// on its row and, opened, a bar with Open in Desk; a Dispatch ask is answered as any other. Scenario: desk-inbox.
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

test("From Dispatch on top, then From sessions (plain, not red); the Dispatch row has its D mark and name", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await openInbox(page);
  expect(await layout(page)).toEqual([["From Dispatch 1", ["dsp-cue"]], ["From sessions 2", ["web", "notes"]]]);
  const d = page.locator('.ib-row[data-inbox-open="dsp-cue"]');
  await expect(d.locator(".ib-dmark")).toHaveText("D");
  await expect(d.locator(".ib-name")).toHaveText("cue Dispatch");
  await expect(d.locator(".ib-for")).toHaveCount(0);
  // The heading isn't red: the same color as any other heading.
  const colors = await page.locator(".ib-sect").evaluateAll((els) => els.map((e) => getComputedStyle(e).color));
  expect(new Set(colors).size).toBe(1);
  await expect(page.locator('.switch-view [data-view="inbox"] .ib-count')).toHaveText("3");
  await expect(page.locator(".ib-counts")).toHaveText("3 need you");
  // The top of the list opens by itself: the Dispatch ask, its head named for the project with Open cue's Desk.
  await expect(page.locator(".inbox-page")).toHaveAttribute("data-inbox-page", "dsp-cue");
  await expect(page.locator(".ib-head .ib-hname")).toHaveText("cue Dispatch");
  await expect(page.locator(".ib-head .ib-desk")).toHaveText("Open cue's Desk ›");
  await expect(page.locator(".ib-forbar")).toHaveCount(0);
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
  expect(await page.locator(".inbox [title], .inbox-page [title]").count()).toBe(0);
});

test("Open in Desk picks the project, goes to the Desk and opens the item (when the Desk is there)", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await openInbox(page);
  // A stand-in for the Desk view, as ui/desk.js gives it.
  await page.evaluate(() => { window.__opened = []; window.deskOpenItem = (id) => window.__opened.push(id); });
  await page.locator('.ib-row[data-inbox-open="web"]').click();
  await page.locator(".ib-forbar .ib-desk").click();
  expect(await page.evaluate(() => [view, projFilter, window.__opened])).toEqual(["desk", "cue", ["d7k2"]]);
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

test("a Dispatch session with nothing asked keeps its D mark and name under Running and Done; no project: plain 'Dispatch'", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await page.evaluate(() => localStorage.setItem("cue.inboxFold.run", "1"));
  // Its ask gone and it at work: a Running row.
  await cue.change((st) => {
    st.items = st.items.filter((i) => i.session_id !== "dsp-cue");
    Object.assign(st.sessions.find((s) => s.session_id === "dsp-cue"), { state: "working", since_ms: Date.now() });
  });
  await openInbox(page);
  expect((await layout(page)).find(([t]) => t.includes("Running"))?.[1]).toEqual(["dsp-cue"]);
  const run = page.locator('.ib-row.run[data-inbox-open="dsp-cue"]');
  await expect(run.locator(".ib-dmark")).toHaveText("D");
  await expect(run.locator(".ib-name")).toHaveText("cue Dispatch");
  // Finished: a Done row, the same mark and name. The others under plain "Needs you" (no Dispatch asks now).
  await cue.change((st) => { Object.assign(st.sessions.find((s) => s.session_id === "dsp-cue"), { state: "idle", since_ms: Date.now() }); });
  await expect(page.locator('.ib-row.done[data-inbox-open="dsp-cue"] .ib-name')).toHaveText("cue Dispatch");
  await expect(page.locator('.ib-row.done[data-inbox-open="dsp-cue"] .ib-dmark')).toHaveText("D");
  expect((await layout(page)).map(([t]) => t)).toEqual(["Needs you", "Done"]);
  // A Dispatch with no project named: just "Dispatch", no leading space.
  await cue.change((st) => {
    st.desk.projects[0].project = "";
    Object.assign(st.sessions.find((s) => s.session_id === "dsp-cue"), { project: "" });
  });
  await expect(page.locator('.ib-row[data-inbox-open="dsp-cue"] .ib-name')).toHaveText("Dispatch");
  await expect(page.locator('.ib-row[data-inbox-open="dsp-cue"] .ib-dmark')).toHaveText("D");
  // Opened: no Desk button, as there's no project's Desk to open.
  await page.locator('.ib-row[data-inbox-open="dsp-cue"]').click();
  await expect(page.locator(".ib-head .ib-hname")).toHaveText("Dispatch");
  await expect(page.locator(".ib-head .ib-desk")).toHaveCount(0);
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
  test(`${what}: a from_dispatch ask still goes under From Dispatch (named by its own project); no 'for' lines`, async ({ cue, page }) => {
    await cue.open("desk-inbox");
    await cue.change((st, d) => { if (d) st.desk = d; else delete st.desk; }, desk);
    await openInbox(page);
    expect(await layout(page)).toEqual([["From Dispatch 1", ["dsp-cue"]], ["From sessions 2", ["web", "notes"]]]);
    await expect(page.locator('.ib-row[data-inbox-open="dsp-cue"] .ib-name')).toHaveText("cue Dispatch");
    await expect(page.locator(".ib-for, .ib-forbar")).toHaveCount(0);
  });
}

test("a from_dispatch ask on a session that isn't any project's Dispatch: From Dispatch, named for its ask's project", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await cue.change((st) => { st.items.find((i) => i.session_id === "notes").from_dispatch = true; });
  await openInbox(page);
  expect(await layout(page)).toEqual([["From Dispatch 2", ["dsp-cue", "notes"]], ["From sessions 1", ["web"]]]);
  await expect(page.locator('.ib-row[data-inbox-open="notes"] .ib-name')).toHaveText("website Dispatch");
  await expect(page.locator('.ib-row[data-inbox-open="notes"] .ib-dmark')).toHaveText("D");
});

test("a Dispatch ask pinned stays under Pinned, one under Later under Later: each keeps its D mark", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await cue.change((st) => { st.sessions.find((s) => s.session_id === "dsp-cue").pinned_ms = Date.now(); });
  await openInbox(page);
  expect(await layout(page)).toEqual([["Pinned", ["dsp-cue"]], ["Needs you", ["web", "notes"]]]);
  await expect(page.locator('.ib-row[data-inbox-open="dsp-cue"] .ib-dmark')).toHaveText("D");
  await expect(page.locator(".ib-counts")).toHaveText("3 need you");
  await cue.change((st) => { Object.assign(st.sessions.find((s) => s.session_id === "dsp-cue"), { pinned_ms: 0, later_ms: Date.now() }); });
  await page.locator('.ib-sect[data-inbox-fold="later"]').waitFor();
  await page.locator('.ib-sect[data-inbox-fold="later"]').click();
  expect((await layout(page)).map(([t, rows]) => [t.replace(/[▸▾]/, "").trim(), rows])).toEqual([["Needs you", ["web", "notes"]], ["Later 1", ["dsp-cue"]]]);
  await expect(page.locator('.ib-row.later[data-inbox-open="dsp-cue"] .ib-name')).toHaveText("cue Dispatch");
});

test("next ask goes From Dispatch first, then the sessions in the list's order, and wraps; even an older Dispatch ask leads", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  // The Dispatch ask older than every session ask.
  await cue.change((st) => { st.items.find((i) => i.session_id === "dsp-cue").created_ms = Date.now() - 3600000; });
  await openInbox(page);
  expect(await page.evaluate(() => inboxNeedOrder())).toEqual(["dsp-cue", "web", "notes"]);
  const at = () => page.locator(".inbox-page").getAttribute("data-inbox-page");
  expect(await at()).toBe("dsp-cue");
  for (const sid of ["web", "notes", "dsp-cue"]) {
    await page.locator(".ib-next").click();
    await expect(page.locator(".inbox-page")).toHaveAttribute("data-inbox-page", sid);
  }
});

test("a Dispatch ask is answered as any other: its choice goes through respond", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await openInbox(page);
  await expect(page.locator(".inbox-page")).toHaveAttribute("data-inbox-page", "dsp-cue");
  await page.locator(".inbox-page button", { hasText: "Wait for web-lead" }).first().click();
  const call = await cue.called("respond");
  expect(call.args.id).toMatch(/1$/);
});

test("the shot: From Dispatch over From sessions", async ({ cue, page }) => {
  await cue.open("desk-inbox", { frozen: true });
  await openInbox(page);
  await page.screenshot({ path: `${process.env.TMPDIR || "/tmp"}/cue-inbox-dispatch.png` });
  await page.locator('.ib-row[data-inbox-open="web"]').click();
  await page.locator(".ib-forbar").waitFor();
  await page.screenshot({ path: `${process.env.TMPDIR || "/tmp"}/cue-inbox-dispatch-held.png` });
});
