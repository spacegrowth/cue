// The Desk's sessions on the left: their order, the three columns, each row's size, cache and live step.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";
import { crewNotLead, blinkOf, openSessions } from "./desk.mjs";

test("a session on no item says the first line of its latest reply, when it came, its size and cache, though no card brings them (#185)", async ({ page, cue }) => {
  await cue.open("desk", { flags: "cost=cold" });
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  await cue.change((st) => st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", thread: [] }));
  await cue.change((st) => st.desk.projects.find((d) => d.project === "cue").members.push("spare"));
  // Cold and on no item, it sits under Older (#485): open it.
  await page.locator('.dk-sessions [data-desk-older="side:cue"]').click({ position: { x: 24, y: 6 } });
  const row = page.locator('.dk-sessions [data-desk-chat-of="spare"]');
  await expect(row.locator(".dk-s2 .dk-w")).toHaveText("Done: spare's rows read as rows.");
  await expect(row.locator(".dk-size")).toHaveText("4h ago · 600k · cold");
  await expect(row.locator("[data-desk-member]")).toHaveCount(0);
});

test("the sessions on the left: working first, then each group most recent first (#184)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  await cue.change((st) => {
    const site = st.sessions.find((x) => x.session_id === "site");
    for (const [sid, h] of [["older", 5], ["newest", 1], ["middle", 3]]) st.sessions.push({ ...site, session_id: sid, project: "cue", name: sid, state: "idle", since_ms: Date.now() - h * 3600000, thread: [] });
    // Their caches still warm, so they stay in Idle (a cold one folds under Older, #485).
    st.insight = Object.fromEntries(["older", "newest", "middle"].map((sid) => [sid, { tokens: 50000, until_ms: Date.now() + 30 * 60000 }]));
  });
  await cue.change((st) => st.desk.projects.find((d) => d.project === "cue").members.push("older", "newest", "middle"));
  const ids = () => page.locator(".dk-sessions .dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-sdiv") ? "—" : el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}` : el.dataset.deskChatOf || el.dataset.deskCrew));
  await expect.poll(ids).toEqual(["# Working", "host", "# Idle", "newest", "middle", "older"]);
  // One comes back from its reply: it moves to the top of its group.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "older").since_ms = Date.now(); });
  await expect.poll(ids).toEqual(["# Working", "host", "# Idle", "older", "newest", "middle"]);
});

test("the lead's chat in the middle, the items at the right; All N sessions puts the project's sessions in the chat's place (#666)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const cards = () => page.locator(".dk-board > :is(.dk-card, .dk-items)");
  const cols = () => cards().evaluateAll((els) => els.map((el) => [...el.classList].find((c) => ["dk-sessions", "dk2b-chat", "dk-items"].includes(c))));
  expect(await cols()).toEqual(["dk2b-chat", "dk-items"]);
  const xs = async () => cards().evaluateAll((els) => els.map((el) => el.getBoundingClientRect().x));
  const [x0, x1] = await xs();
  expect(x0 < x1).toBe(true);
  await openSessions(page);
  expect(await cols()).toEqual(["dk-sessions", "dk-items"]);
  expect(await xs()).toEqual([x0, x1]);
  // A Desk member with no item.
  await cue.change((st) => st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", thread: [] }));
  // The Desk's lead as a card in the head strip (a "Lead" pill by its name, #446), with no crew, not even old ones; the list under it: Working
  // first (host-lead), then Idle.
  await cue.change((st) => { const l = st.sessions.find((x) => x.session_id === "lead"); l.team.push({ ...l.team[0], agent_id: "id-old", call: "id-old", name: "old", state: "committed" }); });
  await cue.change((st) => st.desk.projects.find((d) => d.project === "cue").members.push("spare"));
  const left = page.locator(".dk-sessions");
  await expect(left.locator('.dk-strip .dk-lcard[data-desk-lead-chat="lead"]')).toHaveCount(1);
  await expect(left.locator('.dk-lcard .dk-pill')).toHaveCount(0);
  await expect(left.locator(".dk-strip .dk-lbar + .dk-lcard")).toHaveCount(1);
  await expect(left.locator(".dk-strip .dk-lh")).toHaveCount(0);
  // What each row is as the chat headers' icon before its name, the Dispatcher's signpost on the lead's card (#202).
  await expect(left.locator('.dk-lcard .dk-s1 > .dk-ric[data-desk-type="desk"] svg.dk-kic')).toHaveCount(1);
  await expect(left.locator('[data-desk-chat-of="host"] .dk-ric')).toHaveAttribute("aria-label", "Session");
  // Rows run edge to edge, so their hover and lime reach both sides of the column.
  const [rx, rw, cx, cw] = await left.locator('[data-desk-chat-of="host"]').evaluate((el) => { const r = el.getBoundingClientRect(), c = el.closest(".dk-scroll").getBoundingClientRect(); return [r.x, r.width, c.x, c.width]; });
  expect([rx, Math.round(rw)]).toEqual([cx, Math.round(cw)]);
  // ‹ back: the lead's chat again, in the sessions' place; the items at the right, each row's number under its title.
  await page.locator(".dk-sessions .dk-back").click();
  const items = page.locator(".dk-items");
  await expect(items.locator('[data-desk-item="d1"] .dkn-sub .dkn-id')).toHaveText("#d1");
  await openSessions(page);
  await expect(left.locator(".dk-srow.crew")).toHaveCount(0);
  await expect(left.locator("[data-desk-done-fold]")).toHaveCount(0);
  await expect(left.locator(".dk-strip .dk-sub")).toHaveText("2 sessions · 1 working");
  const rows = await left.locator(".dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-sdiv") ? "—" : el.matches(".dk-gh") ? `# ${el.textContent.trim()}`
    : `${el.matches(".crew") ? "  " : ""}${el.dataset.deskLeadChat || el.dataset.deskChatOf || el.dataset.deskCrew}${el.querySelector(".dk-pill") ? ` [${el.querySelector(".dk-pill").textContent}]` : ""}`));
  expect(rows).toEqual(["# Working 1", "host", "# Idle 1", "spare"]);
  await expect(left.locator('[data-desk-chat-of="host"] .dk-s2')).toContainText("#d5");
  // On no item: no item line (#179); with no reply known yet, a blank line keeps the row's shape (#485).
  await expect(left.locator('[data-desk-chat-of="spare"] .dk-s2')).toHaveText("\u00a0");
  await expect(left.locator('[data-desk-chat-of="spare"] [data-desk-member]')).toHaveCount(0);
  await expect(left.locator('[data-desk-chat-of="host"] [data-desk-member]')).toHaveCount(0);
  // The lead's chat shows: its card is lit, alone.
  await expect(left.locator(".dk-srow.on, .dk-lcard.on")).toHaveCount(1);
  await expect(left.locator('[data-desk-lead-chat="lead"]')).toHaveClass(/\bon\b/);
  // Another session: its chat in the list's place (#666), ‹ All sessions back to the list, its row lit in place there.
  const back = async () => { await page.locator(".dk-right :is(.dk-ph, .dkn-sback) .dk-back").first().click(); await left.waitFor(); };
  await left.locator('[data-desk-chat-of="host"]').click();
  await expect(page.locator('.dk-right [data-desk-chat="host"]')).toHaveCount(1);
  // A session's chat (#744): back, the Board's top bar with its name; under it what it's on (its number opening the item).
  const sp = page.locator('.dk-right .dkn-sp[data-desk-page="session"]');
  await expect(sp.locator(".dkn-sback .dk-back")).toHaveText(/^‹ /);
  await expect(sp.locator(".ap-head")).toContainText("host-lead");
  await expect(sp.locator(".dkn-smeta")).toHaveText(/^on #d5 · Hosted sessions can't open the canvas/);
  await back();
  await expect(left.locator(".dk-srow.on")).toHaveAttribute("data-desk-chat-of", "host");
  // One with nothing on the Desk: back, its name.
  await left.locator('[data-desk-chat-of="spare"]').click();
  await expect(sp.locator(".ap-head")).toContainText("spare");
  if (process.env.SHOT_DIR) for (const c of ["light", "dark"]) { await page.emulateMedia({ colorScheme: c }); await sp.locator(".ap-head").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-sess-head-${c}.png` }); }
  await page.emulateMedia({ colorScheme: "light" });
  await back();
  await expect(left.locator(".dk-srow.on")).toHaveAttribute("data-desk-chat-of", "spare");
  await left.locator('[data-desk-chat-of="host"]').click();
  const hostChat = page.locator('.dk-right [data-desk-chat="host"]');
  await expect(hostChat).toHaveCount(1);
  await expect(page.locator(".dk-right .dk-box textarea")).toHaveAttribute("placeholder", /^Talk to /);
  // It's working: Esc is its Stop first, as on the Board (#494): once arms it, twice stops it, its chat still open.
  await page.keyboard.press("Escape");
  await expect(hostChat).toHaveCount(1);
  await page.keyboard.press("Escape");
  await cue.called("interrupt_session", { sessionId: "host" });
  await expect(hostChat).toHaveCount(1);
  // Stopped: Esc goes back, to the sessions, its row lit; the items at the right all along (B2: the sessions and a
  // session's chat take the lead's chat's place on the left).
  await expect.poll(() => page.evaluate(() => sessionOf("host").state)).toBe("waiting");
  await page.keyboard.press("Escape");
  await expect(left.locator(".dk-srow.on")).toHaveAttribute("data-desk-chat-of", "host");
  await expect(page.locator(".dk-board > .dk-items")).toHaveCount(1);
  // Another session's crew sit under it; a crew row opens its pane, lit in the list when back.
  await crewNotLead(cue);
  await expect(left.locator('[data-desk-crew="id-focus"] .dk-st')).toHaveText("needs you");
  await left.locator('[data-desk-crew="id-search"]').click();
  await expect(page.locator(".dk-right .dk-pg h1 .dk-tname")).toHaveText("search-centre");
  // Opened from All sessions, its ‹ goes back there, as a session's chat does.
  await expect(page.locator(".dk-right .dk-ph .dk-back").first()).toHaveText("‹ All sessions");
  await back();
  await expect(left.locator(".dk-srow.on")).toHaveAttribute("data-desk-crew", "id-search");
  await expect(left.locator(".dk-sfoot").getByRole("button")).toHaveAccessibleName("Add session");
});

test("each session row says its size and cache: \"142k · cache 12m\", \"cold\", amber in its last 5 minutes", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  // Its log read and no numbers in it: it says so, never a blank line (#592).
  await expect(page.locator('.dk-sessions [data-desk-chat-of="host"] .dk-size')).toHaveText("no reply yet · no cache");
  await cue.change((st) => {
    const t = Date.now();
    st.insight = { lead: { tokens: 142000, until_ms: t + 12 * 60000 - 2000 }, host: { tokens: 88000, until_ms: t + 3 * 60000 } };
    st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", thread: [] });
    st.insight.spare = { tokens: 51000, until_ms: t - 60000 };
    st.desk.projects.find((d) => d.project === "cue").members.push("spare");
  });
  const left = page.locator(".dk-sessions");
  // spare is cold and on no item, so it sits under Older (#485): open it.
  await left.locator('[data-desk-older="side:cue"]').click({ position: { x: 24, y: 6 } });
  await expect(left.locator('[data-desk-lead-chat="lead"] .dk-size')).toHaveText("142k · cache 12m");
  await expect(left.locator('[data-desk-lead-chat="lead"] .dk-size')).not.toHaveClass(/hot/);
  await expect(left.locator('[data-desk-chat-of="host"] .dk-size')).toHaveText("88k · cache 3m");
  await expect(left.locator('[data-desk-chat-of="host"] .dk-size')).toHaveClass(/hot/);
  await expect(left.locator('[data-desk-chat-of="spare"] .dk-size')).toHaveText("51k · cold");
  // No context bar on any row: the size is the number (#592).
  await expect(left.locator(".dk-meter")).toHaveCount(0);
  // Each with its state and what it's on.
  await expect(left.locator('[data-desk-chat-of="host"] .dk-st')).toHaveText("working");
  await expect(left.locator('[data-desk-chat-of="host"] .dk-s2')).toContainText("#d5");
  await expect(left.locator('[data-desk-chat-of="spare"] .dk-st')).toHaveText("idle");
  await cue.change((st) => { st.insight.lead = { until_ms: Date.now() + 4 * 60000 - 2000 }; });
  await expect(left.locator('[data-desk-lead-chat="lead"] .dk-size')).toHaveText("size unknown · cache 4m");
  // An agent whose log doesn't say how long it caches (Codex, Pi): said, not left out.
  await cue.change((st) => { st.insight.lead = { tokens: 90000, until_ms: 0 }; });
  await expect(left.locator('[data-desk-lead-chat="lead"] .dk-size')).toHaveText("90k · cache unknown");
  // The model its log says it runs on goes first (#620); none said, none shown.
  await cue.change((st) => { st.insight.host = { ...st.insight.host, model: "claude-opus-5-5" }; });
  await expect(left.locator('[data-desk-chat-of="host"] .dk-size')).toHaveText(/^Opus 5\.5 · 88k · cache \dm$/);
  await expect(left.locator('[data-desk-chat-of="spare"] .dk-size')).toHaveText("51k · cold");
});

test("a working session's dot blinks and its current step shows under its title, with its time, live", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  await cue.change((st) => {
    const h = st.sessions.find((x) => x.session_id === "host");
    h.doing = "Editing ui/desk.js"; h.doing_ms = Date.now() - 4 * 60000 - 5000;
  });
  const host = page.locator('.dk-sessions [data-desk-chat-of="host"]');
  await expect(host.locator(".dk-now")).toHaveText("Editing ui/desk.js · 4m");
  // Its dot, after its name, blinks (#153, option B).
  expect(await blinkOf(host.locator(".dk-st > i"))).toBe("pulse");
  // A new step: the line follows.
  await cue.change((st) => { const h = st.sessions.find((x) => x.session_id === "host"); h.doing = "Running tests"; h.doing_ms = Date.now(); });
  await expect(host.locator(".dk-now")).toHaveText(/^Running tests · /);
  // Stopped: a still hollow dot, no step.
  await cue.change((st) => { const h = st.sessions.find((x) => x.session_id === "host"); h.state = "idle"; h.doing = ""; });
  await expect(host.locator(".dk-now")).toHaveCount(0);
  expect(await blinkOf(host.locator(".dk-st > i.idle"))).toBe("none");
  // A teammate at work: its step too.
  await crewNotLead(cue);
  const mate = page.locator('.dk-sessions [data-desk-crew="id-search"]');
  expect(await blinkOf(mate.locator(".dk-st > i"))).toBe("pulse");
});

test("on first load every session row shows its size and its cache, the lead's too though it holds a card (#592)", async ({ page, cue }) => {
  // The lead waits on a question card the window's state brings no numbers for (they come only for this Mac's waiting
  // cards): its row asked for them itself, as every other row does.
  await cue.open("desk", { flags: "cost=warm" });
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  const left = page.locator(".dk-sessions");
  await expect(left.locator('[data-desk-lead-chat="lead"] .dk-size')).toHaveText("600k · cache 8m");
  await expect(left.locator('[data-desk-chat-of="host"] .dk-size')).toHaveText("600k · cache 8m");
  await expect(left.locator(".dk-meter, .ctx-bar")).toHaveCount(0);
});

test("more than 50 sessions' numbers are all asked for, 50 at a time (#592)", async ({ page, cue }) => {
  await cue.open("desk", { flags: "cost=warm" });
  const before = (await cue.calls("insight_state")).length;
  const got = await page.evaluate(async () => Object.keys(await insAskMany(Array.from({ length: 120 }, (_, i) => `s${i}`))).length);
  expect(got).toBe(120);
  expect((await cue.calls("insight_state")).slice(before).map((c) => c.args.sessionIds.length)).toEqual([50, 50, 20]);
});
