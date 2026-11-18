// The Desk (ui/desk.js, ui/desk_lead.js): the project's sessions on the left, its lead's chat in the middle with
// Decisions beside it, its items on the right (Waiting on you first, In progress, Queued, Done
// today folded); an item, a holder or a
// crew member opened in the middle with ‹ back, its row lit where it is; the one box at the foot going to the lead;
// who runs the Desk (picked, changed, ended); Give to…; the quick-add line; and with All projects, every project's
// Desk as a card.
import { test, expect } from "./cue.mjs";

/** Project `p` picked in the header (All projects with ""). */
const pick = async (page, p) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator(p ? `.pj-pop [data-pj="pick"][data-project="${p}"]` : '.pj-pop [data-pj="pick"][data-project=""]').click();
};
/** The Desk open from the header, on project `p`. */
const openDesk = async (page, p = "cue") => {
  await pick(page, p);
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
/** The list as it reads, top to bottom: group labels and holder heads as "# …", items by id. */
const listOrder = (page) => page.locator(".dk-items .dk-scroll > *").evaluateAll((els) => els.map((el) =>
  el.dataset.deskItem || (el.matches(".dk-holder") ? `# ${el.querySelector(".dk-nm").textContent}` : el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}` : null)).filter(Boolean));

test("the Desk is in the header after Board, and lists Waiting on you, then a group a holder", async ({ page, cue }) => {
  await cue.open("desk");
  const views = await page.locator(".switch-view button").evaluateAll((els) => els.map((el) => el.dataset.view));
  expect(views.slice(0, 3)).toEqual(["inbox", "board", "desk"]);
  await openDesk(page);
  expect(await listOrder(page)).toEqual(["# Waiting on you", "d1", "d2", "# In progress", "d3", "d5", "# Queued", "d4", "d6", "# Done", "d7", "d8", "d9"]);
  await expect(page.locator(".dk-items .dk-gh.you")).toContainText("2");
  // Grouped by state only: no head a session (the sessions are on the left).
  await expect(page.locator(".dk-items .dk-holder")).toHaveCount(0);
  // One row each: state, #id, title, who holds it, age; only the ask has a second line, its question.
  const cells = (id) => page.locator(`[data-desk-item="${id}"]`).evaluate((el) => [".dk-tag", ".dk-id", ".dk-t", ".dk-who"].map((c) => el.querySelector(c)?.textContent));
  expect(await cells("d4")).toEqual(["", "Bug #d4", "Rail dot blinks for a reply you didn't send", "web-lead · after #d3"]);
  expect(await cells("d3")).toEqual(["", "Feature #d3", "Search stays centred in the header", "web-lead → search-centre"]);
  expect(await cells("d2")).toEqual(["", "Feature #d2", "Quick phrases at the right of Send", "web-lead"]);
  // Each waiting row says what to do, in one plain line (#182).
  await expect(page.locator(".dk-items .dk-h")).toHaveCount(2);
  await expect(page.locator('[data-desk-item="d1"] .dk-h')).toContainText("keep focus in the box");
  await expect(page.locator('[data-desk-item="d2"] .dk-h')).toHaveText(/^Try it: /);
  // Ready to check: Looks right on its row. Done today: each with the time it finished.
  await expect(page.locator(".dk-items .dk-ok")).toHaveCount(1);
  await expect(page.locator('[data-desk-item="d2"] .dk-ok')).toHaveAttribute("aria-label", "Looks right");
  await expect(page.locator('[data-desk-item="d7"] .dk-age')).toHaveText(/^\d{1,2}(:\d\d)?(am|pm)$/);
  // The lead's chat on the right, named for it, no crew beside it (a Desk's lead has none).
  await expect(page.locator(".dk-tab.on")).toHaveText("web-lead");
  await expect(page.locator(".dk-tabs")).not.toContainText("crew");
  // One header (#200): its kind's icon, "Dispatcher ·" quiet, then the lead's name, bold; no label pill.
  const h1 = page.locator('.dk-right .dk-pg[data-desk-page="dispatcher"] h1');
  await expect(h1).toHaveText("Dispatcher · web-lead");
  await expect(h1.locator('[data-desk-mark="desk"]')).toHaveText("Dispatcher");
  await expect(h1.locator("svg.dk-kic")).toHaveCount(1);
  await expect(page.locator(".dk-right .dk-lbl")).toHaveCount(0);
  await expect(page.locator(".dk-chat")).toContainText("host-lead is on it");
  // The Sessions head: ‹ to every Desk, the Desk's name, ✎ only on hover; no "Change" anywhere.
  await expect(page.locator(".dk-sh b")).toHaveText("cue");
  await expect(page.locator(".dk-sh .dk-ren")).toHaveCSS("opacity", "0");
  await page.locator(".dk-sh").hover();
  await expect(page.locator(".dk-sh .dk-ren")).toHaveCSS("opacity", "1");
  // No Change for the lead (⋯ hands it over); the only Change is Ask me before's, in the strip's line.
  await expect(page.locator(".dk-board button:not(.dk-ab-change)", { hasText: "Change" })).toHaveCount(0);
  await expect(page.locator(".dk-strip .dk-ab")).toHaveText("Asks before: push to main · Change");
  await expect(page.locator(".dk-box textarea")).toHaveAttribute("placeholder", "Tell web-lead: a bug, a feature, or a question…");
  await expect(page.locator("[data-desk-new]")).toHaveCount(0);
});

test("an item opens on the right with its row lit in place; ‹ web-lead goes back", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const before = await listOrder(page);
  const top = await page.locator('[data-desk-item="d4"]').boundingBox();
  await page.locator('[data-desk-item="d4"]').click();
  await expect(page.locator('[data-desk-item="d4"]')).toHaveClass(/\bon\b/);
  expect(await listOrder(page)).toEqual(before);
  expect((await page.locator('[data-desk-item="d4"]').boundingBox()).y).toBe(top.y);
  const pane = page.locator(".dk-right .dk-pg");
  // Its header names it as the Ready to check card does: kind and number, then the title (#134).
  await expect(pane.locator("h1")).toHaveText("Bug #d4 · Rail dot blinks for a reply you didn't send");
  // No kind pill over an item's page: its header names the kind already (#144).
  await expect(page.locator('.dk-right [data-desk-mark]')).toHaveCount(0);
  await expect(page.locator(".dk-right .dk-markrow")).toHaveCount(0);
  await expect(pane.locator(".dk-state")).toContainText("next for web-lead · added by you · 20m ago");
  await expect(pane.locator(".dk-pnow .dk-pword")).toHaveText("Next");
  await expect(pane.locator(".dk-chain")).toContainText("Search stays centred in the header");
  await expect(pane.locator(".dk-chain")).toContainText("This one");
  await pane.locator('[data-desk-itab="steps"]').click();
  await expect(pane).toContainText("Gave it to web-lead after Search");
  await pane.locator('[data-desk-itab="thread"]').click();
  // The box starts with who gets it and the item, kept as you type (#132); ready to check, it goes back to its holder,
  // and the prefix follows it. The empty box shows the item's short title.
  const ph = page.locator(".dk-right .dk-box textarea"), pre = page.locator(".dk-right .dk-box .dk-pre");
  await expect(pre).toHaveText("To web-lead · Bug #d4");
  await expect(ph).toHaveAttribute("placeholder", "Rail dot blinks…");
  await expect(page.locator(".dk-right .dk-box [data-quick]")).toHaveCount(0);
  await expect(pre).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await ph.fill("still blinks");
  await expect(pre).toHaveText("To web-lead · Bug #d4");
  const tag = await pre.boundingBox(), input = await ph.boundingBox();
  const row = await page.locator(".dk-right .dk-box-row").boundingBox();
  expect(tag.y + tag.height).toBeLessThanOrEqual(input.y);
  expect(Math.abs(tag.x - input.x)).toBeLessThan(2);
  expect(Math.abs(row.width - input.width)).toBeLessThan(2);
  await ph.fill("");
  await pre.click();
  await expect(ph).toBeFocused();
  await cue.change((st) => Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4"), { state: "to_check", holder: { kind: "session", sid: "s-new", name: "cue-117" } }));
  await expect(pre).toHaveText("To cue-117 · Bug #d4");
  await cue.change((st) => Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4"), { state: "queued", holder: { kind: "lead", sid: "lead", name: "web-lead" } }));
  // Put it before the one it waits on.
  await pane.locator('[data-desk-act="put_before"]').click();
  await cue.called("desk_item_act", { id: "d4", act: "put_before" });
  // Back to the lead.
  await expect(page.locator(".dk-back")).toHaveText("‹ web-lead");
  await page.locator(".dk-back").click();
  await expect(page.locator(".dk-chat")).toBeVisible();
  await expect(page.locator(".dk-it.on")).toHaveCount(0);
});

test("written about an item at work, the box goes to the lead naming it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  await page.locator(".dk-box textarea").fill("The More menu is cut off");
  await page.locator(".dk-box textarea").press("Enter");
  await cue.called("desk_send", { project: "cue", text: "About #d3 (Search stays centred in the header): The More menu is cut off" });
  // The item stays open, its box empty; the lead's chat has it when you go back.
  await expect(page.locator(".toast").last()).toContainText("Sent to web-lead");
  await expect(page.locator(".dk-pg h1")).toHaveText("Feature #d3 · Search stays centred in the header");
  await expect(page.locator(".dk-box textarea")).toHaveValue("");
  await page.locator(".dk-back").click();
  await expect(page.locator(".dk-chat")).toContainText("The More menu is cut off");
  // The lead has it (after the Desk's summary Cue put before it): no Sending left, and no summary shown.
  await expect(page.locator(".dk-chat .sending")).toHaveCount(0);
  await expect(page.locator(".dk-chat")).not.toContainText("<desk>");
  await expect(page.locator(".dk-chat")).not.toContainText("3 open: 2 wait on you");
  await expect(page.locator(".dk-chat")).toContainText("Fine. And the canvas one?");
  // Only a block of the summary's shape goes: words of yours that start with <desk> stay.
  const said = await page.evaluate(() => [deskSaid('<desk project="cue">\nx\n</desk>\n\nHi'), deskSaid("<desk> is a tag"), deskSaid("<desk>\nx\n</desk> then"), deskSaid("<desk>\nx\n</desk>\n\nHi"),
    // Only a picture sent: nothing after the block (the blank line too trimmed off), and none of it shows.
    deskSaid('<desk project="cue">\nx\n</desk>'), deskSaid('<desk project="cue">\nx\n</desk>\n\n'), deskSaid('<desk project="cue">\nx\n</desk> then')]);
  expect(said).toEqual(["Hi", "<desk> is a tag", "<desk>\nx\n</desk> then", "<desk>\nx\n</desk>\n\nHi", "", "", '<desk project="cue">\nx\n</desk> then']);
  // Two messages sent while the lead was busy reach it as one, joined by a newline, each with its block: both go.
  const joined = await page.evaluate(() => [deskSaid('<desk project="cue">\nx\n</desk>\n\nFirst\n<desk id="d1" name="Café" project="cue">\n#2 bug: y (sid:abc)\n</desk>\n\nSecond'),
    deskSaid('First\n<desk project="cue">\nx\n</desk>'), deskSaid('I wrote <desk project="cue">\nx\n</desk>\n\nhere')]);
  expect(joined).toEqual(["First\nSecond", "First", 'I wrote <desk project="cue">\nx\n</desk>\n\nhere']);
  // A picture on its own: just the picture in the chat, none of the summary.
  await page.route(/\/tmp\/a\.png$/, (r) => r.fulfill({ status: 200, contentType: "image/png", body: Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==", "base64") }));
  await page.evaluate(() => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: '<desk project="cue">\nItems:\n#9 task: a long line\n</desk>', images: ["/tmp/a.png"] }); renderMain(); });
  await expect(page.locator(".dk-chat .cv-you").last()).not.toContainText("Items:");
  await expect(page.locator(".dk-chat .cv-you").last().locator(".thumb img")).toHaveCount(1);
  // Typed so, it shows in the chat as typed.
  await page.evaluate(() => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: "<desk>\nmy own\n</desk>\n\nHi" }); renderMain(); });
  await expect(page.locator(".dk-chat .cv-you").last()).toContainText("my own");
  await expect(page.locator(".dk-chat .cv-you").last()).toContainText("<desk>");
});

test("the lead at work shows it in its chat: the working line, live, but not its steps (what it read to route isn't yours to read, #157)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.override("session_steps", ({ sessionId }, at) => (sessionId !== "lead" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at - 30000, prompt: "", more: 0, items: [
    { t: "step", id: "lr1", kind: "read", verb: "Read", subject: "header.js", mono: true, result: "40 lines", bad: false, done: true, at_ms: at - 20000 },
  ] }] }), await page.evaluate(() => now()));
  await openDesk(page);
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    Object.assign(s, { state: "working", doing: "Reading header.js…", doing_ms: Date.now() - 5000, since_ms: Date.now() - 30000 });
  });
  const live = page.locator(".dk-chat .cv-status");
  await expect(live).toContainText("Reading header.js");
  await expect(live.locator(".dot-live")).toHaveCount(1);
  // Its steps stay out of its chat: no "1 step · Read header.js" box.
  await expect(page.locator(".dk-chat .stx")).toHaveCount(0);
  // Done: the working line goes.
  await cue.change((st) => { Object.assign(st.sessions.find((x) => x.session_id === "lead"), { state: "idle", doing: "" }); });
  await expect(page.locator(".dk-chat .dot-live")).toHaveCount(0);
});

test("a send from an item that doesn't go keeps the item open, and your words in its box", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  await cue.override("desk_send", () => { throw "offline"; });
  await page.locator(".dk-box textarea").fill("The More menu is cut off");
  await page.locator(".dk-box textarea").press("Enter");
  await expect(page.locator(".toast")).toContainText("offline");
  await expect(page.locator(".dk-right .dk-pg h1")).toHaveText("Feature #d3 · Search stays centred in the header");
  await expect(page.locator(".dk-box textarea")).toHaveValue("The More menu is cut off");
});

test("the lead's own ASK stands in the box's place: its choices, Type answer…; answered, the box is back", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Typed to the lead before it asked: kept, and shown held under its card.
  await page.locator(".dk-box textarea").fill("And the footer too");
  await cue.change((st) => {
    st.sessions.find((s) => s.session_id === "lead").state = "deciding";
    st.items.push({ ...st.items[0], id: "n-lead-ask", agent_id: "", agent_name: "", desk: true, hosted: false,
      tool_input: { questions: [{ question: "Add a rename right on the Desk's line?", header: "Desk", multiSelect: false, options: [{ label: "Yes, add it there", description: "" }, { label: "No, the chat's is enough", description: "" }] }] } });
  });
  const ask = page.locator(".dk-right .dk-ask");
  await expect(ask).toContainText("Add a rename right on the Desk's line?");
  await expect(page.locator(".dk-right .dk-box")).toHaveCount(0);
  await expect(ask.locator(".foot.held textarea")).toHaveValue("And the footer too");
  // Your own words: Type answer… opens a box for them.
  await ask.locator('[data-act="redirect"]').click();
  await expect(ask.locator('textarea[data-text="n-lead-ask"]')).toBeVisible();
  await page.keyboard.press("Escape");
  await ask.locator("[data-pick]", { hasText: "Yes, add it there" }).click();
  const call = await cue.called("respond");
  expect(call.args.id).toBe("n-lead-ask");
  await cue.change((st) => { st.sessions.find((s) => s.session_id === "lead").state = "working"; });
  await expect(page.locator(".dk-right .dk-ask")).toHaveCount(0);
  await expect(page.locator(".dk-box textarea")).toHaveAttribute("placeholder", "Tell web-lead: a bug, a feature, or a question…");
  await expect(page.locator(".dk-box textarea")).toHaveValue("And the footer too");
});

test("a lead opens with its crew; a crew member with where it is, and the box goes to the lead", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  const pane = page.locator(".dk-right .dk-pg");
  await expect(pane.locator("h1 .dk-tname")).toHaveText("web-lead");
  await expect(pane.locator(".dk-crew .dk-mb")).toHaveCount(2);
  await expect(page.locator(".dk-box textarea")).toHaveAttribute("placeholder", "Talk to web-lead…");
  await pane.locator('[data-desk-crew="id-search"]').click();
  await expect(pane.locator("h1 .dk-tname")).toHaveText("search-centre");
  await expect(pane.locator(".dk-files")).toHaveText("ui/header.js · ui/styles/header.css");
  await expect(page.locator('[data-desk-item="d3"]')).toHaveClass(/\bon\b/);
  await expect(pane.locator('[data-mt-act="stop"]')).toBeVisible();
  await page.locator(".dk-box textarea").fill("Leave the Inbox button alone");
  await page.locator(".dk-box textarea").press("Enter");
  await cue.called("send_to_session", { sessionId: "lead", text: "About search-centre: Leave the Inbox button alone" });
  // ‹ web-lead goes back to the lead.
  await page.locator(".dk-back").click();
  await expect(pane.locator("h1 .dk-tname")).toHaveText("web-lead");
});

test("a crew member's words show plain, its files without scratch or node_modules, once in its lead's list; a reply told again shows once (#197)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const team = st.sessions.find((s) => s.session_id === "lead").team, t = team[0];
    Object.assign(t, { state: "done", step: "", ended_ms: t.started_ms + 60000, summary: "Stopped: it ended with its lead's Claude process; its files stay",
      files: [...t.files, "/private/tmp/claude-501/x/scratchpad/mk.py", "node_modules", "/tmp/split.py"] });
    // The same name started again later: one row, the newer run.
    team.push({ ...team[1], agent_id: "id-focus-2", call: "call-focus-2", started_ms: team[1].started_ms + 600000 });
    const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5");
    const said = "None of it is done yet.";
    Object.assign(it, { state: "working", now_on: false, latest: "", said: [{ at: Date.now() - 2000, by: "host-lead", text: said }, { at: Date.now() - 1000, by: "host-lead", text: `${said}\n\nYou've hit your limit` }] });
  });
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  const pane = page.locator(".dk-right .dk-pg");
  await expect(pane.locator(".dk-crew .dk-mb")).toHaveCount(2);
  await expect(pane.locator('[data-desk-crew="id-focus-2"]')).toHaveCount(1);
  await expect(pane.locator('[data-desk-crew="id-search"] .dk-ls')).not.toContainText(/node_modules|mk\.py|split\.py/);
  await pane.locator('[data-desk-crew="id-search"]').click();
  await expect(pane.locator(".dk-files")).toHaveText("ui/header.js · ui/styles/header.css");
  await expect(pane).toContainText("its lead's Claude process");
  await expect(pane).not.toContainText("&#39;");
  await page.locator('[data-desk-item="d5"]').click();
  const replies = page.locator(".dk-right .dk-tm", { hasText: "None of it is done yet." });
  await expect(replies).toHaveCount(1);
  await expect(replies).toContainText("You've hit your limit");
});

test("Decisions: only judgment calls, who decided, what and why; Undo (on hover) asks", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-tab="decisions"]').click();
  // Two calls; "Gave it to web-lead after Search" is a step, not in the list.
  await expect(page.locator(".dk-tab.on")).toContainText("Decisions2");
  const decs = page.locator(".dk-right .dk-dec");
  await expect(decs).toHaveCount(2);
  await expect(page.locator(".dk-right")).not.toContainText("Gave it to web-lead after Search");
  await expect(decs.first().locator(".dk-age")).toHaveText("3m ago");
  // A table row: who, what was decided, why (muted), when; Undo a quiet icon at its end, there on hover.
  await expect(decs.first().locator(".dk-lby")).toHaveText("host-lead");
  expect(await decs.first().evaluate((el) => [...el.children].map((x) => x.className))).toEqual(["dk-lby", "dk-d", "dk-w", "dk-age", "dk-lu"]);
  await decs.first().hover();
  await expect(decs.first()).toContainText("Hosted sessions don't read the settings file at start.");
  await decs.first().locator('[data-desk-undo="c1"]').click();
  await cue.called("desk_undo", { decisionId: "c1" });
  await expect(page.locator(".toast")).toHaveText("Asked its lead to undo it");
  await expect(decs.first()).toContainText("Undo asked");
});

test("a step (a route, a give) is a quiet line on its item, not a decision", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d4"]').click();
  await page.locator('[data-desk-itab="steps"]').click();
  const step = page.locator(".dk-step");
  await expect(step).toHaveCount(1);
  await expect(step).toContainText("Gave it to web-lead after Search, not a new session");
  await expect(step.locator('[data-desk-undo="c3"]')).toHaveAttribute("aria-label", "Undo");
  await expect(step.locator('[data-desk-undo="c3"] svg')).toHaveCount(1);
  await expect(step.locator('[data-desk-undo="c3"]')).toHaveCSS("opacity", "0");
  await step.hover();
  await expect(step.locator('[data-desk-undo="c3"]')).toHaveCSS("opacity", "1");
  await expect(page.locator(".dk-sec h3", { hasText: "Decisions" })).toHaveCount(0);
  // The same step said again in a row is one row, "×3", not three.
  await cue.change((st) => { const d = st.desk.projects.find((x) => x.project === "cue"), c = d.decisions.find((x) => x.id === "c3"); d.decisions.push({ ...c, id: "c3b" }, { ...c, id: "c3c" }); });
  await expect(step).toHaveCount(1);
  await expect(step.locator(".dk-x")).toHaveText("×3");
});

test("an item's Latest shows as written: **bold** in bold, and a link that opens on a click", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").latest = "**Done.** The mocks: https://claude.ai/artifact/abc123"; });
  await page.locator('[data-desk-item="d2"]').click();
  const sec = page.locator(".dk-thread .dk-tm").last();
  await expect(sec.locator("strong")).toHaveText("Done.");
  await expect(sec).not.toContainText("**");
  await sec.locator('a[data-href="https://claude.ai/artifact/abc123"]').click();
  await cue.called("open_link", { url: "https://claude.ai/artifact/abc123" });
});

test("an item page's back link is one line above its title", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d4"]').click();
  const back = await page.locator(".dk-right .dk-back").boundingBox(), h1 = await page.locator(".dk-right .dk-pg h1").boundingBox();
  expect(back.height).toBeLessThan(32);
  expect(back.width).toBeGreaterThan(60);
  expect(back.y + back.height).toBeLessThanOrEqual(h1.y + 1);
});

