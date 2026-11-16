// The Inbox (ui/inbox.js): a folded Running strip at the top (its count and a small bar a session), then two lines a row
// under Pinned, Needs you, Done, and folded Later and Parked; a finished turn
// that asks nothing under Done; the top ask opened by itself beside the list as a brief (never its full chat, which is a
// link): its words, the question with each choice's meaning, what you asked, what it did, what this turn changed, its
// context; several questions stacked as cards with one Send; Send leaving it open (as a sent email), the answered row sinking to
// Running with its bar, and rising again; Pin, Later and Snooze; with nothing needing you, what's running; a session half
// full of context up as housekeeping, with Compact and Hand off.
import { test, expect } from "./cue.mjs";

/** The Inbox open from the header, its list drawn. */
const openInbox = async (page) => {
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator(".inbox .ib-row").first().waitFor();
};
/** The Running strip at the top of the list. */
const strip = (page) => page.locator('.ib-sect.ib-strip[data-inbox-fold="run"]');
/** Running unfolded from the start (remembered, as a click on the strip leaves it). */
const runOpen = (page) => page.evaluate(() => localStorage.setItem("cue.inboxFold.run", "1"));
/** A section's rows' sessions, in order (Running's strip counts as its heading). */
const sectionSids = (page, title) => page.evaluate((t) => {
  const out = [];
  let on = false;
  for (const el of document.querySelectorAll(".ib-scroll > *")) {
    if (el.matches(".ib-sect")) on = (el.matches(".ib-strip") ? "Running" : el.textContent.replace(/[▸▾]/, "").trim()).toLowerCase() === t.toLowerCase();
    else if (on && el.matches(".ib-row")) out.push(el.dataset.inboxOpen);
  }
  return out;
}, title);
/** The section titles, in order. */
const sections = (page) => page.locator(".ib-sect").evaluateAll((els) => els.map((el) => (el.matches(".ib-strip") ? "Running" : el.textContent)));
/** api's ask as three questions, the second allowing several picks. */
const threeQuestions = (st) => {
  const it = st.items.find((i) => i.id === "n6");
  it.tool_input.questions = [
    { question: "Keep the stop button?", header: "Stop", multiSelect: false, options: [{ label: "Keep it" }, { label: "Drop it" }] },
    { question: "Should the new band replace the old one, or sit above it?", header: "Band", multiSelect: false, options: [{ label: "Replace it" }, { label: "Sit above" }, { label: "Show me both" }] },
    { question: "Which tests should it run after?", header: "Tests", multiSelect: false, options: [{ label: "Band only" }, { label: "Whole UI suite" }] },
  ];
};

test("the list: every finished turn waiting on you needs you, a question or not (as the Board's Waiting); two lines a row", async ({ cue, page }) => {
  await cue.open("board-waiting");
  // relay's reply ends on a statement, not a question ("Recommendation: send fixes"): still your turn.
  await cue.change((st) => { st.items.find((i) => i.id === "n1").message = "Reviewed it.\n\nRecommendation: send fixes"; });
  await openInbox(page);
  expect(await sections(page)).toEqual(["Running", "Needs you"]);
  const need = await sectionSids(page, "Needs you");
  expect(need.sort()).toEqual(["api", "det", "pipe", "relay", "sec", "web"]);
  await expect(page.locator('.ib-row[data-inbox-open="relay"] .ib-subj')).toHaveText("Recommendation: send fixes");
  await strip(page).click();
  expect((await sectionSids(page, "Running")).sort()).toEqual(["alerts", "eng", "names"]);
  await expect(page.locator('.switch-view [data-view="inbox"] .ib-count')).toHaveText("6");
  // Line 1 the question it ends on, line 2 a useful line; the time at the right.
  const sec = page.locator('.ib-row[data-inbox-open="sec"]');
  await expect(sec.locator(".ib-subj")).toHaveText("Want me to run the next company?");
  await expect(sec.locator(".ib-l2")).not.toBeEmpty();
  await expect(sec.locator(".ib-time")).toBeVisible();
  // The tabs: Inbox and Snoozed (parked ones are under Parked, on the Inbox tab).
  await expect(page.locator(".ib-tab")).toHaveText(["Inbox", "Snoozed"]);
});

test("Running is a thin strip at the top: its count and a small bar a session, folded until clicked; none with nothing running", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await openInbox(page);
  // At the very top, above Needs you; folded by default, its rows hidden.
  await expect(page.locator(".ib-scroll > .ib-sect").first()).toHaveClass(/\bib-strip\b/);
  await expect(strip(page)).toHaveAttribute("aria-expanded", "false");
  await expect(strip(page)).toContainText("Running · 3");
  await expect(strip(page).locator(".ib-strip-b .ib-bar")).toHaveCount(3);
  await expect(strip(page).locator(".ib-strip-more")).toHaveCount(0);
  await expect(page.locator(".ib-row.run")).toHaveCount(0);
  // A click unfolds its rows under it (remembered); a running row opens its brief; another click folds them.
  await strip(page).click();
  await expect(strip(page)).toHaveAttribute("aria-expanded", "true");
  expect((await sectionSids(page, "Running")).sort()).toEqual(["alerts", "eng", "names"]);
  expect(await page.evaluate(() => localStorage.getItem("cue.inboxFold.run"))).toBe("1");
  await page.locator('.ib-row[data-inbox-open="eng"]').click();
  await expect(page.locator(".inbox-page")).toHaveAttribute("data-inbox-page", "eng");
  await strip(page).click();
  await expect(strip(page)).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator(".ib-row.run")).toHaveCount(0);
  // More than eight running: eight bars, then "+N".
  await cue.change((st) => {
    const eng = st.sessions.find((x) => x.session_id === "eng");
    for (let n = 0; n < 7; n++) st.sessions.push({ ...eng, session_id: `run${n}`, name: `run ${n}` });
  });
  await expect(strip(page)).toContainText("Running · 10");
  await expect(strip(page).locator(".ib-strip-b")).toHaveCount(8);
  await expect(strip(page).locator(".ib-strip-more")).toHaveText("+2");
  // Nothing running: no strip.
  await cue.change((st) => { st.sessions = st.sessions.filter((x) => x.state !== "working" && x.state !== "agent"); });
  await expect(strip(page)).toHaveCount(0);
  await expect(page.locator(".ib-sect").first()).toHaveText("Needs you");
});

