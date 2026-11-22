// Permission cards (#426): No and Allow sit side by side on one line at the right, No first, the ask beside them (or
// above them when narrow); on the Desk's Progress view, and the item page's / session chat's card. The item list's
// row only says what it asks, in its one line (B2, #666). A long command shows its first lines with more on the card,
// which opens the rest in place, and less, which folds it back.
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

const ROW = '.dk-items [data-desk-item="227"]';
const NO = '[data-desk-decision="deny"]', YES = '[data-desk-decision="allow"]';
/** The open item's card: the page's ask, where No and Allow are (B2's row has none, #666). */
const CARD = ".dk-right .act-card";
/** The row's one status line, and whether it holds on one line, cut with … when long. */
const oneLine = (page) => page.locator(`${ROW} .dk2b-ts`).evaluate((e) => [Math.round(e.getBoundingClientRect().height / parseFloat(getComputedStyle(e).lineHeight)), e.scrollWidth > e.clientWidth]);

test("the item list's permission: its row says what it asks in one line, No then Allow on its page's card (#426, #666)", async ({ cue, page }) => {
  await setup(cue, page);
  await expect(page.locator(`${ROW} .dk2b-ts`)).toContainText(/^Asks: Allow node node_modules/);
  // No answer buttons on the row: the row opens the item, its card answers.
  await expect(page.locator(`${ROW} :is(${NO}, ${YES}, button)`)).toHaveCount(0);
  await page.locator(ROW).click();
  await expect.poll(() => layout(page, CARD, '[data-act="deny"]', '[data-act="allow"]')).toBe("ok");
});

test("on a narrow list the row's ask stays one line, cut with … (#426, #666)", async ({ cue, page }) => {
  await setup(cue, page);
  await page.addStyleTag({ content: `${ROW} { width: 300px; }` });
  await expect.poll(() => oneLine(page)).toEqual([1, true]);
});

test("Progress puts No before Allow on one line (#426)", async ({ cue, page }) => {
  await setup(cue, page);
  // B2's list head has no Progress button (#666): the view opens as it did.
  await page.evaluate(() => deskProgToggle(true));
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
  // The list row: one line, cut with …; no more on the row (#666), the card has it.
  await expect.poll(() => oneLine(page)).toEqual([1, true]);
  await expect(page.locator(`${ROW} [data-req-more]`)).toHaveCount(0);
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
  await expect(page.locator(`${ROW} .dk2b-ts`)).toContainText("Asks: Allow npm test?");
  await expect(page.locator(`${ROW} [data-req-more]`)).toHaveCount(0);
  await page.locator('.dk-items [data-desk-item="227"]').click();
  await expect(page.locator(".dk-right .act-card pre.slab")).toContainText("npm test");
  await expect(page.locator(".dk-right .act-card [data-req-more]")).toHaveCount(0);
});
