// One rule in every chat (#837, ui/chat/who.js): you see what you typed as your bubble and what the AI wrote as its
// reply; whatever Cue sent (marked `cue` by the hub, with its `kind`) is Cue's: a quiet line of Cue's, a card, or in
// Activity where a Desk chat moves Cue's rows. Never your bubble, whatever its words say. Checked in the five chats:
// the lead's, a session's on the Desk, an item's page, the Board's, a crew member's. Light and dark, laptop size and the
// smallest window.
import { test, expect, openDesk } from "./cue.mjs";

const ANSWER = "#852 in full, as you asked:\n- #852 bug: Your words show as Cue's | at work with cue-383 | you'll see: only your words in your bubble";
const NUDGE = "Your background agent “docs” has gone quiet. Ask it where it is and to carry on, with SendMessage (to: \"a1\").";
const NOTICE = "Main moved on while you worked: 3 new commits. Pull them in before you say DONE.";
const MINE = "How is the header going?";
const AI = "The header is centred now; I'm running its check.";
const SUMMARY = '<desk project="cue">\nDesk lead: web-lead (you)\nNeeds routing:\nNone.\n</desk>\n\n';
/** Cue's send-back of an item, in words that before the tags were read as yours (your Send back): marked Cue's, it's
 *  Cue's card, never your bubble. */
const BACK = "Desk item #d5 (Header) isn't done: the check failed on the narrow window.\nIts fixes stay in #d5; don't add new items for them.";
const CUE = [ANSWER, NUDGE, NOTICE, BACK];
/** Your words to the lead as its chat shows them: only yours, the Desk's block folded under them (#822). */
const LEAD_BUBBLE = `${MINE}+ Desk update · whole Desk`;
/** Words that only Cue's messages have: none of them may be in a bubble of yours. */
const CUE_WORDS = ["as you asked", "gone quiet", "Main moved on", "failed on the narrow window"];

/** Session `sid`'s thread: Cue's four (an answer, a nudge, one saved before kinds, a send-back), then yours, then the
 *  reply; no earlier messages in Cue's log, so the chat holds only these. */
const thread = async (cue, sid, mine = MINE) => {
  await cue.override("session_log_page", () => []);
  await cue.override("transcript_page", () => []);
  await setThread(cue, sid, mine);
};
const setThread = (cue, sid, mine) => cue.change((st, [sid, ANSWER, NUDGE, NOTICE, BACK, mine, AI]) => {
  const s = st.sessions.find((x) => x.session_id === sid), t0 = Date.now() - 600000;
  s.thread = [
    { role: "you", text: ANSWER, cue: true, kind: "answer" },
    { role: "you", text: NUDGE, cue: true, kind: "nudge" },
    { role: "you", text: NOTICE, cue: true },
    { role: "you", text: BACK, cue: true, kind: "note" },
    { role: "you", text: mine },
    { role: "agent", text: AI },
  ].map((e, n) => ({ ...e, at_ms: t0 + n * 60000, images: [] }));
}, [sid, ANSWER, NUDGE, NOTICE, BACK, mine, AI]);

/** In `chat`: your words are one bubble, the AI's words a reply, and none of Cue's words is in a bubble of yours. */
async function oneRule(chat, bubble = MINE) {
  await expect(chat.locator(".cv-you-text")).toHaveText([bubble]);
  await expect(chat.locator(".cv-agent", { hasText: AI })).toHaveCount(1);
  for (const w of CUE_WORDS) await expect(chat.locator(".cv-you", { hasText: w })).toHaveCount(0);
}

/** A Desk chat moves Cue's rows to Activity: none of Cue's words is in the chat at all. */
async function noneInChat(chat) {
  for (const w of CUE_WORDS) await expect(chat).not.toContainText(w);
}

/** Cue's rows in session `sid`'s Activity, as the Desk files them: each Cue's, never yours. */
const activityOf = (page, sid) => page.evaluate((sid) => dkActivityOf(sid).map((r) => ({ who: r.e.role === "you" && r.e.cue ? "Cue" : r.e.role, text: r.e.text })), sid);

