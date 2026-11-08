// A markdown file a reply names opens in the reader (chat/reader.js openDoc), found from the session's folder by
// read_doc (ipc/docs.rs), and Listen reads it there: a file, so no "Then the next reply".
import { test, expect } from "./cue.mjs";

const REPLY = "Done. The plan is in `docs/rollout-plan.md`, and the hand-off is in HANDOFF.md.";

test("a .md path in a reply opens in the reader", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((s, t) => { s.sessions.find((x) => x.session_id === "relay").thread.at(-1).text = t; }, REPLY);
  await page.locator('.nrow[data-big="n1"]').click();
  const link = page.locator('.active-pane a[data-path$="docs/rollout-plan.md"]');
  await expect(link).toHaveAttribute("data-sid", "relay");
  await link.click();
  const call = await cue.called("read_doc");
  expect(call.args.cwd).toBe("/Users/me/code/agent-relay");
  await expect(page.locator(".rd-head .rd-file")).toHaveText("rollout-plan.md");
  await expect(page.locator(".rd .msg h4").first()).toHaveText("rollout-plan.md");
  expect(await cue.calls("open_link")).toEqual([]);
  // Back, then the bare name links too.
  await page.locator("[data-reader-back]").click();
  await page.locator('.active-pane a[data-path$="/HANDOFF.md"]').click();
  await expect(page.locator(".rd-head .rd-file")).toHaveText("HANDOFF.md");
  // Open hands the file to its app, as a click on the path did before.
  await page.locator("[data-reader-open]").click();
  await cue.called("open_link", { url: "/Users/me/code/agent-relay/HANDOFF.md" });
});
