// A Board session's Files tab (ui/diff.js) is the Desk's Files view: its file count on the tab, the Desk's list
// (biggest first, three then "and N more files ›"), a click opening the file over the page with ↑ ↓ through this
// tab's files. Its four scopes stay, as a small choice over the list; a folder full of new files is one row in it.
import { test, expect } from "./cue.mjs";

const openCard = (page, id) => page.locator(`.nrow[data-big="${id}"]`).click();
const tab = (page) => page.locator('.active-pane .stab[data-stab=":diff"]');

test("the tab is named Files with its count; the list is the Desk's, biggest first; a file opens over the page, ↑ ↓ step, Esc closes", async ({ cue, page }) => {
  await cue.open("board-waiting", { flags: "diff=1" });
  await openCard(page, "n1");
  await expect(tab(page)).toHaveText("Files2");
  await expect(tab(page).locator(".stab-n")).toHaveText("2");
  await tab(page).click();
  const list = page.locator(".dv-body .dk-pfiles");
  await expect(list.locator("h4")).toHaveText("This session's edits · 2");
  // src/board.ts +3 −1 is bigger than src/fmt.ts +3.
  await expect(list.locator(".ib-file code")).toHaveText(["src/board.ts", "src/fmt.ts"]);
  await expect(list.locator(".ib-file").nth(1).locator(".ib-new")).toHaveText("new");
  await expect(list.locator(".ib-file").first().locator(".ib-nums")).toHaveText("+3 −1");
  await expect(page.locator(".dv-body details"), "no diffs stacked in the tab").toHaveCount(0);
  await list.locator(".ib-file").nth(1).click();
  const ov = page.locator(".dov");
  await expect(ov.locator(".dov-fh .dov-p")).toHaveText("src/fmt.ts");
  await expect(ov.locator(".dov-sub")).toHaveText("This session's edits");
  await expect(ov.locator(".dov-f .dov-p")).toHaveText(["src/board.ts", "src/fmt.ts"]);
  await page.keyboard.press("ArrowUp");
  await expect(ov.locator(".dov-fh .dov-p")).toHaveText("src/board.ts");
  await expect(ov.locator(".dov-lines .a", { hasText: "fmt(total)" })).toBeVisible();
  await page.keyboard.press("ArrowDown");
  await expect(ov.locator(".dov-fh .dov-p")).toHaveText("src/fmt.ts");
  await page.keyboard.press("Escape");
  await expect(ov).toHaveCount(0);
  await expect(tab(page), "still on the tab").toHaveClass(/\bon\b/);
});

test("the scopes switch the list; past three files it says and N more, which opens the rest; a folder of new files is one row", async ({ cue, page }) => {
  await cue.open("board-waiting", { flags: "diff=1" });
  // Uncommitted: four changed files of different sizes, and 25 new files under dist/.
  const file = (p, n) => `diff --git a/${p} b/${p}\n--- a/${p}\n+++ b/${p}\n@@ -1 +1,${n} @@\n${Array.from({ length: n }, (_, i) => `+line ${i}`).join("\n")}\n`;
  const diff = file("a.js", 1) + file("b.js", 5) + file("c.js", 2) + file("d.js", 9);
  const untracked = Array.from({ length: 25 }, (_, i) => `dist/out/${i}.js`).join("\n") + "\n";
  await cue.override("diff_git", ({ op, folder }, d) => op === "edited" ? { files: [], ms: 1 }
    : op === "toplevel" ? { code: 0, stdout: `${folder}\n` }
    : op === "diff" ? { code: 0, stdout: d.diff } : op === "untracked" ? { code: 0, stdout: d.untracked } : { code: 0, stdout: "" }, { diff, untracked });
  await openCard(page, "n1");
  await tab(page).click();
  // No edits with its edit tools: it falls to its folder, Uncommitted when there's no turn to read from.
  const seg = page.locator(".dv-seg");
  await expect(seg.locator("button")).toHaveText(["This session's edits", "This turn", "Whole session", "Uncommitted"]);
  await expect(page.locator(".dv-refresh")).toBeVisible();
  await expect(seg.locator("button.on")).toHaveText("Uncommitted");
  const list = page.locator(".dv-body .dk-pfiles");
  await expect(list.locator("h4")).toHaveText("Uncommitted · 4");
  await expect(list.locator("button.ib-file code")).toHaveText(["d.js", "b.js", "c.js"]);
  await expect(list.locator(".dv-bulk code")).toHaveText("dist/out/");
  await expect(list.locator(".dv-bulk .ib-new")).toHaveText("25 new files");
  await list.locator(".ib-link").click();
  await expect(page.locator(".dov-fh .dov-p"), "and 1 more opens the fourth").toHaveText("a.js");
  await expect(page.locator(".dov-f .dov-p")).toHaveText(["d.js", "b.js", "c.js", "a.js"]);
  await page.keyboard.press("Escape");
  // The session's own edits, picked: none, so the list goes.
  await seg.locator("button", { hasText: "This session's edits" }).click();
  await expect(seg.locator("button.on")).toHaveText("This session's edits");
  await expect(page.locator(".dv-body .ib-file")).toHaveCount(0);
  await expect(page.locator(".dv-body .dv-note")).toHaveText("It hasn't edited any files yet.");
  await seg.locator("button", { hasText: "Uncommitted" }).click();
  await expect(list.locator("button.ib-file code")).toHaveText(["d.js", "b.js", "c.js"]);
});