async function cueInActivity(page, sid) {
  const rows = await activityOf(page, sid);
  for (const text of CUE) expect(rows.filter((r) => r.text === text)).toEqual([{ who: "Cue", text }]);
  expect(rows.some((r) => r.text === MINE)).toBe(false);
}

test("the lead's chat: your words a bubble with its Desk update, the lead's reply, Cue's answer, nudge and notice only in Activity under Cue", async ({ page, cue }) => {
  await cue.open("desk");
  await thread(cue, "lead", SUMMARY + MINE);
  await openDesk(page);
  const chat = page.locator(".dk-chat");
  await oneRule(chat, LEAD_BUBBLE);
  await noneInChat(chat);
  // Your words keep the "+ Desk update" fold under them, as before.
  await expect(chat.locator(".cv-you-text", { hasText: MINE }).locator(".cv-desk-note .cv-voice-ask")).toHaveText("+ Desk update · whole Desk");
  await cueInActivity(page, "lead");
  // Activity, opened: Cue's answer is there, under Cue, not as you.
  await page.evaluate(() => { deskMovesShown.set("cue", 200); deskOpenActivity(); });
  const moves = page.locator(".dk-moves");
  await expect(moves.locator(".dk-mv", { hasText: "#852" }).first()).toBeVisible();
  const metas = await page.evaluate(() => { const d = deskOf("cue"); return deskActivity(d).filter((r) => r.line?.e?.cue).map((r) => deskActMeta(r, d)); });
  expect(metas.length).toBeGreaterThanOrEqual(3);
  for (const m of metas) expect(m.who).toBe("Cue");
});

test("a session's chat on the Desk: your bubble, its reply; Cue's three are Activity's, never yours", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await thread(cue, "site");
  await page.evaluate(() => deskShow({ kind: "chat", sid: "site" }));
  const chat = page.locator('.dk-right [data-desk-chat="site"]');
  await oneRule(chat);
  await noneInChat(chat);
  await cueInActivity(page, "site");
});

test("an item's page: your bubble, its holder's reply; Cue's three are Activity's, never yours", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await thread(cue, "host");
  await page.locator('.dkn-items [data-desk-item="d5"]').first().click();
  const chat = page.locator('.dk-right [data-desk-chat="host"]');
  await oneRule(chat);
  await noneInChat(chat);
  await cueInActivity(page, "host");
});

test("the Board's chat: your bubble, its reply, Cue's three a quiet line of Cue's and its send-back Cue's NOT DONE card", async ({ page, cue }) => {
  await cue.open("desk");
  await thread(cue, "site");
  await page.evaluate(() => { view = "board"; setActive(null, "site"); renderMain(); });
  const chat = page.locator(".active-pane");
  await oneRule(chat);
  const lines = chat.locator(".cue-quiet");
  await expect(lines).toHaveCount(3);
  await expect(lines.locator(".dk-cue-t .mg-who")).toHaveText(["Cue", "Cue", "Cue"]);
  // Its words alone would read as your Send back; its mark says it's Cue's: Cue's card (#478's), from Cue.
  const back = chat.locator(".cue-msg", { hasText: "#d5" });
  await expect(back).toHaveCount(1);
  await expect(back.locator(".mg-who").first()).toHaveText("Cue");
  await lines.first().locator(".dk-cue-w").click();
  await expect(chat.locator(".cue-quiet").first().locator(".mg-full")).toContainText("as you asked");
});

