// Away (#361, pick B): an Away switch in the Items head opens its rules card (Until, Budget, what the lead may decide,
// what always waits for you); while Away is on the window takes its night colours with "Away until …" in the head (#415,
// pick D), the calls the lead made since sit in one foldable line at the top of Items, each with its time and Undo
// (calls 1), and the open asks join Waiting on you. A session stopped on a usage limit isn't yours to
// do (Cue carries on when it resets): it stays off Needs you, one quiet line instead.
import { test, expect, openDesk } from "./cue.mjs";

const SINCE = 1793984100 * 1000;   // between the scenario's two calls: c1 after it, c2 before


/** CALLS WHILE AWAY, folded to a line after the list's groups (B2, #666), opened; it says `n`. */
async function openCalls(page, n) {
  const line = page.locator('.dk-items [data-desk2b-fold="away"]');
  await expect(line).toHaveText(new RegExp(`^CALLS WHILE AWAY ${n}`));
  if ((await line.getAttribute("aria-expanded")) !== "true") await line.click();
  await expect(line).toHaveAttribute("aria-expanded", "true");
}

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
  // No column of its own: the lead's chat card and the item list (B2, #666).
  await expect(page.locator(".dk-board > :not(.col-grip)")).toHaveCount(2);
  // The switch says until when and what it spent of the budget.
  await expect(sw.locator(".dk-aw-st")).toHaveText(/^until \d\d:\d\d · \$3\.20 of \$10$/);
  // Waiting on you counts the waiting item, the one ready to check and the asker's ask, not the session on a usage limit.
  const items = page.locator(".dk-items");
  await expect(items.locator('.dk2b-gh[data-desk2b-group="you"]')).toHaveText("WAITS ON YOU 3");
  await expect(items.locator('.dk-aw-asks [data-desk-chat-of="asker"]')).toHaveCount(1);
  await expect(items.locator('[data-desk-chat-of="capped"]')).toHaveCount(0);
  await expect(items.locator(".dk-aw-quiet")).toHaveText(/^capped paused till \d\d:\d\d · Cue carries on then$/);
  // Only the call made since Away went on, under CALLS WHILE AWAY (B2, #666), folded to a line after the groups;
  // opened, the list: its time, what, its item, why, Undo.
  await openCalls(page, 1);
  const fold = items.locator(".dk-scroll > .dk2b-fold.open + .dk-aw-calls .dk-aw-fold");
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
  await expect(page.locator('.dk-items .dk2b-gh[data-desk2b-group="you"]')).toHaveText("WAITS ON YOU 2");
  await expect(page.locator('.dk-items [data-desk2b-fold="away"]')).toHaveCount(0);
});

