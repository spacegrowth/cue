// The Desk (ui/desk/desk.js, ui/desk/lead.js): its items list, an item opened in the middle, the lead's chat
// and Decisions, Desk home and the Desk switcher, + New, who runs the Desk, and Give to….
import { test, expect, pick, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";
import { listOrder, openSessions, openMore, openDone, statusOf } from "./desk.mjs";

test("the Desk is in the header after Board, and lists Waiting on you, then a group a holder", async ({ page, cue }) => {
  await cue.open("desk");
  const views = await page.locator(".switch-view button").evaluateAll((els) => els.map((el) => el.dataset.view));
  expect(views.slice(0, 2)).toEqual(["board", "desk"]);
  await openDesk(page);
  // Items: WORKING over WAITING ON YOU (its ask, then what's ready to check), then UP NEXT and DONE TODAY (#748; Working
  // above Waiting on you since the live Desk work).
  expect(await listOrder(page)).toEqual(["# WORKING", "d3", "d5", "# WAITING ON YOU", "d1", "d2", "# UP NEXT", "d4", "d6", "# DONE TODAY", "d7", "d8", "d9"]);
  await expect(page.locator('.dkn-items .dkn-gh[data-desk2b-group="you"] > span:not(.dkn-fa)')).toHaveText("2");
  // Grouped by state only: no head a session (the sessions are on the left).
  await expect(page.locator(".dkn-items .dk-holder")).toHaveCount(0);
  // One row each: #id, its title, where it stands (no session names on Up next's rows, #666).
  const cells = (id) => page.locator(`[data-desk-item="${id}"]`).evaluate((el) => [".dkn-id", ".dkn-tx", ".dkn-a2 > span:last-child"].map((c) => el.querySelector(c)?.textContent));
  expect(await cells("d4")).toEqual(["#d4", "Rail dot blinks for a reply you didn't send", "After #d3"]);
  expect(await cells("d6")).toEqual(["#d6", "Tidy the Settings copy", "Couldn't give it out · No session in this project could take it."]);
  await expect(page.locator('[data-desk-item="d3"] .dkn-tt')).toHaveText("Search stays centred in the header");
  await expect(page.locator('[data-desk-item="d3"] .dkn-id')).toHaveText("#d3");
  // Each waiting row says what to do in a word: the ask, the one to check.
  await expect(statusOf(page, "d1")).toHaveText("Asks you");
  await expect(page.locator('[data-desk-item="d1"]')).toHaveClass(/\bask\b/);
  await expect(statusOf(page, "d2")).toHaveText("Ready to check");
  // Done today: each with the time it finished.
  await openDone(page);
  await expect(statusOf(page, "d7")).toHaveText(/^landed \d{1,2}(:\d\d)?(am|pm)$/);
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
  // The lead's box, in its chat card (the sessions list takes that place once open, #666). The lead is at work in this
  // scenario: its box says what a message does now, as a session's box on the Board does (#688).
  await expect(page.locator(".dk2b-chat .foot:not(.held) .composer textarea")).toHaveAttribute("placeholder", "Message web-lead… Cue keeps it and sends it when this turn ends");
  // The Sessions head (All N sessions, on the right in the list's place, #666): the Desk's name, ✎ only on hover; no
  // "Change" anywhere.
  await openSessions(page);
  await expect(page.locator(".dk-sh b")).toHaveText("cue");
  await expect(page.locator(".dk-sh .dk-ren")).toHaveCSS("opacity", "0");
  await page.locator(".dk-sh").hover();
  await expect(page.locator(".dk-sh .dk-ren")).toHaveCSS("opacity", "1");
  // No Change for the lead (⋯ hands it over): the one Change is the settings line's, at the end of the counts' line (#417, #415).
  await expect(page.locator(".dk-board button:not(.dk-ab-change)", { hasText: "Change" })).toHaveCount(0);
  await expect(page.locator(".dk-strip .dk-ab")).toHaveCount(1);
  await expect(page.locator(".dk-strip .dk-sh > .dk-ab .dk-ab-change")).toHaveText("Change");
  await expect(page.locator("[data-desk-new]")).toHaveCount(0);
});

test("an item opens in the middle, its row lit in place in the Items list; ‹ Items goes back (#418, #748)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const before = await listOrder(page);
  // In view first: a row partly under the window's edge is scrolled into view as it opens.
  await page.locator('[data-desk-item="d4"]').scrollIntoViewIfNeeded();
  const top = await page.locator('[data-desk-item="d4"]').boundingBox();
  await page.locator('[data-desk-item="d4"]').click();
  // The lead holds it (next after #d3): the lead's session view in the middle, the item's bar on it; the Items list
  // stays, its row lit in place.
  const pane = page.locator('.dk-c > .dk-right .dkn-sp[data-desk-page="session"]');
  await expect(pane).toBeVisible();
  await expect(page.locator(".dk-c > .dkn-items")).toHaveCount(1);
  await expect(page.locator('.dkn-items [data-desk-item="d4"]')).toHaveClass(/\bon\b/);
  expect(await listOrder(page)).toEqual(before);
  await expect(page.locator(".dkn-items .dk-it.on")).toHaveCount(1);
  // Back in the list (‹ Items N): it's as it was, the row where it was. Then the item again.
  await page.locator(".dk-right .dk-back").click();
  expect(await listOrder(page)).toEqual(before);
  expect((await page.locator('[data-desk-item="d4"]').boundingBox()).y).toBe(top.y);
  await page.locator('[data-desk-item="d4"]').click();
  await expect(pane).toBeVisible();
  await expect(pane.locator(".dkn-smeta")).toHaveText("on #d4 · Rail dot blinks for a reply you didn't send");
  await expect(page.locator('.dk-right [data-desk-mark]')).toHaveCount(0);
  await expect(page.locator(".dk-right .dk-markrow")).toHaveCount(0);
  // Not started yet (#626): its card names the item it waits for, in place of the chain.
  await expect(pane.locator(".dkn-ib .dk-ns .dk-ns-t")).toHaveText("It waits for Search stays centred in the header.");
  // The box starts with who gets it and the item, kept as you type (#132); ready to check, it goes back to its holder,
  // and the prefix follows it. The empty box shows the item's short title.
  const ph = page.locator(".dk-right .foot:not(.held) .composer textarea"), pre = page.locator(".dk-right .foot:not(.held) .composer .field-to");
  await expect(pre).toHaveText("To web-lead · Bug #d4");
  await expect(ph).toHaveAttribute("placeholder", "Rail dot blinks…");
  // A tag filled in the box's own tint (--panel-2), at the left of where you type, in the same row.
  const tint = await pre.evaluate((el) => { const t = document.createElement("i"); t.style.background = "var(--panel-2)"; el.parentElement.append(t); const c = getComputedStyle(t).backgroundColor; t.remove(); return c; });
  expect(tint).not.toBe("rgba(0, 0, 0, 0)");
  await expect(pre).toHaveCSS("background-color", tint);
  await ph.fill("still blinks");
  await expect(pre).toHaveText("To web-lead · Bug #d4");
  const tag = await pre.boundingBox(), input = await ph.boundingBox();
  const row = await page.locator(".dk-right .foot:not(.held) .composer .field-row").boundingBox();
  expect(tag.x + tag.width).toBeLessThanOrEqual(input.x);
  expect(tag.x).toBeLessThan(input.x);
  expect(tag.y).toBeLessThan(input.y + input.height);
  expect(tag.y + tag.height).toBeGreaterThan(input.y);
  expect(tag.x).toBeGreaterThanOrEqual(row.x - 8);
  await ph.fill("");
  await pre.click();
  await expect(ph).toBeFocused();
  await cue.change((st) => Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4"), { state: "to_check", holder: { kind: "session", sid: "s-new", name: "cue-117" } }));
  await expect(pre).toHaveText("To cue-117 · Bug #d4");
  await cue.change((st) => Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4"), { state: "queued", holder: { kind: "lead", sid: "lead", name: "web-lead" } }));
  // Put it before the one it waits on: in its bar's ⋯ (#669).
  const bar = page.locator(".dk-right .dkn-ib");
  await openMore(bar);
  await page.locator('.dk-right [data-desk-act="put_before"]').click();
  await cue.called("desk_item_act", { id: "d4", act: "put_before" });
  // An item no session has opens its own view: its header names it, kind and number, then the title (#134); no kind
  // pill over it (#144).
  await page.locator('[data-desk-item="d6"]').click();
  const bare = page.locator('.dk-c > .dk-right .dkn-sp[data-desk-page="item"]');
  await expect(bare.locator(".dkn-ih1 .dkn-k")).toHaveAttribute("aria-label", "task");
  await expect(bare.locator(".dkn-ih1")).toHaveText("#d6 Tidy the Settings copy");
  await expect(page.locator('.dk-right [data-desk-mark]')).toHaveCount(0);
  // Back is ‹ Items N: the lead's chat is back, no row lit.
  await expect(page.locator(".dk-right .dk-back")).toHaveText(/^‹ Items \d+$/);
  await page.locator(".dk-right .dk-back").click();
  await expect(page.locator(".dk-chat")).toBeVisible();
  await expect(page.locator(".dk-it.on")).toHaveCount(0);
});

test("written about an item no session holds, the box goes to the lead naming it (one held goes to its holder, #276)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("The More menu is cut off");
  await page.locator(".dk-right .foot:not(.held) .composer textarea").press("Enter");
  await cue.called("desk_send", { project: "cue", text: "About #d6 (Tidy the Settings copy): The More menu is cut off" });
  // The item stays open, its box empty; the lead's chat has it when you go back.
  await expect(page.locator(".toast").last()).toContainText("Sent to web-lead");
  await expect(page.locator(".dk-right .dkn-ih1")).toHaveText("#d6 Tidy the Settings copy");
  await expect(page.locator(".dk-right .foot:not(.held) .composer textarea")).toHaveValue("");
  await page.locator(".dk-right .dk-back").click();
  await expect(page.locator(".dk-chat")).toContainText("The More menu is cut off");
  // The lead has it (after the Desk's summary Cue put before it): no Sending left, and no summary shown.
  await expect(page.locator(".dk-chat .msg-sending")).toHaveCount(0);
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
  // Waiting on you says what for (#410): your reply, or its open question.
  await cue.change((st) => { Object.assign(st.sessions.find((x) => x.session_id === "lead"), { state: "waiting", since_ms: Date.now() - 60000 }); });
  await expect(live.locator(".cv-what")).toHaveText("Waiting on you for your answer: Keep focus in the box, or move to the next item?");
  await cue.change((st) => { st.items = st.items.filter((i) => !(i.session_id === "lead" && i.status === "pending")); });
  await expect(live.locator(".cv-what")).toHaveText("Waiting on you for your reply");
});

test("a send from an item that doesn't go keeps the item open; the lead's chat has it red with why, and the board's Send again (#422, #688)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  await cue.override("desk_send", () => { throw "offline"; });
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("The More menu is cut off");
  await page.locator(".dk-right .foot:not(.held) .composer textarea").press("Enter");
  await expect(page.locator(".dk-right .dkn-ih1")).toHaveText("#d6 Tidy the Settings copy");
  await expect(page.locator(".dk-right .foot:not(.held) .composer textarea")).toHaveValue("");
  await page.locator(".dk-right .dk-back").click();
  const mine = page.locator(".dk-chat .cv-you.msg-failed", { hasText: "The More menu is cut off" });
  await expect(mine.locator(".msg-line")).toHaveText("! Couldn't send · offline · Send again");
  await expect(mine.locator(".dk-about")).toHaveText("Task #d6 · Tidy the Settings copy");
  // Send again: the same send again, by the same path.
  await cue.override("desk_send", () => ({ sid: "lead" }));
  await mine.locator('[data-desk-act="lead_retry"]').click();
  await cue.called("desk_send", { project: "cue", text: "About #d6 (Tidy the Settings copy): The More menu is cut off" });
  await expect(page.locator(".dk-chat .msg-failed")).toHaveCount(0);
});

