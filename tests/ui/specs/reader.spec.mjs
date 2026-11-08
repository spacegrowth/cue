// A markdown file a reply names opens in Read (read.js), found from the session's folder by read_doc
// (ipc/docs.rs): Recent lists what you opened, Contents its headings, Listen reads it, ‹ <session> goes back.
// Sessions, as tiles, moved under Parked · Closed (All sessions →).
import { test, expect } from "./cue.mjs";

const REPLY = "Done. The plan is in `docs/rollout-plan.md`, and the hand-off is in HANDOFF.md.";
const DOC = "# Rollout plan\n\nThree stages.\n\n## Stage 1\n\nPaper accounts only.\n\n## Stage 2\n\nTen percent of live accounts.";

test("a .md path in a reply opens in Read", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await page.evaluate(() => localStorage.removeItem("cue.readRecent"));
  await cue.override("read_doc", ({ path, cwd }, doc) => ({ path: path.startsWith("/") ? path : `${cwd}/${path}`, text: doc, modified_ms: 0 }), DOC);
  await cue.change((s, t) => { s.sessions.find((x) => x.session_id === "relay").thread.at(-1).text = t; }, REPLY);
  await page.locator('.nrow[data-big="n1"]').click();
  const link = page.locator('.active-pane a[data-path$="docs/rollout-plan.md"]');
  await expect(link).toHaveAttribute("data-sid", "relay");
  await link.click();
  const call = await cue.called("read_doc");
  expect(call.args.cwd).toBe("/Users/me/code/agent-relay");
  await expect(page.locator('[data-view="read"]')).toHaveClass(/on/);
  await expect(page.locator(".rdv-head .rd-file")).toHaveText("rollout-plan.md");
  await expect(page.locator(".rdv .msg h4.dh1")).toHaveText("Rollout plan");
  await expect(page.locator(".rdv-toc [data-read-h]")).toHaveText(["Rollout plan", "Stage 1", "Stage 2"]);
  await expect(page.locator(".rdv-item.on .rdv-name")).toHaveText("rollout-plan.md");
  expect(await cue.calls("open_link")).toEqual([]);
  // Listen from the section picked in Contents: Stage 2's heading is where it starts.
  await page.locator('[data-read-h="2"]').click();
  await page.evaluate(() => { window.__spoken = []; speechSynthesis.speak = (u) => window.__spoken.push(u.text); speechSynthesis.cancel = () => {}; });
  await page.locator(".rdv-head [data-listen]").click();
  await expect.poll(() => page.evaluate(() => window.__spoken[0])).toBe("Stage 2");
  await page.locator("[data-listen-stop]").first().click();
  // Edited meanwhile: Cue coming forward reads it again.
  await cue.override("read_doc", ({ path }) => ({ path, text: "# Rollout plan\n\nStage 2 moved to Monday.", modified_ms: 5 }));
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect(page.locator(".rdv .msg")).toContainText("Stage 2 moved to Monday.");
  // ‹ agent-relay: back to its chat; the bare name links too, and both are in Recent.
  await page.locator("[data-read-back]").click();
  await page.locator('.active-pane a[data-path$="/HANDOFF.md"]').click();
  await expect(page.locator(".rdv-head .rd-file")).toHaveText("HANDOFF.md");
  await expect(page.locator(".rdv-item .rdv-name")).toHaveText(["HANDOFF.md", "rollout-plan.md"]);
  // Open hands the file to its app.
  await page.locator(".rdv-head [data-reader-open]").click();
  await cue.called("open_link", { url: "/Users/me/code/agent-relay/HANDOFF.md" });
});

test("Sessions, as tiles, open from under Parked · Closed", async ({ cue, page }) => {
  await cue.open("parked-ended");
  await expect(page.locator('[data-view="sessions"]')).toHaveCount(0);
  await page.locator('[data-shelf="parked"]').click();
  await page.locator('.sec-parked [data-lv="all"]').click();
  await expect(page.locator(".sv-search")).toBeVisible();
});

test("New docs: the newest .md files in this Mac's sessions' repos, at the foot of Read's left", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await page.evaluate(() => localStorage.removeItem("cue.readRecent"));
  await cue.override("recent_docs", () => [{ path: "/Users/me/code/agent-relay/docs/plan.md", top: "/Users/me/code/agent-relay", created_ms: Date.now() - 7200000, modified_ms: Date.now() }]);
  await cue.override("read_doc", ({ path }) => ({ path, text: "# Plan\n\nIt.", modified_ms: 0 }));
  await page.locator('[data-view="read"]').click();
  const item = page.locator(".rdv-new [data-read-new]");
  await expect(item.locator(".rdv-sub")).toHaveText("agent-relay · 2h ago");
  // Only folders on this Mac are searched.
  const call = await cue.called("recent_docs");
  expect(call.args.folders).toContain("/Users/me/code/agent-relay");
  await item.click();
  await expect(page.locator(".rdv-head .rd-file")).toHaveText("plan.md");
  await expect(page.locator("[data-read-back]")).toHaveText(/agent-relay chat/);
});
