// A finished mocks item says which item builds your pick, and that one where it came from (#606): on the card in Done
// and on each item's page, each number a link to the other. The same for any follow-on the lead files.
import { test, expect, openDesk } from "./cue.mjs";

const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k && i.state !== "dropped"), p); }, [id, patch]);

test("a picked mocks item in Done says which item builds your pick; each page links to the other (#606)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // The lead closed the mocks item on your pick and filed what builds it (ITEM: … | from: #d7 | picked: B).
  await patchItem(cue, "d7", { title: "Mocks: header count", picked: "B", built_as: "d6" });
  await patchItem(cue, "d6", { title: "Build: header count (option B)", builds: "d7", from_pick: "B" });
  await page.locator('.dk-items [data-desk-group="done"]').click();
  const row = page.locator('.dk-items .dk-it[data-desk-item="d7"]');
  await expect(row.locator(".dk-built")).toHaveText("You picked B on #d7; #d6 builds it");
  await expect(page.locator(".dk-items [title]")).toHaveCount(0);
  // Its number opens the build item, not the row's own page.
  await row.locator('.dk-built a[data-desk-item="d6"]').click();
  await expect(page.locator(".dk-right .dk-pg h1")).toContainText("Build: header count (option B)");
  const line = page.locator(".dk-panel[data-side-for] .dk-pnow .dk-built");
  await expect(line).toHaveText("From your pick on #d7");
  await line.locator('a[data-desk-item="d7"]').click();
  await expect(page.locator(".dk-right .dk-pg h1")).toContainText("Mocks: header count");
  await expect(line).toHaveText("You picked B on #d7; #d6 builds it");
  await expect(page.locator(".dk-right [title]")).toHaveCount(0);
  // The build item done too: the mocks item says it was built, and the build item's card in Done says where it came from.
  await patchItem(cue, "d6", { state: "done", done_at: Date.now() / 1000 });
  await expect(line).toHaveText("You picked B on #d7; #d6 built it");
  await expect(page.locator('.dk-items .dk-it[data-desk-item="d6"] .dk-built')).toHaveText("From your pick on #d7");
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    // The number stands out from its line: the accent, or underlined on the lit row, where the accent is the ground.
    const [color, around, line] = await row.locator(".dk-built a").evaluate((a) => [getComputedStyle(a).color, getComputedStyle(a.parentElement).color, getComputedStyle(a).textDecorationLine]);
    expect(color !== around || line.includes("underline"), `${theme}: the number stands out from the line`).toBe(true);
    const page2 = await page.locator(".dk-panel[data-side-for] .dk-pnow .dk-built a").evaluate((a) => [getComputedStyle(a).color, getComputedStyle(a.parentElement).color]);
    expect(page2[0], `${theme}: on the page the number is the accent`).not.toBe(page2[1]);
    if (process.env.SHOT_DIR) await expect(async () => page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-606-${theme}.png`, timeout: 2000 })).toPass();
    if (process.env.SHOT_DIR) await row.locator("xpath=..").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-606-done-${theme}.png` });
  }
});

test("a follow-on that isn't a pick says Followed by and Follows (#606)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await patchItem(cue, "d8", { built_as: "d4" });
  await patchItem(cue, "d4", { builds: "d8" });
  await page.locator('.dk-items [data-desk-group="done"]').click();
  await expect(page.locator('.dk-items .dk-it[data-desk-item="d8"] .dk-built')).toHaveText("Followed by #d4");
  // Still open: its row has no line, its page does.
  await expect(page.locator('.dk-items .dk-it[data-desk-item="d4"] .dk-built')).toHaveCount(0);
  await page.locator('.dk-items .dk-it[data-desk-item="d4"] .dk-t').click();
  await expect(page.locator(".dk-panel[data-side-for] .dk-pnow .dk-built")).toHaveText("Follows #d8");
});