test("a design item shows one \"Open the mocks\" link to its canvas; a newer link replaces it; none without one", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d4"]').click();
  await expect(page.locator(".dk-right .dk-dsg")).toHaveCount(0);
  const set = (u) => cue.change((s, u) => { s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d4").designs = u; }, u);
  await set("https://claude.ai/artifact/AAA");
  const link = page.locator(".dk-right .dk-dsg a");
  await expect(link).toHaveCount(1);
  await expect(link).toHaveText("Open the mocks ↗");
  await set("https://claude.ai/artifact/BBB");
  await link.click();
  await cue.called("open_link", { url: "https://claude.ai/artifact/BBB" });
});

test("the Desk tab opens a Desk, never the list: the one last open, else Desk home's first", async ({ page, cue }) => {
  await cue.open("desk");
  const tab = page.locator('.switch-view [data-view="desk"]'), board = page.locator('.switch-view [data-view="board"]');
  await tab.click();
  await expect(page.locator(".dk-desks")).toHaveCount(0);
  await expect(page.locator("[data-desk-switch]")).toHaveCount(1);
  // Desk home, then into website; away to the Board with All projects, and the Desk tab is back on website.
  await page.locator("[data-desk-switch]").click(); await page.locator("[data-desk-home]").click();
  await page.locator('.dk-desk[data-desk-project="website"]').click();
  await page.locator("[data-desk-switch]").click(); await page.locator("[data-desk-home]").click();
  await board.click();
  await tab.click();
  await expect(page.locator(".dk-desks")).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-it").first()).toHaveAttribute("data-desk-item", "w1");
});

test("Desk home: one row a Desk with what it's about, its people, open tasks, bugs and features, and who's on it; a click opens it", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator("[data-desk-switch]").click(); await page.locator("[data-desk-home]").click();
  await expect(page.locator(".dk-hh")).toHaveText("Desks · 2");
  await expect(page.locator(".dh-hd span")).toHaveText(["Desk", "Project · people", "Tasks", "Bugs", "Features", "On it", ""]);
  const rows = page.locator(".dk-desks .dk-desk");
  await expect(rows).toHaveCount(2);
  const cue1 = rows.first();
  await expect(cue1).toHaveAttribute("data-desk-project", "cue");
  await expect(cue1.locator(".dh-nm")).toHaveText("cue");
  await expect(cue1.locator(".dh-ab")).toHaveText(/^(On|Next): (Bug|Feature|Task) #\S+ .+/);
  await expect(cue1.locator(".dh-pj")).toHaveText("cue");
  await expect(cue1.locator(".dh-cell").nth(1).locator(".dh-sm")).toHaveText(/^\d+ sessions? · (\d+ crew|no crew)$/);
  // A kind's cell: how many are open, then how many are working and how many wait on your check.
  const cells = cue1.locator(".dh-cell").filter({ has: page.locator(".dh-big") });
  await expect(cells).toHaveCount(3);
  await expect(cells.nth(0).locator(".dk-kic")).toHaveCount(1);
  expect(await cells.nth(0).locator(".dh-big").innerHTML()).toContain('d="m8.5 12.2');   // the task mark under Tasks
  for (let k = 0; k < 3; k++) await expect(cells.nth(k)).toHaveText(/^(–|\d+ open(\d+ working|none moving)?( · )?(\d+ to check)?)$/);
  await expect(cue1.locator(".dh-on b")).toHaveText("web-lead");
  await expect(page.locator(".dh-tbl [title]")).toHaveCount(0);
  // A kind whose only open item is finishing (#181) is moving: "1 finishing", not "none moving".
  await expect(cells.nth(0).locator(".dh-sm")).toHaveText("none moving");
  await cue.change((st) => { Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d6"), { state: "finishing", finish: { step: "building" } }); });
  await expect(cells.nth(0).locator(".dh-sm")).toHaveText("1 finishing");
  await rows.nth(1).click();
  await expect(page.locator(".dk-items .dk-it").first()).toHaveAttribute("data-desk-item", "w1");
  // The name in the Sessions head → All Desks: back to Desk home, then into another.
  await expect(page.locator("[data-desk-switch]")).toHaveCount(1);
  await page.locator("[data-desk-switch]").click(); await page.locator("[data-desk-home]").click();
  await expect(rows).toHaveCount(2);
  await rows.first().click();
  await expect(page.locator(".dk-sh b")).toHaveText("cue");
  await expect(page.locator(".dk-sh button:not(.dk-ab-change)", { hasText: "Change" })).toHaveCount(0);
});

test("the Desk's name is a switcher: every Desk with who needs you, typed to find, ↑ ↓ ↵ to open, All Desks and + New Desk (#240)", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator("[data-desk-switch]").click(); await page.locator("[data-desk-home]").click();
  await page.locator('.dk-desk[data-desk-project="website"]').click();
  const btn = page.locator("[data-desk-switch]"), menu = page.locator(".dk-sw-menu");
  await expect(btn).toHaveAttribute("aria-expanded", "false");
  await expect(menu).toHaveCount(0);
  await btn.click();
  await expect(btn).toHaveAttribute("aria-expanded", "true");
  const rows = menu.locator(".dk-sw-row");
  await expect(rows).toHaveCount(2);
  await expect(menu.locator(".dk-sw-row.on")).toHaveAttribute("data-desk-go", "website");
  await expect(menu.locator(".dk-sw-row.on .dk-sw-tick")).toHaveText("✓");
  await expect(menu.locator(".dk-sw-find")).toBeFocused();
  await expect(menu.locator(".dk-sw-foot button")).toHaveText(["All Desks", "+ New Desk"]);
  expect(await page.locator(".dk-sw-menu [title]").count()).toBe(0);
  // Typing finds; the box keeps the cursor through the redraw.
  await page.keyboard.type("cu");
  await expect(rows).toHaveCount(1);
  await expect(menu.locator(".dk-sw-find")).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(menu).toHaveCount(0);
  await expect(page.locator(".dk-sh b")).toHaveText("cue");
  // ↓ ↵ from the current Desk opens the next; Esc closes and gives the button the keyboard.
  await btn.click();
  await expect(menu.locator(".dk-sw-find")).toHaveValue("");
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(btn).toBeFocused();
  await btn.click();
  const lit = await menu.locator(".dk-sw-row.at").getAttribute("data-desk-go");
  await page.keyboard.press("ArrowDown");
  await expect(menu.locator(".dk-sw-row.at")).not.toHaveAttribute("data-desk-go", lit);
  const next = await menu.locator(".dk-sw-row.at").getAttribute("data-desk-go");
  await page.keyboard.press("Enter");
  expect(await page.evaluate(() => deskSelection())).toBe(next);
  // A click outside closes it; a click on a row opens that Desk.
  await btn.click();
  await page.locator(".dk-items").click({ position: { x: 4, y: 4 } });
  await expect(menu).toHaveCount(0);
  await btn.click();
  await menu.locator('.dk-sw-row[data-desk-go="cue"]').click();
  await expect(page.locator(".dk-sh b")).toHaveText("cue");
  // + New Desk opens its sheet, the menu closed.
  await btn.click();
  await menu.locator("[data-desk-new-desk]").click();
  await expect(menu).toHaveCount(0);
  await expect(page.locator(".nd")).toHaveCount(1);
});

test("+ New is a menu: New session as before, New Desk asks a project and a name, starts its lead and opens it", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator("[data-desk-switch]").click(); await page.locator("[data-desk-home]").click();
  await cue.change((st) => { st.history = [...(st.history || []), { cwd: "/work/notes", created_ms: Date.now() }, { cwd: "/work/blog", created_ms: Date.now() }]; });
  const chip = page.locator("[data-new-menu]");
  await chip.click();
  await expect(page.locator(".nw-pop button")).toHaveText(["New session⌘N", "New Desk"]);
  // Esc closes the menu; New session opens + New's form as it always did.
  await page.keyboard.press("Escape");
  await expect(page.locator(".nw-pop")).toHaveCount(0);
  await chip.click();
  await page.locator('.nw-pop [data-lv="new"]').click();
  await expect(page.locator(".nw-pop")).toHaveCount(0);
  await expect(page.locator('[data-text="new-name"]')).toBeVisible();
  await expect(page.locator('[data-lv="desk-start"]')).toHaveCount(0);
  await page.keyboard.press("Escape");
  // New Desk can reuse existing folders and also accepts a new folder.
  await page.evaluate(() => { agentsAvail = ["claude", "codex", "pi"]; });
  await chip.click();
  await page.locator(".nw-pop [data-desk-new-desk]").click();
  const sheet = page.locator(".nd");
  await expect(sheet.locator(".nd-l")).toHaveText(["Recent folders", "Folder", "Name", "Agent"]);
  await expect(sheet.locator("#nd-name")).toBeFocused();
  // Project is Cue's own dropdown, no system menu: the name first, its folder muted beside it.
  await expect(sheet.locator(".nd-agent .seg button")).toHaveText(["Claude Code", "Codex", "Pi"]);
  await expect(sheet.locator('[data-nd-agent="claude"]')).toHaveAttribute("aria-pressed", "true");
  const pick = sheet.locator("#nd-proj"), opts = sheet.locator(".nd-drop [role=option]");
  await pick.click();
  await expect(opts.filter({ hasText: "/work/notes" })).toHaveCount(1);
  await expect(opts.filter({ hasText: "/work/blog" })).toHaveCount(1);
  await opts.filter({ hasText: "/work/notes" }).click();
  await expect(sheet.locator(".nd-drop")).toHaveCount(0);
  await expect(pick.locator(".nd-pn")).toHaveText("notes");
  await expect(pick.locator(".nd-pp")).toHaveText("/work/notes");
  await expect(sheet.locator("#nd-name")).toHaveValue("notes");
  expect(await pick.locator(".nd-pp").evaluate((el) => getComputedStyle(el).color)).not.toBe(await pick.locator(".nd-pn").evaluate((el) => getComputedStyle(el).color));
  // Keys: ↓ on it opens its list, Esc closes just the list, the sheet stays.
  await pick.focus();
  await page.keyboard.press("ArrowDown");
  await expect(opts.filter({ hasText: "/work/notes" })).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(sheet.locator(".nd-drop")).toHaveCount(0);
  await expect(sheet).toHaveCount(1);
  await expect(pick).toBeFocused();
  // Esc closes it with nothing started.
  await page.keyboard.press("Escape");
  await expect(sheet).toHaveCount(0);
  // From Desk home's foot too; the name typed is the Desk's.
  await page.locator(".dh-add").click();
  await pick.click();
  await opts.filter({ hasText: "/work/notes" }).click();
  await sheet.locator("#nd-name").fill("Notes app");
  await sheet.locator('[data-nd-agent="codex"]').click();
  await expect(sheet.locator('[data-nd-agent="codex"]')).toHaveAttribute("aria-pressed", "true");
  await sheet.locator("#nd-name").press("Enter");
  await cue.called("desk_start", { folder: "/work/notes", agent: "codex" });
  await expect(sheet).toHaveCount(0);
  await expect(page.locator(".dk-sh b")).toHaveText("Notes app");
  // The same workspace stays available for another independent Desk.
  await page.locator("[data-desk-switch]").click(); await page.locator("[data-desk-home]").click();
  await expect(page.locator(".dk-desks .dk-desk")).toHaveCount(3);
  await page.locator(".dh-add").click();
  await pick.click();
  await expect(opts.filter({ hasText: "/work/notes" })).toHaveCount(1);
  await expect(opts.filter({ hasText: "/work/blog" })).toHaveCount(1);
  await sheet.locator(".btn[data-nd-close]").click();
  await expect(sheet).toHaveCount(0);
});

test("the Desk's name renames from its Sessions head", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator(".dk-sh").hover();
  await page.locator(".dk-sh .dk-ren").click();
  const box = page.locator(".dk-sh .pj-name-in");
  await box.fill("Cue app");
  await box.press("Enter");
  await expect(page.locator(".dk-sh b")).toHaveText("Cue app");
});

test("a crew member's session opens its lead's crew page on the Desk; Esc and ‹ Back to the Desk come back", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const toCrewPage = async () => {
    await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
    await page.locator('.dk-right [data-desk-crew="id-search"]').click();
    await page.locator('.dk-right [data-desk-crew-open="id-search"]').click();
    await expect(page.locator(".crew-page")).toBeVisible();
    await expect(page.locator(".crew-page .cp-back")).toHaveText("‹ Back to the Desk");
    await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/\bon\b/);
  };
  await toCrewPage();
  await page.locator(".crew-page .cp-back").click();
  await expect(page.locator(".crew-page")).toHaveCount(0);
  await expect(page.locator(".dk-board")).toBeVisible();
  // Esc: the page's own steps first (the member back to its lead), then out to the Desk.
  await toCrewPage();
  await page.locator("body").press("Escape");
  await expect(page.locator(".crew-page")).toBeVisible();
  await page.locator("body").press("Escape");
  await expect(page.locator(".crew-page")).toHaveCount(0);
  await expect(page.locator(".dk-board")).toBeVisible();
});

test("what's open on the right, and Change, close when the header picks another project", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  await expect(page.locator(".dk-right .dk-pg h1 .dk-tname")).toHaveText("web-lead");
  await pick(page, "website");
  await expect(page.locator(".dk-right .dk-ipane")).toHaveCount(0);
  // website has no lead yet: who runs it.
  await expect(page.locator(".dk-pick h2")).toHaveText("Who runs this Desk?");
  await pick(page, "cue");
  await page.locator("[data-desk-change]").first().click();
  await expect(page.locator(".dk-pick")).toBeVisible();
  await pick(page, "website");
  await pick(page, "cue");
  await expect(page.locator(".dk-pick")).toHaveCount(0);
  await expect(page.locator(".dk-tab.on")).toHaveText("web-lead");
});

test("Drop this item asks once more before it drops", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  const drop = page.locator(".dk-right .dk-drop");
  await drop.click();
  await expect(drop).toHaveText("Drop it");
  expect(await cue.calls("desk_item_act")).toEqual([]);
  // Any other click lets it go.
  await page.locator(".dk-right h1").click();
  await expect(drop).toHaveText("Drop this item");
  await drop.click();
  await drop.click();
  await cue.called("desk_item_act", { id: "d6", act: "drop" });
});

test("an item no one holds yet, or waiting on nothing, reads plain: next", async ({ page, cue }) => {
  await cue.open("desk");
  const it = { kind: "bug", state: "queued", holder: null, after: null, from: "you", created: Date.now() / 1000 };
  // Its page's meta line and NOW (#200): no holder named, plain "Next".
  expect(await page.evaluate((i) => deskItemMeta(i, { items: [] }), it)).toMatch(/^added by you · /);
  expect(await page.evaluate((i) => deskItemNow(i), it)).toEqual({ dot: "stop", word: "Next", say: "Next up, not given to a session yet." });
  expect(await page.evaluate((i) => deskTag(i), it)).toEqual(["", ""]);
  // One whose hand-off failed is tagged stuck instead.
  expect(await page.evaluate((i) => deskTag({ ...i, route_failed: "web-lead is gone" }), it)).toEqual(["stuck", "ask"]);
});

test("an item that couldn't be given out says why; a dropped one is gone", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-it", { hasText: "Old idea, dropped" })).toHaveCount(0);
  // Nor is it counted on its project's row on Desk home.
  const card = await page.evaluate(() => {
    state.desk.projects.push({ project: "zz", lead: null, decisions: [], items: [{ id: "z1", project: "zz", kind: "task", title: "Gone", state: "dropped", created: 1, updated: 1 }] });
    return deskAll().split('data-desk-project="zz"')[1];
  });
  expect(card).toContain("Nothing on it yet.");
  expect(card).not.toContain("Gone");
  expect(card).not.toContain(" open");
  await expect(page.locator('[data-desk-item="d6"] .dk-tag')).toHaveText("stuck");
  await expect(page.locator('[data-desk-item="d6"] .dk-who')).toHaveText("Give to…");
  await page.locator('[data-desk-item="d6"]').click();
  await expect(page.locator(".dk-right .dk-pg")).toContainText("Couldn't give it out");
  await expect(page.locator(".dk-right .dk-pg")).toContainText("No session in this project could take it.");
});

test("rows open from the keyboard: Enter or Space, and the focus stays on the row", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d4"]').focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".dk-right .dk-pg h1")).toHaveText("Bug #d4 · Rail dot blinks for a reply you didn't send");
  await expect(page.locator('[data-desk-item="d4"]')).toBeFocused();
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  await expect(page.locator(".dk-right .dk-pg h1 .dk-tname")).toHaveText("web-lead");
  // The Desk's runner's pane: its mark before its name, where a crew member's is (#170); one name wherever it shows (#175).
  await expect(page.locator('.dk-right .dk-pg h1 [data-desk-mark="desk"]')).toHaveText("Dispatcher");
  await expect(page.locator(".dk-right .dk-markrow")).toHaveCount(0);
  if (process.env.SHOT_DIR) await page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-lead-mark.png` });
  await page.locator('.dk-right [data-desk-crew="id-search"]').focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".dk-right .dk-pg h1 .dk-tname")).toHaveText("search-centre");
  // Its mark before its name, in the same place as every chat's (#170): no row of its own above.
  await expect(page.locator('.dk-right .dk-pg h1 [data-desk-mark="crew"]')).toHaveText("Crew");
  await expect(page.locator('.dk-right .dk-pg h1 [data-desk-mark="crew"] svg.dk-kic')).toHaveCount(1);
  if (process.env.SHOT_DIR) await page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-crew-mark.png`, clip: undefined });
});

test("Send back opens the item's own \"Why send it back?\" box: it keeps what you wrote when it doesn't go, and closes when it does", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  // The item's own box, typed in first, isn't where the reason comes from.
  await page.locator(".dk-box textarea").fill("for the chat");
  await page.locator('.dk-right [data-desk-act="send_back"]').click();
  const why = page.locator(".dk-right .dk-sb");
  await expect(why.locator(".dk-sb-l")).toHaveText("Why send it back?");
  const box = why.locator("textarea");
  await expect(box).toBeFocused();
  // A box as the Desk's own: the mic and a picture too.
  await expect(why.locator('[data-mic="back:d2"]')).toHaveCount(1);
  await expect(why.locator('[data-attach="back:d2"]')).toHaveCount(1);
  await page.screenshot({ path: `${process.env.SHOT_DIR || "/tmp"}/send-back-open.png` });
  await box.fill("Still loses focus");
  await cue.override("desk_item_act", () => { throw "offline"; });
  await why.locator('[data-desk-act="back_go"]').click();
  await expect(page.locator(".toast")).toContainText("offline");
  await expect(box).toHaveValue("Still loses focus");
  await cue.override("desk_item_act", () => null);
  // A picture goes with the words.
  await page.evaluate(() => { draft("back:d2").images.push({ name: "shot.png", mime: "image/png", data: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=" }); renderMain(); });
  await expect(why.locator(".chips")).toHaveCount(1);
  await box.press("Enter");
  await expect.poll(async () => (await cue.calls("desk_item_act")).filter((c) => c.args.act === "send_back").map((c) => [c.args.text, c.args.images.map((i) => i.name)]))
    .toEqual([["Still loses focus", []], ["Still loses focus", ["shot.png"]]]);
  await expect(page.locator(".dk-right .dk-sb")).toHaveCount(0);
  await expect(page.locator(".dk-box textarea")).toHaveValue("for the chat");
  expect(await cue.calls("desk_send")).toEqual([]);
});

// ---------- who runs the Desk ----------

test("no lead yet, one session: 'Let <name> run it', or start a new lead; picked, its chat opens", async ({ page, cue }) => {
  await cue.open("desk");
  await pick(page, "website");
  await page.locator('.switch-view [data-view="desk"]').click();
  const card = page.locator(".dk-right .dk-pick");
  await expect(card.locator("h2")).toHaveText("Who runs this Desk?");
  await expect(card.locator(".btn.primary")).toHaveText("Let site-lead run it");
  await expect(card.locator(".dk-fresh")).toHaveText("Start a new dispatcher");
  // No box to anyone, nothing runs it, till someone does.
  await expect(page.locator(".dk-box")).toHaveCount(0);
  await expect(page.locator(".dk-strip .dk-lcard")).toHaveCount(0);
  await card.locator(".btn.primary").click();
  await cue.called("desk_set_lead", { project: "website", sid: "site" });
  await expect(page.locator(".dk-tab.on")).toHaveText("site-lead");
  await expect(page.locator(".dk-strip .dk-lcard")).toContainText("site-lead");
  await expect(page.locator(".dk-box textarea")).toHaveAttribute("placeholder", "Tell site-lead: a bug, a feature, or a question…");
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

test("no sessions in the project: one button, Start a lead; with no folder known, + New's folder step opens", async ({ page, cue }) => {
  await cue.open("desk");
  await pick(page, "website");
  await page.locator('.switch-view [data-view="desk"]').click();
  // Its one session gone (the header's picker still has the project picked).
  await cue.change((st) => { st.sessions = st.sessions.filter((s) => s.project !== "website"); });
  const card = page.locator(".dk-right .dk-pick");
  await expect(card.locator("button")).toHaveCount(1);
  await expect(card.locator("button")).toHaveText("Start a dispatcher");
  await card.locator("button").click();
  await cue.called("desk_set_lead", { project: "website" });
  await expect(page.locator(".toast")).toContainText("No folder known");
  expect(await page.evaluate(() => liveSpot)).toBe(true);
});

test("Start a new lead with a folder known: a '<project>-lead' runs it", async ({ page, cue }) => {
  await cue.open("desk");
  await pick(page, "website");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-pick .dk-fresh").click();
  await cue.called("desk_set_lead", { project: "website" });
  await expect(page.locator(".dk-tab.on")).toHaveText("website-lead");
  await expect(page.locator(".dk-chat")).toBeVisible();
});

test("⋯ in the head strip: the project's other sessions, each with Run it; Keep or Esc goes back; Run it hands it over", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator(".dk-strip [data-desk-change]").click();
  const card = page.locator(".dk-right .dk-pick");
  // web-lead runs it now: not listed, Keep web-lead instead.
  const rows = await card.locator(".dk-who").evaluateAll((els) => els.map((el) => [el.querySelector(".dk-lt").textContent, el.querySelector(".dk-ls").textContent]));
  expect(rows).toEqual([["host-lead", "session"]]);
  await expect(card.locator(".dk-who .btn")).toHaveText("Run it");
  await expect(card.locator(".dk-fresh")).toHaveText("Start a new dispatcher");
  await expect(card.locator(".dk-keep")).toHaveText("Keep web-lead");
  await card.locator(".dk-keep").click();
  await expect(page.locator(".dk-pick")).toHaveCount(0);
  await expect(page.locator(".dk-tab.on")).toHaveText("web-lead");
  await page.locator(".dk-strip [data-desk-change]").click();
  await page.locator("body").press("Escape");
  await expect(page.locator(".dk-pick")).toHaveCount(0);
  await page.locator(".dk-strip [data-desk-change]").click();
  await card.locator('.dk-who [data-desk-lead="host"]').click();
  await cue.called("desk_set_lead", { project: "cue", sid: "host" });
  await expect(page.locator(".dk-strip .dk-lcard")).toContainText("host-lead");
});

test("✎ beside a session's name (its row, its chat's header) renames the session: Enter keeps it, Esc doesn't, the row doesn't open", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const row = page.locator(".dk-sessions .dk-scroll .dk-srow:not(.crew)").first();
  const pen = row.locator(".dk-sren"), box = row.locator(".rename-in");
  await expect(pen).toHaveAttribute("aria-label", "Rename this session");
  await expect(pen).toHaveCSS("opacity", "0");
  await row.hover();
  await expect(pen).toHaveCSS("opacity", "1");
  const before = await row.locator(".dk-nm").textContent();
  await pen.click();
  await expect(box).toBeFocused();
  await expect(box).toHaveValue(before);
  await box.click();
  await expect(page.locator(".dk-right .dk-ph2 .dk-shn")).toHaveCount(0);
  await box.fill("not this");
  await box.press("Escape");
  await expect(box).toHaveCount(0);
  await expect(row.locator(".dk-nm")).toHaveText(before);
  await row.hover();
  await pen.click();
  await box.fill("search-fix");
  await box.press("Enter");
  await expect(row.locator(".dk-nm")).toHaveText("search-fix");
  // Its chat's header has the same ✎.
  // In its page's header, by its name (#200).
  await row.click();
  const head = page.locator(".dk-right .dk-ph2");
  const nm = head.locator(".dk-shn");
  await expect(nm).toHaveText("search-fix");
  await head.hover();
  await head.locator(".dk-sren").click();
  await head.locator(".rename-in").fill("search-fix-2");
  await head.locator(".rename-in").press("Enter");
  await expect(nm).toHaveText("search-fix-2");
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

test("✎ by the Desk's name: Esc keeps the name as it was; a click away keeps what you typed", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const head = page.locator(".dk-sh"), box = head.locator(".pj-name-in");
  await expect(head.locator(".dk-ren")).toHaveAttribute("aria-label", "Rename this Desk");
  await head.hover();
  await head.locator(".dk-ren").click();
  await expect(box).toBeFocused();
  await box.fill("not this");
  await box.press("Escape");
  await expect(box).toHaveCount(0);
  await expect(head.locator("b")).toHaveText("cue");
  await head.hover();
  await head.locator(".dk-ren").click();
  await box.fill("Cue app");
  await page.locator(".dk-items .dk-ch > b").click();
  await expect(box).toHaveCount(0);
  await expect(head.locator("b")).toHaveText("Cue app");
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

test("several sessions and no lead: a row each, leads first (dot), plain ones hollow", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead = null; });
  await openDesk(page);
  const who = page.locator(".dk-right .dk-who");
  expect(await who.evaluateAll((els) => els.map((el) => [el.querySelector(".dk-lt").textContent, el.querySelector(".dk-ls").textContent, el.querySelector("i").className]))).toEqual([
    ["web-lead", "lead, crew of 2", ""], ["host-lead", "session", "idle"],
  ]);
  await expect(page.locator(".dk-keep")).toHaveCount(0);
});

test("a lead that has ended: one line, Resume it or Pick another; no box", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead.state = "ended"; });
  await openDesk(page);
  const line = page.locator(".dk-right .dk-endlead");
  await expect(line.locator("p")).toHaveText("web-lead has ended. Items stay here.");
  await expect(page.locator(".dk-right .dk-box")).toHaveCount(0);
  await line.locator('[data-desk-lead="lead"]').click();
  await cue.called("desk_set_lead", { project: "cue", sid: "lead" });
  await expect(page.locator(".dk-chat")).toBeVisible();
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead.state = "ended"; });
  await page.locator(".dk-endlead [data-desk-change]").click();
  await expect(page.locator(".dk-pick h2")).toHaveText("Who runs this Desk?");
});

// ---------- Give to… ----------

test("Give to… on a row: a menu of the project's sessions over the list (rows stay put); a pick gives it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const before = await listOrder(page);
  const below = await page.locator('[data-desk-item="d4"]').boundingBox();
  await page.locator('[data-desk-item="d6"] [data-desk-give]').click();
  const menu = page.locator('[data-desk-item="d6"] .dk-givemenu');
  await expect(menu.locator("button")).toHaveText([/web-lead\s*lead, crew of 2/, /host-lead\s*session/, "New session"]);
  expect(await listOrder(page)).toEqual(before);
  expect((await page.locator('[data-desk-item="d4"]').boundingBox()).y).toBe(below.y);
  // Its row didn't open.
  await expect(page.locator(".dk-right .dk-ipane")).toHaveCount(0);
  await menu.locator('[data-desk-give-to="host"]').click();
  await cue.called("desk_give", { id: "d6", sid: "host" });
  await expect(page.locator(".dk-givemenu")).toHaveCount(0);
  await expect(page.locator('[data-desk-item="d6"] [data-desk-give]')).toHaveCount(0);
});

test("Give to…'s menu opens whole inside the window, with each session's name in full, at any width (#152)", async ({ page, cue }) => {
  await cue.open("desk");
  for (const [w, h] of [[900, 700], [1300, 900], [1300, 420]]) {
    await page.setViewportSize({ width: w, height: h });
    await openDesk(page);
    await page.locator('[data-desk-item="d6"] [data-desk-give]').click();
    const menu = page.locator(".dk-givemenu");
    await expect(menu).toBeVisible();
    const box = await menu.boundingBox();
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.y).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(w);
    expect(box.y + box.height).toBeLessThanOrEqual(h);
    // Each name whole: none cut by its menu's edge.
    expect(await menu.locator("button > span:first-child").evaluateAll((els) => els.every((el) => el.scrollWidth <= el.clientWidth + 1 && el.getBoundingClientRect().right <= el.closest(".dk-givemenu").getBoundingClientRect().right))).toBe(true);
    await page.keyboard.press("Escape");
    await expect(page.locator(".dk-givemenu")).toHaveCount(0);
  }
});

test("Give to… in the open item; Esc closes the menu, not the item; New session starts one", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"] .dk-t').click();
  const pane = page.locator(".dk-right .dk-pg");
  await pane.locator("[data-desk-give]").click();
  await expect(pane.locator(".dk-givemenu")).toBeVisible();
  await page.locator("body").press("Escape");
  await expect(pane.locator(".dk-givemenu")).toHaveCount(0);
  await expect(pane.locator("h1")).toHaveText("Task #d6 · Tidy the Settings copy");
  await pane.locator("[data-desk-give]").click();
  await pane.locator('.dk-givemenu [data-desk-act="start_now"]').click();
  await cue.called("desk_item_act", { id: "d6", act: "start_now" });
});

// ---------- the quick-add line ----------

test("the quick-add line keeps its height as you type (a one-line field doesn't grow or shrink)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const line = page.locator('.dk-qa [data-qa="line"]');
  const h = async () => (await line.boundingBox()).height;
  const before = await h();
  expect(before).toBe(32);
  await line.pressSequentially("Logo blurs");
  expect(await h()).toBe(before);
  await line.fill("");
  await line.pressSequentially("x");
  expect(await h()).toBe(before);
});

test("the quick-add line: Enter adds an item of no kind (the lead says which), the line keeps the cursor; no New item, no kind switch", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator("[data-desk-new]")).toHaveCount(0);
  await expect(page.locator(".dk-qa button", { hasText: /^(Bug|Feature|Task)$/ })).toHaveCount(0);
  const line = page.locator('.dk-qa [data-qa="line"]');
  await expect(line).toHaveAttribute("placeholder", "Add an item…");
  await line.fill("Dark mode for the Desk");
  await line.press("Enter");
  await cue.called("desk_add", { project: "cue", kind: "", title: "Dark mode for the Desk" });
  await expect(line).toHaveValue("");
  await expect(line).toBeFocused();
  // No kind yet: the pane shows no kind before where it is.
  const row = page.locator(".dk-items .dk-it", { hasText: "Dark mode for the Desk" });
  await row.click();
  await expect(page.locator(".dk-pg .dk-state")).toHaveText(/^added by you · /);
  await expect(page.locator(".dk-pg .dk-pnow .dk-pword")).toHaveText("Not given out");
});

test("Done when is optional: ⇧Enter or + Done when opens it, Enter adds both; Esc closes an empty one", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const line = page.locator('.dk-qa [data-qa="line"]'), done = page.locator('.dk-qa [data-qa="done"]');
  await expect(done).toHaveCount(0);
  await line.fill("Search keeps its place");
  await line.press("Shift+Enter");
  await expect(done).toBeFocused();
  await done.press("Escape");
  await expect(done).toHaveCount(0);
  await expect(line).toHaveValue("Search keeps its place");
  await page.locator("[data-desk-qa-done]").click();
  await done.fill("It reopens where it was");
  await done.press("Enter");
  await cue.called("desk_add", { project: "cue", kind: "", title: "Search keeps its place", doneWhen: "It reopens where it was" });
  await expect(done).toHaveCount(0);
  await expect(line).toHaveValue("");
});

