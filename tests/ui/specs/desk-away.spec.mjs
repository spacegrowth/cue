// Away (#361, pick B): an Away switch in the Items head opens its rules card (Until, Budget, what the lead may decide,
// what always waits for you); while Away is on the window takes its night colours with "Away until …" in the head (#415,
// pick D), the calls the lead made since sit in one foldable line at the top of Items, each with its time and Undo
// (calls 1), and the open asks join Waiting on you. A session stopped on a usage limit isn't yours to
// do (Cue carries on when it resets): it stays off Needs you, one quiet line instead.
import { test, expect } from "./cue.mjs";

const SINCE = 1793984100 * 1000;   // between the scenario's two calls: c1 after it, c2 before

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};

test("Away on: no column; the calls since it went on in one line at the top of Items with Undo, asks join Waiting on you, a limited session is a quiet line (#373, #415)", async ({ page, cue }) => {
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
  // Only the call made since Away went on, in the open list at the top of Items: its time, what, its item, why, Undo.
  const fold = items.locator(".dk-scroll > .dk-aw-calls:first-child .dk-aw-fold");
  await expect(fold).toHaveText(/^›1 call while awaysince \d\d:\d\dtill \d\d:\d\d · \$3\.20 of \$10$/);
  await expect(fold.locator(".dk-aw-meter i")).toHaveAttribute("style", "width:32%");
  await expect(items.locator(".dk-aw-mark")).toHaveCount(1);
  const mark = items.locator('.dk-aw-list [data-desk-call="c1"]');
  await expect(mark).toHaveText(/^\d\d:\d\dAn environment setting, not a settings file · Bug #d5 · Hosted sessions don't read the settings file at start\.Undo$/);
  // Not under its item any more, nor a count on In progress.
  await expect(page.locator('[data-desk-item="d5"] + .dk-aw-mark')).toHaveCount(0);
  await expect(items.locator(".dk-aw-n")).toHaveCount(0);
  // Rows stay where they were.
  expect(await page.locator(".dk-items .dk-it").evaluateAll((els) => els.map((e) => e.dataset.deskItem))).toEqual(order);
  await mark.locator('[data-desk-undo="c1"]').click();
  await cue.called("desk_undo", { decisionId: "c1" });
  // Away off: the lines go.
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").away.on = false; });
  await expect(page.locator(".dk-aw-mark")).toHaveCount(0);
  await expect(page.locator(".dk-aw-calls")).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-gh.you")).toHaveText("Waiting on you 1");
});

test("the calls line folds to one line and stays folded for that Desk; a call with no item is in it too; nothing waiting says so (#415)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st, since) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.away = { on: true, active: true, ended: "", since, until: null, budget: null, spent: 0, rules: {} };
    d.decisions.push({ project: "cue", why: "No session knew that area.", undo: "can", id: "c9", call: true, by: "web-lead", sid: "lead", text: "Started a new session", at: since / 1000 + 60 });
    d.items.find((i) => i.id === "d1").state = "working";
    st.items.find((i) => i.id === "n-focus-ask").status = "answered";
  }, SINCE);
  const items = page.locator(".dk-items");
  const fold = items.locator(".dk-aw-fold");
  // Oldest first, the one with no item as well; no till, spend or bar without them.
  await expect(fold).toHaveText(/^›2 calls while awaysince \d\d:\d\d$/);
  await expect(fold.locator(".dk-aw-meter")).toHaveCount(0);
  expect(await items.locator(".dk-aw-mark").evaluateAll((els) => els.map((e) => e.dataset.deskCall))).toEqual(["c9", "c1"]);
  // Waiting on you 0 says so in words.
  await expect(items.locator(".dk-gh.you")).toHaveText("Waiting on you 0");
  await expect(items.locator(".dk-gh.you + .dk-gbody > .dk-gin > .dk-aw-none")).toHaveText("Nothing waits on you.");
  if (process.env.SHOT_DIR) await page.locator(".dk-items").screenshot({ path: `${process.env.SHOT_DIR}/cue-away-calls.png` });
  // Folded: one line, the Undos gone with the list; it stays folded through a redraw and a reload.
  const order = await items.locator(".dk-it").evaluateAll((els) => els.map((e) => e.dataset.deskItem));
  await fold.click();
  await expect(fold).toHaveAttribute("aria-expanded", "false");
  await expect(items.locator(".dk-aw-mark")).toHaveCount(0);
  expect(await items.locator(".dk-it").evaluateAll((els) => els.map((e) => e.dataset.deskItem))).toEqual(order);
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").away.spent = 1; });
  await expect(items.locator(".dk-aw-mark")).toHaveCount(0);
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem("cue.desk.awayFolded")))).toEqual(["cue"]);
  await fold.click();
  await expect(items.locator(".dk-aw-mark")).toHaveCount(2);
  await items.locator('[data-desk-call="c9"] [data-desk-undo="c9"]').click();
  await cue.called("desk_undo", { decisionId: "c9" });
});

