// What the lead does on the Desk (#360): anything that needs you, its own asks too, waits in the strip over the
// Dispatcher's chat until it's answered; what it did on its own (closed, started, moved up, took back a copy, answered a
// session) is in Activity (#688: out of its chat, a count line there). Its chat has no panel since 692dc421.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";

const s = (ms) => Math.floor(ms / 1000);
const act = (id, item, act, text, why, at) => ({ id, project: "cue", item_id: item, by: "web-lead", sid: "lead", text, why, at: s(at), undo: "none", act });

test("the lead's own moves are in Activity, not in its chat; its ask waits in the strip till answered (#360)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1728, height: 1000 });
  await cue.open("desk");
  await openDesk(page);
  // The lead's chat has no panel (692dc421): Now and Waits on you are always its strip.
  await expect(page.locator(".dk2b-chat .dk-pact")).toBeHidden();
  await expect(page.locator('.dk2b-chat [data-desk-panel="dispatcher-hidden"]')).toHaveCount(0);
  const acts = [
    act("a1", "d4", "start", "Started #d4 now, without waiting for a copy", "the user needs it now", NOW - 3 * 60 * 1000),
    act("a2", "d3", "land_first", "Moved #d3 up in the landing line", "the user waits on it", NOW - 60 * 1000),
    act("a3", "d8", "close", "Closed #d8: its work is on main (ab12cd3)", "it landed by hand", NOW - 2 * 60 * 60 * 1000),
  ];
  await cue.change((st, { acts, at }) => {
    st.desk.projects.find((p) => p.project === "cue").decisions.push(...acts);
    const ask = st.items.find((i) => i.id === "n-focus-ask");
    st.items.push({ ...ask, id: "n-lead-ask", agent_id: "", agent_name: "", created_ms: at,
      tool_input: { questions: [{ question: "Push the release now?", header: "Push", multiSelect: false, options: [{ label: "Yes" }, { label: "Not yet" }] }] } });
  }, { acts, at: NOW - 60 * 1000 });
  // Not in its chat (#688).
  const chat = page.locator(".dk-chat");
  await expect(chat.locator(".dk-act")).toHaveCount(0);
  // The lead's own ask waits on you, first, until it's answered: in Now, and in the Items card's Blocked on you
  // (desk-live, aed67d3ce2: the lead's ask heads the list, over Working); Waiting on you keeps #d1's question and #d2
  // ready to check.
  await expect(page.locator('.dkn-card .dkn-gh[data-desk2b-group="blocked"] > span')).toHaveText("1");
  await expect(page.locator(".dkn-card .dkn-b-blocked .dkn-row[data-desk-c-lead]")).toHaveCount(1);
  const youN = page.locator('.dkn-card .dkn-gh[data-desk2b-group="you"] > span');
  await expect(youN).toHaveText("2");
  const strip = page.locator(".dk2b-chat .dk2b-strip");
  await expect(strip.locator(".dk2b-chip.ask")).toHaveCount(1);
  // Its item number in Activity opens the item.
  await page.locator('.dk-right .dk-tab[data-desk-tab="moves"]').click();
  await page.locator(".dk-moves .dk-mv:not(.dk-mv-ln)").first().locator('[data-desk-chk-item="d3"]').click();
  await expect(page.locator(".dk-right .dkn-sp .dkn-ib")).toBeVisible();
  await expect(page.locator('.dkn-card .dkn-row.on[data-desk-item="d3"]')).toHaveCount(1);
  // Answered: it no longer waits on you.
  await cue.change((st) => { st.items = st.items.filter((i) => i.id !== "n-lead-ask"); });
  await page.locator(".dk-right .dkn-sback .dk-back").first().click();
  await expect(page.locator('.dkn-card .dkn-gh[data-desk2b-group="blocked"]')).toHaveCount(0);
  await expect(youN).toHaveText("2");
  await expect(strip.locator(".dk2b-chip.ask")).toHaveCount(0);
  // On a small window the strip over the chat says what waits (B2, #666); no Just did there.
  await page.setViewportSize({ width: 1100, height: 800 });
  await expect(strip.locator(".dk2b-wn")).toHaveText(/^\d+ waits? on you$/);
  await expect(strip.locator('.dk2b-chip[data-desk-chk-item="d1"]')).toHaveText("#d1");
  await expect(strip).not.toContainText("Just did");
  // In Activity, as it happened: each one with why; the old one too.
  await page.setViewportSize({ width: 1728, height: 1000 });
  await page.locator('.dk-right .dk-tab[data-desk-tab="moves"]').click();
  const moves = page.locator(".dk-moves .dk-mv:not(.dk-mv-ln)");
  await expect(moves).toHaveCount(3);
  await expect(moves.first()).toContainText("Moved #d3 up in the landing line · the user waits on it");
  await expect(moves.nth(2)).toContainText("Closed #d8: its work is on main (ab12cd3)");
});

