// A file clicked anywhere in Cue opens in a pop-up viewer (file_view.js), found from the session's folder by
// peek_file (ipc/docs.rs): code with its line numbers, a picture at full size; Open in app and Copy path in its head,
// Esc closes it. A markdown file still opens in Read (reader.spec.mjs); a folder or a binary opens where it did.
import { test, expect } from "./cue.mjs";

test("a code path in a reply opens in the pop-up viewer, Esc closes it", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((s) => { s.sessions.find((x) => x.session_id === "relay").thread.at(-1).text = "Changed ui/sessions.js and a shot at /tmp/shot.png."; });
  await page.locator('.nrow[data-big="n1"]').click();
  await page.locator('.active-pane a[data-path$="ui/sessions.js"]').click();
  const call = await cue.called("peek_file");
  expect(call.args.cwd).toBe("/Users/me/code/agent-relay");
  const fv = page.locator(".fv");
  await expect(fv.locator(".dov-title")).toHaveText("sessions.js");
  await expect(fv.locator(".fv-nums")).toHaveText("1\n2");
  await expect(fv.locator(".fv-text")).toHaveText("const a = 1;\nconst b = a + 1;");
  await fv.locator("[data-fv-copy]").click();
  await cue.called("copy_text", { text: "/Users/me/code/agent-relay/ui/sessions.js" });
  await fv.locator("[data-fv-open]").click();
  await cue.called("open_link", { url: "/Users/me/code/agent-relay/ui/sessions.js" });
  await page.keyboard.press("Escape");
  await expect(fv).toHaveCount(0);
  // The chat is still open under it: Esc went to the viewer only.
  await expect(page.locator('.active-pane a[data-path="/tmp/shot.png"]')).toBeVisible();
  await page.locator('.active-pane a[data-path="/tmp/shot.png"]').click();
  await expect(page.locator(".fv .fv-img img")).toHaveAttribute("src", /^data:image\/png/);
  await page.locator(".fv-scrim").click({ position: { x: 5, y: 5 } });
  await expect(page.locator(".fv")).toHaveCount(0);
});

test("a path in an ask's question links, and opens in the viewer", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((s) => { s.items.find((x) => x.id === "n6").tool_input.questions[0].question = "OK to commit the leftover in ui/app/clicks_chat.js on main?"; });
  const link = page.locator('.qtext a[data-path="/Users/me/code/api-server/ui/app/clicks_chat.js"]').first();
  await link.click();
  await expect(page.locator(".fv .dov-title")).toHaveText("clicks_chat.js");
});

test("a folder opens where it did before, not in the viewer", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await page.evaluate(() => fileView("/Users/me/code/agent-relay/"));
  await cue.called("open_link", { url: "/Users/me/code/agent-relay/" });
  await expect(page.locator(".fv")).toHaveCount(0);
});
