// The Inbox (ui/inbox.js): two lines a row under Pinned, Needs you, Running and Done; a finished turn that asks nothing
// under Done; a row opened in the list's place as its facts and cards (never its full chat, which is a link); several
// questions stacked as cards with one Send; an answered row sinking to Running with its bar, and rising again; Pin, Later
// and Snooze; a session half full of context up as housekeeping, with Compact and Hand off.
import { test, expect } from "./cue.mjs";

/** The Inbox open from the header, its list drawn. */
const openInbox = async (page) => {
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator(".inbox .ib-row").first().waitFor();
};
/** A section's rows' sessions, in order. */
const sectionSids = (page, title) => page.evaluate((t) => {
  const out = [];
  let on = false;
  for (const el of document.querySelectorAll(".ib-scroll > *")) {
    if (el.matches(".ib-sect")) on = el.textContent.trim().toLowerCase() === t.toLowerCase();
    else if (on && el.matches(".ib-row")) out.push(el.dataset.inboxOpen);
  }
  return out;
}, title);
/** The section titles, in order. */
const sections = (page) => page.locator(".ib-sect").allTextContents();
/** api's ask as three questions, the second allowing several picks. */
const threeQuestions = (st) => {
  const it = st.items.find((i) => i.id === "n6");
  it.tool_input.questions = [
    { question: "Keep the stop button?", header: "Stop", multiSelect: false, options: [{ label: "Keep it" }, { label: "Drop it" }] },
    { question: "Should the new band replace the old one, or sit above it?", header: "Band", multiSelect: false, options: [{ label: "Replace it" }, { label: "Sit above" }, { label: "Show me both" }] },
    { question: "Which tests should it run after?", header: "Tests", multiSelect: false, options: [{ label: "Band only" }, { label: "Whole UI suite" }] },
  ];
};

test("the list: a finished turn that asks nothing is under Done, one that asks back needs you; two lines a row", async ({ cue, page }) => {
  await cue.open("board-waiting");
  // relay's reply ends on a statement, not a question.
  await cue.change((st) => { st.items.find((i) => i.id === "n1").message = "Carried the spend over to the successor. Tests pass."; });
  await openInbox(page);
  expect(await sections(page)).toEqual(["Needs you", "Running", "Done"]);
  const need = await sectionSids(page, "Needs you");
  // det and sec end on a question: they need you; relay is Done.
  expect(need.sort()).toEqual(["api", "det", "pipe", "sec", "web"]);
  expect(await sectionSids(page, "Done")).toContain("relay");
  expect((await sectionSids(page, "Running")).sort()).toEqual(["alerts", "eng", "names"]);
  await expect(page.locator('.switch-view [data-view="inbox"] .ib-count')).toHaveText("5");
  // Line 1 the question it ends on, line 2 a useful line; the time at the right.
  const sec = page.locator('.ib-row[data-inbox-open="sec"]');
  await expect(sec.locator(".ib-subj")).toHaveText("Want me to run the next company?");
  await expect(sec.locator(".ib-l2")).not.toBeEmpty();
  await expect(sec.locator(".ib-time")).toBeVisible();
  // The tabs.
  await expect(page.locator(".ib-tab")).toHaveText(["Inbox", "Later", "Snoozed"]);
});

test("opening a row shows its question as a card, not its chat; Full chat › goes to the Board's chat", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await openInbox(page);
  await page.locator('.ib-row[data-inbox-open="pipe"]').click();
  const pg = page.locator('.inbox-page[data-inbox-page="pipe"]');
  await expect(pg).toBeVisible();
  await expect(pg.locator(".active-pane, [data-chat]")).toHaveCount(0);
  await expect(pg.locator(".ib-facts")).toContainText("Waiting on");
  await expect(pg.locator('[data-act="allow"][data-id="n4"]')).toBeVisible();
  await expect(pg.locator("[title]")).toHaveCount(0);
  await expect(page.locator(".ib-next")).toHaveText("next ask ›");
  await page.locator(".ib-next").click();
  await expect(page.locator(".inbox-page")).not.toHaveAttribute("data-inbox-page", "pipe");
  await page.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("Escape");
  await expect(page.locator(".inbox .ib-row").first()).toBeVisible();
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  await expect(page.locator(".inbox-page .ib-qt")).toContainText("next company?");
  await page.locator("[data-inbox-full]").click();
  await expect(page.locator('.switch-view [data-view="board"]')).toHaveClass(/\bon\b/);
  await expect(page.locator(".active-pane")).toBeVisible();
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
  await openInbox(page);
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  await page.locator('.inbox-page textarea[data-text="s:sec"]').fill("yes, run it");
  await page.locator('.inbox-page [data-act="send"]').click();
  // Back to the list once it's at work again, its row under Running with its bar.
  await expect(page.locator(".inbox-page")).toHaveCount(0);
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

test("Pin puts a row under Pinned (kept by Cue); Later and Snooze move it to their tabs, and back", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await openInbox(page);
  const row = page.locator('.ib-row[data-inbox-open="det"]');
  await row.hover();
  await row.locator("[data-inbox-pin]").click();
  await cue.called("set_pinned", { sessionId: "det", on: true });
  expect(await sections(page)).toEqual(["Pinned", "Needs you", "Running"]);
  expect(await sectionSids(page, "Pinned")).toEqual(["det"]);
  // Later: off the Inbox tab, under Later.
  await page.locator('.ib-row[data-inbox-open="web"]').hover();
  await page.locator('.ib-row[data-inbox-open="web"] [data-inbox-later]').click();
  await cue.called("set_later", { sessionId: "web", on: true });
  await expect(page.locator('.ib-row[data-inbox-open="web"]')).toHaveCount(0);
  await expect(page.locator('[data-inbox-tab="later"]')).toHaveText("Later 1");
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
  await cue.change((st) => { st.items.push({ ...st.items.find((i) => i.id === "n2"), id: "n9", session_id: "sec", project: "filings-bot", created_ms: Date.now(), message: "One more: run the third company too?" }); });
  await expect(page.locator('.ib-row[data-inbox-open="sec"]')).toBeVisible();
  await expect(page.locator('.switch-view [data-view="inbox"] .ib-count')).toHaveText(String(+count + 1));
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
  // relay finished asking nothing (it would be Done), det asks back: both 71% full. eng is at 40%.
  await cue.change((st) => {
    st.items.find((i) => i.id === "n1").message = "Carried the spend over to the successor. Tests pass.";
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