test("Activity reads as lines, not pieces, beside a narrow chat (#392)", async ({ page, cue }) => {
  // The lead's chat has no panel (692dc421): what it did is in its Activity tab, each move one line with why and when.
  await page.setViewportSize({ width: 1500, height: 900 });
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk2b-chat .dk-pact")).toBeHidden();
  await expect(page.locator('.dk2b-chat [data-desk-panel="dispatcher-hidden"]')).toHaveCount(0);
  await cue.change((st, { acts }) => {
    const d = st.desk.projects.find((p) => p.project === "cue");
    d.decisions.push(...acts);
    Object.assign(d.items.find((i) => i.id === "d5"), { state: "working", now_on: true, said: [] });
    st.sessions.find((x) => x.session_id === "host").state = "working";
  }, { acts: [
    act("b1", "d3", "park", "Parked #d3", "the owner wants it held", NOW - 4 * 60 * 1000),
    act("b2", "d4", "start", "Started #d4 now, without waiting for a copy", "", NOW - 60 * 1000),
  ] });
  await page.locator('.dk-right .dk-tab[data-desk-tab="moves"]').click();
  await expect(page.locator('.dk-right .dk-tab[data-desk-tab="moves"]')).toHaveAttribute("aria-pressed", "true");
  const moves = page.locator(".dk-moves .dk-mv:not(.dk-mv-ln)");
  const parked = moves.filter({ hasText: "Parked #d3" });
  await expect(parked).toContainText("Parked #d3 · the owner wants it held");
  await expect(moves.first()).toContainText("Started #d4 now");
  const [h, lh] = await parked.locator(".dk-mv-line").evaluate((e) => [e.getBoundingClientRect().height, parseFloat(getComputedStyle(e).lineHeight)]);
  expect(h, "one line").toBeLessThan(lh * 1.5);
});

test("a long reopen, note and start leave the lead's chat; in Activity the note shows at most three lines, More opening the rest in place and Less folding it (#433, #452, #688)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1728, height: 1000 });
  await cue.open("desk");
  await openDesk(page);
  // A reopen like #418's: twelve lines of feedback.
  const why = Array.from({ length: 12 }, (_, n) => `${n + 1}. The right column must stay the item list while its page is open, and the chat beside it keeps its width.`).join("\n");
  const note = "Top priority from Robin: as soon as the scroll test passes, commit, land it and install, then check the item's page in the installed app at a small window and at a wide one, and only then say DONE. If the scroll test fails again, send it back to its session with what failed and the window size it failed at, and tell me before you start anything else.";
  const start = "it only reads and writes one doc, no code; it shouldn't wait behind five items that each need a copy of the code, and Robin is waiting on it right now, so it starts at once: the recovered notes go to them at seven in the morning, before they open the Desk, with what was lost and what came back.";
  await cue.change((st, { a }) => { st.desk.projects.find((p) => p.project === "cue").decisions.push(...a); }, { a: [
    act("r1", "d3", "send_back", "Reopened #d3 for cue-387", why, NOW - 3 * 60 * 1000),
    act("s1", "d4", "start", "Started #d4 now, without waiting for a copy", start, NOW - 2 * 60 * 1000),
  ] });
  await page.evaluate((t) => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: t }); renderMain(); }, `NOTE: #d2 | ${note}`);
  const chat = page.locator(".dk-chat");
  // Out of the chat (#688): no card of them there.
  await expect(chat.locator('.dk-act, [data-mg="note"]')).toHaveCount(0);
  // In Activity: the note's card, its words cut to two lines under its head, then More.
  await expect(chat.locator(".dk-cnt")).toHaveCount(0);
  await page.evaluate(() => deskOpenActivity());
  const tab = page.locator(".dk-moves");
  await expect(tab.locator(".dk-mv:not(.dk-mv-ln)", { hasText: "Reopened #d3 for cue-387" })).toContainText("12. The right column");
  const r = tab.locator('[data-mg="note"]');
  const lines = (l) => l.evaluate((e) => { const t = e.querySelector(".req-t"), c = getComputedStyle(t), lh = parseFloat(c.lineHeight); return Math.round((t.clientHeight - parseFloat(c.paddingTop) - parseFloat(c.paddingBottom)) / lh); });
  await expect(r.locator(".req-more")).toHaveText(/^more$/i);
  await expect.poll(() => lines(r), { message: "two lines under the head" }).toBeLessThanOrEqual(2);
  await expect.poll(() => r.locator(".req-t").evaluate((e) => e.scrollHeight > e.clientHeight + 1), { message: "cut" }).toBe(true);
  await expect(r.locator(".req-more")).toHaveCSS("text-transform", "capitalize");
  // More opens it in place; Less folds it again; open stays open through a redraw.
  await r.locator(".req-more").click();
  await expect(r.locator(".req-more")).toHaveText(/^less$/i);
  await expect.poll(() => lines(r)).toBeGreaterThan(3);
  await expect(r.locator(".req-t")).toContainText("tell me before you start anything else");
  await r.locator(".req-more").click();
  await expect.poll(() => lines(r)).toBeLessThanOrEqual(2);
  await r.locator(".req-more").click();
  await page.evaluate(() => renderMain());
  await expect(r.locator(".req-more")).toHaveText(/^less$/i);
  await expect(page.locator(".dk-board [title]")).toHaveCount(0);
  if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/lead-cards.png` });
});