test("an item's message that didn't go has no Edit, only Send again; the lead's chat shows it red with Send again (#422, #688)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  await cue.override("desk_send", () => { throw "offline"; });
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("The More menu is cut off");
  await page.locator(".dk-right .foot:not(.held) .composer textarea").press("Enter");
  await expect(page.locator(".dk-right .foot:not(.held) .composer textarea")).toHaveValue("");
  // To the lead on its own: red in its chat, and Send again sends it again.
  await page.evaluate(() => { deskOpen = null; deskTab = "lead"; renderMain(); });
  const mine = page.locator(".dk-chat .cv-you.msg-failed", { hasText: "The More menu is cut off" });
  await expect(mine.locator(".msg-line")).toHaveText("! Couldn't send · offline · Send again");
  await expect(mine.locator('[data-desk-act="msg_edit"]')).toHaveCount(0);
  await page.locator('.dk-right textarea[data-text^="desk:"]').fill("Footer too");
  await page.locator('.dk-right textarea[data-text^="desk:"]').press("Enter");
  const bad = page.locator(".dk-chat .cv-you.msg-failed", { hasText: "Footer too" });
  await expect(bad.locator(".msg-line")).toHaveText("! Couldn't send · offline · Send again");
  await cue.override("desk_send", () => ({ sid: "lead" }));
  await bad.locator('[data-desk-act="lead_retry"]').click();
  await cue.called("desk_send", { project: "cue", text: "Footer too" });
  // Gone red; the item's message, never sent again, still offers Send again.
  await expect(page.locator(".dk-chat .msg-failed", { hasText: "Footer too" })).toHaveCount(0);
  await expect(page.locator(".dk-chat .msg-failed", { hasText: "The More menu is cut off" })).toHaveCount(1);
});

test("the lead's own ASK stands in the box's place: its choices, Type answer…; answered, the box is back", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Typed to the lead before it asked: kept, and shown held under its card.
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("And the footer too");
  await cue.change((st) => {
    st.sessions.find((s) => s.session_id === "lead").state = "deciding";
    st.items.push({ ...st.items[0], id: "n-lead-ask", agent_id: "", agent_name: "", desk: true, hosted: false,
      tool_input: { questions: [{ question: "Add a rename right on the Desk's line?", header: "Desk", multiSelect: false, options: [{ label: "Yes, add it there", description: "" }, { label: "No, the chat's is enough", description: "" }] }] } });
  });
  const ask = page.locator(".dk-right .dk-ask");
  await expect(ask).toContainText("Add a rename right on the Desk's line?");
  await expect(page.locator(".dk-right :is(.dk-box, .foot:not(.held) .composer)")).toHaveCount(0);
  await expect(ask.locator(".foot.held textarea")).toHaveValue("And the footer too");
  // Your own words: Type answer… opens a box for them.
  await ask.locator('[data-act="redirect"]').click();
  await expect(ask.locator('textarea[data-text="n-lead-ask"]')).toBeVisible();
  await page.keyboard.press("Escape");
  await ask.locator("[data-pick]", { hasText: "Yes, add it there" }).click();
  await ask.locator('[data-act="submit-answers"]').click();
  const call = await cue.called("respond");
  expect(call.args.id).toBe("n-lead-ask");
  await cue.change((st) => { st.sessions.find((s) => s.session_id === "lead").state = "working"; });
  await expect(page.locator(".dk-right .dk-ask")).toHaveCount(0);
  // The lead at work: its box says what a message does now, as a session's box on the Board does (#688).
  await expect(page.locator(".dk-right .foot:not(.held) .composer textarea")).toHaveAttribute("placeholder", "Message web-lead… Cue keeps it and sends it when this turn ends");
  await expect(page.locator(".dk-right .foot:not(.held) .composer textarea")).toHaveValue("And the footer too");
});