test("several lines pasted: an item a line (bullets dropped); one line pastes as text", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dk-qa [data-qa="line"]').focus();
  const paste = (text) => page.evaluate((t) => {
    const el = document.querySelector('.dk-qa [data-qa="line"]'), dt = new DataTransfer();
    dt.setData("text/plain", t);
    return el.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }));
  }, text);
  expect(await paste("- First thing\n\n2. Second thing\n* Third thing")).toBe(false);
  await expect.poll(async () => (await cue.calls("desk_add")).map((c) => c.args.title)).toEqual(["First thing", "Second thing", "Third thing"]);
  expect((await cue.calls("desk_add")).every((c) => !("doneWhen" in c.args))).toBe(true);
  expect(await paste("Just one")).toBe(true);
  expect((await cue.calls("desk_add")).length).toBe(3);
});

test("what's half-typed in the quick-add line stays on its own project's Desk", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const line = page.locator('.dk-qa [data-qa="line"]');
  await line.fill("Only for cue");
  await pick(page, "website");
  await expect(line).toHaveValue("");
  await pick(page, "cue");
  await expect(line).toHaveValue("Only for cue");
});

test("a picture dropped on the quick-add line (or picked with its button) goes with the item; its pane shows it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const qa = page.locator(".dk-qa");
  await expect(qa.locator('[data-attach^="desk-qa:"]')).toHaveCount(1);
  await page.evaluate(() => {
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array([137, 80, 78, 71])], "shot.png", { type: "image/png" }));
    const el = document.querySelector('.dk-qa [data-qa="line"]');
    for (const type of ["dragover", "drop"]) el.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: dt }));
  });
  await expect(qa.locator(".chip")).toHaveCount(1);
  const line = qa.locator('[data-qa="line"]');
  await line.fill("Logo blurs on the Desk");
  await line.press("Enter");
  const call = await cue.called("desk_add", { project: "cue", kind: "", title: "Logo blurs on the Desk" });
  expect(call.args.images.map((i) => [i.name, i.mime])).toEqual([["shot.png", "image/png"]]);
  await expect(qa.locator(".chip"), "the picture went with it").toHaveCount(0);
  await expect(line).toBeFocused();
  // The item, with its picture kept: the pane shows it under Pictures.
  const id = await page.evaluate(() => deskOf("cue").items[0].id);
  await cue.change((st, id) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === id).images = ["data:image/png;base64,iVBORw0KGgo="]; }, id);
  await page.locator(`[data-desk-item="${id}"]`).click();
  await expect(page.locator(".dk-pane .thumbs .thumb")).toHaveCount(1);
});

test("pasted lines that don't go: the first back in the line, and how many failed", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.override("desk_add", () => { throw "offline"; });
  await page.evaluate(() => deskQaAdd(["One", "Two", "Three"]));
  await expect(page.locator(".toast")).toContainText("3 not added: offline");
  await expect(page.locator('.dk-qa [data-qa="line"]')).toHaveValue("One");
});

test("one item with Done when that doesn't go: both come back, Done when open", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.override("desk_add", () => { throw "offline"; });
  const line = page.locator('.dk-qa [data-qa="line"]');
  await line.fill("Search keeps its place");
  await line.press("Shift+Enter");
  await page.locator('.dk-qa [data-qa="done"]').fill("It reopens where it was");
  await page.locator('.dk-qa [data-qa="done"]').press("Enter");
  await expect(page.locator(".toast")).toContainText("Not added: offline");
  await expect(line).toHaveValue("Search keeps its place");
  await expect(page.locator('.dk-qa [data-qa="done"]')).toHaveValue("It reopens where it was");
});

test("⋯ in the head strip with an item open: the open pane closes and the picker shows", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  await page.locator(".dk-right .dk-drop").click();
  await page.locator(".dk-strip [data-desk-change]").click();
  await expect(page.locator(".dk-pick h2")).toHaveText("Who runs this Desk?");
  await expect(page.locator(".dk-right .dk-ipane")).toHaveCount(0);
  await expect(page.locator(".dk-it.on")).toHaveCount(0);
  expect(await page.evaluate(() => deskDropArm)).toBe(null);
});

test("a session's name is escaped in Let <name> run it", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.sessions.find((s) => s.session_id === "site").name = "<b>&site"; });
  await pick(page, "website");
  await page.locator('.switch-view [data-view="desk"]').click();
  await expect(page.locator(".dk-pick .btn.primary")).toHaveText("Let <b>&site run it");
});

test("⌘N on a project's Desk focuses the quick-add line", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator(".dk-tab.on").click();
  await page.keyboard.press("Meta+n");
  await expect(page.locator('.dk-qa [data-qa="line"]')).toBeFocused();
});

test("the shots: the lead's Desk, Give to…, Change, the picker", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  const dir = process.env.TMPDIR || "/tmp";
  await page.screenshot({ path: `${dir}/cue-desk-lead.png` });
  await page.locator('[data-desk-item="d6"] [data-desk-give]').click();
  await page.screenshot({ path: `${dir}/cue-desk-give.png` });
  await page.locator("body").press("Escape");
  await page.locator(".dk-strip [data-desk-change]").click();
  await page.screenshot({ path: `${dir}/cue-desk-change.png` });
  await pick(page, "website");
  await page.locator(".dk-pick").waitFor();
  await page.screenshot({ path: `${dir}/cue-desk-pick1.png` });
});

test("the lead's own Desk lines are drawn as lines in its chat, code blocks left as they are; no Send to…", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => {
    sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: [
      "Here's the plan.",
      "",
      "ITEM: bug | Box loses focus | done when: typing keeps focus",
      "- **ROUTE:** #d7k2 -> web-lead | after: #a1 | why: it knows the box",
      "DONE: #d3",
      "CONTEXT: host-lead -> keep | why: more to come",
      "",
      "```",
      "ITEM: task | an example | done when: never",
      "```",
    ].join("\n") });
    renderMain();
  });
  const said = page.locator(".dk-chat .cv-agent").last();
  // Each line one short quiet line at the left (#157): no rules, no box, no chip.
  const ln = said.locator(".own-lines .dk-sl");
  await expect(ln).toHaveCount(4);
  await expect(ln.nth(0)).toHaveText("Added a bug: Box loses focus");
  await expect(ln.nth(0).locator("svg.dk-kic")).toHaveCount(0);
  await expect(said.locator(".own-lines")).toHaveCSS("border-top-width", "0px");
  expect(await said.locator(".own-lines .dk-cl").count()).toBe(0);
  // A hand-off that waits: "#d7k2 waits on #a1", a plain sentence at the left, one line, its why shut; no arrow, no chevron.
  const route = ln.nth(1);
  await expect(route.locator(".dk-sl-h")).toHaveText("#d7k2 waits on #a1");
  await expect(said.locator(".own-lines")).not.toContainText("→");
  await expect(said.locator(".own-lines")).not.toContainText("⌄");
  const lay = await route.evaluate((el) => { const r = el.getBoundingClientRect(), t = el.querySelector(".dk-sl-h").getBoundingClientRect(), m = el.closest(".cv-text").getBoundingClientRect(); return { left: t.left - m.left, h: r.height }; });
  expect(lay.left).toBeLessThan(4);
  expect(lay.h).toBeLessThan(24);
  await expect(route.locator(".dk-sl-d")).toHaveCount(0);
  await expect(route.locator(".dk-sl-r")).toHaveAttribute("aria-expanded", "false");
  // A click opens its why under it, plain (no fill, no box); it stays open through a redraw, and a click shuts it.
  await route.locator(".dk-sl-r").dispatchEvent("click");
  await expect(route.locator(".dk-sl-d")).toHaveText("Then to web-lead · it knows the box");
  await expect(route.locator(".dk-sl-d")).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await expect(route.locator(".dk-sl-d")).toHaveCSS("border-top-width", "0px");
  await page.evaluate(() => renderMain());
  await expect(said.locator(".dk-sl").nth(1).locator(".dk-sl-d")).toHaveCount(1);
  await said.locator(".dk-sl").nth(1).locator(".dk-sl-r").dispatchEvent("click");
  await expect(said.locator(".dk-sl-d")).toHaveCount(0);
  // Done: "Feature #d3 is done", its number a link; its title on a click, which opens the item.
  await expect(ln.nth(2).locator(".dk-sl-h")).toHaveText("Feature #d3 is done");
  await expect(ln.nth(2).locator("button.dk-say-id")).toHaveText("Feature #d3");
  await expect(ln.nth(2).locator("svg.dk-kic")).toHaveCount(0);
  await ln.nth(2).locator(".dk-sl-r").dispatchEvent("click");
  await expect(ln.nth(2).locator(".dk-sl-it")).toHaveCount(1);
  await expect(ln.nth(3).locator(".dk-sl-h")).toHaveText("Kept host-lead's context");
  await expect(said.locator(".cv-text")).toContainText("Here's the plan.");
  await expect(said.locator("pre")).toContainText("ITEM: task | an example");
  await expect(said).not.toContainText("ROUTE:");
  await expect(page.locator(".dk-chat .cv-fwd")).toHaveCount(0);
  // The number is a link: it opens its item, not the line's why; Enter on the line opens the why.
  await page.evaluate(() => deskLnOpen.clear());
  await page.evaluate(() => renderMain());
  await said.locator(".dk-sl").nth(1).locator(".dk-sl-r").focus();
  await page.keyboard.press("Enter");
  await expect(said.locator(".dk-sl").nth(1).locator(".dk-sl-d")).toHaveCount(1);
  await said.locator(".dk-sl").nth(2).locator("button.dk-say-id").click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "d3"]);
  await expect(page.locator(".dk-chat .dk-sl").nth(2).locator(".dk-sl-d")).toHaveCount(0);
});

test("the dispatcher chat keeps the plain answer above its routing update", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: "Yes. All approvals should appear under Waiting on you.\n\nROUTE: #d2 -> web-lead | why: warm" }); renderMain(); });
  const reply = page.locator(".dk-chat .cv-agent").last();
  await expect(reply.locator(".cv-text > p:first-child")).toHaveText("Yes. All approvals should appear under Waiting on you.");
  const l = reply.locator(".dk-sl");
  await expect(l.locator(".dk-sl-h")).toHaveText(/^Gave \w+ #d2 to web-lead$/);
  await expect(l.locator(".dk-rt-to")).toHaveText("web-lead");
});

test("a hand-off to a new session names that session once Cue has started it, and #new is the item just above", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const say = (text) => page.evaluate((t) => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: t }); renderMain(); }, text);
  const last = page.locator(".dk-chat .cv-agent").last();
  await say(["ITEM: bug | \"Looks right\" is too loud | done when: it's quiet", "ROUTE: #new -> new | why: no one has the header"].join("\n"));
  await expect(last.locator(".dk-sl").nth(1).locator(".dk-sl-h")).toHaveText("Gave the new item to a new session");
  // Cue makes the item and starts a session for it: the card says both.
  await page.evaluate(() => {
    const d = state.desk.projects.find((p) => p.project === "cue");
    d.items.push({ id: "q9", project: "cue", kind: "bug", title: "\"Looks right\" is too loud", state: "working", created: Date.now(), updated: Date.now(), holder: { kind: "session", sid: "s-new", name: "cue-117" } });
    renderMain();
  });
  await expect(last.locator(".dk-sl").nth(1).locator(".dk-sl-h")).toHaveText("Gave Bug #q9 to cue-117");
  await expect(last.locator(".dk-sl").nth(0).locator(".dk-sl-h")).toHaveText("Added Bug #q9: \"Looks right\" is too loud");
  // A named session stays as the lead named it.
  await say("ROUTE: #q9 -> web-lead | why: warm");
  await expect(last.locator(".dk-sl .dk-rt-to")).toHaveText("web-lead");
});

test("a reply of only routing lines is short lines at the left: TITLE and ROUTE, its title and why on a click (#157)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: "TITLE: #d2 | A longer title the lead gave it, long enough that it would wrap on a narrow chat\nROUTE: #d2 -> web-lead | kind: feature | after: #d3 | why: free and warm, and it knows the header code from the last change" }); renderMain(); });
  const last = page.locator(".dk-chat .cv-agent").last(), ln = last.locator(".dk-sl");
  await expect(ln).toHaveCount(2);
  await expect(ln.nth(0).locator(".dk-sl-h")).toHaveText(/^Renamed \w+ #d2$/);
  await expect(ln.nth(1).locator(".dk-sl-h")).toHaveText(/^\w+ #d2 waits on #d3$/);
  // Stacked tight, no icon: the second starts right under the first.
  const [a, b] = [await ln.nth(0).boundingBox(), await ln.nth(1).boundingBox()];
  expect(b.y - (a.y + a.height)).toBeLessThan(2);
  expect(a.height).toBeLessThan(21);
  await expect(last.locator(".dk-sl svg")).toHaveCount(0);
  await expect(last.locator(".cv-text")).not.toContainText("TITLE:");
  for (const l of await ln.all()) expect(await l.evaluate((el) => el.getBoundingClientRect().height)).toBeLessThan(24);
  await ln.nth(1).locator(".dk-sl-r").dispatchEvent("click");
  await expect(ln.nth(1).locator(".dk-sl-d")).toContainText("free and warm");
  await expect(ln.nth(1).locator(".dk-sl-d .dk-sl-it")).toHaveCount(1);
  await ln.nth(0).locator(".dk-sl-r").dispatchEvent("click");
  await expect(ln.nth(0).locator(".dk-sl-d")).toHaveText("A longer title the lead gave it, long enough that it would wrap on a narrow chat");
  const dir = process.env.SHOT_DIR || process.env.TMPDIR || "/tmp";
  await last.screenshot({ path: `${dir}/cue-desk-lead-lines-light.png` });
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(ln.nth(1).locator("button.dk-say-id").first()).not.toHaveCSS("color", "rgb(0, 0, 0)");
  await last.screenshot({ path: `${dir}/cue-desk-lead-lines-dark.png` });
  // Its title opens the item.
  await ln.nth(1).locator(".dk-sl-it").click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "d2"]);
});

test("a reply with little to read has no Listen or Read: a lone ROUTE line, a short answer; a longer one has both", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const say = (text) => page.evaluate((t) => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: t }); renderMain(); }, text);
  const last = page.locator(".dk-chat .cv-agent").last();
  await say("ROUTE: #d2 -> web-lead | why: it's free and warm, and the owner wants mocks first. Make 2-3 quick mocks on the Desk picks canvas of how Looks right could show less: a quiet tick that turns into Looks right on hover, one \"Check 3\" button, and so on. Post the link here.");
  await expect(last.locator(".dk-sl")).toHaveCount(1);
  // No box around it; its why shut until you click.
  await expect(last.locator(".own-lines")).toHaveCSS("border-top-width", "0px");
  await expect(last.locator(".dk-sl-d")).toHaveCount(0);
  await expect(last.locator("[data-listen], [data-read]")).toHaveCount(0);
  await say("Sent it to web-lead.");
  await expect(last.locator("[data-listen]")).toHaveCount(0);
  await say("Sent it to web-lead, who has the header code warm from the last change; it can start right away and should be done within the hour, then you check it.");
  await expect(last.locator("[data-listen]")).toHaveCount(1);
  await expect(last.locator("[data-read]")).toHaveCount(1);
});

test("Cue's New item note shows as a small card an item, without its words for the lead", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => {
    sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: '<desk project="cue">\n3 open\n</desk>\n\n' + [
      "New item #d2 (feature): Quick phrases | done when: they sit by Send | from web-lead",
      "New item #d9: Something new",
      "Route each now: ROUTE it to a session, or -> new. Say what one without a kind is in its ROUTE line: | kind: bug, feature or task.",
    ].join("\n") });
    renderMain();
  });
  const news = page.locator(".dk-chat .dk-news").last();
  const cards = news.locator(".dk-nc");
  await expect(cards).toHaveCount(2);
  // Its kind and number on one line, the title under it (#203).
  await expect(cards.nth(0).locator(".dk-nc1")).toHaveText(/^Feature #d2\d+s$/);
  // The Desk's title for it now leads; the words it came with sit under it, lighter.
  await expect(cards.nth(0).locator(".dk-nc-t")).toHaveText("Quick phrases at the right of Send");
  await expect(cards.nth(0).locator(".dk-nc-w")).toHaveText("Quick phrases");
  await expect(cards.nth(0).locator(".dk-nc2")).toHaveText("done when they sit by Send · from web-lead");
  await expect(cards.nth(0).locator("svg.dk-kic")).toHaveCount(1);
  // Said without a kind, but the Desk has it as a bug by now: the card shows what it is now.
  await expect(cards.nth(1).locator(".dk-nc1")).toHaveText(/^Bug #d9\d+s$/);
  // No "New item" line, and nothing under a title with no more to say.
  await expect(cards.nth(1).locator(".dk-nc2")).toHaveCount(0);
  await expect(news).not.toContainText("New item");
  await expect(page.locator(".dk-chat")).not.toContainText("Route each now");
  await cards.nth(0).click();
  await expect(page.locator('.dk-it.on[data-desk-item="d2"]')).toHaveCount(1);
});