test("the top ask opens by itself, its row lit in place; a row opened is a brief, not its chat; Open chat goes to the Board's chat", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await openInbox(page);
  // Never an empty side: the first that needs you is open, its row lit where it was.
  const need = await sectionSids(page, "Needs you");
  await expect(page.locator(".inbox-page")).toHaveAttribute("data-inbox-page", need[0]);
  await expect(page.locator(`.ib-row[data-inbox-open="${need[0]}"]`)).toHaveClass(/\bon\b/);
  expect(await sectionSids(page, "Needs you")).toEqual(need);
  await expect(page.locator(".ib-next")).toHaveText(`1 of ${need.length} ›`);
  await page.locator('.ib-row[data-inbox-open="pipe"]').click();
  const pg = page.locator('.inbox-page[data-inbox-page="pipe"]');
  await expect(pg).toBeVisible();
  // Beside the list, which stays, its row lit.
  expect(await sectionSids(page, "Needs you")).toEqual(need);
  await expect(page.locator('.ib-row[data-inbox-open="pipe"]')).toHaveClass(/\bon\b/);
  await expect(pg.locator(".active-pane, [data-chat]")).toHaveCount(0);
  // Its words before the ask, what it wants (the Board's own buttons), what you asked it.
  await expect(pg.locator(".ib-said")).toContainText("Checking the fix didn't break other tests.");
  await expect(pg.locator('[data-act="allow"][data-id="n4"]')).toBeVisible();
  await expect(pg.locator(".ib-aside")).toContainText("make the line detector survive data gaps");
  await expect(pg.locator("[title]")).toHaveCount(0);
  await expect(page.locator(".ib-next")).toHaveText(`${need.indexOf("pipe") + 1} of ${need.length} ›`);
  await page.locator(".ib-next").click();
  await expect(page.locator(".inbox-page")).not.toHaveAttribute("data-inbox-page", "pipe");
  // A finished turn: its words, then the question it ends on.
  await page.locator('.ib-row[data-inbox-open="det"]').click();
  const det = page.locator('.inbox-page[data-inbox-page="det"]');
  await expect(det.locator(".ib-card.on .ib-qt")).toHaveText("Go ahead, or review the report first?");
  await expect(det.locator(".ib-said")).toContainText("Fills now use the next bar's open");
  await expect(det.locator(".ib-said")).not.toContainText("review the report first?");
  // What it plans next, lifted from its reply (and not said twice).
  await expect(det.locator(".ib-then")).toHaveText("It plans next: Next I'd tune the stop distance.");
  await expect(det.locator(".ib-said")).not.toContainText("tune the stop distance");
  // Esc in a wide window keeps it open: the side is never empty.
  await page.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("Escape");
  await expect(det).toBeVisible();
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  await expect(page.locator(".inbox-page .ib-qt")).toContainText("next company?");
  await page.locator(".inbox-page .ib-head [data-inbox-full]").click();
  await expect(page.locator('.switch-view [data-view="board"]')).toHaveClass(/\bon\b/);
  await expect(page.locator(".active-pane")).toBeVisible();
});

test("a question's choices show what each means and which it recommends; the side says what you asked, what it did, what this turn changed and its context", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "api"), at = st.now_ms;
    s.thread = [{ role: "you", text: "The quotes page feels slow. Find out why.", at_ms: at - 14 * 60000, images: [] }];
    s.context_pct = 38;
    // Cue's start of this turn: what it changed so far is read from git against it, mid-turn.
    s.turn_from = { root: "/x/api-server", commit: "abc123", untracked: [] };
    const it = st.items.find((i) => i.id === "n6");
    it.context = [{ role: "assistant", text: "The pricing lookup is 71% of each request.\n\nThe API runs as 4 pods. Then I'll add the bench to CI." }];
    it.tool_input.questions[0].options[0].label = "Redis (Recommended)";
  });
  await cue.override("session_steps", ({ sessionId }, at) => {
    const step = (id, kind, verb, subject, result) => ({ t: "step", id, kind, verb, subject, mono: true, result, bad: false, done: true, at_ms: at - 60000 });
    return sessionId !== "api" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at - 14 * 60000, prompt: "", more: 0, items: [
      step("r1", "read", "Read", "a.ts", "10 lines"), step("r2", "read", "Read", "b.ts", "10 lines"), step("r3", "read", "Read", "c.ts", "10 lines"),
      step("b1", "run", "Ran", "npm run bench", "p95 182ms"), step("e1", "edit", "Edited", "cache.ts", "+3 −1"),
    ] }] };
  }, await page.evaluate(() => now()));
  const diff = "diff --git a/cache.ts b/cache.ts\nindex 1..2 100644\n--- a/cache.ts\n+++ b/cache.ts\n@@ -1,3 +1,5 @@\n const key = sku;\n-return lookup(key);\n+const hit = cache.get(key);\n+if (hit) return hit;\n+return lookup(key);\n";
  await cue.override("diff_git", ({ op, against }, d) => (op === "toplevel" ? { code: 0, stdout: "/x/api-server\n" } : op === "diff" ? { code: 0, stdout: against === "abc123" ? d : "" } : { code: 0, stdout: "" }), diff);
  await openInbox(page);
  await page.locator('.ib-row[data-inbox-open="api"]').click();
  const pg = page.locator('.inbox-page[data-inbox-page="api"]');
  // Each choice: its number, its label (the "(Recommended)" made a tag), what it means.
  const opts = pg.locator(".ib-opt");
  await expect(opts.locator("b")).toHaveText(["Redis", "In-process LRU", "No cache"]);
  await expect(opts.nth(0).locator(".ib-rec")).toHaveText("Recommended");
  await expect(opts.nth(1).locator(".ib-rec")).toHaveCount(0);
  await expect(opts.locator(".ib-od")).toHaveText(["shared, 30s TTL", "fastest"]);
  await expect(opts.locator(".ib-kbd")).toHaveText(["1", "2", "3"]);
  // Its words, the plan lifted out of them.
  await expect(pg.locator(".ib-said")).toContainText("The pricing lookup is 71% of each request.");
  await expect(pg.locator(".ib-then")).toContainText("Then I'll add the bench to CI.");
  // The side, in order: You asked, It did (three reads folded to one line), Changed this turn, Context.
  const aside = pg.locator(".ib-aside");
  await expect(aside.locator(".ib-blk > .ib-k")).toHaveText([/^You asked · 14m ago$/i, /^It did · 5 steps$/i, /^Changed this turn · 1 file/i, /^Context$/i]);
  await expect(aside.locator(".ib-you")).toHaveText("The quotes page feels slow. Find out why.");
  await expect(aside.locator(".ib-sv")).toHaveText(["Read 3 files", "Ran npm run bench", "Edited cache.ts"]);
  await expect(aside.locator(".ib-file")).toContainText("cache.ts");
  await expect(aside.locator(".ib-peek")).toContainText("const hit = cache.get(key);");
  await expect(aside.locator(".ib-ctx")).toContainText("38%");
  // Two columns side by side at this size.
  const [m, a] = await Promise.all([pg.locator(".ib-main").boundingBox(), aside.boundingBox()]);
  expect(a.x).toBeGreaterThan(m.x + m.width - 1);
  await expect(pg.locator("[title]")).toHaveCount(0);
});

