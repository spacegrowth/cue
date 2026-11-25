// The item list's look, a Desk setting (#896): "Short words" (today's: a few words in the Now column, the full line on
// the open row) or "Sentence under the title" (#810's option B: what each item is doing and what's next as a sentence
// between its title and its #id line, no Now column, rows three lines tall). Saved per Desk in Cue's settings.
import { test, expect, openDesk } from "./cue.mjs";

const open = async (page, cue, size = { width: 1440, height: 900 }) => {
  await page.setViewportSize(size);
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-board.dk-c")).toHaveCount(1);
};
const settings = async (page) => {
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator(".top .hv-menu [data-hv-desk-set]").click();
  return page.locator(".ld-dialog");
};
const pickLook = async (page, name) => {
  const dlg = await settings(page);
  const seg = dlg.locator('[data-ld-sec="look"] .ld-seg');
  await seg.locator("button", { hasText: name }).click();
  await expect(seg.locator("button.on")).toHaveText(name);
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
};

test("Desk settings has the item list's look, Short words by default; the sentence look takes the Now column away and says each item's sentence under its title", async ({ page, cue }) => {
  await open(page, cue);
  // Today's look: Item, Progress and Now, no sentence.
  await expect(page.locator(".dkn-card .dkn-th > span")).toHaveText(["Item", "Progress", "Now"]);
  await expect(page.locator(".dkn-card .dkn-full")).toHaveCount(0);
  const dlg = await settings(page);
  const look = dlg.locator('[data-ld-sec="look"]');
  await expect(look.locator(".ld-h")).toHaveText("Item list");
  await expect(look.locator(".ld-seg button")).toHaveText(["Short words", "Sentence under the title"]);
  await expect(look.locator(".ld-seg button.on")).toHaveText("Short words");
  await look.locator("button", { hasText: "Sentence under the title" }).click();
  await expect(look.locator(".ld-hint")).toContainText("No Now column");
  await expect(dlg.locator(".ld-foot-t")).toHaveText("Unsaved changes");
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  await expect.poll(async () => (await cue.calls("set_setting")).filter((c) => c.args.key === "project.look").at(-1)?.args.value).toEqual({ cue: "sentence" });

  const card = page.locator(".dkn-items.dkn-sent .dkn-card");
  await expect(card).toHaveCount(1);
  await expect(card.locator(".dkn-th > span")).toHaveText(["Item and now", "Progress", ""]);
  await expect(card.locator(".dkn-now, .dkn-xl")).toHaveCount(0);
  // Every row: its title, then the sentence, then its #id line.
  const rows = card.locator(".dkn-row[data-desk-item]");
  const n = await rows.count();
  expect(n).toBeGreaterThan(2);
  await expect(card.locator(".dkn-row[data-desk-item] .dkn-t > .dkn-fl .dkn-full")).toHaveCount(n);
  const order = await rows.first().locator(".dkn-t > *").evaluateAll((els) => els.map((el) => el.className));
  expect(order).toEqual(["dkn-tt", "dkn-fl", "dkn-sub"]);
  // A working item's sentence is #810's open-row line: where it stands, then what's next.
  await expect(card.locator('.dkn-row[data-desk-item="d3"] .dkn-full')).toContainText("Next:");
  // Rows are three lines tall: title, sentence, #id line.
  const h = await rows.first().evaluate((el) => el.getBoundingClientRect().height);
  expect(h).toBeGreaterThanOrEqual(3 * 18);
  // Ready to check keeps its ✓ beside the sentence.
  const ok = card.locator('.dkn-fl [data-desk-act="looks_right"]');
  if (await ok.count()) await expect(ok.first()).toBeVisible();
  await expect(page.locator(".dkn-items [title]")).toHaveCount(0);

  // Light and dark, a laptop and the smallest window: nothing spills sideways and the sentence stays under the title.
  for (const [size, scheme] of [[{ width: 1440, height: 900 }, "light"], [{ width: 1440, height: 900 }, "dark"], [{ width: 900, height: 600 }, "light"], [{ width: 900, height: 600 }, "dark"]]) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize(size);
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(size.width);
    const [t, f, s] = await Promise.all([".dkn-tt", ".dkn-full", ".dkn-sub"].map((q) => rows.first().locator(q).boundingBox()));
    expect(f.y).toBeGreaterThanOrEqual(t.y + t.height - 1);
    expect(s.y).toBeGreaterThanOrEqual(f.y + f.height - 1);
    expect(f.width).toBeGreaterThan(100);
    await page.locator(".dkn-items").screenshot({ path: `tests/ui/shots/desk-look-${size.width}-${scheme}.png` });
  }

  // Back to Short words: today's look again.
  await page.emulateMedia({ colorScheme: "light" });
  await page.setViewportSize({ width: 1440, height: 900 });
  await pickLook(page, "Short words");
  await expect.poll(async () => (await cue.calls("set_setting")).filter((c) => c.args.key === "project.look").at(-1)?.args.value).toEqual({});
  await expect(page.locator(".dkn-items.dkn-sent")).toHaveCount(0);
  await expect(page.locator(".dkn-card .dkn-th > span")).toHaveText(["Item", "Progress", "Now"]);
  await expect(page.locator(".dkn-card .dkn-full")).toHaveCount(0);
});

// Saving to Cue's settings and reading them back is the Rust settings test's (project.look); here, what's drawn from it.
test("the look is the Desk's own: another Desk's setting leaves this one as it is", async ({ page, cue }) => {
  await open(page, cue);
  await cue.change((s) => { s.settings.project.look = { web: "sentence" }; });
  await expect(page.locator(".dkn-card .dkn-th > span")).toHaveText(["Item", "Progress", "Now"]);
  await expect(page.locator(".dkn-items.dkn-sent")).toHaveCount(0);
  await cue.change((s) => { s.settings.project.look = { web: "sentence", cue: "sentence" }; });
  await expect(page.locator(".dkn-items.dkn-sent")).toHaveCount(1);
});