test("Cue's word on a stopped item is a card from Cue, not a message from You; its words for the lead stay hidden (#244)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // As hub/desk_stall.rs stop_words and hub/desk_lead.rs start_failed say them.
  await page.evaluate(() => {
    const t = Date.now(), lead = sessionOf("lead");
    lead.thread.push({ at_ms: t, role: "you", text: "#d2 (Quick phrases) stopped, not committed: desk-ui ended its turn on it with no DONE, and its work isn't committed (ui/desk.js, ui/styles/desk.css), so it isn't in the user's app. Its last line: The row keeps its seat; tests next.\nSettle it: ROUTE #d2 to desk-ui again with what's left as the why (Cue commits its work when it says DONE: #d2), or route it on; only if the user must decide, ASK: #d2 | <question> | choices: … Until then it isn't shown as waiting on the user." });
    lead.thread.push({ at_ms: t, role: "you", text: "#d9 (Something new) stopped: web-lead ended its turn on it with no DONE and no question for the user. Its last line: Checking the box next.\nSettle it: ROUTE #d9 to web-lead again with what to do next as the why, or route it on; only if the user must decide, ASK: #d9 | <question> | choices: … Until then it isn't shown as waiting on the user." });
    lead.thread.push({ at_ms: t, role: "you", text: "Cue couldn't start a new session for #d9 (Something new): no folder for cue. It's still new; ROUTE it to a session, or to new to try again." });
    renderMain();
  });
  const cards = page.locator(".dk-chat .dk-sn");
  await expect(cards).toHaveCount(3);
  await expect(cards.nth(0).locator(".dk-sn-w")).toHaveText("Stopped, not committed");
  await expect(cards.nth(0).locator(".dk-sn2")).toHaveText("Feature #d2 Quick phrases at the right of Send");
  await expect(cards.nth(0).locator(".dk-sn3")).toHaveText("desk-ui · Ended its turn with no DONE, and its work isn't in your app yet.");
  await expect(cards.nth(0).locator(".dk-sn-last")).toHaveText("Its last line: The row keeps its seat; tests next.");
  // The files folded: a count, the paths only once opened.
  const fold = cards.nth(0).locator(".dk-sn-f");
  await expect(fold.locator("summary")).toHaveText("2 files not committed");
  await expect(fold.locator(".dk-sn-fl")).toBeHidden();
  await fold.locator("summary").click();
  await expect(fold.locator(".dk-sn-fl")).toHaveText("ui/desk.jsui/styles/desk.css");
  await expect(cards.nth(1).locator(".dk-sn-w")).toHaveText("Stopped");
  await expect(cards.nth(1).locator(".dk-sn3")).toHaveText("web-lead · Ended its turn with no DONE and no question for you.");
  await expect(cards.nth(1).locator(".dk-sn-f")).toHaveCount(0);
  await expect(cards.nth(2).locator(".dk-sn-w")).toHaveText("Couldn't start");
  await expect(cards.nth(2).locator(".dk-sn3")).toHaveText("Cue couldn't start a session for it: no folder for cue.");
  // Not from You, and none of Cue's words for the lead.
  const chat = page.locator(".dk-chat");
  await expect(chat).not.toContainText("Settle it");
  await expect(chat).not.toContainText("ROUTE it to a session");
  await expect(chat.locator(".dk-sn [title]")).toHaveCount(0);
  if (process.env.SHOT_DIR) await chat.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-stop-card.png` });
  await cards.nth(1).locator("[data-desk-chk-item]").click();
  await expect(page.locator('.dk-it.on[data-desk-item="d9"]')).toHaveCount(1);
});

test("a new item with a picture, no kind, and Cue's SPLIT words before its Route words: a card with a thumbnail; no path, no words for the lead (#147)", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  const pic = "/tests/ui/shots/board-waiting.png";
  await page.evaluate((pic) => {
    // Into the Desk's own items (deskOf gives a copy).
    state.desk.projects.find((x) => x.project === "cue").items.push({ id: "p1", project: "cue", kind: "", title: "The rail's dot is off by a pixel", state: "new", created: Date.now(), updated: Date.now(), holder: null, from: "you", images: [pic] });
    // As desk/mod.rs new_items_words says it: the SPLIT words on the Route line, and the kind words after it.
    sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: '<desk project="cue">\n4 open\n</desk>\n\n' + [
      `New item #p1: The rail's dot is off by a pixel | done when: it sits centred | pictures: \`${pic}\``,
      "If one of yours holds two separate problems, SPLIT it first (Cue tells you the new numbers); if its title isn't short and plain, TITLE it. Route it now: ROUTE it to a session, or -> new. Say what one without a kind is in its ROUTE line: | kind: bug, feature or task.",
    ].join("\n") });
    renderMain();
  }, pic);
  const card = page.locator(".dk-chat .dk-news .dk-nc");
  await expect(card).toHaveCount(1);
  // No kind yet: no kind icon (as on its row), just its number.
  await expect(card.locator("svg.dk-kic")).toHaveCount(0);
  await expect(card.locator(".dk-inum")).toHaveText("#p1");
  await expect(card.locator(".dk-nc-t")).toHaveText("The rail's dot is off by a pixel");
  // No "New item" line (#203): just done when, quiet, under the title.
  await expect(card.locator(".dk-nc2")).toHaveText("done when it sits centred");
  await expect(card).not.toContainText("New item");
  // At the right, where your messages are, as wide as what it says, not floating mid-chat (#167).
  const cb = await card.boundingBox(), you = await page.locator(".dk-chat .cv-you-text").first().boundingBox();
  expect(Math.abs((cb.x + cb.width) - (you.x + you.width))).toBeLessThan(2);
  expect(cb.width).toBeLessThan(360);
  if (process.env.SHOT_DIR) await page.locator(".dk-chat").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-new-card.png` });
  // Its picture as a small thumbnail under the text (#203), rounded, with a line round it; loaded, not a path.
  const img = card.locator(".thumb img");
  await expect(img).toHaveCount(1);
  await expect(img).toHaveAttribute("src", pic);
  await expect.poll(() => img.evaluate((el) => el.complete && el.naturalWidth)).toBeGreaterThan(0);
  const box = await card.locator(".thumb").boundingBox();
  expect(Math.round(box.height)).toBe(40);
  expect(box.y).toBeGreaterThan((await card.locator(".dk-nc2").boundingBox()).y);
  await expect(card.locator(".thumb")).toHaveCSS("border-top-width", "1px");
  await expect(card.locator(".thumb")).toHaveCSS("border-top-left-radius", "6px");
  // Nothing of the raw message: no path, no words for the lead, no "picture" count.
  const chat = page.locator(".dk-chat");
  for (const t of [pic, "pictures:", "SPLIT", "Route it now", "Say what one without a kind", "1 picture"]) await expect(chat).not.toContainText(t);
  await page.screenshot({ path: `${process.env.SHOT_DIR || process.env.TMPDIR || "/tmp"}/cue-desk-new-item-light.png` });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({ path: `${process.env.SHOT_DIR || process.env.TMPDIR || "/tmp"}/cue-desk-new-item-dark.png` });
  // A click on the card opens the item.
  await card.locator(".dk-nc2").click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "p1"]);
  await expect(page.locator('.dk-it.on[data-desk-item="p1"]')).toHaveCount(1);
  // By keyboard too: Tab reaches it (a ring shows), and Enter or Space opens it.
  for (const key of ["Enter", " "]) {
    await page.evaluate(() => deskShow(null));
    await page.keyboard.press("Shift");   // the keyboard in use, as after a Tab: focus shows its ring
    await card.focus();
    await expect(card).toHaveCSS("outline-style", "solid");
    await page.keyboard.press(key === " " ? "Space" : key);
    await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "p1"]);
  }
});

test("a new item the lead gave a short title: its number on one line, the short title under it, your words lighter below, no narrow column (#203)", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  const words = "The new-item card in the Dispatcher chat wraps its long text into a thin column right beside the icon, and its label takes a line of its own, which wastes room and is hard to read";
  await page.evaluate((words) => {
    state.desk.projects.find((x) => x.project === "cue").items.push({ id: "t9", project: "cue", kind: "feature", title: "A tidy new-item card", words, state: "new", created: Date.now(), updated: Date.now(), holder: null, from: "you" });
    sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: '<desk project="cue">\n4 open\n</desk>\n\n' + [
      `New item #t9 (feature): ${words} | done when: one compact card`,
      "Route it now: ROUTE it to a session, or -> new.",
    ].join("\n") });
    renderMain();
  }, words);
  const card = page.locator('.dk-chat .dk-nc[data-desk-chk-item="t9"]');
  await expect(card.locator(".dk-nc1 .dk-inum")).toHaveText("Feature #t9");
  await expect(card.locator(".dk-nc1 svg.dk-kic")).toHaveCount(1);
  await expect(card.locator(".dk-nc-t")).toHaveText("A tidy new-item card");
  await expect(card.locator(".dk-nc-w")).toHaveText(words);
  await expect(card).not.toContainText("New item");
  // The title and the words sit under the icon line, as wide as the card, not in a column beside the icon.
  const cb = await card.boundingBox(), t = await card.locator(".dk-nc-t").boundingBox(), w = await card.locator(".dk-nc-w").boundingBox(), l1 = await card.locator(".dk-nc1").boundingBox();
  expect(t.y).toBeGreaterThanOrEqual(l1.y + l1.height - 1);
  expect(w.y).toBeGreaterThanOrEqual(t.y + t.height - 1);
  expect(w.width).toBeGreaterThan(cb.width - 30);
  // Your words lighter than the title.
  const col = (loc) => loc.evaluate((el) => getComputedStyle(el).color);
  expect(await col(card.locator(".dk-nc-w"))).not.toBe(await col(card.locator(".dk-nc-t")));
  for (const c of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: c });
    if (process.env.SHOT_DIR) await page.locator(".dk-chat").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-new-card-203-${c}.png` });
  }
});

test("Desk handoff shows only the routing note in Desk and session chat", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const note = "Successor now runs Feature A's Desk. Please route Fix the header.";
  await page.evaluate((note) => {
    const s = sessionOf("lead");
    s.harness = "codex";
    const at_ms = Date.now();
    const context = "Desk lead: successor\nNeeds routing: #193\nOther session (sid:abcdef, 142k)";
    s.thread.push({ role: "you", at_ms, text: `<desk project="cue">\n${context}\n</desk>\n\nEarlier handoff` });
    s.thread.push({ role: "you", at_ms: at_ms + 1, text: `<desk id="feature-a" name="Feature A" project="cue">\n${context}\n</desk>\n\n${note}` });
    renderMain();
  }, note);
  const chat = page.locator(".dk-chat");
  await expect(chat.locator(".cv-you-text").last()).toHaveText(note);
  await expect(chat).toContainText("Earlier handoff");
  await expect(chat).not.toContainText(/<desk|#193|abcdef|142k/);
  const deskComposer = page.locator(".dk-box textarea");
  await deskComposer.press("ArrowUp");
  await expect(deskComposer).toHaveValue(note);
  await deskComposer.evaluate((el) => el.setSelectionRange(0, 0));
  await deskComposer.press("ArrowUp");
  await expect(deskComposer).toHaveValue("Earlier handoff");
  await page.evaluate(() => openSession("lead"));
  const sessionChat = page.locator("[data-chat]");
  await expect(sessionChat.locator(".cv-you-text").last()).toHaveText(note);
  await expect(sessionChat).not.toContainText(/<desk|#193|abcdef|142k/);
  const sessionComposer = page.locator('textarea[data-text="s:lead"]');
  await sessionComposer.press("ArrowUp");
  await expect(sessionComposer).toHaveValue(note);
  await sessionComposer.evaluate((el) => el.setSelectionRange(0, 0));
  await sessionComposer.press("ArrowUp");
  await expect(sessionComposer).toHaveValue("Earlier handoff");
});

test("Cue's note on a lead's DONE it couldn't act on shows as a centred line an item, without its words for the lead", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => {
    sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: '<desk project="cue">\n3 open\n</desk>\n\n' + [
      "#d2 not closed: it waits on your check",
      "#zz9 not closed: this Desk has no item #zz9",
      "Only the person closes an item that waits on their check (Looks right). Don't say DONE for these again.",
    ].join("\n") });
    renderMain();
  });
  const notes = page.locator(".dk-chat .dk-news").last();
  await expect(notes.locator(".dk-new")).toHaveCount(2);
  await expect(notes.locator(".dk-new").nth(0)).toHaveText(/^Cue · not closed Feature #d2 Quick phrases at the right of Send · it waits on your check \d+s$/);
  await expect(notes.locator(".dk-new").nth(1)).toHaveText(/^Cue · not closed #zz9 · this Desk has no item #zz9 \d+s$/);
  await expect(page.locator(".dk-chat")).not.toContainText("Don't say DONE");
  await notes.locator('[data-desk-chk-item="d2"]').click();
  await expect(page.locator('.dk-it.on[data-desk-item="d2"]')).toHaveCount(1);
});

test("an item's pane: its words select and copy, its buttons don't", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  const pane = page.locator(".dk-right .dk-pg");
  const sel = (loc) => loc.evaluate((el) => getComputedStyle(el).webkitUserSelect || getComputedStyle(el).userSelect);
  expect(await sel(pane.locator("h1"))).toBe("text");
  expect(await sel(pane.locator(".dk-state"))).toBe("text");
  expect(await sel(pane.locator("button").first())).toBe("none");
  const h1 = pane.locator("h1"), box = await h1.boundingBox();
  // Along its first line: with its number in front, the title can wrap (#134).
  await page.mouse.move(box.x + 2, box.y + 14);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width - 2, box.y + 14, { steps: 5 });
  await page.mouse.up();
  expect((await page.evaluate(() => String(getSelection()))).length).toBeGreaterThan(3);
});

test("an interrupted assignment needs resume while a dependency stays queued", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d5"]').click();
  const pane = page.locator(".dk-right .dk-pg");
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveCount(0);
  await cue.change((st) => {
    st.sessions.find((s) => s.session_id === "host").state = "stopped";
    const items = st.desk.projects.find((d) => d.project === "cue").items;
    items.find((i) => i.id === "d5").state = "needs_resume";
    items.find((i) => i.id === "d4").after = "d5";
  });
  await expect(pane).toContainText("Needs resume");
  const rows = page.locator(".dk-items");
  await expect(rows.locator(".dk-gh", { hasText: "Needs resume" }).locator("+ [data-desk-item='d5']")).toBeVisible();
  await expect(rows.locator("[data-desk-item='d4']")).toContainText("after #d5");
  await pane.locator(".dk-ph [data-desk-back]").click();
  await expect(page.locator('[data-desk-page="dispatcher"] .dk-psay')).toContainText("1 item needs to resume.");
  await expect(page.locator('[data-desk-page="dispatcher"] .dk-pfc')).toContainText("1 to resume");
  await rows.locator('[data-desk-item="d5"]').click();
  await pane.locator('[data-desk-act="start_now"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "start_now" });
  // A crew member's item: Restart only when that teammate isn't at work.
  await page.locator('[data-desk-item="d3"]').click();
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveCount(0);
  await cue.change((st) => {
    st.sessions.find((s) => s.session_id === "lead").team.find((m) => m.agent_id === "id-search").state = "done";
    st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d3").state = "needs_resume";
  });
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveText("Restart");
});

test("an item back to check stands out until you open it; its pane links to what was made", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => {
    const d2 = state.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2");
    d2.made = ["https://claude.ai/artifact/abc123", "/tmp/mocks/list.html"];
    d2.files = ["ui/send.js"];
    renderMain();
  });
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row).toHaveClass(/landed/);
  await expect(row.locator(".dk-tag")).toHaveClass(/landed/);
  await row.locator(".dk-t").click();
  await expect(page.locator(".dk-pane .dk-sec", { hasText: "What it made" }).locator(".dk-made")).toHaveText(["Open the page ↗", "list.html ↗"]);
  await expect(row).not.toHaveClass(/landed/);
  await expect(row.locator(".dk-tag")).toHaveClass(/check/);
});

test("a to-check item's page lists its Files changed, each with its counts; a click opens that file over the Desk, Esc closes it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").files = ["ui/chat/compose.js", "ui/styles/chat.css"]; });
  await page.locator('.dk-items [data-desk-item="d2"] .dk-t').click();
  // The files Looks right will commit, as the hub reads them.
  await cue.called("desk_item_diff", { id: "d2" });
  // Under its Files tab (#149), counted there.
  const ftab = page.locator('.dk-pane [data-desk-itab="files"]');
  await expect(ftab).toHaveText("Files2");
  await ftab.click();
  await expect(page.locator(".dk-pane .dk-thread")).toHaveCount(0);
  const sec = page.locator(".dk-pane .dk-sec", { hasText: "Files changed" });
  await expect(sec.locator(".dk-fc-p")).toHaveText(["ui/chat/compose.js", "ui/styles/chat.css"]);
  await expect(sec.locator(".dk-fc").first().locator(".dv-n")).toHaveText(["+2", "−1"]);
  // Read once for the page, not again on each redraw.
  await page.evaluate(() => { renderMain(); renderMain(); });
  await page.waitForTimeout(200);
  expect((await cue.calls("desk_item_diff")).length).toBe(1);
  await sec.locator(".dk-fc").nth(1).click();
  // Over the Desk: the item's files on the left, the one clicked picked, its diff on the right. The Desk stays put.
  const ov = page.locator(".dov");
  await expect(ov.locator(".dov-title")).toContainText("Feature #d2");
  await expect(ov.locator(".dov-f .dov-p")).toHaveText(["ui/chat/compose.js", "ui/styles/chat.css"]);
  await expect(ov.locator(".dov-f.on .dov-p")).toHaveText("ui/styles/chat.css");
  await expect(ov.locator(".dov-fh .dov-p")).toHaveText("ui/styles/chat.css");
  await expect(ov.locator(".dov-lines .a").first()).toHaveText("+new ui/styles/chat.css");
  await expect(page.locator(".active-pane .dv")).toHaveCount(0);
  // ↑ steps to the file above; a click on a file picks it.
  await page.keyboard.press("ArrowUp");
  await expect(ov.locator(".dov-fh .dov-p")).toHaveText("ui/chat/compose.js");
  await ov.locator(".dov-f").nth(1).click();
  await expect(ov.locator(".dov-fh .dov-p")).toHaveText("ui/styles/chat.css");
  // Esc closes only it: the item's page is still open under it.
  await page.keyboard.press("Escape");
  await expect(page.locator(".dov")).toHaveCount(0);
  await expect(sec.locator(".dk-fc")).toHaveCount(2);
  // A click outside it closes it too.
  await sec.locator(".dk-fc").first().click();
  await expect(page.locator(".dov")).toHaveCount(1);
  await page.mouse.click(10, 10);
  await expect(page.locator(".dov")).toHaveCount(0);
  await expect(page.locator(".dov-scrim [title]")).toHaveCount(0);
});

test("written about an item ready to check, it's sent back with your words: off Waiting on you, to its session", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  await page.locator(".dk-box textarea").fill("The More menu is cut off");
  await page.locator(".dk-box textarea").press("Enter");
  // As Send back: to its holder, not the lead.
  await cue.called("desk_item_act", { id: "d2", act: "send_back", text: "The More menu is cut off" });
  expect(await cue.calls("desk_send")).toEqual([]);
  await expect(page.locator(".toast").last()).toContainText("Sent back");
  await expect.poll(() => listOrder(page)).toEqual(["# Waiting on you", "d1", "# In progress", "d3", "d2", "d5", "# Queued", "d4", "d6", "# Done", "d7", "d8", "d9"]);
  // The box emptied, the item's page still open.
  await expect(page.locator(".dk-box textarea")).toHaveValue("");
  await expect(page.locator(".dk-pane")).toHaveCount(1);
  // One that doesn't go: still waiting on you, your words kept in its box.
  await page.locator('[data-desk-item="d1"]').click();
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1").state = "to_check"; });
  await cue.override("desk_item_act", () => { throw "its session has ended"; });
  await page.locator(".dk-box textarea").fill("Still loses focus");
  await page.locator(".dk-box textarea").press("Enter");
  await expect(page.locator(".toast").last()).toContainText("its session has ended");
  await expect(page.locator(".dk-box textarea")).toHaveValue("Still loses focus");
  await expect(page.locator(".dk-right .dk-pg h1")).toHaveText("Bug #d1 · Reply box loses focus after Send");
});

test("a session on no item says the first line of its latest reply, when it came, its size and cache, though no card brings them (#185)", async ({ page, cue }) => {
  await cue.open("desk", { flags: "cost=cold" });
  await openDesk(page);
  await cue.change((st) => st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", thread: [] }));
  await cue.change((st) => st.desk.projects.find((d) => d.project === "cue").members.push("spare"));
  const row = page.locator('.dk-sessions [data-desk-chat-of="spare"]');
  await expect(row.locator(".dk-s2 .dk-w")).toHaveText("Done: spare's rows read as rows.");
  await expect(row.locator(".dk-size")).toHaveText("4h ago · 600k · cold");
  await expect(row.locator("[data-desk-member]")).toHaveCount(0);
});

test("the sessions on the left: working first, then each group most recent first (#184)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const site = st.sessions.find((x) => x.session_id === "site");
    for (const [sid, h] of [["older", 5], ["newest", 1], ["middle", 3]]) st.sessions.push({ ...site, session_id: sid, project: "cue", name: sid, state: "idle", since_ms: Date.now() - h * 3600000, thread: [] });
  });
  await cue.change((st) => st.desk.projects.find((d) => d.project === "cue").members.push("older", "newest", "middle"));
  const ids = () => page.locator(".dk-sessions .dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-sdiv") ? "—" : el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}` : el.dataset.deskChatOf || el.dataset.deskCrew));
  await expect.poll(ids).toEqual(["# Working", "host", "# On the Desk", "newest", "middle", "older"]);
  // One comes back from its reply: it moves to the top of its group.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "older").since_ms = Date.now(); });
  await expect.poll(ids).toEqual(["# Working", "host", "# On the Desk", "older", "newest", "middle"]);
});

/** The Desk run by host instead, so "lead" (with its crew) is a session in the list like any other. */
const crewNotLead = (cue) => cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead = { sid: "host", name: "host-lead", state: "live" }; });

test("three columns: the project's sessions on the left, the lead's chat in the middle, the items on the right", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const cols = await page.locator(".dk-board > .dk-card").evaluateAll((els) => els.map((el) => [...el.classList].find((c) => ["dk-sessions", "dk-right", "dk-items"].includes(c))));
  expect(cols).toEqual(["dk-sessions", "dk-right", "dk-items"]);
  const xs = await page.locator(".dk-board > .dk-card").evaluateAll((els) => els.map((el) => el.getBoundingClientRect().x));
  expect(xs[0] < xs[1] && xs[1] < xs[2]).toBe(true);
  // A Desk member with no item.
  await cue.change((st) => st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", thread: [] }));
  // The Desk's lead as a "Run by" card in the head strip, with no crew, not even old ones; the list under it: Working
  // first (host-lead), then On the Desk.
  await cue.change((st) => { const l = st.sessions.find((x) => x.session_id === "lead"); l.team.push({ ...l.team[0], agent_id: "id-old", call: "id-old", name: "old", state: "committed" }); });
  await cue.change((st) => st.desk.projects.find((d) => d.project === "cue").members.push("spare"));
  const left = page.locator(".dk-sessions");
  await expect(left.locator('.dk-strip .dk-lcard[data-desk-lead-chat="lead"]')).toHaveCount(1);
  await expect(left.locator('.dk-lcard .dk-pill')).toHaveCount(0);
  await expect(left.locator(".dk-lcard .dk-lk")).toHaveText("Run by");
  // What each row is as the chat headers' icon before its name, the Dispatcher's signpost on the lead's card (#202).
  await expect(left.locator('.dk-lcard .dk-s1 > .dk-ric[data-desk-type="desk"] svg.dk-kic')).toHaveCount(1);
  await expect(left.locator('[data-desk-chat-of="host"] .dk-ric')).toHaveAttribute("aria-label", "Session");
  // Rows run edge to edge, so their hover and lime reach both sides of the column.
  const [rx, rw, cx, cw] = await left.locator('[data-desk-chat-of="host"]').evaluate((el) => { const r = el.getBoundingClientRect(), c = el.closest(".dk-scroll").getBoundingClientRect(); return [r.x, r.width, c.x, c.width]; });
  expect([rx, Math.round(rw)]).toEqual([cx, Math.round(cw)]);
  // The right list: each item's kind as a small icon in the theme color before its number.
  const items = page.locator(".dk-items");
  await expect(items.locator('[data-desk-item="d1"] .dk-i2 .dk-kid > svg.dk-kic')).toHaveCount(1);
  expect(await items.locator(".dk-it").evaluateAll((els) => els.every((el) => el.querySelector(".dk-kic")))).toBe(true);
  const ic = await items.locator('[data-desk-item="d1"] .dk-kic').evaluate((el) => [getComputedStyle(el).color, getComputedStyle(document.documentElement).getPropertyValue("--accent")]);
  expect(ic[0]).not.toBe("");
  await expect(left.locator(".dk-srow.crew")).toHaveCount(0);
  await expect(left.locator("[data-desk-done-fold]")).toHaveCount(0);
  await expect(left.locator(".dk-strip .dk-sub")).toHaveText("2 sessions · 1 working");
  const rows = await left.locator(".dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-sdiv") ? "—" : el.matches(".dk-gh") ? `# ${el.textContent.trim()}`
    : `${el.matches(".crew") ? "  " : ""}${el.dataset.deskLeadChat || el.dataset.deskChatOf || el.dataset.deskCrew}${el.querySelector(".dk-pill") ? ` [${el.querySelector(".dk-pill").textContent}]` : ""}`));
  expect(rows).toEqual(["# Working 1", "host", "# On the Desk 1", "spare"]);
  await expect(left.locator('[data-desk-chat-of="host"] .dk-s2')).toContainText("#d5");
  // On no item: no item line (#179).
  await expect(left.locator('[data-desk-chat-of="spare"] .dk-w')).toHaveCount(0);
  await expect(left.locator('[data-desk-chat-of="spare"] [data-desk-member]')).toHaveCount(0);
  await expect(left.locator('[data-desk-chat-of="host"] [data-desk-member]')).toHaveCount(0);
  // The lead's chat shows: its card is lit, alone.
  await expect(left.locator(".dk-srow.on, .dk-lcard.on")).toHaveCount(1);
  await expect(left.locator('[data-desk-lead-chat="lead"]')).toHaveClass(/\bon\b/);
  // Another session: its chat in the middle, its row lit in place; Esc goes back to the lead's.
  await left.locator('[data-desk-chat-of="host"]').click();
  await expect(page.locator('.dk-right [data-desk-chat="host"]')).toHaveCount(1);
  // A session's page (#200): back, Open its session; its kind's icon, "Session ·" and its name; under it what it's on
  // (its number opening the item); its state in the panel's NOW.
  const ph = page.locator('.dk-right .dk-pg[data-desk-page="session"] .dk-ph');
  await expect(ph.locator("h1 [data-desk-mark]")).toHaveText("Session");
  await expect(ph.locator("h1")).toHaveText("Session · host-lead");
  await expect(ph.locator(".dk-ph1")).toHaveText(/^‹ .+Open its session\s*Hide panel$/);
  await expect(ph.locator(".dk-ph3")).toHaveText(/^on #d5 · Hosted sessions can't open the canvas/);
  await expect(page.locator(".dk-right .dk-pnow .dk-pword")).toHaveText(/^(Idle|Working|Waiting on you)$/);
  // One with nothing on the Desk: back, its name, Open its session; it says it's on nothing.
  await left.locator('[data-desk-chat-of="spare"]').click();
  await expect(ph.locator("h1")).toHaveText("Session · spare");
  await expect(ph.locator(".dk-ph3")).toHaveText(/^nothing on the Desk/);
  if (process.env.SHOT_DIR) for (const c of ["light", "dark"]) { await page.emulateMedia({ colorScheme: c }); await page.locator(".dk-right .dk-ph").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-sess-head-${c}.png` }); }
  await page.emulateMedia({ colorScheme: "light" });
  await left.locator('[data-desk-chat-of="host"]').click();
  await expect(left.locator(".dk-srow.on")).toHaveAttribute("data-desk-chat-of", "host");
  await expect(page.locator(".dk-right .dk-box textarea")).toHaveAttribute("placeholder", /^Talk to /);
  await page.keyboard.press("Escape");
  await expect(left.locator('[data-desk-lead-chat="lead"]')).toHaveClass(/\bon\b/);
  // Another session's crew sit under it; a crew row opens its pane, lit.
  await crewNotLead(cue);
  await expect(left.locator('[data-desk-crew="id-focus"] .dk-st')).toHaveText("waiting on you");
  await left.locator('[data-desk-crew="id-search"]').click();
  await expect(page.locator(".dk-right .dk-pg h1 .dk-tname")).toHaveText("search-centre");
  await expect(left.locator(".dk-srow.on")).toHaveAttribute("data-desk-crew", "id-search");
  await expect(left.locator(".dk-sfoot").getByRole("button")).toHaveAccessibleName("Add session");
});

test("each session row says its size and cache: \"142k · cache 12m\", \"cold\", amber in its last 5 minutes", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // No numbers yet: no line.
  await expect(page.locator(".dk-sessions .dk-size")).toHaveCount(0);
  await cue.change((st) => {
    const t = Date.now();
    st.insight = { lead: { tokens: 142000, until_ms: t + 12 * 60000 - 2000 }, host: { tokens: 88000, until_ms: t + 3 * 60000 } };
    st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", thread: [] });
    st.insight.spare = { tokens: 51000, until_ms: t - 60000 };
    st.desk.projects.find((d) => d.project === "cue").members.push("spare");
  });
  const left = page.locator(".dk-sessions");
  await expect(left.locator('[data-desk-lead-chat="lead"] .dk-size')).toHaveText("142k · cache 12m");
  await expect(left.locator('[data-desk-lead-chat="lead"] .dk-size')).not.toHaveClass(/hot/);
  await expect(left.locator('[data-desk-chat-of="host"] .dk-size')).toHaveText("88k · cache 3m");
  await expect(left.locator('[data-desk-chat-of="host"] .dk-size')).toHaveClass(/hot/);
  await expect(left.locator('[data-desk-chat-of="spare"] .dk-size')).toHaveText("51k · cold");
  // Each with its state and what it's on.
  await expect(left.locator('[data-desk-chat-of="host"] .dk-st')).toHaveText("working");
  await expect(left.locator('[data-desk-chat-of="host"] .dk-s2')).toContainText("#d5");
  await expect(left.locator('[data-desk-chat-of="spare"] .dk-st')).toHaveText("idle");
  await cue.change((st) => { st.insight.lead = { until_ms: Date.now() + 4 * 60000 - 2000 }; });
  await expect(left.locator('[data-desk-lead-chat="lead"] .dk-size')).toHaveText("cache 4m");
});

/** The animation the app's own styles give `el` (the test page turns every animation off, so not what it computes). */
const blinkOf = (loc) => loc.evaluate((el) => [...document.styleSheets].flatMap((sh) => { try { return [...sh.cssRules]; } catch { return []; } })
  .filter((r) => r.selectorText && !r.selectorText.startsWith("*") && el.matches(r.selectorText) && r.style?.animationName).map((r) => r.style.animationName).pop() || "none");
test("a working session's dot blinks and its current step shows under its title, with its time, live", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
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

// ---------- the left column's head and rows (#153: option B) ----------

const cssOf = (loc, ...props) => loc.evaluate((el, ps) => { const c = getComputedStyle(el); return ps.map((p) => c.getPropertyValue(p)); }, props);
const boxOf = (loc) => loc.evaluate((el) => { const r = el.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, right: r.right }; });

test("the head is one strip in the accent, flush across the card's top: the name (a switcher), its meta, then the lead's card in it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const left = page.locator(".dk-sessions"), strip = left.locator(".dk-strip");
  await expect(strip).toHaveCount(1);
  await expect(strip.locator("[data-desk-switch]")).toHaveCount(1);
  await expect(strip.locator(".dk-sh b")).toHaveText("cue");
  await expect(strip.locator(".dk-sub")).toHaveText("1 session · 1 working");
  await expect(strip.locator("[data-desk-change]")).toHaveCount(1);
  // The strip comes first in the card, flush to its top and sides, round only at the top as the card is.
  expect(await left.evaluate((el) => el.firstElementChild.classList.contains("dk-strip"))).toBe(true);
  const card = await boxOf(left), s = await boxOf(strip);
  expect(Math.round(s.x - card.x)).toBe(0);
  expect(Math.round(card.right - s.right)).toBe(0);
  expect(Math.round(s.y - card.y)).toBe(0);
  // Light: the accent (the window's dark green) with on-accent text, no edge (#153, option B).
  const [bg, ink, edge, radius, bottom] = await cssOf(strip, "background-color", "color", "box-shadow", "border-top-left-radius", "border-bottom-left-radius");
  expect(bg).toBe("rgb(63, 74, 34)");
  expect(ink).toBe("rgb(255, 255, 255)");
  expect(edge).toBe("none");
  expect(radius).toBe("14px");
  expect(bottom).toBe("0px");
  // Pick O: the column lifts off the window with a light hairline and a shadow; a 3px lime line closes the strip.
  expect((await cssOf(left, "box-shadow"))[0]).toMatch(/0px 0px 0px 1px.*0px 10px 28px/);
  expect(await cssOf(strip, "border-bottom-width", "border-bottom-color")).toEqual(["3px", "rgb(210, 222, 150)"]);
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

test("Run by shows the current Dispatcher's accessible agent icon", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const runBy = page.locator(".dk-lcard");
  await expect(runBy.getByRole("img", { name: "Claude Code" })).toBeVisible();
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    const s = st.sessions.find((x) => x.session_id === "host");
    s.harness = "codex";
    d.lead = { sid: s.session_id, name: s.name, state: "live" };
  });
  await expect(runBy.getByRole("img", { name: "Codex" })).toHaveText("X");
  await expect(runBy.getByRole("img", { name: "Claude Code" })).toHaveCount(0);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").harness = "pi"; });
  await expect(runBy.getByRole("img", { name: "Pi" })).toHaveText("π");
  await expect(runBy.getByRole("img", { name: "Dispatcher" })).toBeVisible();
});

test("Run by gives idle and its icons contrasting colors in both themes", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const lead = st.sessions.find((s) => s.session_id === "lead");
    Object.assign(lead, { state: "idle", harness: "codex" });
    st.items = st.items.filter((item) => item.session_id !== lead.session_id);
  });
  const card = page.locator(".dk-lcard");
  const status = card.locator(".dk-st");
  const dot = status.locator("i.idle");
  const badge = card.getByRole("img", { name: "Codex" });
  await expect(status).toHaveText("idle");
  for (const colorScheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme });
    await card.click();
    await expect(card).toHaveClass(/\bon\b/);
    const [surface, ink] = await cssOf(card, "background-color", "color");
    expect(await cssOf(status, "color", "opacity")).toEqual([ink, "1"]);
    expect((await cssOf(dot, "box-shadow"))[0]).toContain(ink);
    expect(await cssOf(card.locator(".dk-ric svg"), "color")).toEqual([ink]);
    expect(await cssOf(card.locator(".dk-sren"), "color")).toEqual([ink]);
    expect(await cssOf(badge, "background-color", "color")).toEqual([ink, surface]);
    await page.locator('.dk-sessions [data-desk-chat-of="host"]').click();
    await expect(card).not.toHaveClass(/\bon\b/);
    const [statusInk] = await cssOf(status, "color");
    expect((await cssOf(dot, "box-shadow"))[0]).toContain(statusInk);
  }
});

test("the lead's card in the strip: Run by, its state and the item it holds; open, lit lime as any open row, in both themes", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const lead = page.locator('.dk-strip .dk-lcard[data-desk-lead-chat="lead"]');
  await expect(lead.locator(".dk-larrow")).toHaveText("›");   // open, so it has the › (#219)
  await expect(lead.locator(".dk-lk")).toHaveText("Run by");
  await expect(lead.locator(".dk-nm")).toHaveText("web-lead");
  await expect(lead.locator(".dk-st")).toHaveText("waiting on you");
  await expect(lead.locator(".dk-st > i.ask")).toHaveCount(1);
  await expect(lead.locator(".dk-s2 .dk-id")).toHaveText("Feature #d2");
  await expect(lead.locator(".dk-s2 svg.dk-kic")).toHaveCount(1);
  // Its chat shows: the card lights lime, as an open row does.
  await expect(lead).toHaveClass(/\bon\b/);
  let [bg, ring] = await cssOf(lead, "background-color", "box-shadow");
  expect(bg).toBe("rgb(210, 222, 150)");
  // Another session's chat: the card goes back to white, that row lights in place.
  await page.locator('.dk-sessions [data-desk-chat-of="host"]').click();
  await expect(lead).not.toHaveClass(/\bon\b/);
  [bg, ring] = await cssOf(lead, "background-color", "box-shadow");
  expect(bg).toBe("rgb(255, 255, 255)");
  expect(ring).toBe("none");
  await lead.click();
  await expect(lead).toHaveClass(/\bon\b/);
  await expect(page.locator('.dk-right [data-desk-page="dispatcher"] [data-desk-chat="cue"]')).toBeVisible();
  await expect(page.locator(".dk-tab.on")).toHaveText("web-lead");
  // Dark: the strip is a dark green with the ink, not lime, so the card lights exactly as an open row does.
  await page.emulateMedia({ colorScheme: "dark" });
  const strip = page.locator(".dk-strip");
  const [sbg, sink] = await cssOf(strip, "background-color", "color");
  expect(sbg).not.toBe("rgb(181, 194, 122)");
  expect(sink).not.toBe("rgb(21, 26, 8)");
  const [onBg, onRing] = await cssOf(lead, "background-color", "box-shadow");
  expect(onRing).toBe("none");
  const host = page.locator('.dk-sessions [data-desk-chat-of="host"]');
  await host.click();
  await expect(lead).not.toHaveClass(/\bon\b/);
  const [offBg] = await cssOf(lead, "background-color");
  expect(offBg).not.toBe(onBg);
  expect(offBg).not.toBe(sbg);
  const [rowBg] = await cssOf(host, "background-color");
  expect(rowBg).toBe(onBg);
  await lead.click();
  await expect(lead).toHaveClass(/\bon\b/);
  // At work, its blinking dot takes the card's ink, so it shows on the lime.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").state = "working"; st.items = st.items.filter((i) => i.session_id !== "lead"); });
  const dot = lead.locator(".dk-st > i");
  await expect(dot).not.toHaveClass(/ask|idle/);
  const [dbg, dink] = await cssOf(dot, "background-color", "color");
  expect(dbg).toBe(dink);
  expect(await blinkOf(dot)).toBe("pulse");
});

test("no line under the strip: the list starts right after it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-sessions .dk-sum")).toHaveCount(0);
  expect(await page.locator(".dk-sessions .dk-strip").evaluate((el) => el.nextElementSibling.classList.contains("dk-scroll"))).toBe(true);
});

test("a row's state dot sits after its name with its word: working blinks, waiting on you a still red dot, idle a hollow ring", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const site = st.sessions.find((x) => x.session_id === "site");
    st.sessions.push({ ...site, session_id: "spare", project: "cue", name: "spare", state: "idle", thread: [] },
      { ...site, session_id: "asker", project: "cue", name: "asker", state: "deciding", thread: [] });
    st.desk.projects.find((d) => d.project === "cue").members.push("spare", "asker");
  });
  // The test page stills every animation; take that off to see the dot's own.
  await page.evaluate(() => [...document.querySelectorAll("style")].filter((s) => s.textContent.includes("animation:none!important")).forEach((s) => s.remove()));
  const left = page.locator(".dk-sessions");
  const host = left.locator('[data-desk-chat-of="host"]');
  // Its type's icon, its name, then the dot and its word (#202); no ring.
  expect(await host.locator(".dk-s1").evaluate((el) => [...el.children].map((c) => c.className.split(" ")[0]))).toEqual(["dk-ric", "dk-nm", "dk-ren", "dk-st"]);
  await expect(left.locator(".dk-ring, .dk-mk")).toHaveCount(0);
  await page.emulateMedia({ reducedMotion: "no-preference" });
  expect(await cssOf(host.locator(".dk-st > i"), "animation-name")).toEqual(["pulse"]);
  const ask = left.locator('[data-desk-chat-of="asker"] .dk-st > i.ask');
  const [abg, aanim] = await cssOf(ask, "background-color", "animation-name");
  expect(abg).toBe("rgb(155, 44, 31)");
  expect(aanim).toBe("none");
  await expect(left.locator('[data-desk-chat-of="asker"] .dk-st')).toHaveText("waiting on you");
  const idle = left.locator('[data-desk-chat-of="spare"] .dk-st > i.idle');
  const [ibg, ishadow, ianim] = await cssOf(idle, "background-color", "box-shadow", "animation-name");
  expect(ibg).toBe("rgba(0, 0, 0, 0)");
  expect(ishadow).toContain("inset");
  expect(ianim).toBe("none");
  // Rows stay put: the blink moves nothing.
  const before = await boxOf(host);
  await page.waitForTimeout(400);
  expect(await boxOf(host)).toEqual(before);
});

test("under each session's name: the item it holds with its kind's icon, lined up with the name", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const host = page.locator('.dk-sessions [data-desk-chat-of="host"]');
  await expect(host.locator(".dk-s2 svg.dk-kic")).toHaveCount(1);
  await expect(host.locator(".dk-s2 .dk-id")).toHaveText("Bug #d5");
  await expect(host.locator(".dk-s2")).toContainText("Hosted sessions can't open the canvas");
  // It starts under the name, not under the type's icon.
  const name = await boxOf(host.locator(".dk-nm")), what = await boxOf(host.locator(".dk-s2"));
  expect(Math.abs(what.x - name.x)).toBeLessThanOrEqual(1);
});

test("done teammates fold under their lead as \"N done · show\", opened in place; done but not committed stays out, tagged", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const lead = st.sessions.find((x) => x.session_id === "lead"), t = Date.now(), base = lead.team[0];
    const mate = (agent_id, name, state) => ({ ...base, agent_id, call: agent_id, name, state, what: `${name}'s work`, step: "", ended_ms: t - 60000 });
    lead.team.push({ ...mate("id-tidy", "tidy-copy", "done"), uncommitted: true }, mate("id-a", "old-a", "committed"), mate("id-b", "old-b", "committed"), mate("id-c", "old-c", "aside"));
  });
  await crewNotLead(cue);
  const left = page.locator(".dk-sessions");
  const crewNames = () => left.locator(".dk-srow.crew").evaluateAll((els) => els.map((el) => el.dataset.deskCrew));
  // Folded: the three finished ones are one line; the one not committed stays, tagged.
  const fold = left.locator('[data-desk-done-fold="lead"]');
  await expect(fold).toHaveText("3 done · show");
  expect(await crewNames()).toEqual(["id-search", "id-focus", "id-tidy"]);
  await expect(left.locator('[data-desk-crew="id-tidy"] .dk-pill.loose')).toHaveText("not committed");
  await expect(left.locator('[data-desk-crew="id-search"] .dk-pill.loose')).toHaveCount(0);
  // Opened in place, under the line; nothing above it moves.
  await fold.scrollIntoViewIfNeeded();
  const above = await left.locator('[data-desk-crew="id-tidy"]').boundingBox();
  await fold.click();
  await expect(fold).toHaveText("3 done · hide");
  await expect(fold).toHaveAttribute("aria-expanded", "true");
  expect(await crewNames()).toEqual(["id-search", "id-focus", "id-tidy", "id-a", "id-b", "id-c"]);
  await expect(left.locator('[data-desk-crew="id-c"] .dk-st')).toHaveText("set aside");
  await expect(left.locator(".dk-srow.past")).toHaveCount(3);
  expect((await left.locator('[data-desk-crew="id-tidy"]').boundingBox()).y).toBe(above.y);
  expect((await left.locator('[data-desk-crew="id-a"]').boundingBox()).y).toBeGreaterThan(above.y);
  await fold.click();
  await expect(left.locator(".dk-srow.past")).toHaveCount(0);
  // Git says its files are committed (Cue marks it): nothing of it left in play, it folds too.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").team.find((t) => t.agent_id === "id-tidy").uncommitted = false; });
  await expect(fold).toHaveText("4 done · show");
  expect(await crewNames()).toEqual(["id-search", "id-focus"]);
  // Committed: still in the fold.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").team.find((t) => t.agent_id === "id-tidy").state = "committed"; });
  await expect(fold).toHaveText("4 done · show");
  // Opened, it stays open across a restart.
  await fold.click();
  await expect(fold).toHaveText("4 done · hide");
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { const lead = st.sessions.find((x) => x.session_id === "lead"); lead.team.push({ ...lead.team[0], agent_id: "id-a", call: "id-a", name: "old-a", state: "committed" }); });
  await crewNotLead(cue);
  await expect(left.locator('[data-desk-done-fold="lead"]')).toHaveAttribute("aria-expanded", "true");
  await expect(left.locator('[data-desk-crew="id-a"]')).toBeVisible();
});

