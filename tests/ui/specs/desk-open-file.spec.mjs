// A file on a card's "Files it's on" list opens in Cue's file viewer (#677), as a file named in a chat does
// (chat/reader.js openPathOrDoc, file_view.js), found from the card's own working folder; in its side panel and in its
// Files tab, by a click or the keyboard, before it has changed anything.
import { test, expect, openDesk } from "./cue.mjs";

const unchanged = (st, tree) => {
  const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d3");
  Object.assign(it, { unchanged: true, updated: Math.floor(Date.now() / 1000), tree });
};

test("a file on a card's Files it's on opens in the viewer, from its panel and its Files tab (#677)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change(unchanged, { repo: "/work/cue", path: "/work/copies/cue-1", branch: "desk/d3", base: "", slot: 1 });
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();

  // Its side panel: the file names are buttons now; a click opens the first in the viewer, from its own copy.
  const panel = page.locator(".dk-panel .dk-pfiles");
  await expect(panel.locator(".dk-psh")).toContainText("Files it's on");
  const row = panel.locator('button.ib-file[data-desk-open-file="ui/header.js"]');
  await row.click();
  const call = await cue.called("peek_file", { path: "ui/header.js" });
  expect(call.args.cwd).toBe("/work/copies/cue-1");
  const fv = page.locator(".fv");
  await expect(fv.locator(".dov-title")).toHaveText("header.js");
  await expect(fv.locator(".fv-text")).toHaveText("const a = 1;\nconst b = a + 1;");
  await page.keyboard.press("Escape");
  await expect(fv).toHaveCount(0);

  // The keyboard: focused and Enter opens it too.
  await row.focus();
  await page.keyboard.press("Enter");
  await expect(fv.locator(".dov-title")).toHaveText("header.js");
  await page.keyboard.press("Escape");
  await expect(fv).toHaveCount(0);

  // Its Files tab: the same list, each a button that opens the viewer.
  const pane = page.locator(".dk-right .dk-ipane");
  await pane.locator('[data-desk-itab="files"]').click();
  await expect(pane.locator(".dk-sec h3")).toHaveText("Files it's on");
  await pane.locator('button.dk-fc[data-desk-open-file="ui/styles/header.css"]').click();
  const css = await cue.called("peek_file", { path: "ui/styles/header.css" });
  expect(css.args.cwd).toBe("/work/copies/cue-1");
  await expect(fv.locator(".dov-title")).toHaveText("header.css");
  await page.keyboard.press("Escape");
  await expect(fv).toHaveCount(0);
});

test("without a copy of its own, a card's file is read from its holder's folder (#677)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change(unchanged, null);
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  await page.locator('.dk-panel .dk-pfiles button.ib-file[data-desk-open-file="ui/header.js"]').click();
  const call = await cue.called("peek_file", { path: "ui/header.js" });
  // A teammate of the lead's without a worktree: the lead's folder.
  expect(call.args.cwd).toBe("/work/cue");
  await expect(page.locator(".fv .dov-title")).toHaveText("header.js");
});
