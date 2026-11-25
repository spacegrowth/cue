// The item list (#896, #810's option B, its only look): under each title a sentence says what the item is doing and
// what's next, between the title and its #id line; no Now column, rows three lines tall; Desk settings has no switch.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";

const MIN = 60000;
/** Item `id` of the cue Desk changed by `patch` (merged in). */
const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
const sentence = (page, id) => page.locator(`.dkn-items .dkn-row[data-desk-item="${id}"] .dkn-full`);
const open = async (page, cue, size = { width: 1440, height: 900 }, frozen = false) => {
  await page.setViewportSize(size);
  await cue.open("desk", frozen ? { frozen: true } : undefined);
  await openDesk(page);
  await expect(page.locator(".dk-board.dk-c")).toHaveCount(1);
};

test("every row says its sentence under its title; no Now column, and Desk settings has no switch for it", async ({ page, cue }) => {
  await open(page, cue);
  const card = page.locator(".dkn-items .dkn-card");
  await expect(card.locator(".dkn-th > span")).toHaveText(["Item and now", "", "Progress", ""]);
  await expect(card.locator(".dkn-now, .dkn-xl")).toHaveCount(0);
  const rows = card.locator(".dkn-row[data-desk-item]");
  const n = await rows.count();
  expect(n).toBeGreaterThan(2);
  // Every row: its title, then the sentence, then its #id line.
  await expect(card.locator(".dkn-row[data-desk-item] .dkn-t > .dkn-fl .dkn-full")).toHaveCount(n);
  expect(await rows.first().locator(".dkn-t > *").evaluateAll((els) => els.map((el) => el.className))).toEqual(["dkn-tt", "dkn-fl", "dkn-sub"]);
  // A working item: where it stands, then what's next.
  await expect(sentence(page, "d3")).toContainText("Next:");
  // Three lines tall: title, sentence, #id line.
  expect(await rows.first().evaluate((el) => el.getBoundingClientRect().height)).toBeGreaterThanOrEqual(3 * 18);
  // Ready to check has its ✓ in its own column (#915), not in the sentence.
  await expect(card.locator('.dkn-row[data-desk-item="d2"] > .dkn-okc > [data-desk-act="looks_right"]')).toBeVisible();
  await expect(card.locator('.dkn-fl .dk-ok')).toHaveCount(0);
  await expect(page.locator(".dkn-items [title]")).toHaveCount(0);
  // Desk settings: no Item list part.
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator(".top .hv-menu [data-hv-desk-set]").click();
  await page.locator(".ld-dialog").waitFor();
  await expect(page.locator('.ld-dialog [data-ld-sec="look"], .ld-dialog [data-ld-look]')).toHaveCount(0);
  await expect(page.locator(".ld-dialog")).not.toContainText(/Short words|Sentence under the title/);
});

test("light and dark, a laptop and the smallest window: the sentence stays under the title and nothing spills sideways", async ({ page, cue }) => {
  await open(page, cue);
  const row = page.locator(".dkn-items .dkn-row[data-desk-item]").first();
  for (const [size, scheme] of [[{ width: 1440, height: 900 }, "light"], [{ width: 1440, height: 900 }, "dark"], [{ width: 900, height: 600 }, "light"], [{ width: 900, height: 600 }, "dark"]]) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize(size);
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(size.width);
    const [t, f, s] = await Promise.all([".dkn-tt", ".dkn-full", ".dkn-sub"].map((q) => row.locator(q).boundingBox()));
    expect(f.y).toBeGreaterThanOrEqual(t.y + t.height - 1);
    expect(s.y).toBeGreaterThanOrEqual(f.y + f.height - 1);
    expect(f.width).toBeGreaterThan(100);
    await page.locator(".dkn-items").screenshot({ path: `tests/ui/shots/desk-item-list-${size.width}-${scheme}.png` });
  }
});

