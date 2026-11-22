// An item's side panel (#394; #418: the column at the right): every line starts at the same edge, the files' counts sit
// in one column whatever's new, and pressing Send back or Looks right doesn't make it jump.
import { test, expect, openDesk } from "./cue.mjs";

const DIFF = "diff --git a/ui/a.js b/ui/a.js\nnew file mode 100644\n--- /dev/null\n+++ b/ui/a.js\n@@ -0,0 +1,12 @@\n" + "+a\n".repeat(12)
  + "diff --git a/ui/b.js b/ui/b.js\n--- a/ui/b.js\n+++ b/ui/b.js\n@@ -1,2 +1,3 @@\n-old\n+new\n+more\n same\n";
/** Where each of the panel's sections starts, top to bottom. */
// Read in one go in the page: the panel is drawn again as its holder's steps come in (#418), and nodes from the draw
// before read as 0.
const tops = (page) => page.evaluate(() => [...document.querySelectorAll(".dk-panel > .dk-psec")].map((e) => Math.round(e.getBoundingClientRect().top)));

test("an item's panel lines up and stays put when you press its buttons (#394)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1512, height: 945 });
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { const d2 = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2"); d2.files = ["ui/a.js", "ui/b.js"]; d2.see = ""; });
  await cue.override("desk_item_diff", (_, diff) => ({ machine: "", diff }), DIFF);
  await page.locator('.dk-items [data-desk-item="d2"]').click();
  const panel = page.locator(".dk-panel");
  await expect(panel.locator(".dk-pfiles .ib-file")).toHaveCount(2);
  const right = (loc) => loc.evaluateAll((els) => els.map((e) => Math.round(e.getBoundingClientRect().right)));

  // "The lead hasn't said yet" starts where its heading does.
  const note = panel.locator(".dk-pdw .dk-note");
  await expect(note).toHaveText("The lead hasn't said yet.");
  // Both read in one go: the panel is drawn again as its holder's steps come in (#418), and a node read from the draw
  // before is gone (at 0).
  const [noteX, headX] = await panel.locator(".dk-pdw").evaluate((s) => [".dk-note", ".dk-psh"].map((q) => Math.round(s.querySelector(q).getBoundingClientRect().left)));
  expect(noteX).toBe(headX);
  // (Who holds it is in This item's quiet line now (#418), no "Held by" row to line up.)
  // The files' counts in one column: "new" sits by its path, not in front of its numbers.
  await expect(panel.locator(".ib-file").first().locator(".ib-new")).toHaveText("new");
  const adds = await right(panel.locator(".ib-file .ib-add")), dels = await right(panel.locator(".ib-file .ib-del"));
  expect(new Set(adds).size).toBe(1);
  expect(new Set(dels).size).toBe(1);

  // Send back: its box opens under the thread; the panel keeps its buttons where they were (Send back pressed, Looks
  // right greyed), so nothing in it moves. Pressed again, it closes.
  const before = await tops(page);
  await panel.locator('[data-desk-act="send_back"]').click();
  await expect(panel.locator('[data-desk-act="send_back"][aria-pressed="true"]')).toHaveCount(1);
  await expect(panel.locator(".dk-pacts .btn.primary")).toBeDisabled();
  expect(await tops(page)).toEqual(before);
  await panel.locator('[data-desk-act="send_back"]').click();
  await expect(panel.locator('[data-desk-act="send_back"][aria-pressed="true"]')).toHaveCount(0);
  expect(await tops(page)).toEqual(before);

  // Looks right: its few seconds to take it back keep the buttons' row as tall as it was.
  await panel.locator(".dk-pacts .btn.primary").click();
  await expect(panel.locator(".dk-soon")).toHaveCount(1);
  expect(await tops(page)).toEqual(before);
});