test("a call the lead made in place of asking you shows the question under it (#471)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st, since) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.away = { on: true, active: true, ended: "", since, until: null, budget: null, spent: 0, rules: {} };
    d.decisions.find((x) => x.id === "c1").asked = "Settings file or environment setting?";
  }, SINCE);
  await openCalls(page, 1);
  const mark = page.locator('.dk-aw-list [data-desk-call="c1"]');
  await expect(mark.locator(".dk-aw-asked")).toHaveText("Instead of asking you: Settings file or environment setting?");
  await expect(mark.locator("b")).toHaveText("An environment setting, not a settings file");
});
test("the calls line folds to one line and stays folded for that Desk; a call with no item is in it too; nothing waiting says so (#415)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st, since) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.away = { on: true, active: true, ended: "", since, until: null, budget: null, spent: 0, rules: {} };
    d.decisions.push({ project: "cue", why: "No session knew that area.", undo: "can", id: "c9", call: true, by: "web-lead", sid: "lead", text: "Started a new session", at: since / 1000 + 60 });
    d.items.find((i) => i.id === "d1").state = "working";
    d.items.find((i) => i.id === "d2").state = "working";
    st.items.find((i) => i.id === "n-focus-ask").status = "answered";
  }, SINCE);
  const items = page.locator(".dk-items");
  await openCalls(page, 2);
  const fold = items.locator(".dk-aw-fold");
  // Oldest first, the one with no item as well; no till, spend or bar without them.
  await expect(fold).toHaveText(/^›2 calls while awaysince \d\d:\d\d$/);
  await expect(fold.locator(".dk-aw-meter")).toHaveCount(0);
  expect(await items.locator(".dk-aw-mark").evaluateAll((els) => els.map((e) => e.dataset.deskCall))).toEqual(["c9", "c1"]);
  // Nothing waiting: no WAITS ON YOU group, its tab says 0, and opened it says so in words.
  await expect(items.locator('.dk2b-gh[data-desk2b-group="you"]')).toHaveCount(0);
  await expect(items.locator('[data-desk2b-tab="you"] .n')).toHaveText("0");
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
  test(`while away the window keeps its ${mode} theme; off the Desk's page the head says Away until when, with the spend, readable; on it the Items card's switch does, for room (#415, #459, #666); off, it's gone`, async ({ page, cue }) => {
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
    // On the Desk's own page the head leaves it to the Items card's switch.
    await expect(page.locator(".dk-items .dk-aw-sw")).toHaveAttribute("aria-pressed", "true");
    await expect(mark).toHaveCount(0);
    await page.locator('.switch-view [data-view="board"]').click();
    await expect(mark).toHaveText(/^cue: Away until \d\d:\d\d· \$3\.40 of \$10$/);
    await expect(mark.locator(".dk-aw-ic")).toHaveCount(1);
    // The window's own theme stays: same theme, same ground.
    await expect(root).toHaveAttribute("data-theme", mode);
    await expect(root).toHaveClass(/away-on/);
    expect(await bg()).toBe(before);
    // The mark stands out from the head's ground and its words from the mark: lime capsule, dark words.
    // Read off the mark on the page at that moment: the head is drawn again as Cue's state comes in, and a handle taken
    // before that is a detached element whose computed colours are empty.
    const c = await page.evaluate(() => { const el = document.querySelector(".top .top-away"), s = getComputedStyle(el), lum = (v) => { const [r, g, b] = v.match(/[\d.]+/g).map(Number); return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255; }; return { mark: lum(s.backgroundColor), text: lum(s.color), head: lum(getComputedStyle(document.body).backgroundColor), radius: s.borderTopLeftRadius, left: s.borderLeftWidth }; });
    expect(Math.abs(c.mark - c.head)).toBeGreaterThan(0.35);
    expect(Math.abs(c.mark - c.text)).toBeGreaterThan(0.5);
    expect(c.radius).toBe("999px");
    expect(c.left).toBe("0px");
    expect(await page.locator(".top-away [title], .top-away[title]").count()).toBe(0);
    if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/cue-away-${mode}.png` });
    // The budget reached: it says it paused.
    await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").away.ended = "budget"; });
    await expect(mark).toHaveText("cue: Away· Paused: budget reached");
    // Two Desks away: the Board says how many.
    await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").away.ended = ""; st.desk.projects.find((x) => x.project !== "cue").away = { on: true, active: true, ended: "", rules: {} }; });
    await expect(mark).toHaveText("Away on 2 Desks");
    await page.locator('.switch-view [data-view="desk"]').click();
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
  for (let k = 1; k <= n; k++) st.items.push({ ...ask, status: "pending", desk: true, id: `n-lead${k}`, session_id: "lead", agent_id: "", agent_name: "", created_ms: ask.created_ms + k * 1000,
    tool_input: { questions: [{ question: `Question ${k}?`, header: "Q", multiSelect: false, options: [{ label: "Yes" }, { label: "No" }] }] } });
}, n);

/** As the hub does: the lead's sweep ends (its turn ended); the Desk's away no longer sweeping. */
const sweepEnds = (cue) => cue.change((st) => { const d = st.desk.projects.find((x) => x.project === "cue"); d.away = { ...(d.away || {}), sweeping: false }; });

/** As desk_view will: the Desk's away kept in the world, so later states carry it. */
const keepAway = (cue) => cue.override("desk_away_set", (args) => (CueBridge.world.state.desk.projects.find((x) => x.project === args.desk).away = { ...args.away, active: args.away.on, ended: "", since: Date.now(), spent: 0 }));
/** As the hub does when the Desk changed since the lead's last pass: the pass sent, the Desk sweeping until it ends. */
const passRuns = (cue) => cue.override("desk_away_sweep", (args) => { const d = CueBridge.world.state.desk.projects.find((x) => (x.id || x.project) === args.desk); d.away = { ...(d.away || {}), sweeping: true }; return true; });
const RULES0 = { route: true, how: true, stopped: true, order: true, what: false, outside: false };

test("one click turns Away on for this Desk only (#612, #643); the card opens to show the lead's pass, Done never waits; another Desk on and off on its own", async ({ page, cue }) => {
  await cue.open("desk");
  await keepAway(cue);
  // Something changed on the Desks since their last pass: the hub sends it.
  await passRuns(cue);
  await openDesk(page);
  await clear(cue);
  await page.locator(".dk-items .dk-aw-sw").click();
  // On at once, on this Desk only (the scenario has two: cue and website), 07:00 and $10 the first time.
  const call = await cue.called("desk_away_set", { desk: "cue" });
  expect(call.args.away).toMatchObject({ on: true, budget: 10, rules: RULES0 });
  expect(await page.evaluate((ms) => [new Date(ms).getHours(), new Date(ms).getMinutes()], call.args.away.until)).toEqual([7, 0]);
  const ahead = call.args.away.until - (await page.evaluate(() => Date.now()));
  expect(ahead > 0 && ahead <= 86400000).toBe(true);
  await expect(page.locator(".dk-items .dk-aw-sw")).toHaveAttribute("aria-pressed", "true");
  // This Desk's lead asked to go through its items, not the other's; the card shows it, and Done doesn't wait for it.
  await cue.called("desk_away_sweep", { desk: "cue", rules: RULES0 });
  const card = page.locator(".dk-aw-dialog");
  await expect(card).toBeVisible();
  await expect(card.locator(".end-text")).toHaveText("Away is on. The lead keeps this Desk moving while you're away.");
  await expect(card.locator(".dk-aw-sweep")).toHaveText("The lead is going through the open items…");
  await expect(card.locator('[data-away="done"]')).toBeEnabled();
  expect((await cue.calls("desk_away_set")).filter((c) => c.args.desk === "website")).toHaveLength(0);
  expect((await cue.calls("desk_away_sweep")).map((c) => c.args.desk)).toEqual(["cue"]);
  await sweepEnds(cue);
  await expect(card.locator(".dk-aw-sweep")).toHaveCount(0);
  await expect(card.locator('[data-away-field="until"]')).toHaveValue("07:00");
  await expect(card.locator('[data-away-field="budget"]')).toHaveValue("10");
  // Nothing open: no "Before you go"; the rules fold into one row, the Desks on beside Until and Budget (#618).
  await expect(card.locator(".dk-aw-h")).toHaveCount(0);
  await expect(card.locator(".dk-aw-sum")).toContainText("The lead may decide routing, “how” questions, stopped items and order; the rest waits for you");
  await expect(card.locator(".dk-aw-fld b")).not.toHaveText(/,/);
  await card.locator(".dk-aw-fld").click();
  await expect(card.locator(".dk-aw-h")).toHaveText(["Desks", "The lead may decide", "Always waits for you"]);
  await expect(card.locator("[data-away-desk]")).toHaveCount(2);
  await expect(card.locator('[data-away-desk="cue"]')).toHaveAttribute("aria-checked", "true");
  await expect(card.locator('[data-away-desk="website"]')).toHaveAttribute("aria-checked", "false");
  await expect(card.locator("[data-away-rule]")).toHaveCount(6);
  await expect(card.locator(".dk-aw-row.locked")).toHaveText(["Ask me before: push to main, release, deleting files", "Picking a mock", "Looks right / Send back on a finished item"]);
  // "What" questions wait for you by default: its switch is on (the rule off).
  await expect(card.locator('[data-away-rule="what"]')).toHaveAttribute("aria-checked", "true");
  await expect(card).toContainText("One notification only when nothing can move without you.");
  await expect(card.locator("[title]")).toHaveCount(0);
  // Changing the rules and budget right there: Done saves them on this Desk; the other stays off, untouched.
  await card.locator('[data-away-rule="order"]').click();
  await card.locator('[data-away-rule="what"]').click();
  await card.locator('[data-away-field="budget"]').fill("5");
  await expect(card.locator('[data-away="done"]')).toHaveText("Done");
  await card.locator('[data-away="done"]').click();
  await expect(card).toHaveCount(0);
  const sets = (desk) => cue.calls("desk_away_set").then((c) => c.filter((x) => x.args.desk === desk));
  const set2 = (await sets("cue"))[1];
  expect(set2.args.away).toMatchObject({ on: true, budget: 5, rules: { ...RULES0, order: false, what: true } });
  expect(await sets("website")).toHaveLength(0);
  // Opened again while on: no pass, no "Before you go".
  await leadAsks(cue, 1);
  await page.locator(".dk-items .dk-aw-sw").click();
  await expect(card.locator('[data-away-field="budget"]')).toHaveValue("5");
  await expect(card.locator(".dk-aw-first")).toHaveCount(0);
  await expect(card.locator('[data-away="done"]')).toHaveText("Done");
  expect((await cue.calls("desk_away_sweep")).length).toBe(1);
  // The other Desk on from here, by its own switch: its lead goes through its items too, Done turns it on.
  await card.locator(".dk-aw-sum").click();
  await card.locator('[data-away-desk="website"]').click();
  await cue.called("desk_away_sweep", { desk: "website", rules: set2.args.away.rules });
  await cue.change((st) => { for (const d of st.desk.projects) d.away = { ...(d.away || {}), sweeping: false }; });
  await card.locator('[data-away="done"]').click();
  await cue.called("desk_away_set", { desk: "website", away: set2.args.away });
  await expect(card).toHaveCount(0);
  // One Desk off on its own: website goes off, cue stays on.
  await page.locator(".dk-items .dk-aw-sw").click();
  await card.locator(".dk-aw-sum").click();
  await expect(card.locator('[data-away-desk="website"]')).toHaveAttribute("aria-checked", "true");
  await card.locator('[data-away-desk="website"]').click();
  await card.locator('[data-away="done"]').click();
  await cue.called("desk_away_set", { desk: "website", away: { ...set2.args.away, on: false } });
  await expect(card).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-aw-sw")).toHaveAttribute("aria-pressed", "true");
  expect((await sets("cue")).map((c) => c.args.away.on)).toEqual([true, true, true, true]);
  // Turn Away off: cue goes off; website, already off, isn't touched again.
  await page.locator(".dk-items .dk-aw-sw").click();
  await card.locator(".dk-aw-sum").click();
  await expect(card.locator('[data-away-desk="website"]')).toHaveAttribute("aria-checked", "false");
  await card.locator('[data-away="off"]').click();
  await cue.called("desk_away_set", { desk: "cue", away: { ...set2.args.away, on: false } });
  await expect(card).toHaveCount(0);
  expect((await sets("website")).length).toBe(2);
});

test("nothing changed since the lead's last pass: one click and Away is just on, no card; off and on again the same (#612)", async ({ page, cue }) => {
  await cue.open("desk");
  await keepAway(cue);
  // The hub finds nothing new since the last pass: none sent.
  await cue.override("desk_away_sweep", () => false);
  await openDesk(page);
  await clear(cue);
  const sw = page.locator(".dk-items .dk-aw-sw"), card = page.locator(".dk-aw-dialog");
  for (let k = 1; k <= 2; k++) {
    await sw.click();
    await expect(sw).toHaveAttribute("aria-pressed", "true");
    expect((await cue.calls("desk_away_set")).filter((c) => c.args.desk === "cue").at(-1).args.away.on).toBe(true);
    await expect.poll(async () => (await cue.calls("desk_away_sweep")).length).toBe(k);
    await expect(card).toHaveCount(0);
    await expect(page.locator(".dk-aw-sweep")).toHaveCount(0);
    // Not silent: a note says it's on, until when and on what budget.
    await expect(page.locator(".toast.show")).toHaveText("Away is on until 07:00 · budget $10. Click Away again to change it or turn it off.");
    await expect(page.locator(".toast[title]")).toHaveCount(0);
    // Off again from its card.
    await sw.click();
    await card.locator('[data-away="off"]').click();
    await expect(sw).toHaveAttribute("aria-pressed", "false");
    // Not silent either way.
    await expect(page.locator(".toast.show")).toHaveText("Away is off. Everything waits for you again.");
  }
  // The second time it took the settings it had: 07:00 and $10 kept.
  const ons = (await cue.calls("desk_away_set")).filter((c) => c.args.desk === "cue" && c.args.away.on);
  expect(ons[1].args.away).toMatchObject({ budget: 10, rules: RULES0 });
});

for (const mode of ["light", "dark"]) {
  test(`Away turned on for one Desk shows only on that Desk; the other shows normal until you turn it on there (#643, ${mode})`, async ({ page, cue }) => {
    await cue.open("desk");
    await cue.change((st, mode) => { st.settings = { ...(st.settings || {}), appearance: { ...(st.settings?.appearance || {}), mode } }; }, mode);
    await keepAway(cue);
    await cue.override("desk_away_sweep", () => false);
    // The second Desk gets a lead too, so it has its own Away switch.
    await cue.change((st) => { const w = st.desk.projects.find((x) => x.project === "website"); w.lead = { sid: "site", name: "site-lead", state: "live" }; w.members = [...new Set([...(w.members || []), "site"])]; });
    await openDesk(page);
    await clear(cue);
    const sw = page.locator(".dk-items .dk-aw-sw"), mark = page.locator(".top .top-away"), root = page.locator("html");
    await sw.click();
    await expect(sw).toHaveAttribute("aria-pressed", "true");
    // The Desk's own page leaves it to its switch (#666).
    await expect(mark).toHaveCount(0);
    await expect(root).toHaveClass(/away-on/);
    // Only this Desk was set on.
    expect((await cue.calls("desk_away_set")).map((c) => c.args.desk)).toEqual(["cue"]);
    // The other Desk: no mark, its switch off, no calls line, the root not marked.
    await openDesk(page, "website");
    await expect(sw).toHaveAttribute("aria-pressed", "false");
    await expect(sw.locator(".dk-aw-st")).toHaveCount(0);
    await expect(mark).toHaveCount(0);
    await expect(page.locator(".dk-aw-calls")).toHaveCount(0);
    await expect(root).not.toHaveClass(/away-on/);
    await expect(page.locator(".top [title], .dk-items .dk-ch [title]")).toHaveCount(0);
    if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/cue-away-other-desk-${mode}.png` });
    // Off the Desks, the head names the Desk that's away.
    await page.locator('.switch-view [data-view="board"]').click();
    await expect(mark).toHaveText(/^\S+: Away until 07:00/);
    // Turned on there too, by its own switch: now it shows there, and the first Desk's stays as it was.
    await page.locator('.switch-view [data-view="desk"]').click();
    await sw.click();
    await expect(sw).toHaveAttribute("aria-pressed", "true");
    await expect(mark).toHaveCount(0);
    await page.locator('.switch-view [data-view="board"]').click();
    await expect(mark).toHaveText("Away on 2 Desks");
    await page.locator('.switch-view [data-view="desk"]').click();
    expect((await cue.calls("desk_away_set")).map((c) => c.args.desk)).toEqual(["cue", "website"]);
    await openDesk(page, "cue");
    await expect(sw).toHaveAttribute("aria-pressed", "true");
  });
}

for (const mode of ["light", "dark"]) {
  test(`Before you go is optional (#612): Done works with a question open, and it can still be answered right there (${mode})`, async ({ page, cue }) => {
    await cue.open("desk");
    await cue.change((st, mode) => { st.settings = { ...(st.settings || {}), appearance: { ...(st.settings?.appearance || {}), mode } }; }, mode);
    await keepAway(cue);
    await cue.override("desk_away_sweep", () => false);
    await openDesk(page);
    await clear(cue);
    await leadAsks(cue, 2);
    await page.locator(".dk-items .dk-aw-sw").click();
    const card = page.locator(".dk-aw-dialog");
    // Away is on already; the card shows what you could answer.
    await expect(page.locator(".dk-items .dk-aw-sw")).toHaveAttribute("aria-pressed", "true");
    await expect(card.locator(".dk-aw-h").first()).toHaveText("Before you go");
    await expect(card.locator(".dk-aw-nav")).toContainText("1 of 2");
    await expect(card.locator(".dk-aw-opt")).toHaveText("Optional: Away is already on. Skip any you'd rather leave.");
    const c = await card.locator(".dk-aw-opt").evaluate((el) => { const lum = (v) => { const [r, g, b] = v.match(/[\d.]+/g).map(Number); return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255; }; const bg = getComputedStyle(el.closest(".dk-aw-dialog")).backgroundColor; return Math.abs(lum(getComputedStyle(el).color) - lum(bg)); });
    expect(c).toBeGreaterThan(0.3);
    await expect(card.locator("[title]")).toHaveCount(0);
    if (process.env.SHOT_DIR) await card.screenshot({ path: `${process.env.SHOT_DIR}/cue-away-optional-${mode}.png` });
    const done = card.locator('[data-away="done"]');
    await expect(done).toBeEnabled();
    await expect(done).toHaveText("Done");
    // One answered in place; the next shows.
    await card.locator('.dk-aw-ask [data-pick="0:0"]').click();
    await card.locator('.dk-aw-ask [data-act="submit-answers"]').click();
    await cue.called("respond", { id: "n-lead2" });
    await expect(card.locator(".dk-aw-nav")).toContainText("2 of 2");
    await expect(card.locator(".dk-aw-left")).toHaveText("Left unanswered: 1 goes back to its lead.");
    // The other left: Done closes the card, Away stays on.
    await done.click();
    await expect(card).toHaveCount(0);
    await expect(page.locator(".dk-items .dk-aw-sw")).toHaveAttribute("aria-pressed", "true");
  });
}

test("while the lead sweeps, the card says so; Done works anyway (#618)", async ({ page, cue }) => {
  await cue.open("desk");
  await keepAway(cue);
  await passRuns(cue);
  await openDesk(page);
  await clear(cue);
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  // This Desk's lead sweeps; the other Desk switched on too, both are waited on; off again, only this one.
  await expect(card.locator(".dk-aw-sweep")).toHaveText("The lead is going through the open items…");
  await card.locator(".dk-aw-sum").click();
  await card.locator('[data-away-desk="website"]').click();
  await expect(card.locator(".dk-aw-sweep")).toHaveText("2 leads are going through the open items…");
  await card.locator('[data-away-desk="website"]').click();
  await expect(card.locator(".dk-aw-sweep")).toHaveText("The lead is going through the open items…");
  await expect(card.locator('[data-away="done"]')).toBeEnabled();
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").away.sweeping = false; });
  await expect(card.locator(".dk-aw-sweep")).toHaveCount(0);
  await expect(card.locator('[data-away="done"]')).toBeEnabled();
});

test("Before you go: one question at a time, Send moves on, Skip steps past, a mark jumps back, and Done works any time (#618)", async ({ page, cue }) => {
  await cue.open("desk");
  await keepAway(cue);
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await leadAsks(cue, 3);
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await sweepEnds(cue);
  // One card only, the lead's newest first.
  await expect(card.locator(".dk-aw-ask")).toHaveCount(1);
  await expect(card.locator(".dk-aw-ask .qtext")).toHaveText("Question 3?");
  await expect(card.locator(".dk-aw-nav")).toContainText("1 of 3");
  await expect(card.locator(".dk-aw-nav")).toContainText("If you leave it, its lead decides");
  await expect(card.locator(".dk-aw-pip")).toHaveCount(3);
  await expect(card.locator(".dk-aw-pip").nth(0)).toHaveAttribute("aria-current", "step");
  await expect(card.locator(".dk-aw-left")).toHaveText("Left unanswered: 3 go back to their leads.");
  const done = card.locator('[data-away="done"]');
  await expect(done).toBeEnabled();
  await expect(done).toHaveText("Done");
  // Send answers it and moves on.
  await card.locator('.dk-aw-ask [data-pick="0:0"]').click();
  await card.locator('.dk-aw-ask [data-act="submit-answers"]').click();
  await cue.called("respond", { id: "n-lead3" });
  await expect(card.locator(".dk-aw-ask .qtext")).toHaveText("Question 2?");
  await expect(card.locator(".dk-aw-nav")).toContainText("2 of 3");
  await expect(card.locator(".dk-aw-pip").nth(0)).toHaveClass(/ans/);
  await expect(card.locator(".dk-aw-left")).toHaveText("Left unanswered: 2 go back to their leads.");
  // Skip steps past without answering.
  await card.locator("[data-away-skip]").click();
  await expect(card.locator(".dk-aw-ask .qtext")).toHaveText("Question 1?");
  await card.locator("[data-away-skip]").click();
  await expect(card.locator(".dk-aw-ask")).toHaveCount(0);
  await expect(card.locator(".dk-aw-end")).toHaveText("That's all of them. 2 left for later.");
  // A mark jumps to its question; an answered one says so.
  await card.locator(".dk-aw-pip").nth(1).click();
  await expect(card.locator(".dk-aw-ask .qtext")).toHaveText("Question 2?");
  await card.locator(".dk-aw-pip").nth(0).click();
  await expect(card.locator(".dk-aw-end")).toHaveText("Answered.");
  // Leaving with two open is fine: Away stays on.
  await done.click();
  await expect(card).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-aw-sw")).toHaveAttribute("aria-pressed", "true");
});

test("Before you go holds every open one, no cap, the lead's newest first", async ({ page, cue }) => {
  await cue.open("desk");
  await keepAway(cue);
  await passRuns(cue);
  await openDesk(page);
  await clear(cue);
  await leadAsks(cue, 7);
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await cue.change((st) => { for (const d of st.desk.projects) d.away = { ...(d.away || {}), sweeping: false }; });
  await expect(card.locator(".dk-aw-nav")).toContainText("1 of 7");
  await expect(card.locator(".dk-aw-ask .qtext")).toHaveText("Question 7?");
  await expect(card.locator(".dk-aw-pip")).toHaveCount(7);
});

test("Before you go leaves out a session's question once its items have all finished, and says how many (#618)", async ({ page, cue }) => {
  await cue.open("desk");
  await keepAway(cue);
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await leadAsks(cue, 1);
  // host-lead asks too, but its item has landed since.
  await cue.change((st) => {
    const ask = st.items.find((i) => i.id === "n-lead1");
    st.items.push({ ...ask, id: "n-host1", session_id: "host", desk: false, tool_input: { questions: [{ question: "Rebase first?", header: "Q", multiSelect: false, options: [{ label: "Yes" }, { label: "No" }] }] } });
    st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").state = "done";
  });
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await sweepEnds(cue);
  await expect(card.locator(".dk-aw-nav")).toContainText("1 of 1");
  await expect(card.locator(".dk-aw-ask .qtext")).toHaveText("Question 1?");
  await expect(card.locator(".dk-aw-more")).toHaveText("Not shown: 1 that no longer needs you");
});

test("Before you go holds what the lead's pass left, not what was open when the card opened; Done never waits on the pass", async ({ page, cue }) => {
  await cue.open("desk");
  await keepAway(cue);
  await passRuns(cue);
  await openDesk(page);
  await clear(cue);
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await expect(card.locator(".dk-aw-sweep")).toBeVisible();
  await expect(card.locator('[data-away="done"]')).toBeEnabled();
  // The sweep leaves one question for you, then ends: it shows, 1 of 1.
  await leadAsks(cue, 1);
  await cue.change((st) => { for (const d of st.desk.projects) d.away = { ...(d.away || {}), sweeping: false }; });
  await expect(card.locator(".dk-aw-nav")).toContainText("1 of 1");
  await expect(card.locator(".dk-aw-ask .qtext")).toHaveText("Question 1?");
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
  const you = items.locator('.dk2b-gh[data-desk2b-group="you"]');
  await expect(you).toHaveText("WAITS ON YOU 2");
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.held = [{ id: "held-1", sid: "site", who: "site", what: "delete files", command: "rm -rf build", at: 1 },
      { id: "held-2", sid: "site", who: "site", what: "push", command: "git push origin main", at: 2 }];
  });
  await expect(you).toHaveText("WAITS ON YOU 4");
  const rows = items.locator(".dk-held");
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(0).locator(".dk-request-who")).toHaveText("site wants to delete files · held while you were away");
  await expect(rows.nth(0).locator("pre")).toHaveText("$ rm -rf build");
  await expect(rows.nth(1).locator(".dk-request-who")).toHaveText("site wants to push · held while you were away");
  await expect(rows.nth(0).locator(".row-btns button")).toHaveText(["No", "Allow"]);
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
  const head = page.locator(".dk-items .dk2b-ih"), sw = head.locator(".dk-aw-sw");
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
    if (process.env.SHOT_DIR) await head.screenshot({ path: `${process.env.SHOT_DIR}/away-${width}.png` });
    // One dot between Away and its until time, never two (#666).
    // ("Away · until 01:53": the dot is the status's ::before, so the until time has none of its own.)
    // (Polled: a redraw on resize can swap the head between finding it and reading it, and a swapped-out node has no style.)
    await expect.poll(() => sw.evaluate((el) => [".dk-aw-st", ".dk-aw-until"].map((q) => getComputedStyle(el.querySelector(q), "::before").content))).toEqual(['"· "', "none"]);
    await expect(head.locator(".dk-aw-sw, .dk-qa")).toHaveCount(2);
  }
});

