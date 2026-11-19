// Away (#361, pick B): an Away switch in the Items head opens its rules card (Until, Budget, what the lead may decide,
// what always waits for you); while Away is on, an Away column beside Items holds what needs you, the lead's calls
// since Away went on (each with Undo) and what's done to check. A session stopped on a usage limit isn't yours to
// do (Cue carries on when it resets): it stays off Needs you, one quiet line instead.
import { test, expect } from "./cue.mjs";

const SINCE = 1793984100 * 1000;   // between the scenario's two calls: c1 after it, c2 before

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};

test("Away on: a column beside Items with Needs you, Decided for you (Undo) and Done, to check; a limited session is a quiet line", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-away")).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-aw-sw")).toHaveAttribute("aria-pressed", "false");
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
  const away = page.locator(".dk-away");
  await expect(away).toBeVisible();
  await expect(page.locator(".dk-board.away")).toHaveCount(1);
  await expect(page.locator(".dk-items .dk-aw-sw")).toHaveAttribute("aria-pressed", "true");
  // Its head: until when and what it spent of the budget.
  await expect(away.locator(".dk-aw-st")).toHaveText(/^until \d\d:\d\d · \$3\.20 of \$10$/);
  // Three sections; Needs you counts the waiting item and the asker's ask, not the session on a usage limit.
  await expect(away.locator(".dk-gh")).toHaveText(["Needs you 2", "Decided for you 1", "Done, to check 1"]);
  await expect(away.locator('[data-desk-item="d1"]')).toHaveCount(1);
  await expect(away.locator('.dk-aw-asks [data-desk-chat-of="asker"]')).toHaveCount(1);
  await expect(away.locator('[data-desk-chat-of="capped"]')).toHaveCount(0);
  await expect(away.locator(".dk-aw-quiet")).toHaveText(/^capped paused till \d\d:\d\d · Cue carries on then$/);
  // Only the call made since Away went on, with its Undo.
  await expect(away.locator(".dk-aw-decs .dk-d")).toHaveText(["An environment setting, not a settings file"]);
  await away.locator('.dk-aw-decs [data-desk-undo="c1"]').click();
  await cue.called("desk_undo", { decisionId: "c1" });
  await expect(away.locator('[data-desk-item="d2"]')).toHaveCount(1);
  // Away off: the column goes.
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").away.on = false; });
  await expect(page.locator(".dk-away")).toHaveCount(0);
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

test("the Away switch opens its rules card: defaults from off, switches, and Done turns it on", async ({ page, cue }) => {
  await cue.open("desk");
  // As desk_view will: the Desk's away kept in the world, so later states carry it.
  await cue.override("desk_away_set", (args) => (CueBridge.world.state.desk.projects.find((x) => x.project === args.desk).away = { ...args.away, active: args.away.on, ended: "", since: Date.now(), spent: 0 }));
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await expect(card).toBeVisible();
  // From off, the lead goes through the open items first.
  await cue.called("desk_away_sweep", { desk: "cue", rules: { route: true, how: true, stopped: true, order: true, what: false, outside: false } });
  await expect(card.locator(".dk-aw-sweep")).toBeVisible();
  await sweepEnds(cue);
  await expect(card.locator('[data-away-field="until"]')).toHaveValue("07:00");
  await expect(card.locator('[data-away-field="budget"]')).toHaveValue("10");
  // Nothing open: no "Before you go".
  await expect(card.locator(".dk-aw-h")).toHaveText(["The lead may decide", "Always waits for you"]);
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
  expect(await page.evaluate((ms) => [new Date(ms).getHours(), new Date(ms).getMinutes()], call.args.away.until)).toEqual([7, 0]);
  const ahead = call.args.away.until - (await page.evaluate(() => Date.now()));
  expect(ahead > 0 && ahead <= 86400000).toBe(true);
  await expect(card).toHaveCount(0);
  // The answer is kept on the Desk at once (desk_view brings it too): the column shows.
  await expect(page.locator(".dk-away")).toBeVisible();
  await expect(page.locator(".dk-items .dk-aw-sw")).toHaveAttribute("aria-pressed", "true");
  // Opened again while on: no sweep, no "Before you go"; Turn Away off sends on: false.
  await leadAsks(cue, 1);
  await page.locator(".dk-items .dk-aw-sw").click();
  await expect(card.locator('[data-away-field="budget"]')).toHaveValue("5");
  await expect(card.locator(".dk-aw-first")).toHaveCount(0);
  expect((await cue.calls("desk_away_sweep")).length).toBe(1);
  await card.locator('[data-away="off"]').click();
  await cue.called("desk_away_set", { desk: "cue", away: { ...call.args.away, on: false } });
});

test("while the lead sweeps, the card says so and Done waits", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").away = { on: false, active: false, ended: "", since: 0, until: null, budget: null, spent: 0, sweeping: true, rules: {} }; });
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await expect(card.locator(".dk-aw-sweep")).toHaveText("The lead is going through the open items…");
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
  await card.locator('.dk-aw-ask [data-desk-question-pick="0"]').click();
  await cue.called("respond", { id: "n-lead1" });
  await expect(card.locator(".dk-aw-ask")).toHaveCount(0);
  await expect(card.locator(".dk-aw-first")).toHaveCount(0);
  await expect(done).toBeEnabled();
  await expect(done).toHaveText("Done");
});

test("Before you go shows 5, the lead's newest first, and says how many more wait in the Away column", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await leadAsks(cue, 7);
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await sweepEnds(cue);
  await expect(card.locator(".dk-aw-h").first()).toHaveText("Before you go: 5 to answer");
  await expect(card.locator(".dk-aw-ask p")).toHaveText(["Question 7?", "Question 6?", "Question 5?", "Question 4?", "Question 3?"]);
  await expect(card.locator(".dk-aw-more")).toHaveText("2 more will wait in the Away column");
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
