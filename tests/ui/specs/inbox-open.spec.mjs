// Open on the Inbox side (ui/inbox_open.js): what a crew left open, read by Cue (hub/open.rs `state.open`), under
// Running now. One quiet button a row; only a row waiting on you is red; rows keep their seats. Scenario: open.
import { test, expect } from "./cue.mjs";

/** The Inbox, with the lead's page open beside it. */
const openLead = async (page) => {
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('.ib-row[data-inbox-open="cue-lead"]').click();
  await page.locator(".ib-open").waitFor();
};
const rows = (page) => page.locator(".ib-open .ib-op");
const titles = (page) => page.locator(".ib-open .ib-op-w").allTextContents();

test("Open N sits under Running now: each row is one short line, what's open · whose it is, and one button", async ({ cue, page }) => {
  await cue.open("open");
  await openLead(page);
  const side = page.locator(".ib-aside");
  const heads = await side.locator(":scope > .ib-blk > .ib-k").allTextContents();
  const at = heads.findIndex((h) => h.startsWith("Open"));
  expect(heads[at - 1]).toMatch(/^(Running now|Its crew)/);
  await expect(page.locator(".ib-open > .ib-k")).toHaveText(/^Open · \d+$/);
  const t = await titles(page);
  expect(t.slice(0, 4)).toEqual([
    "Rail dot can start now in a new session, or wait 10m for web-lead. Which?",
    "Merge stopped at 3 conflicts",
    "1 teammate done, not committed",
    "Review not sent",
  ]);
  // #220: whose it is is a name, no files or lines; the review's note waits for Open or Send fixes.
  expect((await page.locator(".ib-open .ib-op-o").allTextContents()).slice(0, 4)).toEqual(["cue-lead", "desk-merge", "desk-ui", "desk-core"]);
  // One line each, however long: what doesn't fit ends in an ellipsis.
  await page.evaluate(() => { state.open[0].rows[0].title = "A very long question ".repeat(12); state.open[0].rows[0].item = ""; renderMain(); });
  const lines = await rows(page).evaluateAll((els) => els.map((el) => [el.getBoundingClientRect().height, getComputedStyle(el.querySelector(".ib-op-w")).textOverflow]));
  expect(lines.every(([h, t]) => h < 40 && t === "ellipsis")).toBe(true);
  expect((await page.locator(".ib-open .ib-op-act").allTextContents()).slice(0, 4)).toEqual(["Reply", "Open", "Open", "Send fixes"]);
  expect(await page.locator(".ib-open [title]").count()).toBe(0);
  // Under the pointer a button lights up, so you know a click lands.
  const btn = page.locator(".ib-open button.ib-op-act").nth(1), bg = () => btn.evaluate((el) => getComputedStyle(el).backgroundColor);
  const still = await bg();
  await btn.hover();
  await expect.poll(bg).not.toBe(still);
});

test("only the row waiting on you is red", async ({ cue, page }) => {
  await cue.open("open");
  await openLead(page);
  const red = await rows(page).evaluateAll((els) => els.map((el) => {
    const who = getComputedStyle(el.querySelector(".ib-op-o")).color;
    const deny = getComputedStyle(document.documentElement).getPropertyValue("--deny").trim();
    const probe = document.createElement("span");
    probe.style.color = deny;
    document.body.append(probe);
    const want = getComputedStyle(probe).color;
    probe.remove();
    return who === want;
  }));
  expect(red[0]).toBe(true);
  expect(red.slice(1).every((x) => !x)).toBe(true);
  // No colored edge on any row.
  const edges = await rows(page).evaluateAll((els) => els.map((el) => getComputedStyle(el).borderLeftWidth));
  expect(edges.every((w) => w === "0px")).toBe(true);
});

test("Send fixes runs the review act and the row stays in its seat as Sent; rows never reorder", async ({ cue, page }) => {
  await cue.open("open");
  await openLead(page);
  const before = await titles(page);
  await page.locator('.ib-open [data-open-act="review:cue-lead:id-core"]').click();
  await cue.called("team_action", { sessionId: "cue-lead", agent: "id-core", action: "fixes" });
  const sent = page.locator(".ib-open .ib-op.sent");
  await expect(sent).toHaveCount(1);
  await expect(sent.locator(".ib-op-w")).toHaveText("Fixes sent");
  await expect(sent.locator(".ib-op-o")).toHaveText("desk-core · just now");
  await expect(sent.locator(".ib-op-act")).toHaveText("Sent");
  const after = await titles(page);
  expect(after.length).toBe(before.length);
  expect(after.indexOf("Fixes sent")).toBe(before.indexOf("Review not sent"));
  // A new row joins at the bottom; the others keep their seats.
  await page.evaluate(() => {
    state.open[0].rows.unshift({ key: "merge:/work/cue", kind: "merge", title: "Merge not finished", who: "cue-lead · in ui/a.js", sid: "cue-lead", act: "open", needs_you: false });
    renderMain();
  });
  const later = await titles(page);
  expect(later.slice(0, after.length)).toEqual(after);
  expect(later.at(-1)).toBe("Merge not finished");
});

test("Send fixes refused: the row stays as it was, not Sent", async ({ cue, page }) => {
  await cue.open("open");
  await openLead(page);
  await cue.override("team_action", () => Promise.reject("Not sent: its review has nothing to fix"));
  const before = await titles(page);
  await page.locator('.ib-open [data-open-act="review:cue-lead:id-core"]').click();
  await expect.poll(() => page.evaluate(() => mateNotes.get("cue-lead|id-core"))).toBe("Not sent: its review has nothing to fix");
  await expect(page.locator(".ib-open .ib-op.sent")).toHaveCount(0);
  await expect(page.locator('.ib-open [data-open-act="review:cue-lead:id-core"]')).toHaveText("Send fixes");
  expect(await titles(page)).toEqual(before);
});

test("Send fixes clicked twice while it's on its way sends once", async ({ cue, page }) => {
  await cue.open("open");
  await openLead(page);
  await cue.override("team_action", () => { window.fixesSent = (window.fixesSent || 0) + 1; return new Promise((ok) => setTimeout(() => ok("fixes: id-core"), 200)); });
  const btn = page.locator('.ib-open [data-open-act="review:cue-lead:id-core"]');
  await btn.click();
  await btn.click();
  await expect(page.locator(".ib-open .ib-op.sent")).toHaveCount(1);
  expect(await page.evaluate(() => window.fixesSent)).toBe(1);
});

test("Reply opens the ask; Open opens the lead's chat on the teammate", async ({ cue, page }) => {
  await cue.open("open");
  await openLead(page);
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('.ib-open [data-open-act="merge:/work/cue-merge"]').click();
  await expect(page.locator('.ib-open')).toHaveCount(0);
  expect(await page.evaluate(() => [view, active?.sid, mateOpenLeads.has("cue-lead")])).toEqual(["board", "cue-lead", true]);
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('.ib-row[data-inbox-open="web"]').click();
  await page.locator('.ib-open [data-open-act^="ask:"]').click();
  expect(await page.evaluate(() => [view, inboxSid])).toEqual(["inbox", "cue-lead"]);
});