test("the lead's question is the option C card: who asks and when, a line per choice, its suggestion picked, Decline and one Send, no summary box (#450)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    st.sessions.find((s) => s.session_id === "lead").state = "deciding";
    st.items.push({ ...st.items[0], id: "n-lead-q", agent_id: "", agent_name: "", desk: true, hosted: false, created_ms: Date.now() - 4 * 60_000,
      tool_input: { questions: [{ question: "Header or footer first?", header: "Desk", multiSelect: false, options: [
        { label: "Header (Recommended)", description: "It's what you see first." },
        { label: "Footer", description: "Smaller; done today." }] }] } });
  });
  const ask = page.locator(".dk-right .dk-ask");
  await expect(ask.locator(".act-head")).toHaveText("web-lead asks you · 4m");
  await expect(ask.locator(".qc-sn")).toHaveCount(0);
  await expect(ask).not.toContainText("Claude Code asks");
  await expect(ask.locator(".qc-opt .qc-d")).toHaveText(["It's what you see first.", "Smaller; done today."]);
  const rec = ask.locator(".qc-opt.rec");
  await expect(rec).toHaveCount(1);
  await expect(rec.locator(".qc-lt")).toHaveText("Header");
  await expect(rec.locator(".qc-rec")).toHaveText("Recommended");
  await expect(rec).toHaveAttribute("aria-pressed", "true");
  await expect(ask.locator(".qc-sum")).toHaveCount(0);
  await expect(ask).not.toContainText("If you send this");
  await expect(ask.locator('[data-act="deny"]')).toHaveText("Decline");
  await expect(ask.locator('[data-act="submit-answers"]')).toHaveCount(1);
  await expect(ask.locator(".act-card [title]")).toHaveCount(0);
  await ask.locator('[data-act="submit-answers"]').click();
  const call = await cue.called("respond", { id: "n-lead-q" });
  expect(call.args.decision.answers).toEqual({ "Header or footer first?": "Header (Recommended)" });
});

test("a lead opens with its crew; a crew member with where it is, and the box goes to the lead", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  const pane = page.locator(".dk-right .dk-pg");
  await expect(pane.locator("h1 .dk-tname")).toHaveText("web-lead");
  await expect(pane.locator(".dk-crew .dk-mb")).toHaveCount(2);
  await expect(page.locator(".dk-right .foot:not(.held) .composer textarea")).toHaveAttribute("placeholder", "Talk to web-lead…");
  await pane.locator('[data-desk-crew="id-search"]').click();
  // Its own page (no panel): its name, whose crew and the item it's on, Chat · Files · Activity tabs.
  const crew = page.locator('.dk-right .dkn-crew[data-desk-page="crew"]');
  await expect(crew.locator(".dkn-cname")).toHaveText("search-centre");
  // Its page took the Items list's place (#666): it names the item it's on, where the list's lit row did.
  await expect(crew.locator(".dkn-smeta")).toContainText("Search stays centred in the header");
  await expect(crew.locator('[data-mt-act="stop"]')).toBeVisible();
  // Its page is its chat (#689): what web-lead asked it first; with no answer yet, none shown. Open its session waits
  // behind ⋯ and a click outside closes it.
  await expect(crew.locator(".dk-crewpg .ag-ask")).toContainText("web-lead asked it");
  await expect(crew.locator(".dk-crewpg .ag-ask")).toContainText("Keep the search field centred unless it would cover a button.");
  await expect(crew.locator(".dk-crewpg .bg-view-reply:not(.ag-ask)")).toHaveCount(0);
  await expect(crew.locator("[data-desk-crew-open]")).toHaveCount(0);
  await crew.locator("[data-desk-crew-more]").click();
  await expect(crew.locator('[data-desk-crew-open="id-search"]')).toBeVisible();
  await crew.locator(".dkn-cname").click();
  await expect(crew.locator("[data-desk-crew-open]")).toHaveCount(0);
  // What it changed, in its Files tab.
  await crew.locator('[data-desk-c-stab="files"]').click();
  await expect(crew.locator(".dk-pfiles .ib-file code")).toHaveText(["ui/header.js", "ui/styles/header.css"]);
  await crew.locator('[data-desk-c-stab="chat"]').click();
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("Leave the Inbox button alone");
  await page.locator(".dk-right .foot:not(.held) .composer textarea").press("Enter");
  await cue.called("send_to_session", { sessionId: "lead", text: "About search-centre: Leave the Inbox button alone" });
  // ‹ web-lead goes back to the lead.
  await page.locator(".dk-right .dk-back").click();
  await expect(pane.locator("h1 .dk-tname")).toHaveText("web-lead");
});

test("a crew member's page shows its whole chat, the same as the board's view of it: the ask, its words, its steps folded a run at a time, its answer (#689)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // The lead's feed has search-centre's log: words, two runs of steps, and its answer once it finished.
  await cue.override("session_steps", ({ sessionId }, now) => {
    if (sessionId !== "lead") return { version: 0, turns: [] };
    const step = (id, kind, verb, subject, m) => ({ t: "step", id, kind, verb, subject, mono: true, result: "", bad: false, done: true, at_ms: now - m * 60000 });
    const say = (text, m) => ({ t: "say", text, at_ms: now - m * 60000 });
    return { version: 7, turns: [], helpers: [{ id: "id-search", desc: "search-centre", kind: "general-purpose", model: "", teammate: true, name: "search-centre", step: "", started_ms: now - 9 * 60000, finished_ms: now - 60000, last_ms: now - 60000, tokens: 0, running: false, more: 0,
      items: [say("Looking at how the header lays out its buttons.", 8), step("s1", "read", "Read", "ui/header.js", 7.5), step("s2", "search", "Searched", "search-box", 7), step("s3", "read", "Read", "ui/styles/header.css", 6.5),
        say("The field is pushed left by the Inbox button.", 5), step("s4", "edit", "Edited", "ui/styles/header.css", 4), step("s5", "run", "Ran", "npm test", 3),
        say("Done: the search field stays centred; it moves only when a button would sit under it.", 1)] }] };
  }, Date.now());
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  await page.locator('.dk-right .dk-pg [data-desk-crew="id-search"]').click();
  const chat = page.locator('.dk-right .dkn-crew[data-desk-page="crew"] .dk-crewpg .ag-chat');
  await expect(chat.locator(".stx-fold")).toHaveCount(2);
  // In order: the lead's ask, its words, a fold line per run of steps, its answer at the end.
  const parts = () => chat.evaluate((el) => [...el.children].map((c) => c.classList.contains("ag-ask") ? "ask" : c.classList.contains("stx-say") ? "says" : c.classList.contains("stx") ? "steps" : c.classList.contains("bg-view-reply") ? "answer" : c.className));
  expect(await parts()).toEqual(["ask", "says", "steps", "says", "steps", "answer"]);
  await expect(chat.locator(".ag-ask")).toContainText("Keep the search field centred");
  await expect(chat.locator(".stx-fold b")).toHaveText(["3 steps", "2 steps"]);
  await expect(chat.locator(".stx-line")).toHaveCount(0);
  await expect(chat.locator(".bg-view-reply").last()).toContainText("Its answer");
  await expect(chat.locator(".bg-view-reply").last()).toContainText("the search field stays centred");
  // A fold line opens its run, and closes it again.
  await chat.locator(".stx-fold").first().click();
  await expect(chat.locator(".stx-line")).toHaveCount(3);
  await expect(chat.locator(".stx-line").first()).toContainText("ui/header.js");
  await chat.locator(".stx-fold").first().click();
  await expect(chat.locator(".stx-line")).toHaveCount(0);
  // The board's view of it (its crew row's Its steps) draws the very same chat.
  const desk = await chat.evaluate((el) => el.outerHTML);
  const board = await page.evaluate(() => { const b = document.createElement("div"); b.innerHTML = helperPane("lead", helpersOf("lead").find((h) => h.id === "id-search")).body; return b.querySelector(".ag-chat").outerHTML; });
  expect(board).toBe(desk);
});

