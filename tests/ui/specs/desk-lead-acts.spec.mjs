// What the lead does on the Desk (#360): anything that needs you, its own asks too, waits in the Dispatcher's
// panel until it's answered; what it did on its own (closed, started, moved up, took back a copy, answered a
// session) is a line in its chat as it happens, and its last few show in the panel for a while, then drop off.
import { test, expect, openDesk, deskShowPanel } from "./cue.mjs";
import { NOW } from "./clock.mjs";

const s = (ms) => Math.floor(ms / 1000);
const act = (id, item, act, text, why, at) => ({ id, project: "cue", item_id: item, by: "web-lead", sid: "lead", text, why, at: s(at), undo: "none", act });

test("the lead's own moves show in its chat and, for a while, in the panel; its ask waits there till answered (#360)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1728, height: 1000 });
  await cue.open("desk");
  await openDesk(page);
  await deskShowPanel(page);   // B2 hides the panel till Show panel (#666)
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
  // In its chat, as it happened: each one, the item's number opening it, and why; the old one too.
  const chat = page.locator(".dk-chat");
  await expect(chat.locator(".dk-act")).toHaveCount(3);
  const a2 = chat.locator('.dk-act[data-key="act:a2"]');
  await expect(a2.locator(".mg-lh")).toHaveText("Moved #d3 up in the landing line");
  await expect(a2.locator(".mg-full")).toHaveText("the user waits on it");
  await expect(a2.locator(".req-more")).toHaveCount(0);
  await expect(chat.locator('.dk-act[data-key="act:a3"]')).toContainText("Closed #d8: its work is on main (ab12cd3)");
  // In the panel: only the last ones, newest first; the close two hours ago has dropped off.
  const panel = page.locator(".dk-right .dk-panel");
  const did = panel.locator(".dk-psec", { hasText: "Just did" }).locator(".dk-pdid");
  await expect(did).toHaveCount(2);
  await expect(did.first().locator(".dk-pdt")).toHaveText("Moved #d3 up in the landing line");
  await expect(did.first().locator(".dk-prs")).toHaveText("the user waits on it · 1m");
  await expect(did.nth(1)).toContainText("Started #d4 now");
  // The lead's own ask waits on you there, first, until it's answered.
  const waits = panel.locator(".dk-psec:not(.dk-pnow)", { hasText: "Waits on you" });
  await expect(waits.locator(".dk-psh")).toHaveText("Waits on you · 2");
  await expect(waits.locator(".dk-prow").first()).toContainText("web-lead");
  await expect(panel.locator(".dk-pnow")).toContainText("It asks you something");
  // Its item number opens the item.
  await did.first().locator('[data-desk-chk-item="d3"]').click();
  await expect(page.locator('.dk-right .dk-pg[data-desk-page="item"]')).toBeVisible();
  // Answered: it no longer waits on you.
  await cue.change((st) => { st.items = st.items.filter((i) => i.id !== "n-lead-ask"); });
  await page.locator(".dk-right .dk-back").first().click();
  await expect(page.locator(".dk-right .dk-panel .dk-psec:not(.dk-pnow)", { hasText: "Waits on you" }).locator(".dk-psh")).toHaveText("Waits on you · 1");
  // On a small window the panel folds into the Now line: what waits and the newest move are in it.
  await page.setViewportSize({ width: 1100, height: 800 });
  await expect(page.locator(".dk-right .dk-pnow .dk-pfc")).toContainText("1 waiting");
  await expect(page.locator(".dk-right .dk-pnow .dk-pfc")).toContainText("Just did: Moved #d3 up in the landing line");
});

