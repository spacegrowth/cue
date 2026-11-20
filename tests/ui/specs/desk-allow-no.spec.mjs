// Permission cards (#426): No and Allow sit side by side on one line at the right, No first, the ask beside them (or
// above them when narrow); on the Desk's item list, its Progress view, and the item page's / session chat's card.
// A long command shows its first lines with more, which opens the rest in place, and less, which folds it back.
import { test, expect, openDesk } from "./cue.mjs";

const CMD = "node node_modules/@playwright/test/cli.js test --config tests/ui/playwright.config.mjs --grep 'a Desk can start in a folder|three columns'";

async function setup(cue, page, cmd = CMD) {
  await cue.open("desk");
  await cue.change((st, command) => {
    const worker = st.sessions.find((s) => s.session_id === "host");
    worker.harness = "codex"; worker.hosted = true; worker.state = "deciding";
    const d = st.desk.projects.find((x) => x.project === "cue");
    for (const item of d.items.filter((i) => i.holder?.sid === "host")) { item.state = "working"; item.now_on = false; item.ask = null; }
    Object.assign(d.items.find((item) => item.holder?.sid === "host"), { id: "227", kind: "feature", title: "Give the Desk its own icon" });
    st.items.push({ id: "perm", session_id: "host", harness: "codex", hosted: true, project: "cue", kind: "permission", status: "pending", tool_name: "Bash", tool_input: { command }, suggestions: [], created_ms: Date.now(), thread: [{ role: "you", text: "Desk item #227 (feature): Give the Desk its own icon" }] });
  }, cmd);
  await openDesk(page);
}

/** Where the card's ask, No and Allow sit, measured at one moment (the Desk redraws its rows): what's wrong, or "ok". */
const layout = (page, box, no, yes, ask) => page.evaluate(([box, no, yes, ask]) => {
  const b = document.querySelector(box), r = (s) => b?.querySelector(s)?.getBoundingClientRect();
  const [B, N, Y, Q] = [b?.getBoundingClientRect(), r(no), r(yes), ask && r(ask)];
  if (!B || !N || !Y || (ask && !Q)) return "missing";
  if (Math.abs(N.top - Y.top) > 2) return "stacked";
  if (N.right > Y.left) return "Allow before No";
  if (B.right - Y.right > 24) return "not at the right";
  if (!Q) return "ok";
  return Q.right <= N.left && Q.width > 120 ? "ok:beside" : Q.bottom <= N.top + 1 ? "ok:above" : "ask overlaps";
}, [box, no, yes, ask]);

const ROW = '.dk-items [data-desk-item="227"] .dk-ask-row';
const NO = '[data-desk-decision="deny"]', YES = '[data-desk-decision="allow"]';

test("the item list's permission: No then Allow on one line at the right, the ask to their left (#426)", async ({ cue, page }) => {
  await setup(cue, page);
  await expect(page.locator(`${ROW} [data-desk-decision]`)).toHaveText(["No", "Allow"]);
  await expect.poll(() => layout(page, ROW, NO, YES, ".dk-h")).toBe("ok:beside");
});

test("on a narrow list the ask goes above No and Allow, still side by side (#426)", async ({ cue, page }) => {
  await setup(cue, page);
  await page.addStyleTag({ content: `${ROW} { width: 200px; }` });
  await expect.poll(() => layout(page, ROW, NO, YES, ".dk-h")).toBe("ok:above");
});

test("Progress puts No before Allow on one line (#426)", async ({ cue, page }) => {
  await setup(cue, page);
  await page.locator("[data-desk-prog-toggle]").click();
  const cell = '[data-desk-prog-row="227"] .dk-grb';
  await expect(page.locator(`${cell} [data-desk-decision]`)).toHaveText(["No", "Allow"]);
  await expect.poll(() => layout(page, cell, NO, YES)).toBe("ok");
});

test("the item page's card puts Deny and Allow on one line, Allow at the right (#426)", async ({ cue, page }) => {
  await setup(cue, page);
  await page.locator('.dk-items [data-desk-item="227"]').click();
  await expect.poll(() => layout(page, ".dk-right .act-card", '[data-act="deny"]', '[data-act="allow"]')).toBe("ok");
});

// One long command (the list row sums up a chain of them as "run x and y", which is short).
const LONG = `node scripts/check.mjs ${Array.from({ length: 40 }, (_, n) => `--case=tests/fixtures/case-${n}.json`).join(" ")}`;

test("a long command shows a few lines with more; more opens the rest in place, less folds it back (#426)", async ({ cue, page }) => {
  await setup(cue, page, LONG);
  // The list row: two lines and more; more opens the words, not the item.
  const row = page.locator(ROW);
  // How many lines its words take.
  const lines = () => row.locator(".dk-h").evaluate((e) => Math.round(e.getBoundingClientRect().height / parseFloat(getComputedStyle(e).lineHeight)));
  await expect.poll(lines).toBe(2);
  await row.getByRole("button", { name: "more", exact: true }).click();
  await expect(row.getByRole("button", { name: "less", exact: true })).toBeVisible();
  await expect.poll(lines).toBeGreaterThan(4);
  await expect(page.locator(".dk-right h1")).not.toContainText("#227");
  await row.getByRole("button", { name: "less", exact: true }).click();
  await expect.poll(lines).toBe(2);
  await expect.poll(() => layout(page, ROW, NO, YES, ".dk-h")).toBe("ok:beside");
  // The item page's card: three lines of the command, more for the rest.
  await page.locator('.dk-items [data-desk-item="227"]').click();
  const card = page.locator(".dk-right .act-card");
  const pre = () => card.locator("pre.slab").evaluate((e) => e.getBoundingClientRect().height);
  const cut = await pre();
  expect(cut).toBeLessThan(90);
  await card.getByRole("button", { name: "more", exact: true }).click();
  await expect(card.getByRole("button", { name: "less", exact: true })).toBeVisible();
  expect(await pre()).toBeGreaterThan(cut * 2);
  await card.getByRole("button", { name: "less", exact: true }).click();
  await expect.poll(pre).toBeLessThan(cut + 1);
  await expect.poll(() => layout(page, ".dk-right .act-card", '[data-act="deny"]', '[data-act="allow"]')).toBe("ok");
});

test("a short command shows in full, with no more (#426)", async ({ cue, page }) => {
  await setup(cue, page, "npm test");
  await expect(page.locator(`${ROW} .dk-h`)).toContainText("npm test");
  await expect(page.locator(`${ROW} [data-req-more]`)).toHaveCount(0);
  await page.locator('.dk-items [data-desk-item="227"]').click();
  await expect(page.locator(".dk-right .act-card pre.slab")).toContainText("npm test");
  await expect(page.locator(".dk-right .act-card [data-req-more]")).toHaveCount(0);
});