test("a lead's long brief opens folded to its first sentence; opened, its steps and rules are lists and a long command has a box of its own (#805)", async ({ page, cue }) => {
  const brief = "You're fixing the search field in the header of the Cue repo copy at /Users/someone/development/cue/.claude/worktrees/search-1. Work only there.\n\n"
    + "Steps: (1) read `ui/header.js` and its styles (2) keep the field centred unless a button would sit under it (3) run `npx playwright test -c tests/ui/playwright.config.mjs tests/ui/specs/header.spec.mjs` once at the end.\n\n"
    + "Rules: don't commit; no title= tooltips; touch only ui/header.js and ui/styles/header.css.\n\n"
    + "Report: what you changed. What you left out.";
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st, b) => { st.sessions.find((s) => s.session_id === "lead").team.find((t) => t.agent_id === "id-search").asked = b; }, brief);
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  await page.locator('.dk-right .dk-pg [data-desk-crew="id-search"]').click();
  const ask = page.locator('.dk-right .dkn-crew[data-desk-page="crew"] .dk-crewpg .ag-ask');
  // Folded: its first sentence, plain, and Show the full brief.
  await expect(ask.locator(".ag-brief-line")).toHaveText("You're fixing the search field in the header of the Cue repo copy at /Users/someone/development/cue/.claude/worktrees/search-1.");
  await expect(ask).not.toContainText("Steps");
  await expect(ask.locator("[data-agent-brief]")).toHaveText("Show the full brief");
  await expect(ask.locator("[title]")).toHaveCount(0);
  await ask.locator("[data-agent-brief]").click();
  // Opened: the brief whole, its words as they were, set out as a short document.
  const doc = ask.locator(".ag-brief-md");
  await expect(doc.locator("h4")).toHaveText(["Steps:", "Rules:", "Report:"]);
  await expect(doc.locator("ol > li")).toHaveText(["read ui/header.js and its styles", "keep the field centred unless a button would sit under it", "run npx playwright test -c tests/ui/playwright.config.mjs tests/ui/specs/header.spec.mjs once at the end."]);
  await expect(doc.locator("ul").first().locator("li")).toHaveText(["don't commit", "no title= tooltips", "touch only ui/header.js and ui/styles/header.css."]);
  await expect(doc.locator("ul").last().locator("li")).toHaveText(["what you changed.", "What you left out."]);
  await expect(doc.locator("code.brief-long")).toHaveText("npx playwright test -c tests/ui/playwright.config.mjs tests/ui/specs/header.spec.mjs");
  await expect(doc.locator("code:not(.brief-long)").first()).toHaveText("ui/header.js");
  // Nothing runs off the side; it folds again.
  expect(await doc.evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
  await ask.locator("[data-agent-brief]").click();
  await expect(ask.locator(".ag-brief-line")).toBeVisible();
  await expect(ask.locator(".ag-brief-md")).toHaveCount(0);
});

test("a crew member's words show plain, its files without scratch or node_modules, once in its lead's list (#197)", async ({ page, cue }) => {
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
  const crew = page.locator('.dk-right .dkn-crew[data-desk-page="crew"]');
  await expect(crew).toContainText("its lead's Claude process");
  await expect(crew).not.toContainText("&#39;");
  // Its Files tab: its own files, without scratch or node_modules.
  await crew.locator('[data-desk-c-stab="files"]').click();
  await expect(crew.locator(".dk-pfiles .ib-file code")).toHaveText(["ui/header.js", "ui/styles/header.css"]);
  // ‹ from the crew member's page goes to its lead's, ‹ from there to the list.
  await page.locator(".dk-right .dk-back").click();
  await expect(page.locator('.dk-right .dk-pg [data-desk-crew="id-search"]')).toHaveCount(1);
  await page.locator(".dk-right .dk-back").click();
  await expect(page.locator('.dkn-items [data-desk-item="d5"]')).toHaveCount(1);
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

test("Decisions: key calls only, housekeeping behind Show all; tick several and Go back on these with your answer", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.decisions.push({ project: "cue", id: "c4", call: true, minor: true, undo: "can", by: "web-lead", sid: "lead", text: "Put the notes in docs/dev", at: d.decisions[0].at });
  });
  await openDesk(page);
  await page.locator('[data-desk-tab="decisions"]').click();
  const right = page.locator(".dk-right");
  // The housekeeping call isn't counted or listed until Show all.
  await expect(page.locator(".dk-tab.on")).toContainText("Decisions2");
  await expect(right.locator(".dk-dec")).toHaveCount(2);
  await expect(right).not.toContainText("Put the notes in docs/dev");
  await right.locator(".dk-showall").click();
  await expect(right.locator(".dk-dec")).toHaveCount(3);
  await expect(right.locator(".dk-showall")).toHaveText("Key decisions only");
  await right.locator(".dk-showall").click();
  await expect(right.locator(".dk-showall")).toHaveText("Show all · 1 more");
  // Tick two; Go back on these asks for your answer, and each goes back with it.
  await expect(right.locator(".dk-gb")).toHaveCount(0);
  await right.locator('[data-desk-tick="c1"]').click();
  await expect(right.locator('[data-desk-tick="c1"]')).toHaveAttribute("aria-checked", "true");
  await right.locator('[data-desk-tick="c2"]').click();
  await expect(right.locator('[data-desk-go-back="desk:cue"]')).toHaveText("Go back on these (2)");
  await right.locator('[data-desk-go-back="desk:cue"]').click();
  const box = right.locator('textarea[data-text="goback"]');
  await expect(box).toBeFocused();
  await right.locator('[data-desk-go-back="send"]').click();
  await expect(page.locator(".toast")).toHaveText("Say what should happen instead first");
  await box.fill("Keep the settings file and focus on a timer");
  await right.locator('[data-desk-go-back="send"]').click();
  await cue.called("desk_undo", { decisionId: "c1", answer: "Keep the settings file and focus on a timer" });
  await cue.called("desk_undo", { decisionId: "c2", answer: "Keep the settings file and focus on a timer" });
  await expect(page.locator(".toast")).toHaveText("2 sent back with your answer");
  await expect(right.locator(".dk-gb, .dk-gb-box")).toHaveCount(0);
  await expect(right.locator(".dk-dec").first()).toContainText("Undo asked");
});

test("Decisions (#420): a call on an item you've checked is settled, behind Show all; the lead's routing calls are Dispatcher Actions", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue"), at = d.decisions[0].at;
    d.decisions.push({ project: "cue", id: "c5", item_id: "d7", call: true, undo: "can", by: "web", sid: "web", text: "The count shows as a tint, not a number", at });
    d.decisions.push({ project: "cue", id: "c6", call: true, minor: true, act: "call", undo: "can", by: "web-lead", sid: "lead", text: "Away mode mocks go to web", why: "it made the last ones", at });
  });
  await openDesk(page);
  // The lead's chat doesn't draw its own DECIDED line a second time as a move.
  await expect(page.locator(".dk-chat .dk-act")).not.toContainText(["Away mode mocks go to web"]);
  await page.locator('[data-desk-tab="decisions"]').click();
  const right = page.locator(".dk-right");
  await expect(page.locator(".dk-tab.on")).toContainText("Decisions2");
  await expect(right.locator(".dk-dec")).toHaveCount(2);
  await expect(right).not.toContainText("The count shows as a tint");
  await expect(right.locator(".dk-showall")).toHaveText("Show all · 2 more");
  await right.locator(".dk-showall").click();
  await expect(right).toContainText("The count shows as a tint");
  await right.locator('.dk-tab[data-desk-tab="moves"]').click();
  await expect(right.locator(".dk-moves .dk-mv").first()).toContainText("Away mode mocks go to web · it made the last ones");
});