test("Send leaves it open with what you sent, as a sent email; the answered row sinks to Running; you move on yourself", async ({ cue, page }) => {
  await cue.open("board-waiting");
  // Cue never answers: the move can't be waiting on the session getting to work.
  await cue.override("reply", () => new Promise(() => {}));
  await runOpen(page);
  await openInbox(page);
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  await page.locator('.inbox-page textarea[data-text="s:sec"]').fill("yes, run it");
  await page.locator('.inbox-page [data-act="send"]').click();
  // It stays open with your words; it doesn't move on by itself.
  await expect(page.locator('.inbox-page[data-inbox-page="sec"]')).toContainText("yes, run it");
  await page.waitForTimeout(2500);
  await expect(page.locator(".inbox-page")).toHaveAttribute("data-inbox-page", "sec");
  expect(await sectionSids(page, "Running")).toContain("sec");
  const row = page.locator('.ib-row[data-inbox-open="sec"]');
  await expect(row).toHaveClass(/\bsinking\b/);
  await expect(row.locator(".ib-subj")).toHaveText("You: “yes, run it”");
  await expect(row.locator(".ib-bar")).toBeVisible();
  // A question picked: the same.
  await page.locator('.ib-row[data-inbox-open="api"]').click();
  await page.locator('.inbox-page .ib-opt', { hasText: "Redis" }).click();
  await cue.called("respond", { id: "n6" });
  await expect(page.locator(".inbox-page")).toHaveAttribute("data-inbox-page", "api");
});

test("nothing needing you: what's running and what's done lately stand in beside the list", async ({ cue, page }) => {
  await cue.open("board-waiting");
  // relay's turn was answered: it's at rest with nothing waiting, so Done.
  await cue.change((st) => { st.items = []; st.sessions.find((x) => x.session_id === "relay").since_ms = Date.now() - 60000; });
  await openInbox(page);
  await expect(page.locator(".inbox-page")).toHaveCount(0);
  const calm = page.locator(".ib-calm");
  await expect(calm.locator(".ib-calm-head b")).toHaveText("Nothing needs you");
  await expect(calm.locator('.ib-calm-card.run[data-inbox-open="eng"]')).toContainText("Refactor the order router");
  await expect(calm.locator('.ib-calm-card.run[data-inbox-open="eng"] .ib-bar')).toBeVisible();
  await expect(calm.locator('.ib-calm-card.done[data-inbox-open="relay"]')).toBeVisible();
  await calm.locator('[data-inbox-open="eng"]').click();
  await expect(page.locator('.inbox-page[data-inbox-page="eng"] .ib-card.run')).toBeVisible();
});

test("several questions stack as cards: an answered one dims with its answer, the next is lit, one Send for all", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change(threeQuestions);
  await openInbox(page);
  await expect(page.locator('.ib-row[data-inbox-open="api"] .ib-cnt')).toHaveText("3 questions");
  await page.locator('.ib-row[data-inbox-open="api"]').click();
  const cards = page.locator(".inbox-page .ib-q");
  await expect(cards).toHaveCount(3);
  await expect(cards.nth(0)).toHaveClass(/\bon\b/);
  await cards.nth(0).locator(".ib-opt", { hasText: "Keep it" }).click();
  await expect(cards.nth(0)).toHaveClass(/\bdone\b/);
  await expect(cards.nth(0).locator(".ib-qa")).toHaveText("→ Keep it");
  await expect(cards.nth(1)).toHaveClass(/\bon\b/);
  await cards.nth(1).locator(".ib-opt", { hasText: "Replace it" }).click();
  await cards.nth(2).locator(".ib-opt", { hasText: "Whole UI suite" }).click();
  // Nothing sent until Send.
  expect(await cue.calls("respond")).toEqual([]);
  const send = page.locator('.inbox-page [data-act="submit-answers"]');
  await expect(send).toHaveText("Send 3 answers");
  await send.click();
  const r = await cue.called("respond", { id: "n6" });
  expect(r.args.decision.answers).toEqual({ "Keep the stop button?": "Keep it", "Should the new band replace the old one, or sit above it?": "Replace it", "Which tests should it run after?": "Whole UI suite" });
});

test("answered, a row sinks to Running with a moving bar; quiet it slows, stuck it's amber and still; asking again it rises", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await runOpen(page);
  await openInbox(page);
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  await page.locator('.inbox-page textarea[data-text="s:sec"]').fill("yes, run it");
  await page.locator('.inbox-page [data-act="send"]').click();
  // At work again, it stays open; its row is under Running with its bar.
  await expect(page.locator(".inbox-page")).toHaveAttribute("data-inbox-page", "sec");
  expect(await sectionSids(page, "Running")).toContain("sec");
  const row = page.locator('.ib-row[data-inbox-open="sec"]');
  await expect(row).toHaveClass(/\bsinking\b/);
  await expect(row.locator(".ib-subj")).toHaveText("You: “yes, run it”");
  await expect(row.locator(".ib-bar")).toHaveClass(/^ib-bar\s*$/);
  // Quiet a while: slower and fainter.
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "sec"); s.since_ms -= 5 * 60000; s.doing_ms = s.since_ms; });
  await expect(row.locator(".ib-bar")).toHaveClass(/\bslow\b/);
  // Stuck: amber and still.
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "sec"); s.stuck_ms = s.since_ms; });
  await expect(row.locator(".ib-bar")).toHaveClass(/\bstuck\b/);
  expect(await row.locator(".ib-bar").evaluate((el) => getComputedStyle(el, "::after").display)).toBe("none");
  // Asks again: back up under Needs you, on top.
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "sec");
    s.state = "waiting"; s.stuck_ms = 0;
    st.items.push({ ...st.items.find((i) => i.id === "n2"), id: "n9", session_id: "sec", project: s.project, created_ms: st.now_ms || Date.now(), message: "Done. Next one too?" });
  });
  await expect.poll(() => sectionSids(page, "Needs you").then((x) => x[0])).toBe("sec");
});

