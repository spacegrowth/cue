// The dispatcher's Away tab (#416): beside Decisions and Dispatcher Actions, each past Away stretch, newest first, folded
// to one line (when, how long, spend, counts); open, the items that moved, the lead's calls with Undo while it can
// still be undone, and the steps held for you, allowed or refused.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";

const H = 60 * 60 * 1000;

test("past Away stretches in their own tab, newest first, each one line opening to what moved, the calls and what was held (#416)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1500, height: 900 });
  await cue.open("desk");
  await openDesk(page);
  const tabs = page.locator(".dk-right .dk-ptabs .dk-tab");
  await expect(tabs).toHaveText([/./, "Decisions", "Activity", "Away"].map((t, k) => (k === 1 ? /^Decisions/ : t)));
  await page.locator('.dk-right .dk-tab[data-desk-tab="away"]').click();
  await expect(page.locator(".dk-away-log .dk-empty")).toContainText("Nothing yet. Each time Away goes off");

  const late = { id: "away-2", desk: "cue", since: NOW - 10 * H, end: NOW - 1 * H - 10 * 60 * 1000, until: null, budget: 10, spent: 3.4, ended: "",
    items: [
      { id: "d3", kind: "bug", title: "Search stays centred", from: "working", to: "done" },
      { id: "x9", kind: "task", title: "Gone since", from: "", to: "to_check" },
    ],
    calls: [
      { id: "k1", item_id: "d5", text: "Header first", why: "it blocks the rest", at: Math.floor((NOW - 8 * H) / 1000), undo: "can" },
      { id: "k2", item_id: null, text: "Kept the old name", why: "renaming touches tests", at: Math.floor((NOW - 7 * H) / 1000), undo: "none" },
    ],
    held: [
      { id: "h1", who: "web", what: "push", command: "git push origin main", at: NOW - 6 * H, answer: "allow" },
      { id: "h2", who: "web", what: "delete files", command: "rm -rf build", at: NOW - 5 * H, answer: "no" },
      { id: "h3", who: "site", what: "release", command: "scripts/release.sh 0.2.0", at: NOW - 4 * H, answer: "" },
    ] };
  const early = { id: "away-1", desk: "cue", since: NOW - 30 * H, end: NOW - 29 * H, until: NOW - 29 * H, budget: null, spent: 0, ended: "until", items: [], calls: [], held: [] };
  await cue.change((st, log) => { st.desk.projects.find((p) => p.project === "cue").away_log = log; }, [late, early]);

  const rows = page.locator(".dk-away-log .dk-al");
  await expect(rows).toHaveCount(2);
  // Folded, one line each: when, how long, spend, counts; the older one says it ran to its end time.
  await expect(rows.nth(0).locator(".dk-al-h")).toHaveText(/^›(\w{3} \d+, )?\d\d:\d\d – \d\d:\d\d 8h 50m · \$3\.40 · 2 moved · 2 calls · 3 held$/);
  await expect(rows.nth(1).locator(".dk-al-h")).toHaveText(/^›\w{3} \d+, \d\d:\d\d – \d\d:\d\d 1h · nothing happened · ran to its end time$/);
  await expect(rows.locator(".dk-aw-list")).toHaveCount(0);
  // Each tab on one line, though there are four now.
  for (const t of await tabs.all()) expect((await t.boundingBox()).height).toBeLessThan(40);

  await rows.nth(0).locator(".dk-al-h").click();
  await expect(rows.nth(0).locator(".dk-al-h")).toHaveAttribute("aria-expanded", "true");
  const list = rows.nth(0).locator(".dk-aw-list");
  await expect(list.locator(".dk-al-s")).toHaveText(["Items", "Calls", "Held for you"]);
  // Items that moved, in plain words; one still on the Desk opens, one gone doesn't.
  await expect(list.locator(".dk-al-row").nth(0)).toHaveText("#d3 Search stays centred · working → done");
  await expect(list.locator(".dk-al-row").nth(1)).toHaveText("#x9 Gone since · added · ready to check");
  await expect(list.locator('button.dk-al-row[data-desk-chk-item="d3"]')).toHaveCount(1);
  await expect(list.locator('[data-desk-chk-item="x9"]')).toHaveCount(0);
  // Calls: Undo only while it can still be undone.
  await expect(list.locator('[data-desk-call="k1"]')).toHaveText(/^\d\d:\d\dHeader first · Bug #d5 · it blocks the rest\.?Undo$/);
  await expect(list.locator('[data-desk-call="k2"] [data-desk-undo]')).toHaveCount(0);
  // Held: allowed, refused, or still waiting on you; the command as it was.
  const held = list.locator(".dk-al-held");
  await expect(held.nth(0)).toContainText("web wanted to push · Allowed");
  await expect(held.nth(0).locator(".dk-al-cmd")).toHaveText("git push origin main");
  await expect(held.nth(1)).toContainText("web wanted to delete files · Refused");
  await expect(held.nth(2)).toContainText("site wanted to release · Not answered yet: under Waiting on you");
  // No hover text, no left stripe.
  await expect(page.locator(".dk-away-log [title]")).toHaveCount(0);
  const stripe = await rows.nth(0).evaluate((e) => getComputedStyle(e).borderLeftWidth);
  expect(stripe).toBe("0px");
  if (process.env.SHOT_DIR) await page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-away-log.png` });

  await list.locator('[data-desk-undo="k1"]').click();
  await cue.called("desk_undo", { decisionId: "k1" });
  // Folds again.
  await rows.nth(0).locator(".dk-al-h").click();
  await expect(rows.nth(0).locator(".dk-aw-list")).toHaveCount(0);
});