/** The lead's crew member "search-centre": its words, then yours, Cue's three, and its answer. */
const crew = (cue) => cue.override("session_steps", ({ sessionId }, [now, ANSWER, NUDGE, NOTICE, MINE, AI]) => {
  if (sessionId !== "lead") return { version: 0, turns: [] };
  const say = (text, m, more = {}) => ({ t: "say", text, at_ms: now - m * 60000, ...more });
  const step = { t: "step", id: "s1", kind: "read", verb: "Read", subject: "ui/header.js", mono: true, result: "", bad: false, done: true, at_ms: now - 7 * 60000 };
  return { version: 7, turns: [], helpers: [{ id: "id-search", desc: "search-centre", kind: "general-purpose", model: "", teammate: true, name: "search-centre", step: "", started_ms: now - 9 * 60000, finished_ms: now - 60000, last_ms: now - 60000, tokens: 0, running: false, more: 0,
    items: [say("Looking at the header.", 8), step, say(ANSWER, 6, { role: "you", cue: true, kind: "answer" }), say(NUDGE, 5, { role: "you", cue: true, kind: "nudge" }),
      say(NOTICE, 4, { role: "you", cue: true }), say(MINE, 3, { role: "you" }), step, say(AI, 1)] }] };
}, [Date.now(), ANSWER, NUDGE, NOTICE, MINE, AI]);

test("a crew member's chat: your bubble, its words and answer, and each of Cue's a quiet line of Cue's", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await crew(cue);
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  await page.locator('.dk-right .dk-pg [data-desk-crew="id-search"]').click();
  const chat = page.locator('.dk-right .dkn-crew[data-desk-page="crew"] .dk-crewpg .ag-chat');
  await expect(chat.locator(".cv-you-text")).toHaveText([MINE]);
  await expect(chat.locator(".cue-quiet")).toHaveCount(3);
  await expect(chat.locator(".cue-quiet .dk-cue-t .mg-who")).toHaveText(["Cue", "Cue", "Cue"]);
  // Its own words (with their Send to…), only them.
  await expect(chat.locator(".stx-say")).toHaveCount(1);
  await expect(chat.locator(".stx-say")).toContainText("Looking at the header.");
  await expect(chat.locator(".bg-view-reply:not(.ag-ask) .msg")).toHaveText(AI);
  for (const w of CUE_WORDS) {
    await expect(chat.locator(".cv-you", { hasText: w })).toHaveCount(0);
    await expect(chat.locator(".stx-say, .bg-view-reply", { hasText: w })).toHaveCount(0);
  }
});

test("a stuck background agent's Resume goes to its session as Cue's nudge: Cue's line in its chat, never yours", async ({ page, cue }) => {
  await cue.open("board-waiting", { frozen: true, flags: "many=1" });
  await page.evaluate(() => openSession("names"));
  await page.locator('[data-bg-tab="agents"]').first().click().catch(() => {});
  await page.locator('.ag-row[data-key="h:h4"] [data-ag-wake="resume"]').first().dispatchEvent("click");
  const sent = (await cue.called("send_to_session")).args;
  expect(sent).toMatchObject({ sessionId: "names", now: true, kind: "nudge" });
  expect(sent.cue).toBeUndefined();
  const copy = await page.evaluate(() => sessionOf("names").thread.at(-1));
  expect(copy).toMatchObject({ role: "you", cue: true, kind: "nudge" });
  const html = await page.evaluate(() => { const s = sessionOf("names"); return chatHtml(null, s, "claude"); });
  expect(html).toContain("cue-quiet");
  expect(html).not.toMatch(/cv-you-text[^>]*>[^<]*has gone quiet/);
});