test("rows keep their place while their state stays: a redraw or a newer ask elsewhere doesn't reorder them", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await openInbox(page);
  const before = await sectionSids(page, "Needs you");
  // The oldest ask now the newest: sorted afresh it would jump to the top.
  await page.evaluate(() => { state.items.forEach((it, n) => { it.created_ms = now() - n * 1000; }); renderMain(); });
  expect(await sectionSids(page, "Needs you")).toEqual(before);
});

test("Parked lists the Board's parked sessions (stopped and kept) with Resume; Later its put-off ones; never closed or ended ones", async ({ cue, page }) => {
  // The Board's PARKED · CLOSED drawer has two parked sessions (drift, guide) and ended ones (lost1, lost2).
  await cue.open("parked-ended");
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "eng").later_ms = st.now_ms; });
  await page.evaluate(() => { localStorage.setItem("cue.inboxFold.later", "1"); localStorage.setItem("cue.inboxFold.parked", "1"); localStorage.setItem("cue.inboxFold.run", "1"); });
  await openInbox(page);
  expect(await sectionSids(page, "Later 1")).toEqual(["eng"]);
  expect(await sectionSids(page, "Running")).not.toContain("eng");
  const parked = page.locator('.ib-row.parked[data-inbox-open="drift"]');
  await expect(parked).toBeVisible();
  await expect(page.locator('.ib-sect.fold[data-inbox-fold="parked"]')).toHaveText("▾Parked 2");
  await parked.hover();
  await expect(parked.locator("[data-wake]")).toHaveText("↺ Resume");
  await expect(page.locator('[data-session="lost1"], [data-session="lost2"], [data-inbox-open="lost1"], [data-inbox-open="lost2"]')).toHaveCount(0);
});

test("a parked row opens beside the list (what it said, Resume, a box that resumes it first), not the Board", async ({ cue, page }) => {
  await cue.open("parked-ended");
  await page.evaluate(() => localStorage.setItem("cue.inboxFold.parked", "1"));
  await openInbox(page);
  await page.locator('.ib-row.parked[data-inbox-open="drift"] .ib-name').click();
  const pg = page.locator('.inbox-page[data-inbox-page="drift"]');
  await expect(pg).toBeVisible();
  await expect(page.locator('.switch-view [data-view="inbox"]')).toHaveClass(/on|active/);
  await expect(page.locator('.ib-row.parked[data-inbox-open="drift"]')).toHaveClass(/\bon\b/);
  await expect(pg.locator(".ib-status")).toContainText("parked");
  await expect(pg.locator(".ib-hacts [data-wake]")).toHaveText("↺ Resume");
  await expect(pg.locator("[data-wake-send]")).toHaveText("Resume & send");
  await expect(pg.locator("[data-attach]")).toHaveCount(0);
  await expect(pg.locator(".ib-said, .ib-dim").first()).toBeVisible();
});

