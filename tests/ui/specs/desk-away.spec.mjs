// Away (#361, pick B): an Away switch in the Items head opens its rules card (Until, Budget, what the lead may decide,
// what always waits for you); while Away is on (#373, pick C) each call the lead made since is a line under the item
// it changed (with Undo), and the open asks join Waiting on you. A session stopped on a usage limit isn't yours to
// do (Cue carries on when it resets): it stays off Needs you, one quiet line instead.
import { test, expect } from "./cue.mjs";

const SINCE = 1793984100 * 1000;   // between the scenario's two calls: c1 after it, c2 before

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};

test("Away on: no column; each call since it went on is a line under its item with Undo, asks join Waiting on you, a limited session is a quiet line (#373)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-aw-mark")).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-aw-sw")).toHaveAttribute("aria-pressed", "false");
  const order = await page.locator(".dk-items .dk-it").evaluateAll((els) => els.map((e) => e.dataset.deskItem));
  await cue.change((st, since) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.away = { on: true, active: true, ended: "", since, until: since + 9 * 3600000, budget: 10, spent: 3.2,
      rules: { route: true, how: true, stopped: true, order: true, what: false, outside: false } };
    const site = st.sessions.find((x) => x.session_id === "site");
    const ask = st.items.find((i) => i.id === "n-focus-ask");
    st.sessions.push({ ...site, session_id: "asker", project: "cue", name: "asker", state: "deciding", thread: [] },
      { ...site, session_id: "capped", project: "cue", name: "capped", state: "idle", thread: [],
        limit: { scope: "all", resets_ms: since + 4 * 3600000 }, send_at: { text: "Carry on where you stopped: your usage limit reset.", at_ms: 0 } });
    st.items.push({ ...ask, id: "n-asker", session_id: "asker" }, { ...ask, id: "n-capped", session_id: "capped" });
    d.members.push("asker", "capped");
  }, SINCE);
  const sw = page.locator(".dk-items .dk-aw-sw");
  await expect(sw).toHaveAttribute("aria-pressed", "true");
  // No fourth column.
  await expect(page.locator(".dk-board > *")).toHaveCount(3);
  // The switch says until when and what it spent of the budget.
  await expect(sw.locator(".dk-aw-st")).toHaveText(/^until \d\d:\d\d · \$3\.20 of \$10$/);
  // Waiting on you counts the waiting item and the asker's ask, not the session on a usage limit.
  const items = page.locator(".dk-items");
  await expect(items.locator(".dk-gh.you")).toHaveText("Waiting on you 2");
  await expect(items.locator('.dk-aw-asks [data-desk-chat-of="asker"]')).toHaveCount(1);
  await expect(items.locator('[data-desk-chat-of="capped"]')).toHaveCount(0);
  await expect(items.locator(".dk-aw-quiet")).toHaveText(/^capped paused till \d\d:\d\d · Cue carries on then$/);
  // Only the call made since Away went on, right under the item it changed, with the Away icon, its why and Undo.
  await expect(items.locator(".dk-aw-mark")).toHaveCount(1);
  const mark = page.locator('[data-desk-item="d5"] + .dk-aw-mark');
  await expect(mark).toHaveText("An environment setting, not a settings file · Hosted sessions don't read the settings file at start.Undo");
  await expect(mark.locator(".dk-aw-ic")).toHaveCount(1);
  await expect(items.locator(".dk-aw-n")).toHaveText("1 call while away");
  // Rows stay where they were.
  expect(await page.locator(".dk-items .dk-it").evaluateAll((els) => els.map((e) => e.dataset.deskItem))).toEqual(order);
  await mark.locator('[data-desk-undo="c1"]').click();
  await cue.called("desk_undo", { decisionId: "c1" });
  // Away off: the lines go.
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").away.on = false; });
  await expect(page.locator(".dk-aw-mark")).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-gh.you")).toHaveText("Waiting on you 1");
});

test("a call while away with no item in the list sits at the end of Waiting on you (#373)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st, since) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.away = { on: true, active: true, ended: "", since, until: null, budget: null, spent: 0, rules: {} };
    d.decisions.push({ project: "cue", why: "No session knew that area.", undo: "can", id: "c9", call: true, by: "web-lead", sid: "lead", text: "Started a new session", at: since / 1000 + 60 });
  }, SINCE);
  await expect(page.locator(".dk-items .dk-aw-mark")).toHaveCount(2);
  await expect(page.locator('.dk-items [data-desk-call="c9"]')).toHaveCount(1);
  // Before Ready to review.
  expect(await page.locator(".dk-items").evaluate((el) => { const m = el.querySelector('[data-desk-call="c9"]'), h = [...el.querySelectorAll(".dk-gh")][1]; return !!(m.compareDocumentPosition(h) & Node.DOCUMENT_POSITION_FOLLOWING); })).toBe(true);
});