for (const scheme of ["light", "dark"]) {
  for (const [size, viewport] of [["laptop", { width: 1440, height: 900 }], ["smallest", { width: 900, height: 600 }]]) {
    test(`the rule looks right: the Board's Cue lines, the lead's chat and its Activity (${scheme}, ${size})`, async ({ page, cue }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await page.setViewportSize(viewport);
      await cue.open("desk");
      await thread(cue, "site");
      await page.evaluate(() => { view = "board"; setActive(null, "site"); renderMain(); });
      const board = page.locator(".active-pane");
      await oneRule(board);
      await expect(board.locator(".cue-quiet")).toHaveCount(3);
      // Cue's lines: no left stripe, no hover text, inside the chat.
      const look = await page.evaluate(() => [...document.querySelectorAll(".active-pane .cue-quiet")].map((el) => ({
        border: getComputedStyle(el).borderLeftWidth, titled: !!el.querySelector("[title]") || el.hasAttribute("title"),
        fits: el.getBoundingClientRect().right <= el.closest(".active-pane").getBoundingClientRect().right + 0.5,
      })));
      for (const l of look) expect(l).toEqual({ border: "0px", titled: false, fits: true });
      if (process.env.SHOT_DIR) await board.screenshot({ path: `${process.env.SHOT_DIR}/chat-rule-board-${scheme}-${size}.png` });
      await openDesk(page);
      await thread(cue, "lead", SUMMARY + MINE);
      const lead = page.locator(".dk-chat");
      await oneRule(lead, LEAD_BUBBLE);
      await noneInChat(lead);
      if (process.env.SHOT_DIR) await lead.screenshot({ path: `${process.env.SHOT_DIR}/chat-rule-lead-${scheme}-${size}.png` });
      await page.evaluate(() => { deskMovesShown.set("cue", 200); deskOpenActivity(); });
      await expect(page.locator(".dk-moves .dk-mv", { hasText: "#852" }).first()).toBeVisible();
      if (process.env.SHOT_DIR) await page.locator(".dk-moves").first().screenshot({ path: `${process.env.SHOT_DIR}/chat-rule-activity-${scheme}-${size}.png` });
    });

    test(`the rule looks right: a Desk session's chat, an item's page and a crew member's chat (${scheme}, ${size})`, async ({ page, cue }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await page.setViewportSize(viewport);
      await cue.open("desk");
      await thread(cue, "site");
      await thread(cue, "host");
      await crew(cue);
      await openDesk(page);
      const shot = (el, name) => (process.env.SHOT_DIR ? el.screenshot({ path: `${process.env.SHOT_DIR}/chat-rule-${name}-${scheme}-${size}.png` }) : null);
      // A Desk session's chat: your bubble, its reply; Cue's are Activity's.
      await page.evaluate(() => deskShow({ kind: "chat", sid: "site" }));
      const sess = page.locator('.dk-right [data-desk-chat="site"]');
      await oneRule(sess);
      await noneInChat(sess);
      await shot(page.locator(".dk-right"), "session");
      // An item's page: the same.
      await page.locator('.dkn-items [data-desk-item="d5"]').first().click();
      const item = page.locator('.dk-right [data-desk-chat="host"]');
      await oneRule(item);
      await noneInChat(item);
      await shot(page.locator(".dk-right"), "item");
      // A crew member's chat: your bubble, Cue's three its quiet lines (no stripe, no hover text, inside the chat).
      await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
      await page.locator('.dk-right .dk-pg [data-desk-crew="id-search"]').click();
      const crewChat = page.locator('.dk-right .dkn-crew[data-desk-page="crew"] .dk-crewpg .ag-chat');
      await expect(crewChat.locator(".cv-you-text")).toHaveText([MINE]);
      await expect(crewChat.locator(".cue-quiet")).toHaveCount(3);
      for (const w of CUE_WORDS) await expect(crewChat.locator(".cv-you", { hasText: w })).toHaveCount(0);
      const look = await page.evaluate(() => [...document.querySelectorAll(".dk-crewpg .cue-quiet")].map((el) => ({
        border: getComputedStyle(el).borderLeftWidth, titled: !!el.querySelector("[title]") || el.hasAttribute("title"),
        fits: el.getBoundingClientRect().right <= el.closest(".dk-crewpg").getBoundingClientRect().right + 0.5,
      })));
      for (const l of look) expect(l).toEqual({ border: "0px", titled: false, fits: true });
      await shot(page.locator(".dk-right"), "crew");
    });
  }
}