test("Pin puts a row under Pinned (kept by Cue); Later folds it under Later at the foot; Snooze moves it to its tab, and back", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await runOpen(page);
  await openInbox(page);
  const row = page.locator('.ib-row[data-inbox-open="det"]');
  await row.hover();
  await row.locator("[data-inbox-pin]").click();
  await cue.called("set_pinned", { sessionId: "det", on: true });
  expect(await sections(page)).toEqual(["Running", "Pinned", "Needs you"]);
  expect(await sectionSids(page, "Pinned")).toEqual(["det"]);
  // Later: the Board's Later (its later mark). Under Later at the foot, folded; not counted as needing you.
  const needN = await page.locator('.switch-view [data-view="inbox"] .ib-count').textContent();
  await page.locator('.ib-row[data-inbox-open="web"]').hover();
  await page.locator('.ib-row[data-inbox-open="web"] [data-inbox-later]').click();
  await cue.called("set_later", { sessionId: "web", on: true });
  await expect(page.locator('.ib-row[data-inbox-open="web"]')).toHaveCount(0);
  await expect(page.locator('.ib-sect.fold[data-inbox-fold="later"]')).toHaveText("▸Later 1");
  await expect(page.locator('.switch-view [data-view="inbox"] .ib-count')).toHaveText(String(+needN - 1));
  await page.locator('.ib-sect.fold[data-inbox-fold="later"]').click();
  await expect(page.locator('.ib-sect.fold[data-inbox-fold="later"]')).toHaveAttribute("aria-expanded", "true");
  await expect(page.locator('.ib-row.later[data-inbox-open="web"]')).toBeVisible();
  // Opened, it stays open (kept per window).
  await page.evaluate(() => renderMain());
  await expect(page.locator('.ib-row.later[data-inbox-open="web"]')).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem("cue.inboxFold.later"))).toBe("1");
  await page.locator('.ib-row[data-inbox-open="web"]').hover();
  await expect(page.locator('.ib-row[data-inbox-open="web"] [data-inbox-later]')).toHaveText("Not later");
  await page.locator('.ib-row[data-inbox-open="web"] [data-inbox-later]').click();
  await cue.called("set_later", { sessionId: "web", on: false });
  expect(await sectionSids(page, "Needs you")).toContain("web");
  await expect(page.locator('.ib-sect.fold[data-inbox-fold="later"]')).toHaveCount(0);
  // Snooze: a menu of times; picked, it's under Snoozed until then.
  const sec = page.locator('.ib-row[data-inbox-open="sec"]');
  await sec.hover();
  await sec.locator("[data-inbox-snooze]").click();
  await expect(page.locator(".ib-snz button")).toHaveText(["In 1 hour", "Tomorrow, 9:00", "Monday, 9:00"]);
  await page.locator(".ib-snz button", { hasText: "In 1 hour" }).click();
  const c = await cue.called("set_snooze", { sessionId: "sec" });
  expect(c.args.untilMs).toBeGreaterThan(await page.evaluate(() => now()));
  await expect(page.locator('.ib-row[data-inbox-open="sec"]')).toHaveCount(0);
  await page.locator('[data-inbox-tab="snoozed"]').click();
  await expect(page.locator('.ib-row[data-inbox-open="sec"] .ib-l2')).toContainText("Back ");
  // It asks something new before its time: back on the Inbox tab at once, and counted.
  await page.locator('[data-inbox-tab="inbox"]').click();
  const count = await page.locator('.switch-view [data-view="inbox"] .ib-count').textContent();
  await page.clock.fastForward(1000);
  // A new finished turn takes the place of its old card, as the hub's does.
  await cue.change((st) => { st.items = st.items.filter((i) => i.id !== "n3"); st.items.push({ ...st.items.find((i) => i.id === "n2"), id: "n9", session_id: "sec", project: "filings-bot", created_ms: Date.now(), message: "One more: run the third company too?" }); });
  await expect(page.locator('.ib-row[data-inbox-open="sec"]')).toBeVisible();
  await expect(page.locator('.switch-view [data-view="inbox"] .ib-count')).toHaveText(String(+count + 1));
  // Cue ends the snooze as the new ask comes in (hub new_item): answered, it stays up, under Running with its bar.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "sec").snooze_ms = 0; });
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  await page.locator('.inbox-page textarea[data-text="s:sec"]').fill("yes, the third too");
  await page.locator('.inbox-page [data-act="send"]').click();
  // It stays open with your words.
  await expect(page.locator('.inbox-page[data-inbox-page="sec"]')).toContainText("yes, the third too");
  expect(await sectionSids(page, "Running")).toContain("sec");
  await expect(page.locator('.ib-row[data-inbox-open="sec"] .ib-bar')).toBeVisible();
  // Snoozed again, it stays hidden until its time comes; then it's back.
  await page.locator('.ib-row[data-inbox-open="sec"]').hover();
  await page.locator('.ib-row[data-inbox-open="sec"] [data-inbox-snooze]').click();
  await page.locator(".ib-snz button", { hasText: "In 1 hour" }).click();
  await expect(page.locator('.ib-row[data-inbox-open="sec"]')).toHaveCount(0);
  await page.clock.fastForward(3600000 + 1000);
  await page.evaluate(() => renderMain());
  await expect(page.locator('.ib-row[data-inbox-open="sec"]')).toBeVisible();
  // No hover text anywhere in the Inbox.
  await expect(page.locator(".inbox [title]")).toHaveCount(0);
});

test("a session half full of context comes up as housekeeping: its meter, Compact now, Hand off, Remind me later", async ({ cue, page }) => {
  await cue.open("board-waiting");
  // relay is at rest with nothing waiting (it would be Done), det asks back: both 71% full. eng is at 40%.
  await cue.change((st) => {
    st.items = st.items.filter((i) => i.id !== "n1");
    for (const sid of ["relay", "det"]) st.sessions.find((x) => x.session_id === sid).context_pct = 71;
    st.sessions.find((x) => x.session_id === "eng").context_pct = 40;
  });
  await openInbox(page);
  expect(await sectionSids(page, "Needs you")).toContain("relay");
  expect(await sectionSids(page, "Done")).not.toContain("relay");
  // Asking back: still its question, with the pill.
  await expect(page.locator('.ib-row[data-inbox-open="det"] .ib-subj')).toHaveText("Go ahead, or review the report first?");
  await expect(page.locator('.ib-row[data-inbox-open="det"] .ib-chore')).toHaveText("context 71%");
  const row = page.locator('.ib-row[data-inbox-open="relay"]');
  await expect(row.locator(".ib-chore")).toHaveText("context 71%");
  await expect(row.locator(".ib-subj")).toHaveText("Getting full: compact or hand off?");
  await expect(row.locator(".ib-meter i")).toHaveAttribute("style", /width:71%/);
  await expect(page.locator('.ib-row[data-inbox-open="eng"] .ib-chore')).toHaveCount(0);
  await row.click();
  const pg = page.locator('.inbox-page[data-inbox-page="relay"]');
  await expect(pg.locator(".ib-meter.big")).toBeVisible();
  await expect(pg.locator('[data-act="ins-compact"]')).toHaveText("Compact now");
  await expect(pg.locator('[data-act="ins-cue"], [data-act="ins-fill"]')).toHaveText("Hand off to a fresh session");
  await pg.locator('[data-inbox-snooze][data-at="card"]').click();
  await expect(pg.locator(".ib-snz button")).toHaveCount(3);
  await page.keyboard.press("Escape");
  await expect(pg.locator(".ib-snz")).toHaveCount(0);
  await pg.locator('[data-act="ins-compact"]').click();
  await cue.called("session_command", { sessionId: "relay", action: "compact" });
});

test("replying is as in mail: you write right under the message, Send under it, Open chat beside it; R puts you there", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  const pg = page.locator('.inbox-page[data-inbox-page="sec"]');
  // No Reply button to press first: the box is there, Send and Open chat under it.
  await expect(pg.locator("[data-inbox-reply]")).toHaveCount(0);
  await expect(pg.locator(".ib-box textarea")).toBeVisible();
  await expect(pg.locator(".ib-box-acts > .btn")).toHaveText(["Send", "Open chat"]);
  await page.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("r");
  await expect(pg.locator(".ib-box textarea")).toBeFocused();
  await expect(pg.locator(".ib-box textarea")).toHaveValue("");
  await page.keyboard.press("Escape");
  await expect(pg.locator(".ib-box textarea")).not.toBeFocused();
});