test("an item page's back link is one line above its title", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  const back = await page.locator(".dk-right .dk-back").boundingBox(), h1 = await page.locator(".dk-right .dkn-ih1").boundingBox();
  expect(back.height).toBeLessThan(32);
  expect(back.width).toBeGreaterThan(40);
  expect(back.y + back.height).toBeLessThanOrEqual(h1.y + 1);
});

test("the Desk tab opens a Desk, never the list: the one last open, else Desk home's first", async ({ page, cue }) => {
  await cue.open("desk");
  const tab = page.locator('.switch-view [data-view="desk"]'), board = page.locator('.switch-view [data-view="board"]');
  await tab.click();
  await expect(page.locator(".dk-desks")).toHaveCount(0);
  await expect(page.locator(".dk-items")).toHaveCount(1);
  // Desk home, then into website; away to the Board with All projects, and the Desk tab is back on website.
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top [data-hv-desk-home]").click();
  await page.locator('.dk-desk[data-desk-project="website"]').click();
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top [data-hv-desk-home]").click();
  await board.click();
  await tab.click();
  await expect(page.locator(".dk-desks")).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-it").first()).toHaveAttribute("data-desk-item", "w1");
});

test("Desk home: one row a Desk with what it's about, its people, open tasks, bugs and features, and who's on it; a click opens it", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top [data-hv-desk-home]").click();
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
  for (let k = 0; k < 3; k++) await expect(cells.nth(k)).toHaveText(/^(–|\d+ open(\d+ working|none moving)?( · )?(\d+ need you)?( · )?(\d+ to review)?)$/);
  await expect(cue1.locator(".dh-on b")).toHaveText("web-lead");
  await expect(page.locator(".dh-tbl [title]")).toHaveCount(0);
  // A kind whose only open item is finishing (#181) is moving: "1 finishing", not "none moving".
  await expect(cells.nth(0).locator(".dh-sm")).toHaveText("none moving");
  await cue.change((st) => { Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d6"), { state: "finishing", finish: { step: "building" } }); });
  await expect(cells.nth(0).locator(".dh-sm")).toHaveText("1 finishing");
  await rows.nth(1).click();
  await expect(page.locator(".dk-items .dk-it").first()).toHaveAttribute("data-desk-item", "w1");
  // The name in the Sessions head → All Desks: back to Desk home, then into another.
  await expect(page.locator(".dk-items")).toHaveCount(1);
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top [data-hv-desk-home]").click();
  await expect(rows).toHaveCount(2);
  await rows.first().click();
  await openSessions(page);
  await expect(page.locator(".dk-sh b")).toHaveText("cue");
  await expect(page.locator(".dk-sh button:not(.dk-ab-change)", { hasText: "Change" })).toHaveCount(0);
});

test("the header's Desk chips: the pinned Desks in a fixed order, the open one lit; another is a click away, its name in full and a dot when it waits on you (#464, #666)", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top [data-hv-desk-home]").click();
  await page.locator('.dk-desk[data-desk-project="website"]').click();
  await page.evaluate(() => { state.desk.projects.push({ project: "docs", name: "Docs site", items: [] }); deskRecent = ["website", "docs", "cue"]; renderMain(); });
  const chips = page.locator(".top .hv-chip[data-hv-desk]"), hot = page.locator(".top .hv-chip[data-hv-desk].hot");
  // Nothing pinned yet: the first three Desks by name, never reordered by use (the pins: header-by-view.spec.mjs);
  // the open one lit, the others by their name, one click away. No hover text.
  const order = () => chips.evaluateAll((els) => els.map((e) => e.dataset.hvDesk));
  expect(await order()).toEqual(["cue", "docs", "website"]);
  await expect(hot).toHaveCount(1);
  await expect(hot).toHaveAttribute("data-hv-desk", "website");
  await expect(hot).toHaveAttribute("aria-current", "true");
  const docs = page.locator('.top .hv-chip[data-hv-desk="docs"]');
  await expect(docs.locator(".hv-cn")).toHaveText("Docs site");
  expect(await page.locator(".top .hv-area [title], .top .hv-chip[title]").count()).toBe(0);
  if (process.env.SHOT_DIR) {
    let clip = null;
    await expect.poll(async () => (clip = await page.locator(".top").boundingBox())).not.toBeNull();
    await page.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-recent.png`, clip });
  }
  // One click opens it: it's the Desk open now, lit, and the one you left stays a chip.
  await docs.click();
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("docs");
  await expect(hot).toHaveAttribute("data-hv-desk", "docs");
  await expect(page.locator('.top .hv-chip[data-hv-desk="website"]')).toHaveCount(1);
  expect(await order()).toEqual(["cue", "docs", "website"]);
  // A Desk waiting on you shows a dot on its chip, and says so to a screen reader.
  expect(await page.evaluate(() => hvDeskGoing(deskOf("cue")).waits), "cue waits on you in the scenario").toBeGreaterThan(0);
  const cueChip = page.locator('.top .hv-chip[data-hv-desk="cue"]');
  await expect(cueChip.locator(".hv-dot.ask")).toHaveCount(1);
  await expect(cueChip).toHaveAttribute("aria-label", /^cue, \d+ waits? on you$/);
  await cueChip.click();
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("cue");
  // Opened another way (the Desks menu), a Desk is a chip again.
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator('.top .hv-menu [data-hv-desk="website"]').click();
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("website");
  await expect(hot).toHaveAttribute("data-hv-desk", "website");
  await expect(page.locator(".top .hv-menu")).toHaveCount(0);
});

test("the head's \"N working\" counts a session whose helpers are working (\"agent\"), as the Working group lists it (#402)", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top [data-hv-desk-home]").click();
  await page.locator('.dk-desk[data-desk-project="cue"]').click();
  await openSessions(page);
  const sub = page.locator(".dk-strip .dk-sub");
  const count = async () => Number(((await sub.textContent()).match(/(\d+) working/) || [0, 0])[1]);
  const before = await count();
  // A new session on this Desk, idle itself, with background helpers at work (state "agent").
  await cue.change((st) => {
    st.sessions.push({ session_id: "s-agent", name: "cue-250", project: "cue", cwd: "/Users/me/cue", state: "agent", helpers: 2, since_ms: Date.now() });
    st.desk.projects.find((x) => x.project === "cue").members.push("s-agent");
  });
  const sid = "s-agent";
  expect(sid).toBeTruthy();
  await expect.poll(count).toBe(before + 1);
  await expect(page.locator(".dk-gh", { hasText: "Working" }).locator(".n")).toHaveText(String(before + 1));
  // Its own row says so too, not "idle" (#457).
  const st = page.locator(`.dk-sessions [data-desk-chat-of="${sid}"] .dk-st`).first();
  await expect(st.locator("b")).toHaveText("working · 2 helpers");
  await expect(st.locator("i")).not.toHaveClass(/idle/);
  // Helpers done: idle again, out of the count.
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "s-agent"); s.state = "idle"; s.helpers = 0; });
  await expect(st.locator("b")).toHaveText("idle");
  await expect.poll(count).toBe(before);
});

test("a long Desk name and the next recent Desk's name show as header chips, inside the header and clear of each other, in a narrow window too (#402, #464)", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top [data-hv-desk-home]").click();
  await page.locator('.dk-desk[data-desk-project="cue"]').click();
  const long = "A very long Desk name that would not fit on one line";
  await page.evaluate((long) => {
    state.desk.projects.find((d) => d.project === "cue").name = long;
    for (let k = 1; k <= 6; k++) state.desk.projects.push({ project: `r${k}`, name: `Recent Desk with a long name number ${k}`, items: [] });
    deskRecent = ["cue", "r1", "r2", "r3", "r4", "r5", "r6"];
    renderMain();
  }, long);
  for (const width of [2200, 1000, 760]) {
    await page.setViewportSize({ width, height: 800 });
    // The open Desk and the next one you opened are chips, each one click away; read in one go (a redraw swaps them).
    await expect.poll(() => page.evaluate((long) => {
      const top = document.querySelector(".top")?.getBoundingClientRect();
      const cur = document.querySelector('.top .hv-chip[data-hv-desk="cue"]'), next = document.querySelector('.top .hv-chip[data-hv-desk="r1"]');
      if (!top || !cur || !next) return "no chips";
      if (cur.querySelector(".hv-cn").textContent !== long || next.querySelector(".hv-cn").textContent !== "Recent Desk with a long name number 1") return "names";
      const a = cur.getBoundingClientRect(), b = next.getBoundingClientRect();
      for (const r of [a, b]) if (!(r.width > 0 && r.left >= top.left - 0.5 && r.right <= top.right + 0.5 && r.bottom <= top.bottom + 0.5)) return "chip out of the header";
      if (a.right > b.left + 0.5) return "chips overlap";
      return "ok";
    }, long)).toBe("ok");
    // Wide enough, the names show in full, never cut.
    // (Polled: the header fits itself to the new width on its next frame.)
    if (width === 2200) await expect.poll(() => page.evaluate(() => [...document.querySelectorAll('.top .hv-chip[data-hv-desk="cue"] .hv-cn, .top .hv-chip[data-hv-desk="r1"] .hv-cn')].every((e) => e.scrollWidth <= e.clientWidth + 1))).toBe(true);
    if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-head-${width}.png`, clip: await page.locator(".top").boundingBox() });
  }
  await page.locator('.top .hv-chip[data-hv-desk="r1"]').click();
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("r1");
});