for (const mode of ["light", "dark"]) {
  test(`Before you go says each question's Desk, this Desk's first; another Desk's and a finished item's never show; older ones fold under Older (#613, ${mode})`, async ({ page, cue }) => {
    await cue.open("desk");
    await cue.change((st, mode) => { st.settings = { ...(st.settings || {}), appearance: { ...(st.settings?.appearance || {}), mode } }; }, mode);
    await keepAway(cue);
    await cue.override("desk_away_sweep", () => null);
    await openDesk(page);
    await clear(cue);
    await leadAsks(cue, 1);
    await cue.change((st) => {
      // The other Desk listed first, so this Desk coming first is the card's doing.
      st.desk.projects.reverse();
      const ask = st.items.find((i) => i.id === "n-lead1"), q = (text) => ({ questions: [{ question: text, header: "Q", multiSelect: false, options: [{ label: "Yes" }, { label: "No" }] }] });
      ask.tool_input = { ...q("Question 1?"), desk_id: "cue" };
      // The lead asked about another Desk, and about an item that has landed (d7 is done): neither is this card's.
      st.items.push({ ...ask, id: "n-other", tool_input: { ...q("Other Desk's question?"), desk_id: "elsewhere" } });
      st.items.push({ ...ask, id: "n-done", tool_input: { ...q("Finished item's question?"), desk_id: "cue", desk_item: "d7" } });
      // Asked two days ago: under Older.
      st.items.push({ ...ask, id: "n-old", created_ms: ask.created_ms - 2 * 86400000, tool_input: { ...q("Old question?"), desk_id: "cue" } });
      // The website Desk's session asks too.
      st.items.push({ ...ask, id: "n-site", session_id: "site", desk: false, created_ms: ask.created_ms + 5000, tool_input: q("Website question?") });
    });
    await page.locator(".dk-items .dk-aw-sw").click();
    const card = page.locator(".dk-aw-dialog");
    // The website Desk goes away too.
    await card.locator(".dk-aw-sum").click();
    await card.locator('[data-away-desk="website"]').click();
    await sweepEnds(cue);
    await expect(card.locator(".dk-aw-nav").first()).toContainText("1 of 2");
    await expect(card.locator(".dk-aw-first .dk-aw-ask .qtext")).toHaveText("Question 1?");
    await expect(card.locator(".dk-aw-nav .dk-aw-from").first()).toHaveText(/^cue Desk$/i);
    await expect(card).not.toContainText("Other Desk's question?");
    await expect(card).not.toContainText("Finished item's question?");
    // Older: folded, one row.
    const older = card.locator("[data-away-older]");
    await expect(older).toHaveAttribute("aria-expanded", "false");
    await expect(older).toContainText("Older");
    await expect(older).toContainText("1");
    await expect(card).not.toContainText("Old question?");
    // The old one counts with the rest: the lead's two go back to it, the website session's waits for you.
    await expect(card.locator(".dk-aw-left")).toHaveText("Left unanswered: 2 go back to their leads, 1 waits in Waiting on you.");
    await expect(card.locator("[title]")).toHaveCount(0);
    if (process.env.SHOT_DIR) await card.screenshot({ path: `${process.env.SHOT_DIR}/cue-away-desks-${mode}.png` });
    // Skip: the website Desk's, saying so.
    await card.locator("[data-away-skip]").click();
    await expect(card.locator(".dk-aw-first .dk-aw-ask .qtext")).toHaveText("Website question?");
    await expect(card.locator(".dk-aw-nav .dk-aw-from").first()).toHaveText(/^website Desk$/i);
    // Unfolded, the old one shows with its Desk and day, answerable there.
    await older.click();
    await expect(older).toHaveAttribute("aria-expanded", "true");
    const row = card.locator(".dk-aw-oldrow");
    await expect(row).toHaveCount(1);
    await expect(row.locator(".dk-aw-from")).toHaveText(/^cue Desk$/i);
    await expect(row).toContainText("asked ");
    await expect(row.locator(".qtext")).toHaveText("Old question?");
    if (process.env.SHOT_DIR) await card.screenshot({ path: `${process.env.SHOT_DIR}/cue-away-older-${mode}.png` });
    await row.locator('[data-pick="0:0"]').click();
    await row.locator('[data-act="submit-answers"]').click();
    await cue.called("respond", { id: "n-old" });
    await expect(card.locator("[data-away-older]")).toHaveCount(0);
  });
}