test("a lead whose crew waits on you needs you: its crew's rows, with their own buttons, in its brief", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => {
    st.items = st.items.filter((i) => i.session_id !== "eng");
    const s = st.sessions.find((x) => x.session_id === "eng");
    s.team = [{ name: "v5-loop", call: "c1", agent_id: "a1", state: "reviewed", recommendation: "send fixes", findings: [{ file: "ui/a.js", line: 3, what: "dead code" }], fix_rounds: 9 }];
  });
  await openInbox(page);
  expect(await sectionSids(page, "Needs you")).toContain("eng");
  const row = page.locator('.ib-row[data-inbox-open="eng"]');
  await expect(row.locator(".ib-subj")).toHaveText("1 of its crew needs you");
  await expect(row.locator(".ib-l2")).toContainText("v5-loop");
  await row.click();
  const crew = page.locator('.inbox-page[data-inbox-page="eng"] .ib-crew');
  await expect(crew).toContainText("v5-loop");
  await expect(crew.locator(".mt-row")).toHaveCount(1);
});

test("suggested replies, as mail's: Claude's suggestion goes into the box, a quick phrase sends", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => {
    st.settings.quick = { phrases: ["go ahead", "show me"] };
    const s = st.sessions.find((x) => x.session_id === "sec");
    s.state = "waiting";
    s.suggestion = "run the next company";
  });
  await openInbox(page);
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  const pg = page.locator('.inbox-page[data-inbox-page="sec"]');
  await expect(pg.locator(".ib-sug:visible")).toHaveText(["Suggestedrun the next company", "go ahead", "show me"]);
  await pg.locator(".ib-sug.ai").click();
  await expect(pg.locator(".ib-box textarea")).toHaveValue("run the next company");
  await pg.locator(".ib-box textarea").fill("");
  await pg.locator('.ib-sug[data-inbox-phrase="go ahead"]').click();
  await cue.called("reply", { text: "go ahead" });
});

test("more context: the conversation before its latest reply, a line a message (a click opens it), a usual reply whole, a very long one folded to its end", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "sec"), t0 = Date.now() - 3600000;
    s.thread = Array.from({ length: 9 }, (_, n) => ({ role: n % 2 ? "agent" : "you", text: n % 2 ? `Reply ${n}: did part ${n}.\n\nMore detail on part ${n}.` : `Message ${n}`, at_ms: t0 + n * 60000 }));
    const it = st.items.find((i) => i.session_id === "sec");
    it.message = Array.from({ length: 6 }, (_, n) => `Paragraph ${n} of the latest reply, long enough to count as words that matter here.`).join("\n\n") + "\n\nWant me to run the next company?";
  });
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  const pg = page.locator('.inbox-page[data-inbox-page="sec"]');
  // The latest six of the nine, folded; "Show 3 earlier" for the rest.
  await expect(pg.locator(".ib-msg")).toHaveCount(6);
  await expect(pg.locator(".ib-earlier-more")).toHaveText("Show 3 earlier");
  await pg.locator(".ib-earlier-more").click();
  await expect(pg.locator(".ib-msg")).toHaveCount(9);
  const one = pg.locator(".ib-msg").nth(1);
  await expect(one.locator(".ib-msg-line")).toContainText("Reply 1: did part 1.");
  await one.click();
  await expect(one.locator(".ib-msg-body")).toContainText("More detail on part 1.");
  // Its latest reply of a usual length: whole, with nothing to unfold.
  await expect(pg.locator(".ib-said .msg")).toContainText("Paragraph 0");
  await expect(pg.locator(".ib-said .msg")).toContainText("Paragraph 5");
  await expect(pg.locator("[data-inbox-whole]")).toHaveCount(0);
  // A very long one: its last paragraphs, then all of it on asking.
  await cue.change((st) => { st.items.find((i) => i.session_id === "sec").message = Array.from({ length: 40 }, (_, n) => `Paragraph ${n} of a very long reply, long enough to count as words that matter here.`).join("\n\n") + "\n\nWant me to run the next company?"; });
  await expect(pg.locator(".ib-said .msg")).not.toContainText("Paragraph 0 ");
  await expect(pg.locator(".ib-said .msg")).toContainText("Paragraph 39");
  await pg.locator("[data-inbox-whole]").click();
  await expect(pg.locator(".ib-said .msg")).toContainText("Paragraph 0 ");
});

test("an open session can be closed from its head, as on the Board: Close, then Close?", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  const close = page.locator('.inbox-page[data-inbox-page="sec"] .ib-head [data-sv="close"]');
  await expect(close).toHaveText("Close");
  await expect(close).not.toHaveAttribute("title", /.*/);
  await close.click();
  await expect(close).toHaveText("Close?");
  await close.click();
  await cue.called("close_session", { sessionId: "sec" });
  // One that asks you something has no Close (you'd lose the question).
  await page.locator('.ib-row[data-inbox-open="pipe"]').click();
  await expect(page.locator('.inbox-page[data-inbox-page="pipe"] [data-sv="close"]')).toHaveCount(0);
});

test("what you sent that hasn't gone yet shows: queued on its row, and in its brief with Send now, as in its chat", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "eng"); s.queued = { text: "also check the retry path", at_ms: Date.now(), images: [] }; });
  await runOpen(page);
  await page.locator('.switch-view [data-view="inbox"]').click();
  const row = page.locator('.ib-row[data-inbox-open="eng"]');
  await expect(row.locator(".ib-l2")).toHaveText("Queued: “also check the retry path”");
  await row.click();
  const pend = page.locator('.inbox-page[data-inbox-page="eng"] .ib-pending');
  await expect(pend).toContainText("also check the retry path");
  await expect(pend).toContainText("Queued");
  await pend.locator('[data-act="send-now"]').click();
  await cue.called("send_queued_now", { sessionId: "eng" });
});

test("the reply talks and takes pictures, as the chat's box does: the mic, the picture button, and a picture pasted goes with it", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  const box = page.locator('.inbox-page[data-inbox-page="sec"] .ib-box');
  await expect(box.locator('[data-mic="s:sec"]')).toBeVisible();
  await expect(box.locator('[data-attach="s:sec"]')).toBeVisible();
  await expect(box).toHaveAttribute("data-drop", "s:sec");
  await page.evaluate(() => { draft("s:sec").images.push({ name: "shot.png", mime: "image/png", data: "data:image/png;base64,iVBORw0KGgo=" }); renderMain(); });
  await expect(box.locator(".chip")).toHaveCount(1);
  // A question's answer is words only: the mic, no picture button.
  await page.locator('.ib-row[data-inbox-open="api"]').click();
  const q = page.locator('.inbox-page[data-inbox-page="api"] .ib-box');
  await expect(q.locator("[data-mic]")).toBeVisible();
  await expect(q.locator("[data-attach]")).toHaveCount(0);
});