for (const mode of ["light", "dark"]) {
  test(`while away the window keeps its ${mode} theme and only the head says Away until when, with the spend, readable; off, it's gone (#415, #459)`, async ({ page, cue }) => {
    await cue.open("desk");
    await cue.change((st, mode) => { st.settings = { ...(st.settings || {}), appearance: { ...(st.settings?.appearance || {}), mode } }; }, mode);
    await openDesk(page);
    const root = page.locator("html");
    const bg = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    await expect(root).toHaveAttribute("data-theme", mode);
    const before = await bg();
    await expect(page.locator(".top .top-away")).toHaveCount(0);
    await cue.change((st, since) => {
      st.desk.projects.find((x) => x.project === "cue").away = { on: true, active: true, ended: "", since, until: since + 9 * 3600000, budget: 10, spent: 3.4, rules: {} };
    }, SINCE);
    const mark = page.locator(".top .top-away");
    await expect(mark).toHaveText(/^Away until \d\d:\d\d· \$3\.40 of \$10$/);
    await expect(mark.locator(".dk-aw-ic")).toHaveCount(1);
    // The window's own theme stays: same theme, same ground.
    await expect(root).toHaveAttribute("data-theme", mode);
    await expect(root).toHaveClass(/away-on/);
    expect(await bg()).toBe(before);
    // The mark stands out from the head's ground and its words from the mark: lime capsule, dark words.
    const c = await mark.evaluate((el) => { const s = getComputedStyle(el), lum = (v) => { const [r, g, b] = v.match(/[\d.]+/g).map(Number); return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255; }; return { mark: lum(s.backgroundColor), text: lum(s.color), head: lum(getComputedStyle(document.body).backgroundColor), radius: s.borderTopLeftRadius, left: s.borderLeftWidth }; });
    expect(Math.abs(c.mark - c.head)).toBeGreaterThan(0.35);
    expect(Math.abs(c.mark - c.text)).toBeGreaterThan(0.5);
    expect(c.radius).toBe("999px");
    expect(c.left).toBe("0px");
    expect(await page.locator(".top-away [title], .top-away[title]").count()).toBe(0);
    if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/cue-away-${mode}.png` });
    // The budget reached: it says it paused.
    await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").away.ended = "budget"; });
    await expect(mark).toHaveText("Away· Paused: budget reached");
    // Two Desks away: how many.
    await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").away.ended = ""; st.desk.projects.find((x) => x.project !== "cue").away = { on: true, active: true, ended: "", rules: {} }; });
    await expect(mark).toHaveText("Away on 2 Desks");
    // Off: no mark, still the same theme.
    await cue.change((st) => { for (const d of st.desk.projects) if (d.away) d.away.on = false; });
    await expect(page.locator(".top .top-away")).toHaveCount(0);
    await expect(root).not.toHaveClass(/away-on/);
    await expect(root).toHaveAttribute("data-theme", mode);
    expect(await bg()).toBe(before);
  });
}

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

test("steps held while you were away list under Waiting on you in plain words with Allow and No, Away on or off (#406)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const items = page.locator(".dk-items");
  await expect(items.locator(".dk-gh.you")).toHaveText("Waiting on you 1");
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.held = [{ id: "held-1", sid: "site", who: "site", what: "delete files", command: "rm -rf build", at: 1 },
      { id: "held-2", sid: "site", who: "site", what: "push", command: "git push origin main", at: 2 }];
  });
  await expect(items.locator(".dk-gh.you")).toHaveText("Waiting on you 3");
  const rows = items.locator(".dk-held");
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(0).locator(".dk-request-who")).toHaveText("site wants to delete files · held while you were away");
  await expect(rows.nth(0).locator("pre")).toHaveText("rm -rf build");
  await expect(rows.nth(1).locator(".dk-request-who")).toHaveText("site wants to push · held while you were away");
  await expect(rows.nth(0).locator("button")).toHaveText(["No", "Allow"]);
  await expect(items.locator(".dk-held [title]")).toHaveCount(0);
  await rows.nth(1).locator('[data-allow="1"]').click();
  await cue.called("desk_held_answer", { id: "held-2", allow: true });
  await expect(rows).toHaveCount(1);
  await rows.nth(0).locator('[data-allow="0"]').click();
  await cue.called("desk_held_answer", { id: "held-1", allow: false });
  await expect(rows).toHaveCount(0);
});

test("the Away switch shows in full, its until time with it, in the Items head at every window width (#412)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st, since) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.away = { on: true, active: true, ended: "", since, until: since + 9 * 3600000, budget: 10, spent: 3.2, rules: {} };
  }, SINCE);
  const head = page.locator(".dk-items .dk-ch"), sw = head.locator(".dk-aw-sw");
  for (const width of [1600, 1360, 1200, 1100, 1000, 900, 821, 700, 420]) {
    await page.setViewportSize({ width, height: 900 });
    await expect(sw.locator(".dk-aw-st")).toHaveText(/^until \d\d:\d\d · \$3\.20 of \$10$/);
    // Every button inside the head, none clipped; "Away" and its until time whole (what it spent may shorten).
    const cut = await head.evaluate((h) => {
      const box = h.getBoundingClientRect(), pad = parseFloat(getComputedStyle(h).paddingRight), out = [];
      for (const c of h.children) { const r = c.getBoundingClientRect(); if (r.left < box.left - 0.5 || r.right > box.right - pad + 0.5) out.push(`${c.className} outside`); }
      const u = h.querySelector(".dk-aw-until"), s = h.querySelector(".dk-aw-st"), su = u.getBoundingClientRect(), ss = s.getBoundingClientRect();
      if (su.right > ss.right + 0.5 || u.scrollWidth > u.clientWidth + 1) out.push("until cut");
      return out;
    });
    expect(cut, `at ${width}px`).toEqual([]);
    if (process.env.SHOT_DIR) await page.locator(".dk-items .dk-ch").screenshot({ path: `${process.env.SHOT_DIR}/away-${width}.png` });
    await expect(head.locator(".dk-prog-btn, .dk-pz, .dk-aw-sw")).toHaveCount(3);
  }
});