/** Nothing waits on the cue Desk: d1 no longer asks. */
const clear = (cue) => cue.change((st) => {
  st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d1").state = "working";
  st.items.find((i) => i.id === "n-focus-ask").status = "answered";
});
/** `n` open questions from the lead, the newest last; a choice each. */
const leadAsks = (cue, n) => cue.change((st, n) => {
  const ask = st.items.find((i) => i.id === "n-focus-ask");
  for (let k = 1; k <= n; k++) st.items.push({ ...ask, status: "pending", id: `n-lead${k}`, session_id: "lead", agent_id: "", agent_name: "", created_ms: ask.created_ms + k * 1000,
    tool_input: { questions: [{ question: `Question ${k}?`, header: "Q", multiSelect: false, options: [{ label: "Yes" }, { label: "No" }] }] } });
}, n);

/** As the hub does: the lead's sweep ends (its turn ended); the Desk's away no longer sweeping. */
const sweepEnds = (cue) => cue.change((st) => { const d = st.desk.projects.find((x) => x.project === "cue"); d.away = { ...(d.away || {}), sweeping: false }; });

test("one Away switch for every Desk: defaults from off, a sweep on each, Done turns each on, one Desk off on its own", async ({ page, cue }) => {
  await cue.open("desk");
  // As desk_view will: the Desk's away kept in the world, so later states carry it.
  await cue.override("desk_away_set", (args) => (CueBridge.world.state.desk.projects.find((x) => x.project === args.desk).away = { ...args.away, active: args.away.on, ended: "", since: Date.now(), spent: 0 }));
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await expect(card).toBeVisible();
  // From off, every Desk's lead goes through its open items first (the scenario has two Desks: cue and website).
  const rules0 = { route: true, how: true, stopped: true, order: true, what: false, outside: false };
  await cue.called("desk_away_sweep", { desk: "cue", rules: rules0 });
  await cue.called("desk_away_sweep", { desk: "website", rules: rules0 });
  await expect(card.locator(".dk-aw-sweep")).toBeVisible();
  await sweepEnds(cue);
  await expect(card.locator('[data-away-field="until"]')).toHaveValue("07:00");
  await expect(card.locator('[data-away-field="budget"]')).toHaveValue("10");
  // Nothing open: no "Before you go".
  await expect(card.locator(".dk-aw-h")).toHaveText(["Desks", "The lead may decide", "Always waits for you"]);
  await expect(card.locator("[data-away-desk]")).toHaveCount(2);
  await expect(card.locator('[data-away-desk="website"]')).toHaveAttribute("aria-checked", "true");
  await expect(card.locator("[data-away-rule]")).toHaveCount(6);
  await expect(card.locator(".dk-aw-row.locked")).toHaveText(["Ask me before: push to main, release, deleting files", "Picking a mock", "Looks right / Send back on a finished item"]);
  await expect(card.locator('[data-away-rule="route"]')).toHaveAttribute("aria-checked", "true");
  // "What" questions wait for you by default: its switch is on (the rule off).
  await expect(card.locator('[data-away-rule="what"]')).toHaveAttribute("aria-checked", "true");
  await expect(card).toContainText("One notification only when nothing can move without you.");
  await expect(card.locator('[data-away="off"]')).toHaveCount(0);
  // Let the lead not pick the order; let it answer "what" questions; budget $5.
  await card.locator('[data-away-rule="order"]').click();
  await card.locator('[data-away-rule="what"]').click();
  await expect(card.locator('[data-away-rule="order"]')).toHaveAttribute("aria-checked", "false");
  await card.locator('[data-away-field="budget"]').fill("5");
  await card.locator('[data-away="done"]').click();
  const call = await cue.called("desk_away_set", { desk: "cue" });
  expect(call.args.away).toMatchObject({ on: true, budget: 5, rules: { route: true, how: true, stopped: true, order: false, what: true, outside: false } });
  // The other Desk gets the same settings from the one switch.
  await cue.called("desk_away_set", { desk: "website", away: call.args.away });
  expect(await page.evaluate((ms) => [new Date(ms).getHours(), new Date(ms).getMinutes()], call.args.away.until)).toEqual([7, 0]);
  const ahead = call.args.away.until - (await page.evaluate(() => Date.now()));
  expect(ahead > 0 && ahead <= 86400000).toBe(true);
  await expect(card).toHaveCount(0);
  // The answer is kept on the Desk at once (desk_view brings it too).
  await expect(page.locator(".dk-items .dk-aw-sw")).toHaveAttribute("aria-pressed", "true");
  // Opened again while on: no sweep, no "Before you go"; Turn Away off sends on: false.
  await leadAsks(cue, 1);
  await page.locator(".dk-items .dk-aw-sw").click();
  await expect(card.locator('[data-away-field="budget"]')).toHaveValue("5");
  await expect(card.locator(".dk-aw-first")).toHaveCount(0);
  expect((await cue.calls("desk_away_sweep")).length).toBe(2);
  // One Desk off on its own: website goes off, cue stays on.
  await card.locator('[data-away-desk="website"]').click();
  await card.locator('[data-away="done"]').click();
  await cue.called("desk_away_set", { desk: "website", away: { ...call.args.away, on: false } });
  await expect(card).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-aw-sw")).toHaveAttribute("aria-pressed", "true");
  expect((await cue.calls("desk_away_set")).filter((c) => c.args.desk === "cue").map((c) => c.args.away.on)).toEqual([true, true]);
  // Turn Away off: cue goes off; website, already off, isn't touched again.
  await page.locator(".dk-items .dk-aw-sw").click();
  await expect(card.locator('[data-away-desk="website"]')).toHaveAttribute("aria-checked", "false");
  await card.locator('[data-away="off"]').click();
  await cue.called("desk_away_set", { desk: "cue", away: { ...call.args.away, on: false } });
  await expect(card).toHaveCount(0);
  expect((await cue.calls("desk_away_set")).filter((c) => c.args.desk === "website").length).toBe(2);
});