test("at work, it reads as a mail thread: earlier, its message whole, your reply under it, then what it's doing now over the box", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "eng"), t0 = Date.now() - 600000;
    s.thread = [{ role: "you", text: "Refactor the order router", at_ms: t0 }, { role: "agent", text: "Split the router in two; partial fills next?", at_ms: t0 + 60000 }, { role: "you", text: "yes, partial fills too", at_ms: t0 + 120000 }];
  });
  await runOpen(page);
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('.ib-row[data-inbox-open="eng"]').click();
  const pg = page.locator('.inbox-page[data-inbox-page="eng"]');
  await expect(pg.locator(".ib-said .msg")).toContainText("Split the router in two");
  await expect(pg.locator(".ib-yours")).toContainText("yes, partial fills too");
  await expect(pg.locator(".ib-msg")).toHaveCount(1);
  const order = await pg.locator(".ib-main").evaluate((m) => [".ib-earlier", ".ib-said", ".ib-yours", ".ib-card.run", ".ib-box"].map((q) => [...m.querySelectorAll("*")].indexOf(m.querySelector(q))));
  expect(order.every((x, i) => x >= 0 && (i === 0 || x > order[i - 1]))).toBe(true);
});

test("a picture its reply points to shows under it, and ![words](path) reads as a link, not markdown", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => { st.items.find((i) => i.session_id === "sec").message = "Here's how it looks now:\n\n![The reply area](/tmp/inbox-reply.png)\n\nWant me to run the next company?"; });
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  const said = page.locator('.inbox-page[data-inbox-page="sec"] .ib-said');
  await expect(said.locator('a[data-path="/tmp/inbox-reply.png"]')).toHaveText("The reply area");
  await expect(said).not.toContainText("![");
  await expect(said.locator(".thumbs.shots .thumb")).toHaveCount(1);
});

test("the project's action buttons stay in its chat, not in the Inbox reply", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('[data-inbox-fold="run"]').click();
  await page.locator('.ib-row[data-inbox-open="eng"]').click();
  const pg = page.locator('.inbox-page[data-inbox-page="eng"]');
  await expect(pg.locator(".ib-box textarea")).toBeVisible();
  // Its project has actions (the chat shows them under its latest reply), and the Inbox leaves them out.
  await expect.poll(() => page.evaluate(() => actReplyHtml(sessionOf("eng"), 0))).toContain("ac-sugs");
  await expect(pg.locator(".ac-sugs")).toHaveCount(0);
});

test("an earlier message shows two lines, with more when that cuts it; a click opens it whole, with less", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const long = Array.from({ length: 12 }, (_, n) => `Sentence ${n} of an earlier reply that runs on for a while.`).join(" ");
  await cue.change((st, long) => {
    const s = st.sessions.find((x) => x.session_id === "sec"), t0 = Date.now() - 3600000;
    s.thread = [{ role: "agent", text: long, at_ms: t0 }, { role: "you", text: "short one", at_ms: t0 + 60000 }];
  }, long);
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  const pg = page.locator('.inbox-page[data-inbox-page="sec"]');
  const [cut, short] = [pg.locator(".ib-msg").nth(0), pg.locator(".ib-msg").nth(1)];
  // Two lines tall, not one, and cut with "more"; a short one has no "more".
  const lines = await cut.locator(".ib-msg-line").evaluate((el) => Math.round(el.getBoundingClientRect().height / parseFloat(getComputedStyle(el).lineHeight)));
  expect(lines).toBe(2);
  await expect(cut.locator(".ib-msg-more")).toHaveText("more");
  await expect(short.locator(".ib-msg-more")).toHaveCount(0);
  // What a click opens shows the hand, as everywhere else in Cue: the message, a row, a section's heading.
  for (const el of [cut, page.locator(".ib-row").first(), page.locator("button.ib-sect").first()]) expect(await el.evaluate((x) => getComputedStyle(x).cursor)).toBe("pointer");
  await cut.click();
  await expect(cut.locator(".ib-msg-body")).toContainText("Sentence 11");
  await expect(cut.locator(".ib-msg-more")).toHaveText("less");
});

test("pictures sent with a message show in the Inbox, as in the chat: in an earlier row, under your reply and under its message; a click opens one", async ({ cue, page }) => {
  // The pictures, served as a real (one-pixel) PNG.
  const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==", "base64");
  await page.route(/\/tmp\/[a-d]\.png$/, (r) => r.fulfill({ status: 200, contentType: "image/png", body: png }));
  await cue.open("board-waiting");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "eng"), t0 = Date.now() - 600000;
    s.thread = [
      { role: "you", text: "Look at this layout", images: ["/tmp/a.png"], at_ms: t0 },
      { role: "agent", text: "Split the router in two; partial fills next?", images: ["/tmp/b.png"], at_ms: t0 + 60000 },
      { role: "you", text: "like this one", images: ["/tmp/c.png", "/tmp/d.png"], at_ms: t0 + 120000 },
    ];
  });
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('[data-inbox-fold="run"]').click();
  await page.locator('.ib-row[data-inbox-open="eng"]').click();
  const pg = page.locator('.inbox-page[data-inbox-page="eng"]');
  await expect(pg.locator(".ib-msg .thumbs .thumb")).toHaveCount(1);
  await expect(pg.locator(".ib-said .thumbs .thumb")).toHaveCount(1);
  await expect(pg.locator(".ib-yours .thumbs .thumb")).toHaveCount(2);
  await pg.locator(".ib-yours .thumb").first().click();
  await expect(page.locator(".lightbox img")).toHaveAttribute("src", /c\.png/);
  await expect(page.locator(".lightbox")).toBeVisible();
});