/** No earlier messages from Cue's log: a chat holds only what a test gives it. */
const noEarlier = async (cue) => {
  await cue.override("session_log_page", () => []);
  await cue.override("transcript_page", () => []);
};

// Cue's kinds that keep a design of their own (#837): each is Cue's, by its kind, never your bubble.
const HAND = "Desk item #d5 (feature): Header stays put\nYou'll see: the header stays put as you scroll\nWhy you: you built the header";
const CREW = "Stop your teammate docs now (TaskStop, task id a1), and say so.";

test("a hand-off (brief) is its hand-off card and a crew message (crew) Cue's CREW card on the Board, never your bubble", async ({ page, cue }) => {
  await cue.open("desk");
  await noEarlier(cue);
  await cue.change((st, [HAND, CREW, MINE]) => {
    const s = st.sessions.find((x) => x.session_id === "site"), t0 = Date.now() - 600000;
    s.thread = [{ role: "you", text: HAND, cue: true, kind: "brief", at_ms: t0 }, { role: "you", text: CREW, cue: true, kind: "crew", at_ms: t0 + 60000 },
      { role: "you", text: MINE, at_ms: t0 + 120000 }].map((e) => ({ ...e, images: [] }));
  }, [HAND, CREW, MINE]);
  await page.evaluate(() => { view = "board"; setActive(null, "site"); renderMain(); });
  const chat = page.locator(".active-pane");
  await expect(chat.locator(".cv-you-text")).toHaveText([MINE]);
  await expect(chat.locator(".mg-hand")).toHaveCount(1);
  await expect(chat.locator(".mg-hand")).toContainText("Header stays put");
  const crewCard = chat.locator(".mg.cue-you", { hasText: "docs" });
  await expect(crewCard).toHaveCount(1);
  await expect(crewCard.locator(".mg-who").first()).toHaveText("Cue");
  await expect(crewCard.locator(".mg-tag")).toHaveText("Crew");
});

test("another Desk's session (peer) is its band in the lead's chat, never your bubble", async ({ page, cue }) => {
  await cue.open("desk");
  await noEarlier(cue);
  const said = "Can #452 wait about 20 min for my scraper fix?";
  await cue.change((st, said) => {
    const site = st.desk.projects.find((p) => p.project === "website"), at = Date.now() - 120000;
    site.name = "Docs site";
    site.items = [...(site.items || []), { id: "w118", project: "website", kind: "task", title: "Fix the scraper", state: "working", created: at, updated: at, holder: { kind: "session", sid: "site", name: "site-lead" }, from: "you" }];
    st.sessions.find((x) => x.session_id === "lead").thread.push({ role: "peer", from: "site-lead", from_sid: "site", text: said, at_ms: at });
  }, said);
  await openDesk(page);
  const chat = page.locator(".dk-chat");
  await expect(chat.locator('.dk-band[data-band="from"]', { hasText: said })).toHaveCount(1);
  await expect(chat.locator(".cv-you", { hasText: said })).toHaveCount(0);
});

test("a session's question for the lead (question) is Activity's TO LEAD card with the lead's answer under it, never your bubble", async ({ page, cue }) => {
  await cue.open("desk");
  await noEarlier(cue);
  const words = "is stuck in a loop: Cue keeps sending it back. Please close it by hand.";
  const asked = `From cue-790 on #d2 (Away is just on or off):\n${words}\nAnswer it with NOTE: #d2 | <your words>.`;
  const answer = "I'll close #d2 by hand once its check is in.";
  await cue.change((st, [asked, answer]) => {
    const at = Date.now() - 120000;
    st.sessions.find((x) => x.session_id === "lead").thread.push({ role: "you", text: asked, cue: true, kind: "question", at_ms: at }, { role: "agent", text: answer, at_ms: at + 12000 });
  }, [asked, answer]);
  await openDesk(page);
  const chat = page.locator(".dk-chat");
  await expect(chat).not.toContainText("stuck in a loop");
  await expect(chat).not.toContainText(answer);
  await page.evaluate(() => { deskMovesShown.set("cue", 200); deskOpenActivity(); });
  const row = page.locator(".dk-moves .dk-mv", { hasText: "stuck in a loop" });
  await expect(row).toHaveCount(1);
  await expect(row.locator(".dk-ans")).toContainText(answer);
});

