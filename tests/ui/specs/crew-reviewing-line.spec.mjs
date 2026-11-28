// The lead's "Reviewing <name> in the background." in its chat (ui/team.js reviewingLineHtml): one quiet line that
// follows the member's review, from running to its verdict with the crew row's own button; an old chat's line stays
// the plain reply. world-team: lead "lead", its member skill finished.
import { test, expect } from "./cue.mjs";
import { NOW } from "./clock.mjs";

const TEAM_ASK = "n1793984390004";
const ask = (name) => `Review your teammate ${name}'s work with a fresh agent in the background: one Agent call. Then reply only \`Reviewing ${name} in the background.\` and end your turn.\nWhat it was asked: Write the team skill\nIts files: skills/team/SKILL.md`;
const FIXES = "Review of skill:\n1. skills/team/SKILL.md:4 — should-fix — the steps skip the review.\n2. skills/team/review.md:9 — should-fix — no example.\nRecommendation: send fixes";
const CLEAN = "Review of skill:\nRecommendation: commit";

/** The lead's thread: per review, Cue's ask, the lead's "Reviewing skill…", and (when given) the review that came back. */
const thread = (reviews) => reviews.flatMap((r, n) => {
  const at = NOW - (10 - n * 3) * 60000;
  return [{ role: "you", at_ms: at, text: ask("skill") }, { role: "agent", at_ms: at + 1000, text: "Reviewing skill in the background." },
    ...(r ? [{ role: "agent", at_ms: at + 60000, text: r }] : [])];
});
const setUp = (cue, member, reviews, said = null) => cue.change((st, [member, reviews, said]) => {
  const lead = st.sessions.find((s) => s.session_id === "lead");
  lead.crew_commit = "wait";
  Object.assign(lead.team.find((t) => t.name === "skill"), member);
  lead.thread = said || reviews;
}, [member, reviews, said]);