test("selecting a session lights nothing on the right: it can hold several items (#170)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const items = page.locator(".dk-items");
  const before = await listOrder(page);
  const ys = () => items.locator("[data-desk-item]").evaluateAll((els) => els.map((el) => Math.round(el.getBoundingClientRect().y)));
  const top = await ys();
  await expect(items.locator(".dk-it.on")).toHaveCount(0);
  await page.locator('.dk-sessions [data-desk-chat-of="host"]').click();
  await expect(page.locator('.dk-right [data-desk-chat="host"]')).toHaveCount(1);
  await expect(items.locator(".dk-it.on")).toHaveCount(0);
  expect(await listOrder(page)).toEqual(before);
  expect(await ys()).toEqual(top);
  await page.locator('.dk-sessions [data-desk-lead-chat="lead"]').click();
  await expect(items.locator(".dk-it.on")).toHaveCount(0);
});

test("the dispatcher composer stays below the conversation and hides on Decisions", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const dispatcher = page.locator('[data-desk-page="dispatcher"]');
  const chat = dispatcher.locator(".dk-chat");
  const box = dispatcher.locator(".dk-box");
  await expect(chat.locator("xpath=following-sibling::*[1]")).toHaveClass(/dk-box/);
  await box.locator("textarea").fill("Keep this draft");
  await dispatcher.locator('[data-desk-tab="decisions"]').click();
  await expect(box).toHaveCount(0);
  await expect(chat).toHaveCount(0);
  await dispatcher.locator('[data-desk-tab="lead"]').click();
  await expect(chat).toBeVisible();
  await expect(box.locator("textarea")).toHaveValue("Keep this draft");
  const conversation = await chat.boundingBox(), composer = await box.boundingBox();
  expect(conversation.y + conversation.height).toBeLessThanOrEqual(composer.y);
});

test("the lead's box takes pictures and has the mic, as a session's chat box does", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const box = page.locator(".dk-box");
  const key = await box.locator("textarea").getAttribute("data-text");
  await expect(box).toHaveAttribute("data-drop", key);
  await expect(box.locator("[data-mic]")).toHaveAttribute("data-mic", key);
  await expect(box.locator("[data-attach]")).toHaveAttribute("data-attach", key);
  // A picture pasted (as the paste handler adds it): shown on the box, and sent with the words.
  const png = "data:image/png;base64,iVBORw0KGgo=";
  await page.evaluate(([k, data]) => { draft(k).images.push({ name: "pasted image", mime: "image/png", data }); renderMain(); }, [key, png]);
  await expect(box.locator(".chips .chip")).toHaveCount(1);
  await box.locator("textarea").fill("This is cut off");
  await box.locator("textarea").press("Enter");
  await cue.called("desk_send", { project: "cue", text: "This is cut off", images: [{ name: "pasted image", mime: "image/png", data: png }] });
  await expect(page.locator(".dk-box .chips .chip")).toHaveCount(0);
});

test("working sessions show first in a Working group; a session moves when its state changes, never on a click", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // spare on this Desk: only its own sessions go up to Working (#189).
  await cue.change((st) => { st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", state: "idle", thread: [] }); st.desk.projects.find((d) => d.project === "cue").members.push("spare"); });
  const left = page.locator(".dk-sessions");
  const order = () => left.locator(".dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-sdiv") ? "—" : el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}`
    : `${el.matches(".crew") ? "  " : ""}${el.dataset.deskLeadChat || el.dataset.deskChatOf || el.dataset.deskCrew}`));
  const before = ["# Working", "host", "# On the Desk", "spare"];
  await expect.poll(order).toEqual(before);
  // A click lights a row where it is.
  await left.locator('[data-desk-chat-of="spare"]').click();
  await expect(left.locator('[data-desk-chat-of="spare"]')).toHaveClass(/\bon\b/);
  expect(await order()).toEqual(before);
  // spare starts work: it moves up to Working, after host; host stops: it leaves Working for On the Desk.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "spare").state = "working"; });
  await expect.poll(order).toEqual(["# Working", "host", "spare"]);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").state = "idle"; });
  await expect.poll(order).toEqual(["# Working", "spare", "# On the Desk", "host"]);
});

test("an idle session whose crew is at work is listed under Working, its working crew first, never below idle sessions", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "host").state = "idle";
    st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", state: "idle", thread: [],
      team: [{ agent_id: "m1", name: "tidy", state: "done", uncommitted: true }, { agent_id: "m2", name: "busy-one", state: "working" }] });
    st.desk.projects.find((d) => d.project === "cue").members.push("spare");
  });
  const left = page.locator(".dk-sessions");
  const order = () => left.locator(".dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-sdiv") ? "—" : el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}`
    : el.matches(".dk-sfold") ? null : `${el.matches(".crew") ? "  " : ""}${el.dataset.deskLeadChat || el.dataset.deskChatOf || el.dataset.deskCrew}`).filter(Boolean));
  await expect.poll(order).toEqual(["# Working", "spare", "  m2", "  m1", "# On the Desk", "host"]);
  // Its row shows the idle mark, so the strip's meta line doesn't count it as working.
  await expect(left.locator('[data-desk-chat-of="spare"] .dk-st > i.idle')).toHaveCount(1);
  await expect(left.locator(".dk-strip .dk-sub")).toHaveText("2 sessions");
});

test("Looks right commits the item's files at once: no sheet; the row and a toast say \"Committed <hash> · N files\" with Undo, which takes it back", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").files = ["ui/send.js", "ui/styles/send.css"]; });
  await openDesk(page);
  await page.locator('[data-desk-item="d2"] .dk-ok').click();
  await cue.called("desk_looks_right", { id: "d2" });
  await expect(page.locator(".sheet")).toHaveCount(0);
  await expect(page.locator(".toast")).toContainText("Committed abc1234 · 2 files");
  const line = page.locator(".dk-cmt").first();
  await expect(line).toContainText("Committed abc1234 · 2 files");
  await expect(line.locator('[data-desk-act="reopen"]')).toHaveText("Undo · back to check");
  // Undo only puts it back to check: its commit stays, no code changes (#159).
  await line.locator('[data-desk-act="reopen"]').click();
  await cue.called("desk_item_act", { id: "d2", act: "reopen" });
  await expect(page.locator(".toast").last()).toHaveText("Back to check · nothing in the work changed");
  await expect(page.locator('[data-desk-item="d2"] .dk-ok')).toBeVisible();
  expect(await cue.calls("desk_commit_undo")).toHaveLength(0);
});

test("Undo on a done row: only on hover; back to waiting on you, its commit kept, nothing else; readable on the dimmed row; no hover text (#159)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => { Object.assign(s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2"), { state: "done", done_at: Math.floor(Date.now() / 1000), commit: "abc1234" }); });
  await openDesk(page);
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row).toHaveClass(/\bdk-done\b/);
  const undo = row.locator('.dk-undo[data-desk-act="reopen"]');
  // Only on hover: hidden until the pointer is on the row.
  await page.mouse.move(0, 0);
  await expect(undo).toBeHidden();
  await row.hover();
  await expect(undo).toBeVisible();
  await expect(undo).toHaveText("Undo");
  await expect(undo).not.toHaveAttribute("title", /.*/);
  expect(await undo.evaluate((el) => getComputedStyle(el).opacity)).toBe("1");
  expect(await row.locator(".dk-t").evaluate((el) => getComputedStyle(el.parentElement).opacity)).toBe("0.55");
  await undo.click();
  await cue.called("desk_item_act", { id: "d2", act: "reopen" });
  // Clicking Undo doesn't open the item; the row is waiting on you again, its ✓ back.
  await expect(page.locator('.dk-right .dk-pg h1:has-text("#d2")')).toHaveCount(0);
  await expect(row).toHaveClass(/\bdk-you\b/);
  await expect(row.locator(".dk-ok")).toBeVisible();
  await expect(row.locator(".dk-undo")).toHaveCount(0);
  expect(await cue.calls("desk_commit_undo")).toHaveLength(0);
});

test("✓ on a row: it dims while Undo can take it back, then folds away (height to nothing) before it goes", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await row.locator(".dk-ok").click();
  await expect(row).toHaveClass(/\bsoon\b/);
  await expect(row.locator(".dk-i1")).toHaveCSS("opacity", "0.45");
  // The fold lasts a moment: watched as it happens, its height then.
  const h = await page.evaluate(() => new Promise((done) => {
    const seen = () => { const el = document.querySelector('.dk-items .dk-it.dk-leave[data-desk-item="d2"]'); if (el) { obs.disconnect(); done(parseFloat(el.style.getPropertyValue("--h"))); } };
    const obs = new MutationObserver(seen);
    obs.observe(document.body, { subtree: true, childList: true, attributes: true });
    seen();
  }));
  expect(h).toBeGreaterThan(20);
  await cue.called("desk_looks_right", { id: "d2" });
  await expect(page.locator('.dk-items .dk-it.dk-leave')).toHaveCount(0);
});

test("Looks right on an item whose file another session is editing: done at once, \"Commit waiting: <file> (<session> is editing it)\", then \"Committed <hash>\" with Undo once it's made; with no files just done", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => {
    s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").files = ["ui/send.js", "ui/styles/send.css"];
    s.desk_busy = [{ path: "ui/styles/send.css", by: "tidier" }];
  });
  await openDesk(page);
  await page.locator('[data-desk-item="d2"] .dk-ok').click();
  await expect(page.locator(".toast")).toContainText("Done · commit waiting: ui/styles/send.css (tidier is editing it)");
  const line = page.locator(".dk-cmt").first();
  await expect(line).toHaveText("Commit waiting: ui/styles/send.css (tidier is editing it) · Undo · back to check");
  await expect(page.locator('[data-desk-item="d2"] .dk-ok')).toHaveCount(0);

  // Cue makes the commit once the tidier is done with the file: the same line says so, with Undo.
  await cue.change((s) => { const d = s.desk.projects.find((d) => d.project === "cue"); for (const i of [...d.items, ...(d.checks || [])].filter((i) => i.id === "d2")) Object.assign(i, { commit: "abc1234", commit_wait: null }); });
  await expect(line).toContainText("Committed abc1234");
  await expect(line.locator('[data-desk-act="reopen"]')).toBeVisible();

  // Why it couldn't be made, when a word stopped it.
  await cue.change((s) => { const d = s.desk.projects.find((d) => d.project === "cue"); for (const i of [...d.items, ...(d.checks || [])].filter((i) => i.id === "d2")) Object.assign(i, { commit: "", commit_wait: { files: [], busy: [], error: 'Not committed: "x" is in ui/send.js' } }); });
  await expect(line).toContainText('Commit not made: Not committed: "x" is in ui/send.js');

  await cue.change((s) => { const i = s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2"); Object.assign(i, { state: "to_check", commit: "", commit_wait: null, files: [] }); });
  await page.locator('[data-desk-item="d2"] .dk-ok').click();
  await expect(page.locator(".toast").last()).toContainText("Done");
  await expect(page.locator('[data-desk-item="d2"] .dk-cm')).toHaveCount(0);
  // Done with nothing committed still has its Undo: back to check, no code changed.
  await page.locator('[data-desk-item="d2"]').first().click();
  const plain = page.locator('.dk-cmt:has-text("Looked right")').first();
  await expect(plain).toHaveText("Looked right · Undo · back to check");
  await plain.locator('[data-desk-act="reopen"]').click();
  await cue.called("desk_item_act", { id: "d2", act: "reopen" });
  await expect(page.locator(".toast").last()).toHaveText("Back to check · nothing in the work changed");
});