test("the budget box takes any number you type, 0 and 100 too, with a question on the card: digits pick an answer only outside a text box (#614)", async ({ page, cue }) => {
  await cue.open("desk");
  await keepAway(cue);
  await cue.override("desk_away_sweep", () => false);
  await openDesk(page);
  await clear(cue);
  await leadAsks(cue, 2);
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await expect(card.locator(".dk-aw-ask")).toHaveCount(1);
  const budget = card.locator('[data-away-field="budget"]');
  for (const n of ["100", "0", "25"]) {
    await budget.click();
    await budget.fill("");
    await page.keyboard.type(n);
    await expect(budget).toHaveValue(n);
    await expect(budget).toBeFocused();
  }
  // Typing in the box picked nothing on the question under it.
  await expect(card.locator(".dk-aw-ask .picked, .dk-aw-ask [aria-pressed=\"true\"], .dk-aw-ask [aria-checked=\"true\"]")).toHaveCount(0);
  // A redraw between keys (the Desk changes all the time) keeps the cursor in the box and what you typed.
  await budget.fill("");
  await page.keyboard.type("1");
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").away.spent = 0.5; });
  await expect(budget).toBeFocused();
  await page.keyboard.type("00");
  await expect(budget).toHaveValue("100");
  // Done saves the budget typed.
  await card.locator('[data-away="done"]').click();
  await expect(card).toHaveCount(0);
  expect((await cue.calls("desk_away_set")).at(-1).args.away.budget).toBe(100);
});