test("every row's ✓ sits in one spot and every strip starts and ends at the same place, short sentence or long (#915)", async ({ page, cue }) => {
  await open(page, cue, undefined, true);
  // Two rows ready to check, one with a short sentence and one whose sentence takes two lines.
  const at = await page.evaluate(() => now() - 30 * 60000);
  await patchItem(cue, "d3", { state: "to_check", finish: { step: "checked", at, commit: "c71e0b4", try: "open it" } });
  await patchItem(cue, "d2", { finish: { step: "checked", at, commit: "c71e0b4", try: "in the box, type a long line, send it, and wait for the lead to answer it in its own chat, then open the item" } });
  const card = page.locator(".dkn-items .dkn-card");
  await expect(card.locator(".dkn-row[data-desk-item] .dk-ok")).toHaveCount(2);
  for (const [size, scheme] of [[{ width: 1440, height: 900 }, "light"], [{ width: 1440, height: 900 }, "dark"], [{ width: 900, height: 600 }, "light"], [{ width: 900, height: 600 }, "dark"]]) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize(size);
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(size.width);
    const at = await card.locator(".dkn-row[data-desk-item]").evaluateAll((rows) => rows.map((r) => {
      const ok = r.querySelector(":scope > .dkn-okc").getBoundingClientRect(), ks = r.querySelector(".dkn-ks").getBoundingClientRect();
      const kp = [...r.querySelectorAll(".dkn-kp")].map((e) => e.getBoundingClientRect());
      // A row not started yet has only stubs for steps to come: nothing to fill.
      const begun = !!r.querySelector(".dkn-kp:not(.todo)");
      return { ok: Math.round(ok.left), ks: [Math.round(ks.left), Math.round(ks.right)], fill: begun ? Math.round(kp[kp.length - 1].right) : Math.round(ks.right) };
    }));
    expect(new Set(at.map((a) => a.ok)).size).toBe(1);
    expect(new Set(at.map((a) => a.ks.join())).size).toBe(1);
    // Every begun strip's pieces fill it to its end.
    for (const a of at) expect(Math.abs(a.fill - a.ks[1])).toBeLessThanOrEqual(1);
    const oks = await card.locator(".dkn-row[data-desk-item] .dk-ok").evaluateAll((els) => els.map((e) => Math.round(e.getBoundingClientRect().left)));
    expect(new Set(oks).size).toBe(1);
    await card.locator('.dkn-row[data-desk-item="d2"]').scrollIntoViewIfNeeded();
    await page.locator(".dkn-items").screenshot({ path: `tests/ui/shots/desk-item-list-aligned-${size.width}-${scheme}.png` });
  }
});

test("a command its session runs is in the sentence in full, with what's next; the open row grows nothing", async ({ page, cue }) => {
  await open(page, cue, undefined, true);
  await patchItem(cue, "d3", { act: { at_ms: NOW - 2 * MIN, now: "Running: git diff --cached --stat" } });
  await expect(sentence(page, "d3")).toHaveText(/^Running: git diff --cached --stat · \S+\. Next: tests$/);
  const row = page.locator('.dkn-items .dkn-row[data-desk-item="d3"]');
  const before = await row.boundingBox();
  await row.locator(".dkn-tt").click();
  await expect(row).toHaveClass(/\bon\b/);
  expect((await row.boundingBox()).height).toBe(before.height);
});

test("an item whose session went back to its prompt reads Quiet, amber after 5 minutes, never Thinking", async ({ page, cue }) => {
  await open(page, cue, undefined, true);
  const quiet = (mins) => cue.change((st, t) => { const s = st.sessions.find((x) => x.session_id === "host"); s.state = "waiting"; s.since_ms = t; s.doing = "Thinking…"; }, NOW - mins * MIN);
  await quiet(2);
  await expect(sentence(page, "d5")).toHaveText(/^Quiet · \S+: its session is back at its prompt; open its chat to nudge it$/);
  await expect(sentence(page, "d5")).not.toHaveClass(/\bamber\b/);
  await quiet(14);
  await expect(sentence(page, "d5")).toHaveClass(/\bamber\b/);
  await expect(sentence(page, "d5")).not.toContainText("Thinking");
});

test("a waiting row says what it waits on in full, with no Next while it doesn't move", async ({ page, cue }) => {
  await open(page, cue, undefined, true);
  await patchItem(cue, "d3", { waiting: "the test run on main to finish" });
  await expect(sentence(page, "d3")).toHaveText("Waits: the test run on main to finish");
  await patchItem(cue, "d3", { waiting: "", state: "finishing", finish: { step: "line", at: NOW - 3 * MIN } });
  await expect(sentence(page, "d3")).toContainText("In the landing line");
});