test("the header's Desks menu: every Desk with what it has going, ✓ on the open one, typed to find, ↑ ↓ ↵ to open, All Desks and New Desk… (#240)", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top [data-hv-desk-home]").click();
  await page.locator('.dk-desk[data-desk-project="website"]').click();
  const btn = page.locator('.top [data-hv-menu="desks"]'), menu = page.locator(".top .hv-menu"), find = menu.locator(".hv-find");
  await expect(btn).toHaveAttribute("aria-expanded", "false");
  await expect(menu).toHaveCount(0);
  await btn.click();
  await expect(btn).toHaveAttribute("aria-expanded", "true");
  const rows = menu.locator(".hv-row[data-hv-desk]");
  await expect(rows).toHaveCount(2);
  await expect(menu.locator(".hv-row.on")).toHaveAttribute("data-hv-desk", "website");
  await expect(menu.locator(".hv-row.on .hv-tick")).toHaveText("✓");
  await expect(menu.locator('.hv-row[data-hv-desk="cue"] .hv-tick')).toHaveText("");
  await expect(find).toBeFocused();
  await expect(menu.locator("[data-hv-desk-home]")).toHaveText("All Desks");
  await expect(menu.locator("[data-hv-new-desk]")).toHaveText("New Desk…");
  expect(await page.locator(".top .hv-menu [title]").count()).toBe(0);
  // A row is the name and what it has going: no project or dispatcher name beside it.
  await expect(rows.first().locator(".pdot")).toHaveCount(0);
  await expect(rows.first()).toHaveText(/^✓?(cue|website)(\d+ waits? on you|\d+ working|nothing going)$/);
  // A name too long for its room ends before what it has going.
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").name = "A very long Desk name that runs on and on past the count and further still"; });
  await expect(menu.locator('.hv-row[data-hv-desk="cue"] .hv-rn')).toHaveText(/^A very long Desk name/);
  const fit = await page.evaluate(() => { const r = document.querySelector('.top .hv-menu .hv-row[data-hv-desk="cue"]'), nm = r.querySelector(".hv-rn"), rs = r.querySelector(".hv-rs"); return { right: nm.getBoundingClientRect().right, rs: rs.getBoundingClientRect().left, row: r.getBoundingClientRect().right, rsRight: rs.getBoundingClientRect().right }; });
  expect(fit.right).toBeLessThanOrEqual(fit.rs);
  expect(fit.rsRight).toBeLessThanOrEqual(fit.row + 0.5);
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").name = undefined; });
  // Typing finds; the box keeps the cursor through the redraw.
  await find.focus();
  await page.keyboard.type("cu");
  await expect(rows).toHaveCount(1);
  await expect(find).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(menu).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("cue");
  // Opened again, the box is empty; Esc closes and gives the Desks chip the keyboard...
  await btn.click();
  await expect(find).toHaveValue("");
  await expect(rows).toHaveCount(2);
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(btn).toBeFocused();
  // ...and keeps it through the next redraw, which builds the chip afresh.
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").name = "cue"; });
  await expect(btn).toBeFocused();
  // ↓ ↵ from the current Desk opens the next.
  await btn.click();
  const lit = await menu.locator(".hv-row.at").getAttribute("data-hv-desk");
  expect(lit).toBe("cue");
  await page.keyboard.press("ArrowDown");
  await expect(menu.locator(".hv-row.at")).not.toHaveAttribute("data-hv-desk", lit);
  const next = await menu.locator(".hv-row.at").getAttribute("data-hv-desk");
  await page.keyboard.press("ArrowUp");
  await expect(menu.locator(".hv-row.at")).toHaveAttribute("data-hv-desk", lit);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe(next);
  await expect(menu).toHaveCount(0);
  // A click outside closes it; a click on a row opens that Desk.
  await btn.click();
  const items = await page.locator(".dk-items").boundingBox();
  await page.mouse.click(items.x + items.width - 6, items.y + items.height - 6);
  await expect(menu).toHaveCount(0);
  await btn.click();
  await menu.locator('.hv-row[data-hv-desk="cue"]').click();
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("cue");
  // New Desk… opens its sheet, the menu closed.
  await btn.click();
  await menu.locator("[data-hv-new-desk]").click();
  await expect(menu).toHaveCount(0);
  await expect(page.locator(".nd")).toHaveCount(1);
});

test("+ New is a menu: New session as before, New Desk asks a project and a name, starts its lead and opens it", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top [data-hv-desk-home]").click();
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
  await openSessions(page);
  await expect(page.locator(".dk-sh b")).toHaveText("Notes app");
  // The same workspace stays available for another independent Desk.
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top [data-hv-desk-home]").click();
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
  await openSessions(page);
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
    await page.locator(".dk-right [data-desk-crew-more]").click();
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
  await openSessions(page); await page.locator("[data-desk-change]").first().click();
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
  const drop = page.locator(".dk-right .dkn-ib .dk-drop");
  await drop.click();
  await expect(drop).toHaveText("Drop it");
  expect(await cue.calls("desk_item_act")).toEqual([]);
  // Any other click lets it go.
  await page.locator(".dk-right .dkn-ih1").click();
  await expect(drop).toHaveText("Drop this item");
  await drop.click();
  await drop.click();
  await cue.called("desk_item_act", { id: "d6", act: "drop" });
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
  // Its row says why in its status line (#666); Give to… is on its page, not its row.
  await expect(statusOf(page, "d6")).toHaveText("Couldn't give it out · No session in this project could take it.");
  await expect(page.locator('[data-desk-item="d6"] [data-desk-give]')).toHaveCount(0);
  await page.locator('[data-desk-item="d6"]').click();
  await expect(page.locator(".dk-right .dkn-sp")).toContainText("Couldn't give it out");
  await expect(page.locator(".dk-right .dkn-sp")).toContainText("No session in this project could take it.");
});