/** A session of cue's that isn't on its Desk ("spare", holding nothing). */
const spare = (cue) => cue.change((st) => st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", thread: [] }));
/** The left card's groups and rows, as "# group" and session ids. */
const leftRows = (page) => page.locator(".dk-sessions .dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-sdiv") ? "—" : el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}` : el.dataset.deskChatOf || el.dataset.deskCrew).filter(Boolean));

test("Add session toggles project sessions on and off this Desk", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await spare(cue);
  await cue.change((st) => { const s = st.sessions.find((s) => s.session_id === "spare"); s.state = "working"; s.since_ms = 1; st.desk.projects.find((d) => d.project === "cue").work_history = ["host"]; });
  const workerInPanel = page.locator('.dk-right [data-desk-chat-of="spare"]');
  await expect(workerInPanel).toHaveCount(0);
  await expect.poll(() => leftRows(page)).toEqual(["# Working", "host"]);
  const add = page.getByRole("button", { name: "Add session", exact: true });
  const picker = page.getByRole("group", { name: "Desk membership" });
  await add.click();
  const spareBox = picker.getByRole("checkbox", { name: "spare", exact: true });
  const dispatcher = picker.getByRole("checkbox", { name: "web-lead · Dispatcher" });
  await expect(spareBox).not.toBeChecked();
  await expect(dispatcher).toBeChecked();
  const host = picker.getByRole("checkbox", { name: "host-lead Has work history" });
  await expect(host).toBeChecked();
  await expect(host).toBeDisabled();
  expect(await picker.getByRole("checkbox").evaluateAll((els) => els.map((el) => el.checked))).toEqual([true, true, false]);
  await expect(add).toHaveText("+");
  await expect(picker.locator(".dk-add-option svg")).toHaveCount(3);
  const anchor = await add.boundingBox(), popup = await picker.boundingBox();
  expect(popup.y + popup.height).toBeLessThanOrEqual(anchor.y);
  expect(Math.abs(popup.x - anchor.x)).toBeLessThan(2);
  await page.keyboard.press("Escape");
  await expect(picker).toHaveCount(0);
  await expect(add).toBeFocused();
  await add.click();
  await spareBox.check();
  expect((await cue.called("desk_member")).args).toEqual({ project: "cue", sid: "spare", on: true });
  await expect.poll(() => leftRows(page)).toEqual(["# Working", "host", "spare"]);
  await expect(workerInPanel).toHaveCount(1);
  await expect(spareBox).toBeChecked();
  await spareBox.uncheck();
  await expect.poll(() => leftRows(page)).toEqual(["# Working", "host"]);
  await expect(workerInPanel).toHaveCount(0);
  // The click was the button's, not the row's: the lead's chat still shows.
  await expect(page.locator('.dk-sessions [data-desk-lead-chat="lead"]')).toHaveClass(/\bon\b/);

  await dispatcher.uncheck();
  await expect.poll(async () => (await cue.calls("desk_member")).at(-1).args).toEqual({ project: "cue", sid: "lead", on: false });
  await expect(page.locator('[data-desk-lead-chat="lead"]')).toHaveCount(0);
  await expect(picker.getByRole("checkbox", { name: "web-lead", exact: true })).not.toBeChecked();
});

test("a session's ⋯ menu adds it to its project's Desk, or takes it off", async ({ page, cue }) => {
  await cue.open("board-waiting");
  await page.locator('.nrow[data-big="n1"]').click();
  const project = await page.evaluate(() => CueBridge.world.state.sessions.find((x) => x.session_id === "relay").project);
  expect(project).toBeTruthy();
  const item = page.locator('.more-menu [data-desk-member-set="relay"]');
  await page.locator('.active-pane [data-more="relay"]').click();
  await expect(item).toContainText("Add to Desk");
  await item.click();
  expect((await cue.called("desk_member")).args).toEqual({ project, sid: "relay", on: true });
  await expect(page.locator(".more-menu")).toHaveCount(0);
  await page.locator('.active-pane [data-more="relay"]').click();
  await expect(item).toContainText("Remove from Desk");
  await item.click();
  expect((await cue.calls("desk_member")).at(-1).args).toEqual({ project, sid: "relay", on: false });
  await page.locator('.active-pane [data-more="relay"]').click();
  await expect(item).toContainText("Add to Desk");
});

test("after a restart the lead's chat has its past (Cue's log beyond its last few) and what you half typed", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator(".dk-box textarea").fill("half a thought about the");
  await page.waitForTimeout(500);   // drafts are saved a moment after typing (window.js saveDrafts)
  // Quit and reopen: a new window, Cue's state from scratch; drafts from cue.db.
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-box textarea")).toHaveValue("half a thought about the");
  const chat = page.locator(".dk-chat");
  await expect(chat).toContainText("Earlier message 75");
  await expect(chat.locator(".cv-older button[data-act='load-older']")).toHaveText("Earlier messages");
  await chat.locator(".cv-older button[data-act='load-older']").click();
  await expect(chat).toContainText("Earlier message 45");
});

test("↑ in the lead's box brings back what you sent it (without the Desk's summary), ↓ goes forward to what you typed", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  expect(await page.evaluate(() => deskLead(deskOf(projFilter)).sid)).toBe("lead");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead"), t = Date.now();
    s.thread = [...(s.thread || []), { role: "you", text: '<desk project="cue">\nItems: none\n</desk>\n\nfirst ask', at_ms: t - 2000, images: [] },
      { role: "agent", text: "ok", at_ms: t - 1500, images: [] }, { role: "you", text: "second ask", at_ms: t - 1000, images: [] }];
  });
  const box = page.locator(".dk-box textarea");
  await box.fill("draft");
  await box.press("ArrowUp");
  await expect(box).toHaveValue("second ask");
  await box.press("ArrowUp");
  await expect(box).toHaveValue("first ask");
  await box.press("ArrowDown");
  await box.press("ArrowDown");
  await expect(box).toHaveValue("draft");
});

test("a DONE is one Ready to check line in the lead's chat: its item, its age, Check it › opens the item's page, where what to test, Looks right and Send back are; once checked a quiet line says what you said", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const line = page.locator(".dk-chat .dk-rchk");
  await expect(line).toHaveCount(1);
  await expect(page.locator(".dk-chat .dk-chk")).toHaveCount(0);
  await expect(line).toHaveText(/^Ready to check:Feature #d2Quick phrases at the right of Send\d+[smhd]Check it ›$/);
  await expect(line.locator(".dk-rchk-it svg.dk-kic")).toHaveCount(1);
  await expect(line.locator(".dk-inum")).toHaveText("Feature #d2");
  // Thin rules above and below, no box, no buttons of its own but the two that open the item.
  await expect(line).toHaveCSS("border-top-width", "1px");
  await expect(line).toHaveCSS("border-bottom-width", "1px");
  await expect(line).toHaveCSS("border-left-width", "0px");
  await expect(line.locator("[data-desk-act]")).toHaveCount(0);
  expect(await line.evaluate((el) => el.getBoundingClientRect().height)).toBeLessThan(48);

  // Not installed yet: nothing to try, so it's waiting for install, on its line, row and page (#130).
  await page.evaluate(() => { state.desk.installed_ms = 1793984280000 - 60000; renderMain(); });
  await expect(line.locator(".dk-rchk-w")).toHaveText("Waiting for install:");
  const row = page.locator('.dk-it[data-desk-item="d2"]');
  await expect(row.locator(".dk-who")).toContainText("waiting for install");
  await expect(row.locator(".dk-ok")).toHaveCount(0);
  // Its page while it waits: says so under what to test, and no Looks right among its buttons (Send back stays).
  const lead = page.locator(".dk-right .dk-box textarea");
  await lead.fill("a word for the lead");
  await line.locator(".dk-rchk-go").click();
  const pane = page.locator(".dk-pg");
  const info = pane.locator(".dk-chk-info");
  await expect(info.locator(".dk-chk-in")).toHaveText("Not installed yet: install Cue to try it");
  await expect(pane.locator('.dk-acts [data-desk-act="looks_right"]')).toHaveCount(0);
  await expect(pane.locator('.dk-acts [data-desk-act="send_back"]')).toHaveText("Send back");
  // Closing the page brings the lead's box back as you left it.
  await page.evaluate(() => deskShow(null));
  await expect(lead).toHaveValue("a word for the lead");
  // Installed before the DONE but after its work last changed (you install, then say DONE): it's in, too (#115).
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").checks[0].edited = 1793984280000 - 120000; });
  await page.evaluate(() => { state.desk.installed_ms = 1793984280000 - 60000; renderMain(); });
  await expect(line.locator(".dk-rchk-w")).toHaveText("Ready to check:");

  // Check it › opens the item's page: what to test as a checklist, then quiet under it who did it, when and how many
  // files, and that the installed Cue has it; Looks right and Send back.
  // A window wide enough for the panel to show beside the thread (#200).
  await page.setViewportSize({ width: 1512, height: 945 });
  await line.locator(".dk-rchk-go").click();
  await expect(page.locator('.dk-it.on[data-desk-item="d2"]')).toBeVisible();
  await expect(pane.locator(".dk-panel .dk-ptk li").first()).toBeVisible();
  await expect(info.locator(".dk-chk-w")).toHaveText(/^Done by web-lead · \d+[smhd] ago · 2 files$/);
  await expect(info.locator(".dk-chk-in")).toContainText("In the installed Cue · installed ");
  // It sits under what to test, in the same section.
  expect(await pane.evaluate((el) => { const w = el.querySelector(".dk-idw"), i = el.querySelector(".dk-chk-info"); return w.parentElement === i.parentElement && !!(w.compareDocumentPosition(i) & Node.DOCUMENT_POSITION_FOLLOWING); })).toBe(true);
  await expect(pane.locator('.dk-pnow [data-desk-act="looks_right"]')).toHaveText("Looks right");
  await expect(pane.locator('.dk-acts [data-desk-act="send_back"]')).toHaveText("Send back");
  // A design item's: its mocks in place of installed.
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").checks[0].designs = "https://claude.ai/artifact/mocks"; });
  await expect(info.locator('.dk-chk-in a[data-href="https://claude.ai/artifact/mocks"]')).toHaveText("Open the mocks ↗");
  await info.locator(".dk-chk-in a").click();
  await cue.called("open_link", { url: "https://claude.ai/artifact/mocks" });
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").checks[0].designs = ""; });
  await expect(info.locator(".dk-chk-in")).toContainText("In the installed Cue · installed ");
  // A link in what to test opens.
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").done_when = "See https://example.com/spec for it"; });
  await pane.locator('.dk-ptk a[data-href="https://example.com/spec"]').click();
  await cue.called("open_link", { url: "https://example.com/spec" });

  // Send back opens a box of its own on the page; with nothing typed its Send asks why, and sends nothing.
  await pane.locator('[data-desk-act="send_back"]').click();
  await expect(pane.locator(".dk-sb-l")).toHaveText("Why send it back?");
  await pane.locator('[data-desk-act="back_go"]').click();
  await expect(page.locator(".toast")).toContainText("Say why it goes back first");
  expect((await cue.calls("desk_item_act")).filter((c) => c.args.act === "send_back")).toEqual([]);
  // Its words go to the item's holder only; nothing goes to the lead, and the lead's box is left as it was.
  await pane.locator(".dk-sb textarea").fill("The arrows still skip one");
  await pane.locator('[data-desk-act="back_go"]').click();
  await cue.called("desk_item_act", { id: "d2", act: "send_back", text: "The arrows still skip one" });
  expect(await cue.calls("desk_send")).toEqual([]);
  // The page's Thread has why it went back, in your words; once the hub has it as a step, with how many pictures went too.
  await pane.locator('[data-desk-itab="thread"]').click();
  const back = pane.locator(".dk-tm", { has: page.locator(".dk-ttag.back") }).last();
  await expect(back.locator(".dk-md")).toHaveText("The arrows still skip one");
  await cue.change((s) => {
    const d = s.desk.projects.find((x) => x.project === "cue");
    d.decisions = d.decisions.filter((c) => !(c.item_id === "d2" && c.by === "You" && c.text === "The arrows still skip one"));
    d.decisions.unshift({ project: "cue", item_id: "d2", id: "sb1", by: "You", sid: "", text: "The arrows still skip one", why: "sent back to web-lead", at: Math.floor(Date.now() / 1000) });
    d.checks[0].back_pics = 2;
  });
  await expect(pane.locator(".dk-tm", { has: page.locator(".dk-ttag.back") })).toHaveCount(1);
  await expect(back.locator(".dk-md")).toHaveText("The arrows still skip one");
  await expect(back.locator(".dk-tpn")).toHaveText("2 pictures");
  // Who did it opens its chat: web-lead, the Desk's lead, so the lead's chat, its box as you left it.
  await cue.change((s) => { const d = s.desk.projects.find((x) => x.project === "cue"); Object.assign(d.items.find((i) => i.id === "d2"), { state: "to_check" }); d.checks[0].state = "to_check"; });
  await info.locator(".dk-chk-w .dk-chk-a").click();
  await expect.poll(() => page.evaluate(() => [deskOpen, deskTab])).toEqual([null, "lead"]);
  await expect(page.locator('.dk-right .dk-pg[data-desk-page="dispatcher"] h1')).toContainText("web-lead");
  await expect(lead).toHaveValue("a word for the lead");

  // Once checked, the lead's chat says what you said, quiet and centred; the last one's Undo after it.
  await page.evaluate(() => deskShow(null));
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").checks[0].state = "done"; });
  const said = page.locator(".dk-chat .dk-chk-done");
  await expect(line).toHaveCount(0);
  await expect(said).toHaveText("You · looked at #d2 and said it looks right · Undo · back to check");
  await expect(said.locator("svg.dk-kic")).toHaveCount(1);
  await expect(said.locator(".dk-cl-t")).toHaveCSS("text-align", "center");
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").checks[0].state = "back"; });
  await expect(said).toHaveText("You · sent #d2 back");
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

test("two send-backs in the same words, each on its own DONE: each shows its own pictures, by when it went, not by its words", async ({ page, cue }) => {
  await cue.open("desk");
  // The first DONE an hour before the second (ms); each sent back a minute after it, in the same words.
  await cue.change((s, [t1, t2]) => {
    const d = s.desk.projects.find((x) => x.project === "cue");
    const k = d.checks[0];
    d.checks = [{ ...k, at: t1, last: false, state: "back", back: "Still off", back_pics: 1 }, { ...k, at: t2, last: true, state: "back", back: "Still off", back_pics: 3 }];
    Object.assign(d.items.find((i) => i.id === "d2"), { state: "working" });
    const step = (id, at) => ({ project: "cue", item_id: "d2", id, by: "You", sid: "", text: "Still off", why: "sent back to web-lead", at: Math.floor(at / 1000) });
    d.decisions.unshift(step("sbA", t1 + 60000));
    d.decisions.unshift(step("sbB", t2 + 60000));
  }, [1793984280000 - 3600000, 1793984280000]);
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  const pane = page.locator(".dk-pg");
  await pane.locator('[data-desk-itab="thread"]').click();
  const backs = pane.locator(".dk-tm", { has: page.locator(".dk-ttag.back") });
  await expect(backs).toHaveCount(2);
  await expect(backs.locator(".dk-md")).toHaveText(["Still off", "Still off"]);
  await expect(backs.nth(0).locator(".dk-tpn")).toHaveText("1 picture");
  await expect(backs.nth(1).locator(".dk-tpn")).toHaveText("3 pictures");
});

test("finished items: the 10 newest, then 10 more as the list's end comes into view; nothing jumps, and 500 stay quick", async ({ page, cue }) => {
  await cue.open("desk");
  // 500 finished long ago, kept only in Cue's database (not in the Desk's state): the list loads them as you scroll.
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.older = Array.from({ length: 500 }, (_, k) => ({ id: `o${String(k).padStart(3, "0")}`, project: "cue", kind: "task", title: `Old ${k}`, state: "done", holder: null, created: 1000 + k, updated: 1000 + k, done_at: 1000 + k, from: "you" }));
  });
  await openDesk(page);
  await page.evaluate(() => { deskDonePages.clear(); renderMain(); });   // as the Desk opens with them there
  const list = page.locator(".dk-items .dk-scroll");
  const doneIds = async () => { const o = await listOrder(page); return o.slice(o.indexOf("# Done") + 1); };
  const count = (n) => expect.poll(async () => (await doneIds()).length).toBe(n);
  await count(10);
  await expect(list.locator('[data-desk-done-more="cue"]')).toHaveCount(1);
  // The newest first: today's three, then the newest of the old.
  expect((await doneIds()).slice(0, 5)).toEqual(["d7", "d8", "d9", "o499", "o498"]);
  // To the end: 10 more, and where you were reading stays where it was.
  const anchor = list.locator('[data-desk-item="o495"]');
  const top0 = await anchor.evaluate((e) => e.getBoundingClientRect().top);
  await list.evaluate((e) => { e.scrollTop = e.scrollHeight; });
  await count(20);
  const scrolled = await list.evaluate((e) => e.scrollTop);
  expect(await anchor.evaluate((e) => e.getBoundingClientRect().top) + scrolled).toBeCloseTo(top0, 0);
  await list.evaluate((e) => { e.scrollTop = e.scrollHeight; });
  await count(30);
  // Every one loaded: the end goes, and redrawing 500 stays quick.
  await expect.poll(async () => { await list.evaluate((e) => { e.scrollTop = e.scrollHeight; }); return (await doneIds()).length; }, { timeout: 20000, intervals: [50] }).toBe(503);
  await expect(list.locator('[data-desk-done-more]')).toHaveCount(0);
  const ms = await page.evaluate(() => { const t = performance.now(); for (let n = 0; n < 10; n++) deskList("cue"); return (performance.now() - t) / 10; });
  expect(ms).toBeLessThan(50);
});

test("your words about an item show its kind's icon, kind and number over them in the lead's chat; a click opens it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: "About #d1 (Reply box loses focus after Send): keep focus (in the box): please" }); renderMain(); });
  const you = page.locator(".dk-chat .cv-you").last();
  await expect(you.locator(".dk-about svg.dk-kic")).toHaveCount(1);
  await expect(you.locator(".dk-about")).toHaveText("Bug #d1 · Reply box loses focus after Send");
  await expect(you.locator(".cv-you-text")).toHaveText("Bug #d1 · Reply box loses focus after Sendkeep focus (in the box): please");
  await you.locator(".dk-about").click();
  await expect(page.locator(".dk-pg h1")).toHaveText("Bug #d1 · Reply box loses focus after Send");
});

test("a chat opened on the Desk takes the cursor in its box: the lead's, an item's page; not from the keys on the list (#154)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const box = page.locator(".dk-right .dk-box textarea[data-text]");
  await expect(box).toBeFocused();
  await page.locator('[data-desk-item="d2"]').click();
  await expect(page.locator(".dk-right .dk-ipane")).toBeVisible();
  await expect(box).toBeFocused();
  // Back on the lead's chat, then off the Desk and back: its box again.
  await page.locator(".dk-right .dk-back").first().click();
  await expect(page.locator(".dk-right .dk-ipane")).toHaveCount(0);
  await expect(box).toBeFocused();
  // Enter on a row in the list opens it and leaves the keys on the list.
  await page.locator('[data-desk-item="d4"]').focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".dk-right .dk-ipane")).toBeVisible();
  await expect(page.locator('[data-desk-item="d4"]')).toBeFocused();
});

test("Looks right is in an item's panel, in reach however far its thread is scrolled; Send back with it (#163, #200)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => { state.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d2").said = Array.from({ length: 30 }, (_, k) => ({ at: Date.now() - (60 - k) * 60000, by: "cue-117", text: `Step ${k}, written out at some length so it takes a line or two.` })); });
  await page.locator('[data-desk-item="d2"]').click();
  // In the panel's NOW (#200), beside the thread: it stays put however far the thread scrolls; Send back with it.
  const pane = page.locator(".dk-right .dk-ipane"), now = page.locator(".dk-right .dk-pnow"), top = now.locator('[data-desk-act="looks_right"]');
  await expect(top).toHaveText("Looks right");
  await expect(pane.locator('[data-desk-act="looks_right"]')).toHaveCount(0);
  await expect(now.locator('[data-desk-act="send_back"]')).toBeVisible();
  const at = await top.boundingBox();
  for (const y of [0, 600, 99999]) {
    await pane.evaluate((e, y) => { e.scrollTop = y; }, y);
    expect(await top.boundingBox()).toEqual(at);
  }
  // Clicked there, its few seconds to take it back are there too.
  await top.click();
  await expect(now.locator(".dk-soon")).toHaveText("Done · Undo · back to check");
});

test("Looks right waits a moment with \"Done · Undo\" on the item and in a toast; Undo then leaves it to check, nothing sent", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"] .dk-ok').click();
  await expect(page.locator('[data-desk-item="d2"] .dk-soon')).toHaveText("Done · Undo · back to check");
  await expect(page.locator(".toast")).toHaveText("Done · Undo · back to check");
  await page.locator(".toast .toast-undo").click();
  await expect(page.locator(".toast")).toHaveText("Still to check");
  await expect(page.locator('[data-desk-item="d2"] .dk-ok')).toBeVisible();
  await page.waitForTimeout(3300);
  expect(await cue.calls("desk_looks_right")).toEqual([]);
  // Undo on the item does the same; left alone, it goes.
  await page.locator('[data-desk-item="d2"] .dk-ok').click();
  await page.locator('[data-desk-item="d2"] .dk-soon [data-desk-act="looks_undo"]').click();
  await expect(page.locator('[data-desk-item="d2"] .dk-ok')).toBeVisible();
  await page.locator('[data-desk-item="d2"] .dk-ok').click();
  expect(await cue.calls("desk_looks_right")).toEqual([]);
  await cue.called("desk_looks_right", { id: "d2" });
});

test("Waiting on you is never red, with items or at 0; an open item takes the solid highlight with its words in the highlight's ink", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const you = page.locator(".dk-items .dk-gh.you");
  const deny = await page.evaluate(() => { const t = document.createElement("i"); t.style.color = "var(--deny)"; document.body.append(t); const c = getComputedStyle(t).color; t.remove(); return c; });
  expect(await you.evaluate((e) => getComputedStyle(e).color)).not.toBe(deny);
  await cue.change((st) => { for (const i of st.desk.projects.find((d) => d.project === "cue").items) if (["needs_you", "to_check"].includes(i.state)) i.state = "done"; });
  await expect(you).toHaveText(/Waiting on you\s*0/);
  expect(await you.evaluate((e) => getComputedStyle(e).color)).not.toBe(deny);
  const row = page.locator('[data-desk-item="d3"]');
  await row.click();
  await expect(row).toHaveClass(/\bon\b/);
  const [bg, sel, ink, idInk] = await row.evaluate((e) => { const c = (v) => { const t = document.createElement("i"); t.style.color = v; document.body.append(t); const r = getComputedStyle(t).color; t.remove(); return r; }; return [getComputedStyle(e).backgroundColor, c("var(--sel)"), c("var(--on-sel)"), getComputedStyle(e.querySelector(".dk-id")).color]; });
  expect(bg).toBe(sel);
  expect(idInk).toBe(ink);
});

test("an open item's box keeps the cursor: through redraws, and after Enter sends it, as you go on typing", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  const box = page.locator(".dk-right .dk-box textarea");
  await box.click();
  await page.keyboard.type("half a thought");
  const at = await box.boundingBox();
  for (let n = 0; n < 3; n++) await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d3").title += "."; });
  await page.keyboard.type(" more");
  expect(await page.locator(".dk-card.dk-right").evaluate((e) => getComputedStyle(e).overflow)).toBe("clip");   // can't be scrolled to show the box
  expect(await box.boundingBox()).toEqual(at);
  await expect(box).toBeFocused();
  await expect(box).toHaveValue("half a thought more");
  // Enter sends it; the send takes a moment, and you go on typing meanwhile: it all stays in this item's box.
  await cue.override("desk_send", () => new Promise((r) => { window.sendGoes = () => r({ sid: "lead" }); }));
  await page.keyboard.press("Enter");
  await expect(box).toHaveValue("");
  await page.keyboard.type("and then");
  await page.evaluate(() => window.sendGoes());
  await expect(page.locator(".toast").last()).toContainText("Sent to");
  await page.keyboard.type(" more");
  await expect(page.locator(".dk-pg h1")).toContainText("Search stays centred");
  await expect(box).toBeFocused();
  await expect(box).toHaveValue("and then more");
  expect(await box.boundingBox()).toEqual(at);
});

// Read next to Listen on a lead's reply (#117, #108): it opens the reader in the chat's place, at every text size
// (a smaller window stands in for a larger one), with its whole bar inside the card; ‹ › step, Esc goes back.
for (const z of [0.9, 1, 1.15, 1.3]) test(`Read on a Desk reply opens the reader, text size ${z}`, async ({ page, cue }) => {
  await page.setViewportSize({ width: Math.round(1280 / z), height: Math.round(800 / z) });
  await cue.open("desk");
  const long = "Here is what I found. " + "The reply goes on with plain words worth hearing out loud and reading larger. ".repeat(12);
  await cue.change((s, t) => { for (const x of s.sessions) if (x.session_id === "lead") x.thread.at(-1).text = t; }, long);
  await openDesk(page);
  const read = page.locator(".dk-chat .cv-agent.last [data-read]");
  await expect(page.locator(".dk-chat .cv-agent.last [data-listen]")).toBeVisible();
  await expect(read).toBeVisible();
  await read.click();
  await expect(page.locator(".dk-right .rd-head")).toBeVisible();
  await expect(page.locator(".dk-right .rd .msg")).toContainText("Here is what I found.");
  await expect(page.locator(".dk-right .dk-box")).toHaveCount(0);
  const card = await page.locator(".dk-right").boundingBox();
  for (const b of ["[data-reader-back]", ".rd-listen [data-listen]", "[data-reader-copy]", "[data-reader-reply]"]) {
    const r = await page.locator(`.dk-right ${b}`).boundingBox();
    expect(r.x + r.width).toBeLessThanOrEqual(card.x + card.width);
  }
  const of = await page.locator(".dk-right .rd-of").textContent();
  await page.locator("[data-reader-step='-1']").click();
  await expect(page.locator(".dk-right .rd-of")).not.toHaveText(of);
  await page.keyboard.press("Escape");
  await expect(page.locator(".dk-right .rd-head")).toHaveCount(0);
  await expect(page.locator(".dk-right .dk-box")).toHaveCount(1);
  // Reply: back to the chat, the cursor in the Desk's box.
  await read.click();
  await page.locator("[data-reader-reply]").click();
  await expect(page.locator(".dk-right .dk-box textarea")).toBeFocused();
});

test("Desk row approvals send Allow and No for the pending request", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const row = page.locator('[data-desk-item="d1"]');
  const questionRight = (await row.getByRole("button", { name: "Answer", exact: true }).boundingBox());
  for (const [index, [label, behavior]] of [["Allow", "allow"], ["No", "deny"]].entries()) {
    const id = `row-approval-${index}`;
    await cue.change((st, id) => {
      const item = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1");
      Object.assign(item.ask, { item_id: id, kind: "permission", text: "Run npm test" });
      st.items.push({ ...st.items.find((i) => i.id === "n-focus-ask"), id, kind: "permission", tool_name: "Bash", tool_input: { command: "npm test" } });
    }, id);
    await expect(row.locator(".dk-h")).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
    const no = await row.getByRole("button", { name: "No", exact: true }).boundingBox();
    expect(no.x + no.width).toBeCloseTo(questionRight.x + questionRight.width);
    await row.getByRole("button", { name: label, exact: true }).click();
    await cue.called("respond", { id, decision: { behavior } });
  }
});

test("an item's Answer opens its question at the page's foot and keeps the answer on its page", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d1"]').getByRole("button", { name: "Answer", exact: true }).click();
  const chain = page.locator(".dk-pane .dk-chain"), foot = page.locator(".dk-right .dk-ask");
  // The page's section shows only the question; its card is the foot, in the box's place.
  await expect(chain).toContainText(await page.evaluate(() => deskItem("cue", "d1").ask.text));
  await expect(chain.locator("button")).toHaveCount(0);
  await expect(page.locator(".dk-right .dk-box")).toHaveCount(0);
  await foot.locator("[data-pick]", { hasText: "Keep it in the box" }).click();
  await cue.called("respond");
  await cue.change((st) => {
    st.items = st.items.filter((i) => i.id !== "n-focus-ask");
    const d1 = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1");
    d1.state = "working"; d1.ask = null;
  });
  await expect(chain).toContainText("You answered: Keep it in the box");
  await expect(foot).toHaveCount(0);
  expect(await page.evaluate(() => view)).toBe("desk");
  await expect(page.locator(".dk-pg h1")).toHaveText("Bug #d1 · Reply box loses focus after Send");
  // A turn that ended on it: its whole last paragraph, and a box to its holder at the foot.
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "site");
    s.state = "waiting";
    s.thread = [...(s.thread || []), { role: "agent", text: "Moved the box.\n\nIf the box stays put, I'll write DONE.\nIf it still jumps, tell me what you did just before.", at_ms: Date.now() }];
    const d1 = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1");
    d1.state = "needs_you"; d1.ask = null; d1.holder = { kind: "session", sid: "site", name: "site-lead" };
  });
  await expect(chain).toContainText("If it still jumps, tell me what you did just before.");
  await expect(chain).not.toContainText("Moved the box.");
  const box = page.locator(".dk-right .dk-box textarea");
  await expect(page.locator(".dk-right .dk-box .dk-pre")).toHaveText("To site-lead · Bug #d1");
  await expect(box).toHaveAttribute("placeholder", "Reply box loses…");
  await box.fill("It stays put now");
  await page.locator(".dk-right [data-desk-send]").click();
  // Sent as about this item, so its session answers on it (#176).
  await cue.called("send_to_session", { sessionId: "site", text: "About #d1 (Reply box loses focus after Send): It stays put now" });
  await expect(chain).toContainText("You answered: It stays put now");
  expect(await page.evaluate(() => [view, deskOpen.kind])).toEqual(["desk", "item"]);
  // Its session at work again: the old ask and your answer leave the page.
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "site").state = "working";
    st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1").state = "working";
  });
  await expect(page.locator(".dk-pane")).not.toContainText("You answered");
  await expect(page.locator(".dk-pane")).not.toContainText("It asked you");
});

test("an open item turning ready to check draws Files changed at once, never a Files line swapped out", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  await page.locator(".dk-pg h1").waitFor();
  await page.locator('.dk-pane [data-desk-itab="files"]').click();
  await expect(page.locator(".dk-pane .dk-sec h3")).toHaveText("Changed so far");
  // Every section heading the pane has drawn, from here on.
  await page.evaluate(() => {
    window.heads = new Set();
    const look = () => document.querySelectorAll(".dk-pane .dk-sec > :first-child").forEach((h) => window.heads.add(h.textContent.trim()));
    new MutationObserver(look).observe(document.body, { childList: true, subtree: true });
  });
  await cue.change((st) => { const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d3");
    Object.assign(it, { state: "to_check", updated: Math.floor(Date.now() / 1000), files: ["ui/a.js", "ui/b.css"] }); });
  await expect(page.locator(".dk-pane .dk-fc .dv-add")).toHaveCount(2);
  expect([...await page.evaluate(() => [...window.heads])]).not.toContain("Files it's on");
  await expect(page.locator(".dk-pane .dk-sec h3")).toHaveText("Files changed");
});

test("an item's Files tab: what's changed so far while it's worked on, with counts, a click opening the diff; none, a line saying so (#149)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  const pane = page.locator(".dk-right .dk-ipane");
  await expect(pane.locator(".dk-itab")).toHaveText([/^Thread/, /^Decisions/, /^Steps/, /^Files/]);
  await expect(pane.locator(".dk-inow .dk-fc")).toHaveCount(0);
  await cue.called("desk_item_diff", { id: "d3" });
  const n = await page.evaluate(() => state.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d3").files.length);
  expect(n).toBeGreaterThan(0);
  await expect(pane.locator('[data-desk-itab="files"] .n')).toHaveText(String(n));
  await pane.locator('[data-desk-itab="files"]').click();
  await expect(pane.locator(".dk-itab.on")).toHaveText(/^Files/);
  await expect(pane.locator(".dk-sec h3")).toHaveText("Changed so far");
  await expect(pane.locator("button.dk-fc")).toHaveCount(n);
  await expect(pane.locator("button.dk-fc").first().locator(".dv-n")).toHaveText(["+2", "−1"]);
  await expect(pane.locator(".dk-fc-p").first()).toContainText("/");
  // Its counts at the right, apart from its name, as in the diff it opens.
  const row = await pane.locator("button.dk-fc").first().boundingBox(), del = await pane.locator("button.dk-fc").first().locator(".dv-del").boundingBox();
  expect(row.x + row.width - (del.x + del.width)).toBeLessThan(12);
  // A click opens that file's diff over the Desk; Esc closes it.
  await pane.locator("button.dk-fc").first().click();
  await expect(page.locator(".dov .dov-lines .a").first()).toBeVisible();
  const fh = await page.locator(".dov-fh").boundingBox(), fdel = await page.locator(".dov-fh .dv-del").boundingBox();
  expect(fh.x + fh.width - (fdel.x + fdel.width)).toBeLessThan(24);
  await page.keyboard.press("Escape");
  await expect(page.locator(".dov")).toHaveCount(0);
  // Nothing changed in them yet: the files it's on, as they are.
  await cue.change((st) => { const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d3"); it.unchanged = true; it.updated = Math.floor(Date.now() / 1000); });
  await page.locator('[data-desk-item="d2"]').click();
  await page.locator('[data-desk-item="d3"]').click();
  await pane.locator('[data-desk-itab="files"]').click();
  await expect(pane.locator(".dk-sec h3")).toHaveText("Files it's on");
  await expect(pane.locator(".dk-fc.wait")).toHaveCount(n);
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d3").files = []; });
  await expect(pane.locator(".dk-note")).toHaveText("No files on it yet.");
  await expect(pane.locator('[data-desk-itab="files"] .n')).toHaveText("0");
});

test("the lead's box has the chat box's quick phrases and Claude's suggested reply", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    st.settings.quick = { phrases: ["go ahead", "show me"] };
    const s = st.sessions.find((x) => x.session_id === "lead");
    s.state = "waiting";
    s.suggestion = "take the next item";
  });
  await openDesk(page);
  const box = page.locator(".dk-box"), ta = box.locator("textarea"), ghost = box.locator(".sug-line");
  // Over the text, as in a session's chat box: the phrases; in the empty box, the suggestion.
  await expect(box.locator(".quick .qp")).toHaveText(["go ahead", "show me"]);
  await expect(ghost).toBeVisible();
  await expect(ghost).toHaveText("Suggestedtake the next itemTab");
  // Tab types it in.
  await ta.focus();
  await page.keyboard.press("Tab");
  await expect(ta).toHaveValue("take the next item");
  await expect(ghost).toBeHidden();
  await ta.fill("");
  // A phrase sends, to the lead.
  await box.locator('.qp[data-phrase="go ahead"]').click();
  await cue.called("desk_send", { project: "cue", text: "go ahead" });
  // About an item: no phrases (#143) and no suggestion (those words aren't the lead's own next prompt).
  await page.locator('[data-desk-item="d3"]').click();
  await expect(page.locator(".dk-box .quick .qp")).toHaveCount(0);
  await expect(page.locator(".dk-box .sug-line")).toBeHidden();
});

test("a reply that asks for set words (Reply \"…\") offers them as the suggested reply: the lead's chat, an item's page; else Claude's own (#186)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    s.state = "waiting"; s.suggestion = "take the next item";
    s.thread = [...(s.thread || []), { role: "agent", text: 'It\'s built and its tests pass.\n\nReply "commit and install" and I\'ll finish it.', at_ms: Date.now(), images: [] }];
  });
  await openDesk(page);
  const ta = page.locator(".dk-right .dk-box textarea"), ghost = page.locator(".dk-right .dk-box .sug-line");
  await expect(ghost).toHaveText("Suggestedcommit and installTab");
  await ta.focus();
  await page.keyboard.press("Tab");
  await expect(ta).toHaveValue("commit and install");
  await ta.fill("");
  // Its latest message without such a line: Claude's own, as before.
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "lead"); s.thread = [...s.thread, { role: "agent", text: "Done.", at_ms: Date.now() + 1, images: [] }]; });
  await expect(ghost).toHaveText("Suggestedtake the next itemTab");
  // An item whose newest message asks (ready to check: it goes back to its holder) (curly quotes too): its words, Tab types them in, Enter sends.
  await cue.change((st) => { const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d2"); it.latest = ""; it.said = [{ at: Date.now() - 5000, by: "cue-new-idea", text: "#153 isn't done. What's left is putting it into main.\n\nReply “merge” and I'll finish it." }]; });
  await page.locator('[data-desk-item="d2"]').click();
  await expect(page.locator(".dk-right .dk-ipane")).toBeVisible();
  await expect(ghost).toHaveText("SuggestedmergeTab");
  await ta.focus();
  await page.keyboard.press("Tab");
  await expect(ta).toHaveValue("merge");
  await page.keyboard.press("Enter");
  await cue.called("desk_item_act", { id: "d2", act: "send_back", text: "merge" });
  // Answered: no suggestion till it asks again.
  await expect(ghost).toBeHidden();
});

test("what you write on an item's page shows in its Steps, with whom it went to (#136)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  const box = page.locator(".dk-right .dk-box textarea");
  await box.fill("Keep the search box centred on narrow windows too");
  await page.locator(".dk-right [data-desk-send]").click();
  await cue.called("desk_item_act", { id: "d3", act: "wrote_lead", text: "Keep the search box centred on narrow windows too" });
  await page.locator('[data-desk-itab="steps"]').click();
  const step = page.locator(".dk-pane .dk-step", { hasText: "Keep the search box centred on narrow windows too" });
  await expect(step).toHaveCount(1);
  await expect(step.locator(".dk-lby")).toHaveText("You");
  await expect(step.locator(".dk-w")).toContainText("to ");
  expect(await page.evaluate(() => [view, deskOpen?.kind, deskOpen?.id])).toEqual(["desk", "item", "d3"]);
});

test("Open on an item's page, or its holder's, opens that session's chat here on the Desk, never the board; ‹ goes back to it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const sid = await page.evaluate(() => state.sessions.find((s) => s.project === "cue" && s.session_id !== "lead").session_id);
  await cue.change((st, s) => Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4"), { holder: { kind: "session", sid: s, name: "cue-117" } }), sid);
  await page.locator('[data-desk-item="d4"]').click();
  await page.locator(".dk-ilinks .btn", { hasText: "Open cue-117" }).click();
  expect(await page.evaluate(() => view)).toBe("desk");
  await expect(page.locator(`.dk-right [data-desk-chat="${sid}"]`)).toBeVisible();
  // ‹ goes back to the item it was opened from; so does Esc.
  await expect(page.locator(".dk-right .dk-ph1 .dk-back")).toHaveText("‹ Bug #d4");
  await page.locator(".dk-right .dk-ph1 .dk-back").click();
  await expect(page.locator(".dk-right .dk-pg h1")).toContainText("Rail dot blinks");
  await page.locator(".dk-ilinks .btn", { hasText: "Open cue-117" }).click();
  await page.keyboard.press("Escape");
  await expect(page.locator(".dk-right .dk-pg h1")).toContainText("Rail dot blinks");
  // The holder's page: the same.
  await page.evaluate((s) => deskShow({ kind: "holder", sid: s }), sid);
  const hname = await page.locator(".dk-right .dk-pg h1 .dk-tname").textContent();
  await page.locator(".dk-right .dk-ph1 .btn", { hasText: "Open its chat" }).click();
  expect(await page.evaluate(() => view)).toBe("desk");
  await expect(page.locator(`.dk-right [data-desk-chat="${sid}"]`)).toBeVisible();
  await expect(page.locator(".dk-right .dk-ph1 .dk-back")).toHaveText(`‹ ${hname}`);
  await page.locator(".dk-right .dk-ph1 .dk-back").click();
  await expect(page.locator(".dk-right .dk-pg h1 .dk-tname")).toHaveText(hname);
  // From the sessions on the left, with the lead's chat open: back to the lead's chat.
  await page.evaluate(() => deskShow(null));
  await page.evaluate((s) => document.querySelector(`[data-desk-chat-of="${s}"]`).click(), sid);
  await expect(page.locator(".dk-right .dk-ph1 .dk-back")).toHaveText("‹ web-lead");
  await page.locator(".dk-right .dk-ph1 .dk-back").click();
  await expect(page.locator('.dk-chat[data-desk-chat="cue"]')).toBeVisible();
  // The Desk's own lead: its chat, still on the Desk.
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  await page.locator(".dk-right .dk-ph1 .btn", { hasText: "Open its chat" }).click();
  expect(await page.evaluate(() => view)).toBe("desk");
  await expect(page.locator('.dk-chat[data-desk-chat="cue"]')).toBeVisible();
});

test("an item's page is a Thread under Thread, Decisions and Steps tabs: every message in order, a send-back's dot red (#140)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue"), it = d.items.find((i) => i.id === "d4"), t = it.created < 1e12 ? it.created : it.created / 1000;
    it.said = [{ at: (t + 120) * 1000, by: "cue-117", text: "Fixed: the dot checks **who sent** it.", done: true }];
    d.decisions.push({ project: "cue", item_id: "d4", id: "y1", by: "You", sid: "", text: "Still blinks after a reply from someone else", why: "sent back to cue-117", at: t + 240 });
  });
  await page.locator('[data-desk-item="d4"]').click();
  const pane = page.locator(".dk-right .dk-pg");
  await expect(pane.locator(".dk-itab")).toHaveText([/^Thread/, /^Decisions/, /^Steps/, /^Files/]);
  await expect(pane.locator(".dk-itab.on")).toHaveText(/^Thread/);
  const msgs = pane.locator(".dk-thread .dk-tm");
  // Added first, its reply, then the send-back; the hand-off (a step) sits where its time puts it.
  const who = await msgs.locator(".dk-tw b").allTextContents();
  expect(who.slice(-3)).toEqual(["You", "cue-117", "You"]);
  await expect(msgs.nth(who.length - 2).locator(".dk-ttag")).toHaveText("Done");
  await expect(msgs.nth(who.length - 2).locator("strong")).toHaveText("who sent");
  const back = msgs.last();
  await expect(back.locator(".dk-ttag.back")).toHaveText("Sent back");
  await expect(back.locator(".dk-tdot.back")).toHaveCount(1);
  expect(await back.locator(".dk-tdot i").evaluate((e) => getComputedStyle(e).backgroundColor)).not.toBe(await msgs.first().locator(".dk-tdot i").evaluate((e) => getComputedStyle(e).backgroundColor));
  await expect(pane.locator(".dk-thread .dk-ttag", { hasText: "Hand-off" })).toHaveCount(0);   // "Gave it…" isn't a hand-off line
  // The answer box is the pane's foot.
  const foot = await page.locator(".dk-right .dk-box").boundingBox(), last = await back.boundingBox();
  expect(foot.y).toBeGreaterThan(last.y);
  // Steps and Decisions behind their tabs; the pick stays while the item's open.
  await pane.locator('[data-desk-itab="steps"]').click();
  await expect(pane.locator(".dk-itab.on")).toHaveText(/^Steps/);
  await expect(pane.locator(".dk-thread")).toHaveCount(0);
  await expect(pane.locator(".dk-step").first()).toBeVisible();
  await pane.locator('[data-desk-itab="decisions"]').click();
  await expect(pane).toContainText("No decisions on it yet.");
});

test("what you send on an item's page shows in its Thread at once, before it comes back as a step (#140)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  await cue.override("desk_item_act", () => new Promise((r) => { window.__release = r; }));
  await page.locator(".dk-right .dk-box textarea").fill("Narrow windows too");
  await page.locator(".dk-right [data-desk-send]").click();
  const mine = page.locator(".dk-thread .dk-tm", { hasText: "Narrow windows too" });
  await expect(mine).toHaveCount(1);
  await expect(mine.locator(".dk-tw b")).toHaveText("You");
  // Back as a step: still one message.
  await cue.change((st) => st.desk.projects.find((x) => x.project === "cue").decisions.push({ project: "cue", item_id: "d3", id: "y2", by: "You", sid: "", text: "Narrow windows too", why: "to web-lead", at: Math.floor(Date.now() / 1000) }));
  await page.evaluate(() => window.__release?.(null));
  await expect(mine).toHaveCount(1);
});

test("an item page's header (title, tabs) stays at the top as its thread scrolls, on its tinted band (#142, #200)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // A long thread, so the page scrolls.
  await cue.change((st) => { const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4"); it.said = Array.from({ length: 30 }, (_, n) => ({ at: Date.now() - (30 - n) * 60000, by: "cue-117", text: `Step ${n} of the fix, written out at some length so it takes a line or two.` })); });
  await page.locator('[data-desk-item="d4"]').click();
  const pane = page.locator(".dk-right .dk-ipane"), head = pane.locator(".dk-ihead"), title = page.locator(".dk-right .dk-ph h1");
  const before = await head.boundingBox(), tabs = await pane.locator(".dk-itabs").boundingBox(), h1 = await title.boundingBox();
  await pane.evaluate((e) => { e.scrollTop = e.scrollHeight; });
  expect(await pane.evaluate((e) => e.scrollTop)).toBeGreaterThan(200);
  expect(await head.boundingBox()).toEqual(before);
  expect(await pane.locator(".dk-itabs").boundingBox()).toEqual(tabs);
  expect(await title.boundingBox()).toEqual(h1);
  // The thread goes under the tabs, not over them.
  expect(await head.evaluate((e) => getComputedStyle(e).backgroundColor)).not.toBe("rgba(0, 0, 0, 0)");
});

test("an item's Done when is in its panel: n of m, ticked once it's ready to check, none before (#140, #200)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d4"]').click();
  const pane = page.locator(".dk-right .dk-ipane");
  // Not in the thread: the panel's own section.
  await expect(pane.locator(".dk-dwl, .dk-ptk")).toHaveCount(0);
  const sec = page.locator(".dk-right .dk-panel .dk-pdw");
  await expect(sec.locator("h4")).toHaveText("Done when · 0 of 1");
  await expect(sec.locator(".dk-ptk li")).toContainText(["The rail's dot blinks only for a session you wrote to"]);
  await expect(sec.locator(".dk-tk.y")).toHaveCount(0);
  // Ready to check: all ticked.
  await page.locator('[data-desk-item="d2"]').click();
  await expect(sec.locator("h4")).toHaveText("Done when · 2 of 2");
  await expect(sec.locator(".dk-ptk li").first()).toContainText("Quick phrases sit at the right of Send");
  await expect(sec.locator(".dk-tk.y")).toHaveCount(2);
});

test("an item's thread scrolls to the newest message when one arrives or you send one (#145)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4").said = Array.from({ length: 30 }, (_, k) => ({ at: Date.now() - (60 - k) * 60000, by: "cue-117", text: `Step ${k} of the fix, written out at some length so it takes a line or two.` })); });
  await page.locator('[data-desk-item="d4"]').click();
  const pane = page.locator(".dk-right .dk-ipane");
  const inView = async (loc) => {
    const r = await loc.boundingBox(), p = await pane.boundingBox(), h = await pane.locator(".dk-ihead").boundingBox();
    if (!r || !p || !h) return false;   // mid-redraw
    return r.y >= h.y + h.height - 1 && r.y + r.height <= p.y + p.height + 1;
  };
  // It opens at its newest message, as a chat does.
  await expect.poll(() => inView(pane.locator(".dk-thread .dk-tm").last())).toBe(true);
  expect(await pane.evaluate((e) => e.scrollTop)).toBeGreaterThan(200);
  // Scrolled up, it stays there till something comes.
  await pane.evaluate((e) => { e.scrollTop = 0; });
  // One arrives: it's in view.
  await cue.change((st) => { const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4"); it.said = [...it.said, { at: Date.now(), by: "cue-117", text: "A new reply just came in." }]; });
  const arrived = pane.locator(".dk-thread .dk-tm", { hasText: "A new reply just came in." });
  await expect(arrived).toHaveCount(1);
  await expect.poll(() => inView(arrived)).toBe(true);
  // Scrolled back up, you send one: it's in view too.
  await pane.evaluate((e) => { e.scrollTop = 0; });
  await cue.override("desk_item_act", () => new Promise((r) => { window.__release = r; }));
  await page.locator(".dk-right .dk-box textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  const sent = pane.locator(".dk-thread .dk-tm", { hasText: "Check the narrow window too" });
  await expect(sent).toHaveCount(1);
  await expect.poll(() => inView(sent)).toBe(true);
  // A redraw with nothing new leaves where you scrolled alone.
  await pane.evaluate((e) => { e.scrollTop = 0; });
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4").title += " "; });
  await page.waitForTimeout(200);
  expect(await pane.evaluate((e) => e.scrollTop)).toBe(0);
});

test("the chats' pages: the Dispatcher's NOW (its state, routing, with sessions, Check #…) and what waits on you; a session's header is its name, what it's on, its state in NOW (#200)", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  const dir = process.env.SHOT_DIR || process.env.TMPDIR || "/tmp";
  // For the shots: a new item from Cue, and the lead's own lines for it, as its chat shows them.
  await page.evaluate(() => {
    const t = sessionOf("lead").thread;
    t.push({ at_ms: Date.now() - 60000, role: "you", text: '<desk project="cue">\n3 open\n</desk>\n\nNew item #d6 (task): Tidy the Settings copy | done when: every label fits | from you\nRoute each now: ROUTE it to a session, or -> new.' });
    t.push({ at_ms: Date.now(), role: "agent", text: "ROUTE: #d6 -> host-lead | why: it has the Settings page warm\nDONE: #d3\nDESIGNS: #d6 https://example.com/mocks" });
    renderMain();
  });
  // The Dispatcher's page (#200): its header names it; its tabs (its chat, named for the lead, and Decisions) over the
  // chat; the panel's NOW says its state once, one sentence, and Check #… for the first that waits; WAITS ON YOU rows.
  const pg = page.locator('.dk-right .dk-pg[data-desk-page="dispatcher"]');
  await expect(pg.locator("h1")).toHaveText("Dispatcher · web-lead");
  await expect(pg.locator(".dk-ptabs .dk-tab.on")).toHaveText("web-lead");
  await expect(pg.locator(".dk-ptabs")).toContainText("Decisions");
  const now = pg.locator(".dk-pnow");
  await expect(now.locator(".dk-pdot")).toHaveCount(1);
  await expect(now.locator(".dk-pword")).toHaveText(/^(Working|Idle|Waiting on you)$/);
  await expect(now.locator(".dk-psay")).toContainText("2 items wait on you.");
  await expect(now.locator(".dk-pgo")).toHaveText("Check #d1");
  const waitsSec = pg.locator(".dk-panel .dk-psec", { has: page.locator("h4", { hasText: "Waits on you" }) });
  await expect(waitsSec.locator("h4")).toHaveText("Waits on you · 2");
  await expect(waitsSec.locator(".dk-prow")).toHaveCount(2);
  await page.screenshot({ path: `${dir}/cue-desk-lead-chat-light.png` });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({ path: `${dir}/cue-desk-lead-chat-dark.png` });
  await page.emulateMedia({ colorScheme: "light" });

  // One waits: its kind's icon and number in the sentence; Check #d2 opens it.
  await page.evaluate(() => { const d = deskOf("cue"); d.items.find((i) => i.id === "d1").state = "working"; renderMain(); });
  await expect(now.locator(".dk-psay")).toContainText("#d2 waits on you.");
  await expect(now.locator(".dk-psay svg.dk-kic")).toHaveCount(1);
  // Working (its ask answered) with one new item no one has: "Routing #d6"; how many are with sessions.
  await page.evaluate(() => { sessionOf("lead").state = "working"; state.items = state.items.filter((i) => i.session_id !== "lead"); renderMain(); });
  await expect(now.locator(".dk-pword")).toHaveText("Working");
  await expect(now.locator(".dk-psay")).toContainText("Routing #d6 to a session.");
  await expect(now.locator(".dk-psay")).toContainText(/\d+ items? (is|are) with sessions/);
  await now.locator(".dk-pgo").click();
  await expect(page.locator('.dk-it.on[data-desk-item="d2"]')).toBeVisible();
  await page.evaluate(() => deskShow(null));
  // None waits: no Check button, and WAITS ON YOU · 0.
  await page.evaluate(() => { const d = deskOf("cue"); d.items.find((i) => i.id === "d2").state = "done"; renderMain(); });
  await expect(now.locator(".dk-pgo")).toHaveCount(0);
  await expect(waitsSec).toHaveCount(0);

  // A session holding an item: the same header, "Session ·" and its name, Open its session at the right; under it
  // what it's on, its number opening the item; its state in NOW.
  await page.locator('.dk-sessions .dk-srow:not(.crew)', { hasText: "host-lead" }).first().click();
  const sp = page.locator('.dk-right .dk-pg[data-desk-page="session"]');
  await expect(sp).toHaveCount(1);
  await expect(sp.locator("h1 [data-desk-mark]")).toHaveText("Session");
  await expect(sp.locator("h1 .dk-shn")).toHaveText("host-lead");
  await expect(sp.locator(".dk-ph1 [data-desk-session]")).toHaveText("Open its session");
  await expect(sp.locator(".dk-ph3")).toHaveText(/^on #d5 · /);
  await expect(sp.locator(".dk-pnow .dk-pword")).toHaveText(/^(Working|Idle|Waiting on you)$/);
  // The same type as the Dispatcher's header.
  const font = (l) => l.evaluate((e) => { const c = getComputedStyle(e); return [c.fontSize, c.fontWeight]; });
  await page.evaluate(() => deskShow(null));
  const leadFont = await font(pg.locator("h1"));
  await page.locator('.dk-sessions .dk-srow:not(.crew)', { hasText: "host-lead" }).first().click();
  expect(await font(sp.locator("h1"))).toEqual(leadFont);
  if (process.env.SHOT_DIR) for (const c of ["light", "dark"]) { await page.emulateMedia({ colorScheme: c }); await sp.locator(".dk-ph").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-sess-held-${c}.png` }); }
  await page.emulateMedia({ colorScheme: "light" });
  await sp.locator(".dk-ph3 .dk-sch-t").click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "d5"]);
  await page.locator('.dk-sessions .dk-srow:not(.crew)', { hasText: "host-lead" }).first().click();
  await expect(sp).toHaveCount(1);
  await page.screenshot({ path: `${dir}/cue-desk-session-chat-light.png` });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({ path: `${dir}/cue-desk-session-chat-dark.png` });
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