const LONG_TITLE = "Reply box loses focus after Send, and the cursor lands in the search field at the top of the window instead of the box";
const LONG_SEE = "After Send the cursor is still in the reply box, ready for the next line, and nothing else on the page takes it, not even the search field at the top.";

test("Before you go shows a waiting item whole, answered right there; nothing in it opens a page (#618)", async ({ page, cue }) => {
  await cue.open("desk");
  await keepAway(cue);
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  // d1 waits on you with a question that has choices.
  await cue.change((st, [t, s]) => { const i = st.desk.projects.find((x) => x.project === "cue").items.find((x) => x.id === "d1"); i.title = t; i.see = s; }, [LONG_TITLE, LONG_SEE]);
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await sweepEnds(cue);
  const item = card.locator('.dk-aw-item[data-away-item="d1"]');
  await expect(item).toHaveCount(1);
  // Whole: no list row, the title and You'll see in full, on as many lines as they take.
  await expect(card.locator(".dk-it")).toHaveCount(0);
  await expect(item.locator(".dk-aw-ih")).toContainText("focus-fix");
  await expect(item.locator(".dk-aw-it")).toHaveText(LONG_TITLE);
  await expect(item.locator(".dk-aw-see")).toHaveText(`You'll see: ${LONG_SEE}`);
  if (process.env.SHOT_DIR) await card.screenshot({ path: `${process.env.SHOT_DIR}/cue-away-item.png` });
  for (const sel of [".dk-aw-it", ".dk-aw-see"]) expect(await item.locator(sel).evaluate((el) => el.scrollWidth <= el.clientWidth && getComputedStyle(el).textOverflow !== "ellipsis" && el.getBoundingClientRect().height > 20)).toBe(true);
  // The question's whole card, its choices right here; nothing that opens a page.
  await expect(item.locator(".qtext")).toHaveText("Keep focus in the box, or move to the next item?");
  await expect(card.locator("[data-desk-item], [data-desk-chk-item]")).toHaveCount(0);
  await expect(card.locator("[title]")).toHaveCount(0);
  await item.locator('[data-pick="0:0"]').click();
  await item.locator('[data-act="submit-answers"]').click();
  await cue.called("respond", { id: "n-focus-ask" });
  await expect(item).toHaveCount(0);
  await expect(card).toBeVisible();
});