test("Just did and Working read as lines, not pieces, at the panel's narrowest (#392)", async ({ page, cue }) => {
  // The narrowest window that still shows the panel beside the chat: the chat under 660px, the panel 272px.
  await page.setViewportSize({ width: 1500, height: 900 });
  await cue.open("desk");
  await openDesk(page);
  await deskShowPanel(page);   // B2 hides the panel till Show panel (#666)
  await cue.change((st, { acts }) => {
    const d = st.desk.projects.find((p) => p.project === "cue");
    d.decisions.push(...acts);
    Object.assign(d.items.find((i) => i.id === "d5"), { state: "working", now_on: true, said: [] });
    st.sessions.find((x) => x.session_id === "host").state = "working";
  }, { acts: [
    act("b1", "d3", "park", "Parked #d3", "the owner wants it held", NOW - 4 * 60 * 1000),
    act("b2", "d4", "start", "Started #d4 now, without waiting for a copy", "", NOW - 60 * 1000),
  ] });
  const panel = page.locator(".dk-right .dk-panel");
  await expect(panel).toBeVisible();
  const box = await panel.boundingBox();
  expect(box.width).toBeLessThanOrEqual(272);
  // B2 (#666): the lead's card is 878px wide with the panel inside it; its chat beside the panel is the narrow part.
  const [chatPart, side] = await page.evaluate(() => [".dk-right .dk-pgm", ".dk-right .dk-panel"].map((q) => { const r = document.querySelector(q).getBoundingClientRect(); return { x: r.x, right: r.right, w: r.width }; }));
  expect(chatPart.w).toBeLessThan(660);
  expect(side.x).toBeGreaterThanOrEqual(chatPart.right - 1);
  const did = panel.locator(".dk-psec", { hasText: "Just did" }).locator(".dk-pdid");
  // What it did on one line, why and when under it.
  const parked = did.nth(1);
  await expect(parked.locator(".dk-pdt")).toHaveText("Parked #d3");
  await expect(parked.locator(".dk-prs")).toHaveText("the owner wants it held · 4m");
  await expect(did.first().locator(".dk-prs")).toHaveText("1m");
  const lineH = async (l) => l.evaluate((e) => [e.getBoundingClientRect().height, parseFloat(getComputedStyle(e).lineHeight)]);
  for (const l of [parked.locator(".dk-pdt"), parked.locator(".dk-prs")]) {
    const [h, lh] = await lineH(l);
    expect(h, "one line").toBeLessThan(lh * 1.5);
  }
  // The link under it says More and opens Dispatcher Actions.
  await expect(panel.locator(".dk-pall")).toHaveText("More");
  // Working: "host on #d5", one line.
  const work = panel.locator(".dk-psec", { hasText: "Working" }).locator(".dk-prow").first();
  await expect(work).toHaveText(/^\S+ on #d5$/);
  const [wh] = await lineH(work.locator(".dk-prt"));
  expect(wh, "one line").toBeLessThan(24);
  if (process.env.SHOT_DIR) await panel.screenshot({ path: `${process.env.SHOT_DIR}/just-did-working.png` });
  await panel.locator(".dk-pall").click();
  await expect(page.locator('.dk-right .dk-tab[data-desk-tab="moves"]')).toHaveAttribute("aria-pressed", "true");
});

test("a long reopen, note and start each show at most three lines in the lead's chat, More opening the rest in place and Less folding it (#433, #452)", async ({ page, cue }) => {
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
  const cards = [chat.locator('.dk-act[data-key="act:r1"]'), chat.locator('.dk-act[data-key="act:s1"]'), chat.locator('[data-mg="note"]')];
  await expect(cards[0].locator(".mg-lh")).toHaveText("Reopened #d3 for cue-387");
  // Its text's lines: the box's height less its padding (a why is the family's .mg-full, a padded fill, #501).
  const lines = (l) => l.evaluate((e) => { const t = e.querySelector(".req-t"), c = getComputedStyle(t), lh = parseFloat(c.lineHeight); return Math.round((t.clientHeight - parseFloat(c.paddingTop) - parseFloat(c.paddingBottom)) / lh); });
  for (const card of cards) {
    // Its head, then its words cut to two lines, then More.
    await expect(card.locator(".req-more")).toHaveText(/^more$/i);
    await expect.poll(() => lines(card), { message: "two lines under the head" }).toBeLessThanOrEqual(2);
    await expect.poll(() => card.locator(".req-t").evaluate((e) => e.scrollHeight > e.clientHeight + 1), { message: "cut" }).toBe(true);
    await expect(card.locator(".req-more")).toHaveCSS("text-transform", "capitalize");
  }
  // More opens the whole reopen in place; Less folds it again.
  const r = cards[0];
  await r.locator(".req-more").click();
  await expect(r.locator(".req-more")).toHaveText(/^less$/i);
  await expect.poll(() => lines(r)).toBeGreaterThan(10);
  await expect(r.locator(".req-t")).toContainText("12. The right column");
  // In the panel's Just did, the same reopen is cut to two lines too.
  const did = page.locator(".dk-right .dk-panel .dk-pdid", { hasText: "Reopened" }).locator(".dk-prs");
  await expect.poll(() => did.evaluate((e) => Math.round(e.getBoundingClientRect().height / parseFloat(getComputedStyle(e).lineHeight)))).toBeLessThanOrEqual(2);
  await r.locator(".req-more").click();
  await expect.poll(() => lines(r)).toBeLessThanOrEqual(2);
  // Open stays open through a redraw; the other cards stay cut.
  await r.locator(".req-more").click();
  await page.evaluate(() => renderMain());
  await expect(r.locator(".req-more")).toHaveText(/^less$/i);
  await expect.poll(() => lines(cards[1])).toBeLessThanOrEqual(2);
  await expect(chat.locator("[title]")).toHaveCount(0);
  // Words that fit in two lines have no More.
  await cue.change((st, { a }) => { st.desk.projects.find((p) => p.project === "cue").decisions.push(a); },
    { a: act("s2", "d5", "start", "Started #d5 now, without waiting for a copy", "it is mocks only: no code, no copy needed, and it should not wait behind five items that need one today", NOW - 60 * 1000) });
  const short = chat.locator('.dk-act[data-key="act:s2"]');
  await expect(short.locator(".mg-full")).toBeVisible();
  await expect(short.locator(".req-more")).toBeHidden();
  if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/lead-cards.png` });
});
