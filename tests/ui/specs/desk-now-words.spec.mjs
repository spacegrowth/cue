// The item list's Now column (#810, option C): a few plain words on every row, the full line only on the open one.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";
import { statusOf, fullOf } from "./desk.mjs";

/** Item `id` of the cue Desk changed by `patch` (merged in). */
const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
const MIN = 60000;

test("a command its holder runs reads as what it does; the exact command and what's next are on the open row only", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  await patchItem(cue, "d3", { act: { at_ms: NOW - 2 * MIN, now: "Running: git diff --cached --stat" } });
  await expect(statusOf(page, "d3")).toHaveText(/^Looking at changes · \S+$/);
  await expect(statusOf(page, "d3")).not.toHaveClass(/\bmono\b/);
  const row = page.locator('.dkn-items .dkn-row[data-desk-item="d3"]'), line = row.locator(".dkn-xl");
  // Not open: its full line isn't shown, and the row is as tall as the rest.
  await expect(line).toBeHidden();
  await row.locator(".dkn-tt").click();
  await expect(row).toHaveClass(/\bon\b/);
  await expect(line).toBeVisible();
  await expect(fullOf(page, "d3")).toHaveText(/^Running: git diff --cached --stat · \S+$/);
  await expect(line.locator(".dkn-xn")).toHaveText("Next: tests");
  // Another row stays one line of Now, its full line hidden.
  await expect(page.locator('.dkn-items .dkn-row[data-desk-item="d5"] .dkn-xl')).toBeHidden();
});

test("an item whose session went back to its prompt reads Quiet, amber after 5 minutes, never Thinking", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  const quiet = (mins) => cue.change((st, t) => { const s = st.sessions.find((x) => x.session_id === "host"); s.state = "waiting"; s.since_ms = t; s.doing = "Thinking…"; }, NOW - mins * MIN);
  await quiet(2);
  await expect(statusOf(page, "d5")).toHaveText(/^Quiet · \S+$/);
  await expect(statusOf(page, "d5")).not.toHaveClass(/\bamber\b/);
  await quiet(14);
  await expect(statusOf(page, "d5")).toHaveText(/^Quiet · \S+$/);
  await expect(statusOf(page, "d5")).toHaveClass(/\bamber\b/);
  await expect(page.locator('.dkn-items [data-desk-item="d5"] .dkn-now')).not.toContainText("Thinking");
});

test("waiting rows say what they wait on in a few words; the full line has it whole", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  await patchItem(cue, "d3", { waiting: "the test run on main to finish" });
  await expect(statusOf(page, "d3")).toHaveText("Waiting");
  await expect(fullOf(page, "d3")).toHaveText("Waits: the test run on main to finish");
  // Waiting isn't moving: no "Next:" on its line.
  await expect(page.locator('.dkn-items [data-desk-item="d3"] .dkn-xn')).toHaveCount(0);
  await patchItem(cue, "d3", { waiting: "", state: "finishing", finish: { step: "line", at: NOW - 3 * MIN } });
  await expect(statusOf(page, "d3")).toHaveText("Waits to land");
  await expect(fullOf(page, "d3")).toContainText("In the landing line");
});