test.describe("the lead's Reviewing line follows the member's review", () => {
  test.beforeEach(async ({ cue, page }) => {
    await cue.open("world-team");
    await page.locator(`.nrow[data-big="${TEAM_ASK}"]`).click();
    await page.locator('.mt-row.need [data-act="allow"]').click();
    await page.waitForFunction((id) => !state.items.some((i) => i.id === id), TEAM_ASK);
  });
  const line = (page) => page.locator(".active-pane .cv-rv");

  test("while it runs: Reviewing skill… with the live dot, not the reply's words", async ({ cue, page }) => {
    await setUp(cue, { state: "reviewing", review_ms: NOW - 60000 }, thread([null]));
    await expect(line(page)).toHaveText("Reviewing skill…");
    await expect(line(page).locator(".dot-live")).toHaveCount(1);
    await expect(page.locator(".active-pane .cv-agent", { hasText: "in the background" })).toHaveCount(0);
  });

  test("back with fixes: N fixes to send · Send fixes · See review, each doing what the crew row does", async ({ cue, page }) => {
    await setUp(cue, { state: "reviewed", review: FIXES, recommendation: "send fixes", findings: FIXES.split("\n").slice(1, 3) }, thread([FIXES]));
    await expect(line(page)).toHaveText("Review of skill: 2 fixes to send·Send fixes·See review");
    await expect(page.locator(".active-pane .cv-agent", { hasText: "Recommendation" }), "the review stays folded").toHaveCount(0);
    // See review: the crew's tray opens on its full review, under the member's row (as Full review there does).
    await line(page).locator("[data-mt-see]").click();
    await expect(page.locator(".crew-tray .mt-review")).toContainText("the steps skip the review");
    await line(page).locator('[data-mt-act="fixes"]').click();
    expect((await cue.called("team_action", { action: "fixes" })).args).toMatchObject({ sessionId: "lead", agent: "id-skill" });
  });

  test("back clean: ready to commit · Commit · See review", async ({ cue, page }) => {
    await setUp(cue, { state: "reviewed", review: CLEAN, recommendation: "commit", findings: [] }, thread([CLEAN]));
    await expect(line(page)).toHaveText("Review of skill: ready to commit·Commit·See review");
    await line(page).locator('[data-mt-act="commit"]').click();
    expect((await cue.called("team_action", { action: "commit" })).args).toMatchObject({ sessionId: "lead", agent: "id-skill" });
  });

  test("two reviews of one member: each line shows the review that followed it; only the current one has a button", async ({ cue, page }) => {
    await setUp(cue, { state: "reviewed", review: CLEAN, recommendation: "commit", findings: [] }, thread([FIXES, CLEAN]));
    await expect(line(page)).toHaveText(["Review of skill: 2 fixes to send·See review", "Review of skill: ready to commit·Commit·See review"]);
    // The older one's See review opens it under its line, in place; Hide review folds it again.
    await line(page).first().locator("[data-ask-open]").click();
    await expect(line(page).first().locator(".cv-rv-full")).toContainText("the steps skip the review");
    await line(page).first().locator("[data-ask-open]", { hasText: "Hide review" }).click();
    await expect(page.locator(".active-pane .cv-rv-full")).toHaveCount(0);
  });

  test("the newest line goes by the crew row: a chat copy worded differently still gets its button", async ({ cue, page }) => {
    const chat = `**Review of skill:**\n\n${FIXES.split("\n").slice(1).join("\n\n")}\n`;
    await setUp(cue, { state: "reviewed", review: FIXES, recommendation: "send fixes", findings: FIXES.split("\n").slice(1, 3) }, thread([chat]));
    await expect(line(page)).toHaveText("Review of skill: 2 fixes to send·Send fixes·See review");
  });

  test("an older line whose review reads the same as the current one has no button", async ({ cue, page }) => {
    await setUp(cue, { state: "reviewed", review: CLEAN, recommendation: "commit", findings: [] }, thread([CLEAN, CLEAN]));
    await expect(line(page)).toHaveText(["Review of skill: ready to commit·See review", "Review of skill: ready to commit·Commit·See review"]);
    await expect(line(page).first().locator("[data-mt-act]")).toHaveCount(0);
  });

  test("after Send fixes, while it fixes: the last result stays, or a quiet line when the chat has none", async ({ cue, page }) => {
    await setUp(cue, { state: "working", review: FIXES, recommendation: "send fixes", findings: [] }, thread([FIXES]));
    await expect(line(page)).toHaveText("Review of skill: 2 fixes to send·See review");
    await expect(line(page).locator("[data-mt-act]")).toHaveCount(0);
    await setUp(cue, { state: "working", review: "", recommendation: "", findings: [] }, thread([null]));
    await expect(line(page)).toHaveText("Review of skill: fixes sent · skill is fixing them");
    await expect(line(page).locator(".dot-live")).toHaveCount(1);
    await expect(page.locator(".active-pane .cv-agent", { hasText: "in the background" })).toHaveCount(0);
  });

  test("reviewed with no saved review text: the row's findings still say what it calls for", async ({ cue, page }) => {
    await setUp(cue, { state: "reviewed", review: "", recommendation: "commit", findings: [] }, thread([null]));
    await expect(line(page)).toHaveText("Review of skill: ready to commit·Commit");
  });

  test("no such member, or no review to follow (an old chat): the plain reply as before", async ({ cue, page }) => {
    const old = [{ role: "agent", at_ms: NOW - 300000, text: "Reviewing nobody in the background." }, { role: "agent", at_ms: NOW - 200000, text: "Reviewing skill in the background." }];
    await setUp(cue, { state: "done" }, [], old);
    await expect(page.locator(".active-pane .cv-agent", { hasText: "Reviewing nobody in the background." })).toHaveCount(1);
    await expect(page.locator(".active-pane .cv-agent", { hasText: "Reviewing skill in the background." })).toHaveCount(1);
    await expect(line(page)).toHaveCount(0);
  });
});