test("Before you go: an item asking in its own words gets a box; Send answers it and moves on (#618)", async ({ page, cue }) => {
  await cue.open("desk");
  await keepAway(cue);
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await leadAsks(cue, 1);
  // d1 asks in its holder's words, no choices; the lead's question comes first.
  await cue.change((st) => {
    st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "id-focus", project: "cue", name: "focus-fix", state: "idle", thread: [{ role: "agent", text: "Which file should keep the focus code: box.js or compose.js?" }] });
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((x) => x.id === "d1"), { state: "needs_you", ask: null, holder: { kind: "lead", sid: "id-focus", name: "focus-fix" } });
  });
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await sweepEnds(cue);
  await expect(card.locator(".dk-aw-nav")).toContainText("1 of 2");
  await card.locator("[data-away-skip]").click();
  const item = card.locator('.dk-aw-item[data-away-item="d1"]');
  await expect(card.locator(".dk-aw-nav")).toContainText("2 of 2");
  await expect(item.locator(".dk-aw-q")).toHaveText("Which file should keep the focus code: box.js or compose.js?");
  await item.locator("textarea").fill("compose.js");
  await item.locator("[data-desk-send]").click();
  await expect(item).toHaveCount(0);
  await expect(card.locator(".dk-aw-end")).toHaveText("That's all of them. 1 left for later.");
  await expect(card.locator(".dk-aw-pip").nth(1)).toHaveClass(/ans/);
});