test("while the lead sweeps, the card says so and Done waits", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").away = { on: false, active: false, ended: "", since: 0, until: null, budget: null, spent: 0, sweeping: true, rules: {} }; });
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  // Both Desks' leads sweep; with the other Desk switched off, only this one's lead is waited on.
  await expect(card.locator(".dk-aw-sweep")).toHaveText(/^(2 leads are|.+'s lead is) going through the open items…$/);
  await card.locator('[data-away-desk="website"]').click();
  await expect(card.locator(".dk-aw-sweep")).toHaveText(/^\S+'s lead is going through the open items…$/);
  await expect(card.locator('[data-away="done"]')).toBeDisabled();
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").away.sweeping = false; });
  await expect(card.locator(".dk-aw-sweep")).toHaveCount(0);
  await expect(card.locator('[data-away="done"]')).toBeEnabled();
});

test("Before you go: an open ask holds Done until it's answered right there", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await leadAsks(cue, 1);
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await sweepEnds(cue);
  await expect(card.locator(".dk-aw-h").first()).toHaveText("Before you go: 1 to answer");
  await expect(card.locator(".dk-aw-ask")).toContainText("Question 1?");
  const done = card.locator('[data-away="done"]');
  await expect(done).toBeDisabled();
  await expect(done).toHaveText("Answer these first");
  await card.locator('.dk-aw-ask [data-pick="0:0"]').click();
  await card.locator('.dk-aw-ask [data-act="submit-answers"]').click();
  await cue.called("respond", { id: "n-lead1" });
  await expect(card.locator(".dk-aw-ask")).toHaveCount(0);
  await expect(card.locator(".dk-aw-first")).toHaveCount(0);
  await expect(done).toBeEnabled();
  await expect(done).toHaveText("Done");
});

test("Before you go shows 5, the lead's newest first, and says how many more wait in Waiting on you", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await leadAsks(cue, 7);
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await sweepEnds(cue);
  await expect(card.locator(".dk-aw-h").first()).toHaveText("Before you go: 5 to answer");
  await expect(card.locator(".dk-aw-ask .qtext")).toHaveText(["Question 7?", "Question 6?", "Question 5?", "Question 4?", "Question 3?"]);
  await expect(card.locator(".dk-aw-more")).toHaveText("2 more will wait in Waiting on you");
});

test("Before you go holds what the lead's sweep left, not what was open when the card opened", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  // Sent: the card waits for the lead, even before the hub's state says it's sweeping.
  await expect(card.locator(".dk-aw-sweep")).toBeVisible();
  await expect(card.locator('[data-away="done"]')).toBeDisabled();
  // The sweep leaves one question for you, then ends: it holds Done.
  await leadAsks(cue, 1);
  await sweepEnds(cue);
  await expect(card.locator(".dk-aw-h").first()).toHaveText("Before you go: 1 to answer");
  await expect(card.locator('[data-away="done"]')).toBeDisabled();
});

test("while away, a session stopped on a permission shows in Waiting on you with Allow / Deny", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st, since) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.away = { on: true, active: true, ended: "", since, until: since + 9 * 3600000, budget: 10, spent: 0,
      rules: { route: true, how: true, stopped: true, order: true, what: false, outside: false } };
    const site = st.sessions.find((x) => x.session_id === "site"), ask = st.items.find((i) => i.id === "n-focus-ask");
    st.sessions.push({ ...site, session_id: "stuck", project: "cue", name: "stuck", state: "deciding", thread: [] });
    st.items.push({ ...ask, id: "n-perm", session_id: "stuck", kind: "permission", tool_name: "Bash", tool_input: { command: "rm -rf build" }, questions: undefined });
    d.members.push("stuck");
  }, SINCE);
  const row = page.locator('.dk-items [data-desk-request="n-perm"]');
  await expect(row).toBeVisible();
  await expect(row).toContainText("stuck");
  await expect(row).toContainText("rm -rf build");
  await expect(row.locator('[data-act="deny"]')).toHaveText("Deny");
  await row.locator('[data-act="allow"]').click();
  expect((await cue.called("respond", { id: "n-perm" })).args.decision.behavior).toBe("allow");
});