test("on a named Desk, a session's head holds the items it holds on that Desk, not on its folder's Desk", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => {
    // A second Desk in the same folder; host-lead holds #d5 on the folder's Desk and #n1 on this one.
    const t = Date.now();
    state.desk.projects.push({ project: "nd1", id: "nd1", independent: true, name: "Feature A", workspace_project: "cue", lead: { sid: "lead", name: "web-lead", state: "live" }, decisions: [], members: ["host"],
      items: [{ id: "n1", project: "nd1", kind: "task", title: "Only on Feature A", state: "working", holder: { sid: "host", name: "host-lead", kind: "session" }, created: t, updated: t, from: "you", files: [] }] });
    deskSelect("nd1");
    deskShow({ kind: "chat", sid: "host" });
  });
  const sch = page.locator('.dk-right .dk-pg[data-desk-page="session"] .dk-ph');
  await expect(sch).toHaveCount(1);
  await expect(sch.locator(".dk-sch-t")).toHaveText(["#n1"]);
  await expect(sch.locator("[data-desk-mark]")).toHaveText("Session");
  await sch.locator(".dk-sch-t").click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id, deskOpen.project])).toEqual(["item", "n1", "nd1"]);
});

test("a message you send on an item's page is its thread's last, under its holder's latest words (#160); with its holder at work, its now under it (#165)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Its holder between turns, its latest words not in a reply (kept from before replies were).
  await cue.change((st) => {
    const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4");
    Object.assign(it, { said: [], latest: "Moved the search into the header's middle.", updated: Math.floor(Date.now() / 1000) - 60 });
    const s = st.sessions.find((x) => x.session_id === it.holder.sid); if (s) s.state = "idle";
  });
  await page.locator('[data-desk-item="d4"]').click();
  const rows = page.locator(".dk-right .dk-ipane .dk-thread .dk-tm");
  await expect(rows.last()).toContainText("Moved the search into the header's middle.");
  await cue.override("desk_item_act", () => new Promise((r) => { window.__release = r; }));
  await page.locator(".dk-right .dk-box textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  await expect(rows.last()).toContainText("Check the narrow window too");
  await expect(rows.nth(-2)).toContainText("Moved the search into the header's middle.");
  // Its holder at work: its "now" is last, under your message, as it answers it (#165).
  await cue.change((st) => { const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4"); const s = st.sessions.find((x) => x.session_id === it.holder.sid); it.state = "working"; it.now_on = true; if (s) s.state = "working"; });
  await expect(page.locator(".dk-right .dk-ipane .dk-tm.live")).toHaveCount(1);
  await page.locator(".dk-right .dk-box textarea").fill("And the wide one");
  await page.locator(".dk-right [data-desk-send]").click();
  await expect(rows.last()).toHaveClass(/live/);
  await expect(rows.nth(-2)).toContainText("And the wide one");
});

test("on the Board, a session working for a Desk has the Desk's mark before its name; one that isn't, none (#156)", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="board"]').click();
  const card = (name) => page.locator("[data-session], [data-detail]").filter({ has: page.locator(`.proj, .tl-name`, { hasText: new RegExp(`^${name}$`) }) }).first();
  await expect(card("host-lead").locator(".dk-name-ic svg.dk-kic")).toHaveCount(1);
  await expect(card("site-lead").locator(".dk-name-ic")).toHaveCount(1);
  // Membership keeps its type mark after its items finish; leaving removes it.
  await cue.change((st) => { for (const d of st.desk.projects) { for (const i of d.items) if (i.holder?.sid === "site") i.state = "done"; if (d.lead?.sid === "site") d.lead = null; } });
  await expect(card("site-lead").locator(".dk-name-ic")).toHaveCount(1);
  await cue.change((st) => { for (const d of st.desk.projects) d.members = (d.members || []).filter((sid) => sid !== "site"); });
  await expect(card("site-lead").locator(".dk-name-ic")).toHaveCount(0);
  await expect(card("host-lead").locator(".dk-name-ic")).toHaveCount(1);
});

test("Board Desk markers show dispatcher, lead, and session roles", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="board"]').click();
  const types = await page.evaluate(() => {
    const d = state.desk.projects.find((x) => x.project === "cue");
    d.lead = { sid: "lead", name: "lead" };
    d.members = ["lead", "host", "site"];
    d.items = [{ state: "working", holder: { sid: "host", kind: "lead" } }, { state: "working", holder: { sid: "site", kind: "session" } }];
    return ["lead", "host", "site"].map((sid) => deskNameMark(sid));
  });
  expect(types[0]).toContain('data-desk-type="desk"');
  expect(types[1]).toContain('data-desk-type="lead"');
  expect(types[2]).toContain('data-desk-type="session"');
});

test("the Board Desk link uses the tray and omits repeated task details", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const desk = st.desk.projects.find((d) => d.project === "cue");
    Object.assign(desk, { project: "desk-feature", workspace_project: "cue", name: "Feature Desk" });
    for (const item of desk.items) item.project = desk.project;
    st.sessions.find((session) => session.session_id === "host").prompt = "Desk item #d5 (bug): Hosted sessions can't open the canvas";
    st.sessions.find((session) => session.session_id === "site").prompt = "About #w1 (Keep this task name): Check the link";
  });
  await page.evaluate(() => deskSelect("web"));
  await page.locator('.switch-view [data-view="board"]').click();
  const card = (name) => page.locator("[data-session], [data-detail]").filter({ has: page.locator(".proj", { hasText: new RegExp(`^${name}$`) }) }).filter({ has: page.locator(".dk-chip") }).first();
  const lead = card("host-lead").locator(".dk-chip");
  await expect(lead).toHaveCount(1);
  await expect(lead).toHaveText("Feature Desk›");
  await expect(lead).toHaveAttribute("data-inbox-desk", "desk-feature");
  await expect(lead.locator("svg path").first()).toHaveAttribute("d", /^M4 13l3-7/);
  await expect(card("host-lead")).not.toContainText("Hosted sessions can't open the canvas");
  await expect(lead.locator("svg.dk-kic")).toHaveCount(1);
  await expect(page.locator("[title]", { has: page.locator(".dk-chip") })).toHaveCount(0);
  await expect(card("site-lead")).toContainText("Check the link");
  await expect(card("site-lead")).not.toContainText("Keep this task name");
  await lead.click();
  await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/on/);
  await expect(page.locator('.dk-right [data-desk-page="dispatcher"] [data-desk-chat="desk-feature"]')).toBeVisible();
  await expect(page.locator(".dk-sh b")).toHaveText("Feature Desk");
});