test("quick phrases sit at the right of Send's row; those that don't fit open upward from More", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => { st.settings.quick = { phrases: ["go ahead", "show me", "run the tests and tell me what fails", "commit it", "explain that again more simply", "keep going"] }; });
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('[data-inbox-fold="run"]').click();
  await page.locator('.ib-row[data-inbox-open="eng"]').click();
  const main = page.locator('.inbox-page[data-inbox-page="eng"] .ib-main');
  // At work too, they sit under the box, on the row with Send.
  const acts = main.locator(".ib-box .ib-box-acts");
  await expect(acts.locator('.ib-sug[data-inbox-phrase="go ahead"]')).toBeVisible();
  const send = await acts.locator(".btn.primary").boundingBox(), first = await acts.locator('.ib-sug[data-inbox-phrase="go ahead"]').boundingBox();
  expect(Math.abs(send.y - first.y)).toBeLessThan(6);
  expect(first.x).toBeGreaterThan(send.x);
  // Not all fit: More shows, and its menu (above it) lists just the hidden ones.
  const more = acts.locator("[data-inbox-phr-more]");
  await expect(more).toBeVisible();
  const shown = await acts.locator(".ib-phr-row .ib-sug:visible").count();
  expect(shown).toBeLessThan(6);
  await more.click();
  const menu = main.locator(".ib-phr-menu");
  await expect(menu.locator("button:visible")).toHaveCount(6 - shown);
  expect((await menu.boundingBox()).y + (await menu.boundingBox()).height).toBeLessThanOrEqual((await more.boundingBox()).y + 1);
  await menu.locator("button:visible").last().click();
  await cue.called("send_to_session", { sessionId: "eng", text: "keep going" });
  await expect(menu).toHaveCount(0);
});

test("at work, Thinking counts its dots and its bar runs the full width", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('[data-inbox-fold="run"]').click();
  await page.locator('.ib-row[data-inbox-open="eng"]').click();
  const card = page.locator('.inbox-page[data-inbox-page="eng"] .ib-card.run');
  const bar = await card.locator(".ib-bar").boundingBox(), box = await card.boundingBox();
  expect(Math.abs(bar.x - box.x)).toBeLessThan(1);
  expect(Math.abs(bar.width - box.width)).toBeLessThan(1);
});

test("at work, what you wrote that waits shows in its own block, not as what it's doing now", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "eng"); s.held = { text: "check the retry path" }; });
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('[data-inbox-fold="run"]').click();
  const row = page.locator('.ib-row[data-inbox-open="eng"]');
  await expect(row.locator(".ib-l2")).toHaveText("Not sent yet: “check the retry path”");
  await row.click();
  const pg = page.locator('.inbox-page[data-inbox-page="eng"]');
  await expect(pg.locator(".ib-card.run .ib-qt")).not.toContainText("Not sent yet");
  await expect(pg.locator(".ib-pending")).toContainText("check the retry path");
});

test("writing in the reply box, a click on a button in the Inbox leaves the cursor in the box", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => { st.settings.quick = { phrases: ["go ahead", "show me", "run the tests and tell me what fails", "commit it", "explain that again more simply", "keep going"] }; });
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('[data-inbox-fold="run"]').click();
  await page.locator('.ib-row[data-inbox-open="eng"]').click();
  const box = page.locator('.inbox-page[data-inbox-page="eng"] .ib-box textarea');
  await box.click();
  await box.type("half wri");
  await page.locator("[data-inbox-phr-more]").click();
  await expect(page.locator(".ib-phr-menu")).toBeVisible();
  await expect(box).toBeFocused();
  await page.keyboard.type("tten");
  await expect(box).toHaveValue("half written");
});

test("on a long page the reply stays in view at the foot of the pane, the words scrolling under it", async ({ cue, page }) => {
  await page.setViewportSize({ width: 1300, height: 700 });
  await cue.open("board-waiting");
  await cue.change((st) => { st.items.find((i) => i.session_id === "sec").message = Array.from({ length: 30 }, (_, n) => `Paragraph ${n} of a long reply with enough words to fill a line or two here.`).join("\n\n") + "\n\nWant me to run the next company?"; });
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  const main = page.locator('.inbox-page[data-inbox-page="sec"] .ib-main');
  const send = main.locator('.ib-box [data-act="send"]');
  const inView = async () => { const m = await main.boundingBox(), b = await send.boundingBox(); return b.y >= m.y && b.y + b.height <= m.y + m.height + 1; };
  expect(await main.evaluate((m) => m.scrollHeight > m.clientHeight)).toBe(true);
  expect(await inView()).toBe(true);
  await main.evaluate((m) => { m.scrollTop = m.scrollHeight; });
  expect(await inView()).toBe(true);
});

test("opened, its reply box has the cursor: you can write at once", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await page.evaluate(() => localStorage.setItem("cue.inboxFold.run", "1"));
  await openInbox(page);
  await page.locator('.ib-row[data-inbox-open="names"]').click();
  await expect.poll(() => page.evaluate(() => document.activeElement?.dataset?.text)).toBe("s:names");
  await page.keyboard.type("hi");
  await expect(page.locator('.inbox-page textarea[data-text="s:names"]')).toHaveValue("hi");
});

test("what's at work shows live at the top of the side: the session on its step, its agents and its scripts, each with its newest step; a click opens it in its chat", async ({ cue, page }) => {
  await cue.open("board-waiting", { flags: "many=1&scripts=1" });
  await page.evaluate(() => localStorage.setItem("cue.inboxFold.run", "1"));
  await openInbox(page);
  await page.locator('.ib-row[data-inbox-open="names"]').click();
  const live = page.locator(".inbox-page .ib-aside .ib-live");
  await expect(live.locator(".ib-k")).toHaveText("Running now · 7");
  await expect(live.locator(".ib-live-main")).toContainText("Claude Code");
  await expect(live.locator('.ib-lv[data-inbox-helper="names|h1"] .ib-lv-step')).toHaveText("Reading hub.rs");
  await expect(live.locator('.ib-lv[data-inbox-helper="names|h4"]')).toHaveClass(/stuck/);
  await expect(live.locator('.ib-lv[data-inbox-helper="names|b1"] .ib-lv-meta')).toContainText("script");
  await expect(live.locator('[data-inbox-helper="names|h2"], [data-inbox-helper="names|b2"]')).toHaveCount(0);   // finished ones aren't at work
  // The reply area says what else is at work beside it.
  await expect(page.locator(".inbox-page .ib-card.run .ib-also")).toHaveText("+ 5 agents · 1 script at work");
  await live.locator('.ib-lv[data-inbox-helper="names|h1"]').click();
  await expect(page.locator(".inbox-page")).toHaveCount(0);
  await expect(page.locator(".active-pane")).toContainText("Backend: session name sources");
});