test("rows open from the keyboard: Enter or Space, and the focus stays on the row", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".dk-right .dkn-ih1")).toHaveText("#d6 Tidy the Settings copy");
  await expect(page.locator('[data-desk-item="d6"]')).toBeFocused();
  // Space opens a row too.
  await page.locator(".dk-right .dk-back").click();
  await page.locator('[data-desk-item="d4"]').focus();
  await page.keyboard.press(" ");
  await expect(page.locator('.dk-right .dkn-sp[data-desk-page="session"] .dkn-smeta')).toHaveText("on #d4 · Rail dot blinks for a reply you didn't send");
  await expect(page.locator('[data-desk-item="d4"]')).toBeFocused();
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  await expect(page.locator(".dk-right .dk-pg h1 .dk-tname")).toHaveText("web-lead");
  // The Desk's runner's pane: its mark before its name, where a crew member's is (#170); one name wherever it shows (#175).
  await expect(page.locator('.dk-right .dk-pg h1 [data-desk-mark="desk"]')).toHaveText("Dispatcher");
  await expect(page.locator(".dk-right .dk-markrow")).toHaveCount(0);
  if (process.env.SHOT_DIR) await page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-lead-mark.png` });
  await page.locator('.dk-right [data-desk-crew="id-search"]').focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".dk-right .dkn-crew .dkn-cname")).toHaveText("search-centre");
  // Its mark (the crew icon) right before its name, in its head (#170): no row of its own above.
  await expect(page.locator(".dk-right .dkn-crew .dkn-chead > svg + .dkn-cname")).toHaveCount(1);
  await expect(page.locator(".dk-right .dkn-crew .dkn-smeta")).toContainText("in web-lead's crew");
  await expect(page.locator(".dk-right .dk-markrow")).toHaveCount(0);
  if (process.env.SHOT_DIR) await page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-crew-mark.png`, clip: undefined });
});

test("Send back opens the item's own \"Why send it back?\" box: when it doesn't go it's red in its chat with Send again; it closes when it goes", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  // The item's own box, typed in first, isn't where the reason comes from.
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("for the chat");
  await page.locator('.dk-right .dkn-ib [data-desk-act="send_back"]').click();
  const why = page.locator(".dk-right .dkn-ibback .dk-sb");
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
  // Didn't go: red in the chat it opened with why and the board's Send again (#422, #688), which sends the same words again.
  const bad = page.locator(".dk-right .cv-you.msg-failed", { hasText: "Still loses focus" });
  await expect(bad.locator(".msg-line")).toHaveText("! Couldn't send · offline · Send again · sent back");
  await expect(bad.locator('[data-desk-act="msg_edit"]')).toHaveCount(0);
  await cue.override("desk_item_act", () => null);
  await bad.locator('[data-desk-act="msg_retry"]').click();
  await expect(bad).toHaveCount(0);
  // Gone, its box closes; opened again, a picture goes with the words.
  await expect(why).toHaveCount(0);
  await page.locator('.dk-right .dkn-ib [data-desk-act="send_back"]').click();
  await box.fill("Still loses focus");
  // A picture goes with the words.
  await page.evaluate(() => { draft("back:d2").images.push({ name: "shot.png", mime: "image/png", data: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=" }); renderMain(); });
  await expect(why.locator(".chips")).toHaveCount(1);
  await box.press("Enter");
  await expect.poll(async () => (await cue.calls("desk_item_act")).filter((c) => c.args.act === "send_back").map((c) => [c.args.text, c.args.images.map((i) => i.name)]))
    .toEqual([["Still loses focus", []], ["Still loses focus", []], ["Still loses focus", ["shot.png"]]]);   // went, Send again, with the picture
  await expect(page.locator(".dk-right .dk-sb")).toHaveCount(0);
  await expect(page.locator(".dk-right .foot:not(.held) .composer textarea")).toHaveValue("for the chat");
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
  await expect(card.locator(".dk-fresh")).toHaveText("Start a new lead");
  // No box to anyone, nothing runs it, till someone does.
  await expect(page.locator(":is(.dk-box, .dk-board .foot .composer)")).toHaveCount(0);
  await expect(page.locator(".dk-strip .dk-lcard")).toHaveCount(0);
  await card.locator(".btn.primary").click();
  await cue.called("desk_set_lead", { project: "website", sid: "site" });
  await expect(page.locator(".dk-tab.on")).toHaveText("site-lead");
  // Its chat, with its box, in the chat card; then its card in the sessions list, which takes that place (#666).
  await expect(page.locator(".dk2b-chat .foot:not(.held) .composer textarea")).toHaveAttribute("placeholder", "Tell site-lead: a bug, a feature, or a question…");
  await openSessions(page);
  await expect(page.locator(".dk-strip .dk-lcard")).toContainText("site-lead");
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
  await expect(card.locator("button")).toHaveText("Start a lead");
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
  await openSessions(page); await page.locator(".dk-strip [data-desk-change]").click();
  const card = page.locator(".dk-right .dk-pick");
  // web-lead runs it now: not listed, Keep web-lead instead.
  const rows = await card.locator(".dk-whorow").evaluateAll((els) => els.map((el) => [el.querySelector(".dk-lt").textContent, el.querySelector(".dk-ls").textContent]));
  expect(rows).toEqual([["host-lead", "session · working now"]]);
  await expect(card.locator(".dk-whorow .btn")).toHaveText("Run it");
  await expect(card.locator(".dk-fresh")).toHaveText("Start a new lead");
  await expect(card.locator(".dk-keep")).toHaveText("Keep web-lead");
  await card.locator(".dk-keep").click();
  await expect(page.locator(".dk-pick")).toHaveCount(0);
  await expect(page.locator(".dk-tab.on")).toHaveText("web-lead");
  await openSessions(page); await page.locator(".dk-strip [data-desk-change]").click();
  await page.locator("body").press("Escape");
  await expect(page.locator(".dk-pick")).toHaveCount(0);
  await openSessions(page); await page.locator(".dk-strip [data-desk-change]").click();
  await card.locator('.dk-whorow [data-desk-lead="host"]').click();
  await cue.called("desk_set_lead", { project: "cue", sid: "host" });
  await openSessions(page);
  await expect(page.locator(".dk-strip .dk-lcard")).toContainText("host-lead");
});

test("Who runs this Desk? rows use the box's width: full name, its line, Run it at the right never over the text; warm first, cold under Older (#479)", async ({ page, cue }) => {
  const long = "cue-desk-lead-with-a-really-long-session-name-that-goes-on";
  await cue.open("desk");
  await openDesk(page);
  // Three idle sessions more: one warm (its cache still alive), two cold.
  await cue.change((st, long) => {
    const host = st.sessions.find((s) => s.session_id === "host");
    for (const [sid, name] of [["warm1", long], ["cold1", "cue-old-one"], ["cold2", "cue-older-two"]])
      st.sessions.push({ ...host, session_id: sid, name, state: "idle", since_ms: host.since_ms - 3 * 86400000, team: [], thread: [] });
    // The hub's list of sessions with work on the Desk: cold2's numbers never come, so it counts as cold by this.
    st.desk.projects.find((d) => d.project === "cue").with_work = ["lead", "host"];
  }, long);
  await page.evaluate(() => {
    insCache.set("warm1", { tokens: 142000, last_ms: now() - 2 * 3600000, until_ms: now() + 30 * 60000 });
    insCache.set("cold1", { tokens: 88000, last_ms: now() - 26 * 3600000, until_ms: now() - 3600000 });
  });
  await openSessions(page); await page.locator(".dk-strip [data-desk-change]").click();
  const card = page.locator(".dk-right .dk-pick");
  await expect(card.locator(".dk-fresh")).toHaveText("Start a new lead");
  await expect(card.locator(".dk-keep")).toHaveText("Keep web-lead");
  await expect(card).not.toContainText("dispatcher");
  const lines = () => card.locator(".dk-whorow").evaluateAll((els) => els.map((el) => [el.querySelector(".dk-lt").textContent, el.querySelector(".dk-ls").textContent]));
  // Warm first, the one at work on top; the cold ones folded.
  expect(await lines()).toEqual([["host-lead", "session · working now"], [long, "session · 142k · active 2h ago"]]);
  const older = card.locator(".dk-whos > .dk-older");
  await expect(older).toHaveText("Older · 2›");
  await expect(older).toHaveAttribute("aria-expanded", "false");
  await older.click();
  await expect(card.locator(".dk-whorow")).toHaveCount(4);
  expect((await lines()).slice(2)).toEqual([["cue-old-one", "session · 88k · active 1d ago"], ["cue-older-two", "session"]]);
  /** Each row: the box's width, its whole name shown (not cut), and Run it clear of the name and the line. */
  const fits = () => card.locator(".dk-whorow").evaluateAll((els) => els.map((el) => {
    const r = (q) => el.querySelector(q).getBoundingClientRect(), box = el.parentElement.getBoundingClientRect();
    const b = r(".btn"), t = r(".dk-lt"), l = r(".dk-ls"), nm = el.querySelector(".dk-lt");
    return { wide: Math.abs(el.getBoundingClientRect().width - (box.width - 2)) <= 1, whole: nm.scrollWidth <= nm.clientWidth + 1,
      clear: t.right <= b.left && l.right <= b.left, right: box.right - b.right < 24,
      seen: el.getBoundingClientRect().bottom <= box.bottom + 1 && el.getBoundingClientRect().height >= t.height + l.height };
  }));
  const ok = { wide: true, whole: true, clear: true, right: true, seen: true };
  for (const width of [1440, 900, 640]) {
    await page.setViewportSize({ width, height: 900 });
    expect(await fits(), `at ${width}px`).toEqual([ok, ok, ok, ok]);
  }
  expect(await page.locator(".dk-pick [title]").count()).toBe(0);
});

test("✎ beside a session's name (its row, its chat's header) renames the session: Enter keeps it, Esc doesn't, the row doesn't open", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);
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
  await expect(page.locator('.dk-right .dkn-sp[data-desk-page="session"]')).toHaveCount(0);
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
  const head = page.locator('.dk-right .dkn-sp[data-desk-page="session"] .ap-head');
  const nm = head.locator(".proj");
  await expect(nm).toHaveText("search-fix");
  await head.locator(".rename-btn").click();
  await head.locator(".rename-in").fill("search-fix-2");
  await head.locator(".rename-in").press("Enter");
  await expect(nm).toHaveText("search-fix-2");
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

test("✎ by the Desk's name: Esc keeps the name as it was; a click away keeps what you typed", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);
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
  await page.locator(".dk-items .dk2b-ih > b").click();   // away: the Items list's head, beside the sessions list (#666)
  await expect(box).toHaveCount(0);
  await expect(head.locator("b")).toHaveText("Cue app");
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

test("several sessions and no lead: a row each, warm ones first (filled dot), the one at work on top (#479)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead = null; });
  await openDesk(page);
  const who = page.locator(".dk-right .dk-whorow");
  expect(await who.evaluateAll((els) => els.map((el) => [el.querySelector(".dk-lt").textContent, el.querySelector(".dk-ls").textContent, el.querySelector("i").className]))).toEqual([
    ["host-lead", "session · working now", ""], ["web-lead", "lead, crew of 2", ""],
  ]);
  await expect(page.locator(".dk-keep")).toHaveCount(0);
});

test("a lead that has ended: one line, Resume it or Pick another; no box", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead.state = "ended"; });
  await openDesk(page);
  const line = page.locator(".dk-right .dk-endlead");
  await expect(line.locator("p")).toHaveText("web-lead has ended. Items stay here.");
  await expect(page.locator(".dk-right .foot:not(.held) .composer")).toHaveCount(0);
  await line.locator('[data-desk-lead="lead"]').click();
  await cue.called("desk_set_lead", { project: "cue", sid: "lead" });
  await expect(page.locator(".dk-chat")).toBeVisible();
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead.state = "ended"; });
  await page.locator(".dk-endlead [data-desk-change]").click();
  await expect(page.locator(".dk-pick h2")).toHaveText("Who runs this Desk?");
});