for (const where of ["the Board", "a Desk session's chat"]) {
  test(`Cue's kept and queued messages (a kind, no cue mark, as the hub sends them) are Cue's lines in ${where}, never your bubble`, async ({ page, cue }) => {
    await cue.open("desk");
    await noEarlier(cue);
    await cue.change((st, [NOTICE, NUDGE, MINE]) => {
      const s = st.sessions.find((x) => x.session_id === "site"), now = Date.now();
      Object.assign(s, { state: "working", since_ms: now - 5000 });
      s.thread = [{ role: "you", text: MINE, at_ms: now - 60000, images: [] }];
      s.held = { role: "you", text: NOTICE, kind: "notice", at_ms: now - 30000, images: [] };
      s.queued = { role: "you", text: NUDGE, kind: "nudge", at_ms: now - 20000, images: [] };
    }, [NOTICE, NUDGE, MINE]);
    let chat;
    if (where === "the Board") {
      await page.evaluate(() => { view = "board"; setActive(null, "site"); renderMain(); });
      chat = page.locator(".active-pane");
    } else {
      await openDesk(page);
      await page.evaluate(() => deskShow({ kind: "chat", sid: "site" }));
      chat = page.locator('.dk-right [data-desk-chat="site"]');
    }
    await expect(chat.locator(".cv-you-text")).toHaveText([MINE]);
    for (const w of ["Main moved on", "gone quiet"]) await expect(chat.locator(".cv-you", { hasText: w })).toHaveCount(0);
    const lines = chat.locator(".cue-quiet");
    await expect(lines).toHaveCount(2);
    await expect(lines.locator(".dk-cue-t .mg-v")).toContainText(["goes when its turn ends", "queued"]);
  });
}

test("a crew member's chat: Cue's line after its answer sits under the answer, one before it above, by their time", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.override("session_steps", ({ sessionId }, [now, NOTICE, NUDGE, AI]) => {
    if (sessionId !== "lead") return { version: 0, turns: [] };
    const say = (text, m, more = {}) => ({ t: "say", text, at_ms: now - m * 60000, ...more });
    const step = { t: "step", id: "s1", kind: "read", verb: "Read", subject: "ui/header.js", mono: true, result: "", bad: false, done: true, at_ms: now - 7 * 60000 };
    return { version: 7, turns: [], helpers: [{ id: "id-search", desc: "search-centre", kind: "general-purpose", model: "", teammate: true, name: "search-centre", step: "", started_ms: now - 9 * 60000, finished_ms: now - 60000, last_ms: now - 30000, tokens: 0, running: false, more: 0,
      items: [say("Looking at the header.", 8), step, say(NUDGE, 3, { role: "you", kind: "nudge" }), say(AI, 2), say(NOTICE, 1, { role: "you", cue: true })] }] };
  }, [Date.now(), NOTICE, NUDGE, AI]);
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  await page.locator('.dk-right .dk-pg [data-desk-crew="id-search"]').click();
  const chat = page.locator('.dk-right .dkn-crew[data-desk-page="crew"] .dk-crewpg .ag-chat');
  await expect(chat.locator(".bg-view-reply:not(.ag-ask) .msg")).toHaveText(AI);
  const order = await chat.evaluate((el) => [...el.children].map((c) => (c.classList.contains("cue-quiet") ? `cue:${c.querySelector(".dk-cue-w")?.textContent}` : c.classList.contains("bg-view-reply") && !c.classList.contains("ag-ask") ? "answer" : "")).filter(Boolean));
  expect(order).toEqual(["cue:Asked it to wake its background agent docs", "answer", "cue:Main moved on while you worked: 3 new commits."]);
});