test("the Desk's mark is on the session's chat title and its Inbox page head too, never twice on a card (#156)", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="board"]').click();
  // Each card has it once (nameSpan carries it), never doubled.
  for (const el of await page.locator(".tl-top, .card-head, .nrow-top, .nrow").all()) expect(await el.locator(":scope > .dk-name-ic, :scope > * > .dk-name-ic").count()).toBeLessThanOrEqual(1);
  // Opened, its chat's title has it before the name.
  await page.locator("[data-session], [data-detail]").filter({ has: page.locator(".proj, .tl-name", { hasText: /^host-lead$/ }) }).first().click();
  const title = page.locator(".proj", { hasText: /^host-lead$/ }).last();
  await expect(title.locator("xpath=preceding-sibling::*[1]")).toHaveClass(/dk-name-ic/);
  expect(await page.evaluate(() => typeof deskSessMark === "function" && deskSessMark("host").includes("dk-name-ic"))).toBe(true);
  expect(await page.evaluate(() => deskSessMark("nobody"))).toBe("");
});

test("tasks, bugs and features show a short question and right-aligned choices", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const desk = st.desk.projects.find((d) => d.project === "cue");
    for (const [id, kind] of [["d1", "bug"], ["d2", "feature"], ["d3", "task"]]) {
      const item = desk.items.find((i) => i.id === id);
      const question = "Which layout should we use?";
      item.kind = kind;
      item.state = "needs_you";
      item.ask = { item_id: `layout-${id}`, kind: "question", text: question, choices: ["Compact", "Spacious"] };
      st.items.push({ id: item.ask.item_id, session_id: "host", project: "cue", harness: "codex",
        kind: "question", status: "pending", tool_input: { questions: [{ question, options: [{ label: "Compact" }, { label: "Spacious" }] }] }, created_ms: Date.now() });
    }
  });
  const requests = page.locator(".dk-items .dk-ask-row").filter({ hasText: "Which layout should we use?" });
  await expect(requests).toHaveCount(3);
  for (const id of ["d1", "d2", "d3"]) {
    const row = page.locator(`.dk-items [data-desk-item="${id}"] .dk-ask-row`);
    await expect(row.locator(".dk-h")).toHaveText("Which layout should we use?");
    await expect(row.getByRole("button")).toHaveText(["Compact", "Spacious"]);
    await expect(row).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
    const rowBounds = await row.boundingBox(), choices = await row.locator(".dk-row-decisions").boundingBox();
    expect(Math.abs(rowBounds.x + rowBounds.width - choices.x - choices.width)).toBeLessThan(2);
  }
  await requests.first().getByRole("button", { name: "Compact", exact: true }).click();
  const sent = await cue.called("respond");
  expect(sent.args.decision).toEqual({ behavior: "allow", answers: { "Which layout should we use?": "Compact" } });
});

test("an open item shows the question without a heading or unrelated decisions", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "site");
    s.state = "waiting";
    s.thread = [...(s.thread || []), { role: "agent", text: "Found it.\n\nWhich + did you mean?\n\nDECIDED: left the other files alone — another session edits them", at_ms: Date.now() }];
    const d1 = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1");
    d1.state = "needs_you"; d1.ask = null; d1.holder = { kind: "session", sid: "site", name: "site-lead" };
  });
  await page.locator('[data-desk-item="d1"]').click();
  const chain = page.locator(".dk-pane .dk-chain");
  await expect(chain).toHaveText("Which + did you mean?");
  await expect(chain.locator(".dk-ln > i")).toHaveCount(0);
  await expect(chain).not.toContainText("Decided");
  await expect(page.locator(".dk-right")).not.toContainText("It asks you");
});

test("an item whose new session couldn't start says so, with why and Retry, not stuck in its queue (#172)", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  await page.evaluate(() => {
    state.desk.projects.find((x) => x.project === "cue").items.push({ id: "f1", project: "cue", kind: "bug", title: "The footer overlaps", state: "new", created: Date.now(), updated: Date.now(), holder: null, from: "you", start_failed: "the folder is gone" });
    renderMain();
  });
  await expect(page.locator('[data-desk-item="f1"] .dk-tag')).toHaveText("couldn't start");
  await page.locator('[data-desk-item="f1"]').click();
  const pane = page.locator(".dk-right .dk-pg");
  await expect(pane).toContainText("Couldn't start");
  await expect(pane).toContainText("Cue couldn't start a new session for it: the folder is gone");
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveText("Retry");
  if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-start-failed.png` });
  await pane.locator('[data-desk-act="start_now"]').click();
  await cue.called("desk_item_act", { id: "f1", act: "start_now" });
  // Held by a session once it starts: the words go.
  expect(await page.evaluate(() => deskTag({ state: "working", holder: { sid: "x" }, start_failed: "old" }))).toEqual(["", ""]);
});

test("the Desk's hand-off shows in the session's chat as a card: from the lead and the item, its title, Done when as lines, Why; Cue's standing words left out (#174)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => {
    const words = [
      "Done? Write DONE: #d5, or say what's left",
      "",
      "Desk item #d5 (bug): Hosted sessions can't open the canvas",
      "Done when: (1) the canvas opens from a hosted session (2) it says why when it can't; light and dark. Installed and committed.",
      "Why you: you know the host side",
      "Fixes and next steps for it stay in #d5: do them, or say them in your reply; don't add new items for them. Add one only for something truly separate, with why in one line: ITEM: <bug|feature|task> | <title> | done when: <…> | why: <how it's separate from #d5>",
      "If you put its designs on a canvas, or move them, say DESIGNS: #d5 <canvas link> on a line of its own.",
      "Say DONE: #d5 on a line of its own when it's done.",
    ].join("\n");
    sessionOf("host").thread.push({ at_ms: Date.now() - 60_000, role: "you", text: words });
    deskShow({ kind: "chat", sid: "host" });
  });
  const card = page.locator('.dk-right [data-desk-chat="host"] .dk-hand');
  await expect(card).toHaveCount(1);
  await expect(card.locator(".dk-hand-h")).toHaveText("From web-lead · Bug #d5");
  await expect(card.locator(".dk-hand-t")).toHaveText("Hosted sessions can't open the canvas");
  await expect(card.locator(".dk-dwl li")).toHaveText(["the canvas opens from a hosted session", "it says why when it can't", "light and dark", "Installed and committed"]);
  await expect(card.locator(".dk-hand-w")).toHaveText("you know the host side");
  await expect(card.locator(".dk-hand-pre")).toHaveText("Done? Write DONE: #d5, or say what's left");
  await expect(card).not.toContainText("Fixes and next steps");
  await expect(card).not.toContainText("DESIGNS:");
  await expect(card).not.toContainText("on a line of its own");
  const dir = process.env.SHOT_DIR || process.env.TMPDIR || "/tmp";
  await card.screenshot({ path: `${dir}/cue-desk-hand-light.png` });
  await page.emulateMedia({ colorScheme: "dark" });
  await card.screenshot({ path: `${dir}/cue-desk-hand-dark.png` });
  await page.emulateMedia({ colorScheme: "light" });
  // Its number opens the item.
  await card.locator(".dk-hand-n").click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "d5"]);
});

test("an item's thread holds only its own messages: its session's words about another item, and its working row, stay off it (#176)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Its session at work on another item: its latest words are about that one, not this.
  await cue.change((st) => {
    const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5");
    const s = st.sessions.find((x) => x.session_id === it.holder.sid); if (s) s.state = "working";
    // The hub leaves its latest empty when its session is on another item.
    Object.assign(it, { state: "working", now_on: false, said: [], latest: "" });
  });
  await page.locator('[data-desk-item="d5"]').click();
  const pane = page.locator(".dk-right .dk-ipane");
  await expect(pane.locator(".dk-tm.live")).toHaveCount(0);
  // Its session's turn ended on it: what you write on its page goes to its session as about it, and shows in its
  // thread as you wrote it.
  await cue.change((st) => {
    const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5");
    st.sessions.find((x) => x.session_id === it.holder.sid).state = "waiting";
    Object.assign(it, { state: "needs_you", now_on: true });
  });
  await page.locator(".dk-right .dk-box textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  const sent = await cue.called("send_to_session");
  expect(sent.args.text).toMatch(/^About #d5 \(.+\): Check the narrow window too$/);
  await expect(pane.locator(".dk-thread .dk-tm").last()).toContainText("Check the narrow window too");
  // Its session on it now: its working row comes back.
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").state = "working"; st.sessions.find((x) => x.session_id === "host").state = "working"; });
  await expect(pane.locator(".dk-tm.live")).toHaveCount(1);
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    if (process.env.SHOT_DIR) await page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-176-${theme}.png` });
  }
});

test("an item with mocks has \"Open the mocks\" in its NOW; waiting on a pick, a plain line there says which (#178, #200)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5");
    Object.assign(it, { state: "to_check", designs: "https://claude.ai/artifact/MOCKS", said: [{ at: Date.now(), by: "host-lead", text: "Put three ways to show **working** items on the canvas.\nPick one for working items: D, E or F." }] });
  });
  await page.locator('[data-desk-item="d5"]').click();
  const head = page.locator(".dk-right .dk-pnow");
  await expect(head.locator(".dk-pacts a.dk-dlink")).toHaveText("Open the mocks ↗");
  // Under the sentence, not over it.
  const box = async (l) => l.evaluate((e) => { const r = e.getBoundingClientRect(); return [r.top, r.bottom]; });
  expect((await box(head.locator("a.dk-dlink")))[0]).toBeGreaterThanOrEqual((await box(head.locator(".dk-psay")))[1]);
  await expect(head.locator(".dk-pickl")).toHaveText("Pick one for working items: D, E or F.");
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    if (process.env.SHOT_DIR) await page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-178-${theme}.png` });
  }
  // Picked (no reply asks for one): no pick line, only Try it (#182); no mocks: no link.
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").said = [{ at: Date.now(), by: "host-lead", text: "Built E. DONE: #d5" }]; });
  await expect(head.locator(".dk-pickl")).toHaveText(/^Try it: /);
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").designs = ""; });
  await expect(head.locator("a.dk-dlink")).toHaveCount(0);
});

// Done means it's in your app (#181): a DONE is committed, built and installed before it's yours to check.
/** Item `id` of the cue Desk changed by `patch` (merged in). */
const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);

test("a finishing item: amber and steady, its step under its title; in the next install, the Items header says when with Install now; not waiting on you; its row stays put (#181, #190)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const before = await listOrder(page);
  await patchItem(cue, "d5", { state: "finishing", finish: { step: "commit_wait", wait: "web-lead is editing ui/desk.js", at: Date.now() } });
  const row = page.locator('.dk-items [data-desk-item="d5"]');
  await expect(row).toHaveClass(/\bdk-fin\b/);
  await expect(row.locator(".dk-who")).toHaveText("committing · waits: web-lead is editing ui/desk.js");
  // Steady: no blinking dot; amber, not the red of an ask.
  await expect(row.locator(".dk-si .dk-blink")).toHaveCount(0);
  const amber = await page.evaluate(() => { const e = document.createElement("i"); e.style.color = "var(--amber)"; document.body.append(e); const c = getComputedStyle(e).color; e.remove(); return c; });
  await expect(row.locator(".dk-fstep")).toHaveCSS("color", amber);
  // Only its step: its title keeps the ink of any row.
  await expect(row.locator(".dk-t")).not.toHaveCSS("color", amber);
  await expect(page.locator(".dk-items .dk-instnow")).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-next")).toHaveCount(0);
  // Not counted as waiting on you, and where it was in the list.
  await expect(page.locator(".dk-items .dk-gh.you")).toContainText("2");
  expect(await listOrder(page)).toEqual(before);

  // In the next install (#190): its row says so and when; the Items header has the one Install now, for all of it.
  const due = Date.now() + 30 * 60_000;
  await cue.change((st, t) => { st.desk.install_at = t; }, due);
  await patchItem(cue, "d5", { finish: { step: "queued", commit: "abc9999", at: Date.now() } });
  const clock = await page.evaluate((t) => clockAt(t), due);
  await expect(row.locator(".dk-who")).toHaveText(`in the next install · ${clock}`);
  await expect(row.locator(".dk-instnow")).toHaveCount(0);
  const head = page.locator(".dk-items .dk-ch");
  await expect(head.locator(".dk-next")).toHaveText(`Next install ${clock} · 1 item`);
  await expect(head.locator(".dk-instnow")).toHaveText("Install now");
  await expect(head.locator(".dk-instnow")).not.toHaveAttribute("title", /.*/);
  // Its page: the Finishing pill, the step in its line, Install now among its buttons.
  await row.locator(".dk-t").click();
  const pane = page.locator(".dk-right .dk-pg");
  await expect(pane.locator(".dk-pnow .dk-pword")).toHaveText("Finishing");
  await expect(pane.locator(".dk-pnow")).toContainText(`in the next install · ${clock}`);
  await expect(pane.locator('.dk-acts [data-desk-act="install_now"]')).toHaveText("Install now");
  await expect(pane.locator('[data-desk-act="looks_right"]')).toHaveCount(0);
  await cue.override("desk_item_act", () => null);
  await head.locator(".dk-instnow").click();
  await cue.called("desk_item_act", { id: "d5", act: "install_now" });
  // Clicking it doesn't close the page it's on.
  await expect(pane.locator("h1")).toContainText("#d5");

  await patchItem(cue, "d5", { finish: { step: "building", commit: "abc9999", at: Date.now() } });
  await expect(row.locator(".dk-who")).toHaveText("building…");
  await expect(page.locator('[data-desk-act="install_now"]')).toHaveCount(0);
  await patchItem(cue, "d5", { finish: { step: "installing", commit: "abc9999", at: Date.now() } });
  await expect(row.locator(".dk-who")).toHaveText("installing…");
  expect(await listOrder(page)).toEqual(before);
});

test("a failed build waits on you with its question; its page has the log's last lines in mono, Show all opens the rest (#181)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const log = Array.from({ length: 20 }, (_, n) => `line ${n + 1}`).join("\n");
  await patchItem(cue, "d5", { state: "needs_you", latest: "The build failed", finish: { step: "failed", question: "The build failed", log, at: Date.now() } });
  const row = page.locator('.dk-items [data-desk-item="d5"]');
  await expect(row.locator(".dk-h")).toHaveText("Answer: The build failed");
  await expect(page.locator(".dk-items .dk-gh.you")).toContainText("3");
  await row.locator(".dk-t").click();
  const pane = page.locator(".dk-right .dk-pg");
  await expect(pane.locator(".dk-chain")).toContainText("The build failed");
  const pre = pane.locator(".dk-flog");
  await expect(pre).toHaveText(Array.from({ length: 6 }, (_, n) => `line ${n + 15}`).join("\n"));
  await expect(pre).toHaveCSS("font-family", /mono|Menlo|Courier/i);
  await pane.locator(".dk-flog-more").click();
  await expect(pre).toHaveText(log);
  await expect(pre).toHaveCSS("overflow-y", "auto");
  await expect(pane.locator(".dk-flog-more")).toHaveText("Show less");  // Try again, once you've seen to it: finishing runs again (retry_finish).
  await cue.override("desk_item_act", () => null);
  await pane.locator('.dk-acts [data-desk-act="retry_finish"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "retry_finish" });
});

test("Looks right on an item committed as it finished only closes it: its commit in its state line, never \"waiting for install\" (#181)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => { s.desk.installed_ms = 1793984280000 - 60000; });
  await openDesk(page);
  await patchItem(cue, "d2", { files: ["ui/a.js"], commit: "abc9999", finish: { step: "installed", commit: "abc9999", at: Date.now() } });
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row.locator(".dk-who")).toHaveText("web-lead");
  await expect(row.locator(".dk-ok")).toHaveCount(1);
  await row.locator(".dk-t").click();
  const pane = page.locator(".dk-right .dk-pg");
  await expect(pane.locator(".dk-pnow .dk-pword")).toHaveText("Ready to check");
  await expect(pane.locator(".dk-state")).toContainText("with web-lead · committed abc9999");
  await expect(pane.locator(".dk-chk-in")).toContainText("In the installed Cue");
  await expect(pane.locator('[data-desk-act="looks_right"]')).toHaveCount(1);
  // The hub only closes it (nothing more committed).
  await cue.override("desk_looks_right", ({ id }) => {
    const it = CueBridge.world.state.desk.projects[0].items.find((i) => i.id === id);
    Object.assign(it, { state: "done", done_at: Math.floor(Date.now() / 1000) });
    CueBridge.emit("state", CueBridge.world.snapshot());
    return { commit: it.commit, files: 0, waiting: [] };
  });
  await pane.locator('[data-desk-act="looks_right"]').click();
  await cue.called("desk_looks_right", { id: "d2" });
  await expect(page.locator(".toast").last()).toHaveText("Committed abc9999");
  await expect(row).toHaveClass(/\bdk-done\b/);
});

test("an item to check from before (files, no commit) keeps today's: waiting for install till Cue has it, and Looks right commits its files (#181)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").files = ["ui/send.js", "ui/styles/send.css"]; s.desk.installed_ms = 1793984280000 - 60000; });
  await openDesk(page);
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row.locator(".dk-who")).toContainText("waiting for install");
  await expect(row.locator(".dk-ok")).toHaveCount(0);
  await cue.change((s) => { s.desk.installed_ms = 1793984280000 + 60000; });
  await expect(row.locator(".dk-ok")).toHaveCount(1);
  await expect(row.locator(".dk-cm")).toHaveCount(0);
  await row.locator(".dk-ok").click();
  await cue.called("desk_looks_right", { id: "d2" });
  await expect(page.locator(".toast").last()).toContainText("Committed abc1234 · 2 files");
});

test("each waiting row says what to do in one line, and what it unblocks; the most-unblocking first; a stopped item waits on the lead, not on you (#182)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const items = st.desk.projects.find((x) => x.project === "cue").items;
    const it = (id) => items.find((i) => i.id === id);
    // d2 waits on a pick, and four items wait on it (one through another).
    Object.assign(it("d2"), { state: "needs_you", ask: { item_id: "a2", sid: "web-lead", text: "Phrases left or right of Send?", choices: ["Left", "Right"], kind: "question" } });
    Object.assign(it("d4"), { after: "d2" });
    Object.assign(it("d6"), { after: "d4" });
    items.push({ ...it("d6"), id: "d10", title: "Ten", after: "d2" }, { ...it("d6"), id: "d11", title: "Eleven", after: "d10" });
    // d1's ask is a tool to allow.
    Object.assign(it("d1").ask, { kind: "permission", text: "Run npm test", choices: [] });
    // d3's holder stopped on it with no ask: the lead settles it.
    Object.assign(it("d3"), { stopped: true });
  });
  const you = page.locator(".dk-items .dk-gh.you");
  await expect(you).toContainText("2");
  expect((await listOrder(page)).slice(0, 3)).toEqual(["# Waiting on you", "d2", "d1"]);
  await expect(page.locator('[data-desk-item="d2"] .dk-h')).toHaveText("Phrases left or right of Send?");
  await expect(page.locator('[data-desk-item="d2"] .dk-ub')).toHaveText("unblocks #d4, #d10, #d6 +1 more");
  await expect(page.locator('[data-desk-item="d1"] .dk-h')).toHaveText("Allow: Run npm test");
  await expect(page.locator('[data-desk-item="d1"] .dk-ub')).toHaveCount(0);
  const d3 = page.locator('[data-desk-item="d3"]');
  await expect(d3).toContainText("stopped · the Desk lead is on it");
  await expect(d3.locator(".dk-h")).toHaveCount(0);
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    if (process.env.SHOT_DIR) await page.locator(".dk-items").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-182-${theme}.png` });
  }
  // Its page: the same line, and what it unblocks.
  await page.locator('[data-desk-item="d2"]').click();
  const head = page.locator(".dk-right .dk-pnow");
  await expect(head.locator(".dk-pickl")).toHaveText("Pick: Phrases left or right of Send?");
  await expect(head.locator(".dk-iub")).toHaveText("unblocks #d4, #d10, #d6 +1 more");
  await page.locator('[data-desk-item="d3"]').click();
  await expect(head.locator(".dk-pword")).toHaveText("Stopped");
});

test("Hide panel folds the panel for that kind of page and is remembered; a narrow card folds it on its own (#200)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1512, height: 945 });
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d4"]').click();
  const pg = page.locator(".dk-right .dk-pg");
  await expect(pg).not.toHaveClass(/\bhid\b/);
  await expect(pg.locator(".dk-panel .dk-pnow")).toBeVisible();
  await pg.locator(".dk-ptog").click();
  await expect(pg).toHaveClass(/\bhid\b/);
  await expect(pg.locator(".dk-ptog")).toHaveText("Show panel");
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem("cue.deskPanelHidden")))).toContain("item");
  // The Dispatcher is another kind: its panel still shows.
  await page.locator(".dk-back").click();
  await expect(page.locator('.dk-right .dk-pg[data-desk-page="dispatcher"]')).not.toHaveClass(/\bhid\b/);
  // Another item: still folded; Show panel brings it back.
  await page.locator('[data-desk-item="d2"]').click();
  await expect(pg).toHaveClass(/\bhid\b/);
  await pg.locator(".dk-ptog").click();
  await expect(pg).not.toHaveClass(/\bhid\b/);
  await expect(pg.locator(".dk-ptog")).toHaveText("Hide panel");
  // A narrow card folds the panel by itself; its toggle goes and the NOW buttons stay.
  await page.setViewportSize({ width: 1280, height: 800 });
  await expect(pg.locator(".dk-ptog")).toBeHidden();
  await expect(pg.locator(".dk-pnow button, .dk-pnow .btn").first()).toBeVisible();
});

test("a stopped item's row, open or not, keeps its full-size icon and its kind, number and age under the title (#218)", async ({ cue, page }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await expect(page.locator('.dk-items [data-desk-item="d3"]')).toBeVisible();
  await page.evaluate(() => {
    for (const d of state.desk?.projects || []) for (const i of d.items || []) if (i.id === "d3") { i.stopped = true; i.title = 'Desk header: drop "Delete Desk…", show the desk icon instead of the word "Desk", line up the name and sessions line'; }
    renderMain();
  });
  const row = page.locator('.dk-items [data-desk-item="d3"]');
  const shape = () => row.evaluate((el) => {
    const si = el.querySelector(".dk-si").getBoundingClientRect(), i1 = el.querySelector(".dk-i1").getBoundingClientRect(), i2 = el.querySelector(".dk-i2").getBoundingClientRect();
    return { icon: [si.width, si.height], lines: Math.round(i1.height / parseFloat(getComputedStyle(el.querySelector(".dk-i1")).lineHeight)), under: i2.top >= i1.bottom - 1, sameLeft: Math.abs(i2.left - i1.left) < 1 };
  });
  await expect(row).toHaveClass(/dk-stop/);
  expect(await shape()).toEqual({ icon: [14, 14], lines: 3, under: true, sameLeft: true });
  await row.click();
  await expect(row).toHaveClass(/\bon\b/);
  expect(await shape()).toEqual({ icon: [14, 14], lines: 3, under: true, sameLeft: true });
});

test("the Desk list's › sits only on the open row or card and moves with it, in both themes (#219)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const left = page.locator(".dk-sessions");
  const arrows = left.locator(".dk-larrow, .dk-go");
  const owner = () => arrows.evaluateAll((els) => els.map((el) => { const r = el.closest(".dk-srow, .dk-lcard, .dk-holder"); return [r.dataset.deskLeadChat || r.dataset.deskChatOf || r.dataset.deskHolder || r.dataset.deskAddon, r.classList.contains("on")]; }));
  for (const scheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: scheme });
    // The lead's chat shows: the Run by card alone has it.
    await expect(left.locator('[data-desk-lead-chat="lead"]')).toHaveClass(/\bon\b/);
    expect(await owner()).toEqual([["lead", true]]);
    // Open another session: the › goes with it, none on the Run by card.
    await left.locator('[data-desk-chat-of="host"]').click();
    await expect(left.locator('[data-desk-chat-of="host"]')).toHaveClass(/\bon\b/);
    expect(await owner()).toEqual([["host", true]]);
    await expect(left.locator('[data-desk-lead-chat="lead"] .dk-larrow')).toHaveCount(0);
    // Back to the lead's: back on its card.
    await page.keyboard.press("Escape");
    await expect(left.locator('[data-desk-lead-chat="lead"]')).toHaveClass(/\bon\b/);
    expect(await owner()).toEqual([["lead", true]]);
  }
});

test("the Desk's session list: one hairline above each session but a group's first; crew and N done stay in its block; none at the end, open or not (#225)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const base = st.sessions.find((x) => x.session_id === "site");
    const m = (name, state, extra = {}) => ({ also: [], ended_ms: 0, files: [], summary: "", state, agent_id: `id-${name}`, call: `c-${name}`, name, what: "Check the build", asked: "", started_ms: 1793983560000, ...extra });
    const add = (id, team, since) => st.sessions.push({ ...base, session_id: id, name: id, project: "cue", thread: [], state: "idle", doing: "", team, since_ms: since });
    add("s-a", [m("a1", "committed")], 9);
    add("s-b", [], 8);
    add("s-c", [m("c-live", "reviewed", { uncommitted: true })], 7);
    add("s-d", [m("d-live", "reviewed", { uncommitted: true }), m("d1", "committed"), m("d2", "committed")], 6);
    add("s-e", [], 5);
    st.desk.projects.find((d) => d.project === "cue").members.push("s-a", "s-b", "s-c", "s-d", "s-e");
  });
  const left = page.locator(".dk-sessions");
  await expect(left.locator('[data-desk-chat-of="s-e"]')).toHaveCount(1);
  // Each child of the list: what it is, and whether a line is drawn on it (above it).
  const lines = () => left.locator(".dk-scroll > *").evaluateAll((els) => els.map((el) => {
    const what = el.matches(".dk-gh") ? "#" : el.matches(".dk-sfold") ? "fold" : el.matches(".crew") ? "crew" : el.dataset.deskChatOf;
    const cs = getComputedStyle(el), line = cs.boxShadow !== "none" || parseFloat(cs.borderTopWidth) > 0 || parseFloat(cs.borderBottomWidth) > 0;
    return `${what}${line ? " —" : ""}`;
  }));
  const want = ["#", "host", "#", "s-a", "fold", "s-b —", "s-c —", "crew", "s-d —", "crew", "fold", "s-e —"];
  for (const scheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: scheme });
    expect(await lines()).toEqual(want);
    // Opening a session leaves the lines as they were, above it and under it.
    await left.locator('[data-desk-chat-of="s-c"]').click();
    await expect(left.locator('[data-desk-chat-of="s-c"]')).toHaveClass(/\bon\b/);
    expect(await lines()).toEqual(want);
    await page.keyboard.press("Escape");
  }
  // No line closing the list: the last row draws none under it.
  expect(await left.locator(".dk-scroll > :last-child").evaluate((el) => [getComputedStyle(el).borderBottomWidth, getComputedStyle(el).boxShadow.includes("-1px")])).toEqual(["0px", false]);
});
