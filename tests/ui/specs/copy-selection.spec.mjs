// Text you select in the window goes onto your clipboard as you let go (window.js copySelected), unless a picture is
// on it (#567): a screenshot about to be pasted into a send-back was replaced by the selected words, so ⌘V pasted them.
import { test, expect } from "./cue.mjs";

async function selectReply(page) {
  await page.locator(".active-pane p", { hasText: "Feature #542" }).last().click({ clickCount: 3 });
}

test("selected text is copied, but never over a picture on the clipboard", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((s) => { s.sessions.find((x) => x.session_id === "relay").thread.at(-1).text = "Feature #542 stopped only because its copy was in use."; });
  await page.locator('.nrow[data-big="n1"]').click();
  await selectReply(page);
  await cue.called("copy_text", { text: "Feature #542 stopped only because its copy was in use." });
  await expect(page.locator(".active-pane .copied")).toContainText("Copied");

  // A screenshot on the clipboard: the selection leaves it there, and says how to copy the text anyway.
  await cue.override("clipboard_has_picture", () => true);
  const before = (await cue.calls("copy_text")).length;
  await page.mouse.click(5, 5);
  await selectReply(page);
  await expect(page.locator(".toast")).toContainText("a picture is on your clipboard");
  expect(await cue.calls("copy_text")).toHaveLength(before);
});