test("something that leaves the sheet sets it aside: Back to Away and Esc bring it back where you were; Esc again closes it (#618)", async ({ page, cue }) => {
  await cue.open("desk");
  await keepAway(cue);
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await leadAsks(cue, 2);
  // A session that stopped: its card has Open its chat, which leaves the sheet.
  await cue.change((st) => {
    const ask = st.items.find((i) => i.id === "n-lead1");
    st.items.push({ ...ask, id: "n-stop", kind: "stop", tool_name: "", tool_input: {}, message: "Stopped: tests need a decision", created_ms: ask.created_ms - 60000 });
  });
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await sweepEnds(cue);
  await card.locator('[data-away-field="budget"]').fill("25");
  await card.locator("[data-away-skip]").click();
  await card.locator("[data-away-skip]").click();
  await expect(card.locator(".dk-aw-nav")).toContainText("3 of 3");
  await card.locator('[data-desk-lead-chat="lead"]').click();
  await expect(card).toHaveCount(0);
  const back = page.locator(".dk-aw-back [data-away-back]");
  await expect(back).toHaveText("‹ Back to Away · 3 of 3");
  if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/cue-away-back.png` });
  await back.click();
  await expect(card.locator(".dk-aw-nav")).toContainText("3 of 3");
  await expect(card.locator('[data-away-field="budget"]')).toHaveValue("25");
  await expect(page.locator(".dk-aw-back")).toHaveCount(0);
  // Esc on the page it opened goes back to the sheet; Esc on the sheet closes it, with no bar left.
  await card.locator('[data-desk-lead-chat="lead"]').click();
  await expect(card).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(card.locator(".dk-aw-nav")).toContainText("3 of 3");
  // × lets a set-aside sheet go: no bar, and Esc doesn't bring it back.
  await card.locator('[data-desk-lead-chat="lead"]').click();
  await page.locator(".dk-aw-back [data-away-back-x]").click();
  await expect(page.locator(".dk-aw-back")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(card).toHaveCount(0);
});

test("Esc on the sheet closes it and leaves no Back to Away (#618)", async ({ page, cue }) => {
  await cue.open("desk");
  await keepAway(cue);
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await leadAsks(cue, 1);
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await sweepEnds(cue);
  await expect(card.locator(".dk-aw-nav")).toContainText("1 of 1");
  await page.keyboard.press("Escape");
  await expect(card).toHaveCount(0);
  await expect(page.locator(".dk-aw-back")).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-aw-sw")).toHaveAttribute("aria-pressed", "true");
});

test("the Desks field names every Desk in full, wrapping, never cut (#618)", async ({ page, cue }) => {
  await cue.open("desk");
  await keepAway(cue);
  await cue.override("desk_away_sweep", () => null);
  await openDesk(page);
  await clear(cue);
  await page.locator(".dk-items .dk-aw-sw").click();
  await page.locator(".dk-items .dk-aw-sw").click();
  const card = page.locator(".dk-aw-dialog");
  await card.locator(".dk-aw-sum").click();
  await card.locator('[data-away-desk="website"]').click();
  const b = card.locator(".dk-aw-fld b");
  await expect(b).toContainText(",");
  expect(await b.evaluate((el) => getComputedStyle(el).textOverflow !== "ellipsis" && el.scrollWidth <= el.clientWidth)).toBe(true);
});