// ---------- Give to… ----------

test("Give to… is not on a row (#666): a row is #id, title, status, steps; a click opens the item, Give to… on its page", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const before = await listOrder(page);
  await expect(page.locator(".dkn-items [data-desk-give], .dkn-items .dk-givemenu")).toHaveCount(0);
  await page.locator('[data-desk-item="d6"] .dkn-tx').click();
  await expect(page.locator(".dk-right [data-desk-give]")).toHaveCount(1);
  // The list stays as it was, the row lit in place.
  expect(await listOrder(page)).toEqual(before);
  await expect(page.locator('.dkn-items [data-desk-item="d6"]')).toHaveClass(/\bon\b/);
  // Its menu lists the project's sessions; a pick gives it.
  await page.locator(".dk-right [data-desk-give]").click();
  const menu = page.locator(".dk-right .dk-givemenu");
  await expect(menu.locator("button")).toHaveText([/web-lead\s*lead, crew of 2/, /host-lead\s*session/, "New session"]);
  await menu.locator('[data-desk-give-to="host"]').click();
  await cue.called("desk_give", { id: "d6", sid: "host" });
  await expect(page.locator(".dk-givemenu")).toHaveCount(0);
});

test("Give to…'s menu opens whole inside the window, with each session's name in full, at any width (#152)", async ({ page, cue }) => {
  await cue.open("desk");
  for (const [w, h] of [[900, 700], [1300, 900], [1300, 420]]) {
    await page.setViewportSize({ width: w, height: h });
    await openDesk(page);
    // Give to… is on the item's page (#666).
    await page.locator('[data-desk-item="d6"] .dkn-tx').click();
    await page.locator(".dk-right [data-desk-give]").click();
    const menu = page.locator(".dk-givemenu");
    await expect(menu).toBeVisible();
    // Measured again if a redraw swaps the menu between finding it and measuring it.
    await expect(async () => {
      const box = await menu.boundingBox();
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.y).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(w);
      expect(box.y + box.height).toBeLessThanOrEqual(h);
    }).toPass();
    // Each name whole: none cut by its menu's edge.
    expect(await menu.locator("button > span:first-child").evaluateAll((els) => els.every((el) => el.scrollWidth <= el.clientWidth + 1 && el.getBoundingClientRect().right <= el.closest(".dk-givemenu").getBoundingClientRect().right))).toBe(true);
    await page.keyboard.press("Escape");
    await expect(page.locator(".dk-givemenu")).toHaveCount(0);
    // Esc closes the menu, not the item (a click on an open row's title would close it): back to the list.
    await page.locator(".dk-right .dk-back").click();
    await expect(page.locator('[data-desk-item="d6"]')).not.toHaveClass(/\bon\b/);
  }
});

test("Give to… in the open item; Esc closes the menu, not the item; New session starts one", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"] .dkn-tx').click();
  const pane = page.locator('.dk-right .dkn-sp[data-desk-page="item"]'), side = pane.locator(".dkn-ib");
  await side.locator("[data-desk-give]").click();
  await expect(side.locator(".dk-givemenu")).toBeVisible();
  await page.locator("body").press("Escape");
  await expect(side.locator(".dk-givemenu")).toHaveCount(0);
  await expect(pane.locator(".dkn-ih1")).toHaveText("#d6 Tidy the Settings copy");
  await side.locator("[data-desk-give]").click();
  await side.locator('.dk-givemenu [data-desk-act="start_now"]').click();
  await cue.called("desk_item_act", { id: "d6", act: "start_now" });
});
