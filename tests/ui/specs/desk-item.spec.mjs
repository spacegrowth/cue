// The Desk: Add session, the lead's chat (its past, ↑ ↓, DONE lines, Read), an item's page (Thread, Files,
// questions, approvals, mocks), and the Desk's marks on the Board.
import { test, expect, pick, openDesk, deskShowPanel } from "./cue.mjs";
import { NOW } from "./clock.mjs";
import { listOrder, openSessions, statusOf, openMore } from "./desk.mjs";

/** A session of cue's that isn't on its Desk ("spare", holding nothing). */
const spare = (cue) => cue.change((st) => st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", thread: [] }));
/** The left card's groups and rows, as "# group" and session ids. */
const leftRows = (page) => page.locator(".dk-sessions .dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-sdiv") ? "—" : el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}` : el.dataset.deskChatOf || el.dataset.deskCrew).filter(Boolean));

test("Add session offers this Desk's sessions, a line, then only free, recent ones: none another Desk has, none stopped or idle for days (#189)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const base = st.sessions.find((x) => x.session_id === "site"), now = Date.now();
    const add = (sid, more = {}) => st.sessions.push({ ...base, session_id: sid, project: "cue", name: sid, thread: [], state: "waiting", since_ms: now, ...more });
    add("spare"); add("theirs", { state: "working" }); add("their-lead", { state: "working" }); add("holds"); add("crew-lead");
    add("stopped", { state: "stopped" }); add("stale", { since_ms: now - 3 * 86400000 });
    st.desk.projects.push({ project: "desk-other", id: "desk-other", independent: true, name: "Other", workspace_project: "cue", folder: "/x",
      lead: { sid: "their-lead", name: "their-lead" }, members: ["theirs"], decisions: [],
      items: [{ id: "o1", title: "theirs", state: "working", holder: { kind: "session", sid: "holds" }, created: 1, updated: 1 },
        { id: "o2", title: "by crew", state: "working", holder: { kind: "crew", sid: "agent-x", lead_sid: "crew-lead", name: "x" }, created: 1, updated: 1 }] });
  });
  await openSessions(page);   // Add session is in All sessions (#666)
  await page.getByRole("button", { name: "Add session", exact: true }).click();
  const picker = page.getByRole("group", { name: "Desk membership" });
  const rows = () => picker.locator(":scope > *").evaluateAll((els) => els.map((el) => el.matches(".dk-add-div") ? "—" : `${el.querySelector("input").dataset.deskMember}${el.querySelector("input").checked ? " ✓" : ""}`));
  await expect.poll(rows).toEqual(["lead ✓", "host ✓", "—", "spare"]);
});

test("Add session toggles project sessions on and off this Desk", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await spare(cue);
  await cue.change((st) => { const s = st.sessions.find((s) => s.session_id === "spare"); s.state = "working"; s.since_ms = 1; st.desk.projects.find((d) => d.project === "cue").work_history = ["host"]; });
  // The lead's panel lists the sessions at work (its Working rows). B2 (#666): All sessions takes the lead's chat's
  // place, so the panel is looked at back on the lead's chat, Esc closing the list.
  await deskShowPanel(page);
  const workerInPanel = page.locator('.dk2b-chat .dk-panel .dk-prows [data-desk-chat-of="spare"]');
  const backToLead = async () => { await page.keyboard.press("Escape"); await expect(page.locator(".dk-sessions")).toHaveCount(0); };
  const reopen = async () => { await openSessions(page); await add.click(); await expect(picker).toBeVisible(); };
  await expect(page.locator(".dk2b-chat .dk-panel .dk-prows").first()).toBeVisible();
  await expect(workerInPanel).toHaveCount(0);
  await openSessions(page);   // the sessions list and Add session are in All sessions (#666)
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
  await expect(spareBox).toBeChecked();
  await page.keyboard.press("Escape");   // the picker
  await backToLead();
  await expect(workerInPanel).toHaveCount(1);
  await reopen();
  await spareBox.uncheck();
  await expect.poll(() => leftRows(page)).toEqual(["# Working", "host"]);
  await page.keyboard.press("Escape");
  await backToLead();
  await expect(workerInPanel).toHaveCount(0);
  await reopen();
  // The click was the button's, not the row's: the lead's chat still shows.
  await expect(page.locator('.dk-sessions [data-desk-lead-chat="lead"]')).toHaveClass(/\bon\b/);

  // A click, not uncheck(): off the Desk, its box is renamed "web-lead" as the redraw lands, so uncheck() can't find it
  // again to see it unchecked. The lines below check what it did.
  await dispatcher.click();
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
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("half a thought about the");
  // Drafts are saved a moment after typing (window.js saveDrafts): wait for that save, not a fixed time.
  await expect.poll(async () => (await cue.calls("set_draft")).some((c) => c.args?.text === "half a thought about the")).toBe(true);
  // Quit and reopen: a new window, Cue's state from scratch; drafts from cue.db.
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-right .foot:not(.held) .composer textarea")).toHaveValue("half a thought about the");
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
  const box = page.locator(".dk-right .foot:not(.held) .composer textarea");
  await box.fill("draft");
  await box.press("ArrowUp");
  await expect(box).toHaveValue("second ask");
  await box.press("ArrowUp");
  await expect(box).toHaveValue("first ask");
  await box.press("ArrowDown");
  await box.press("ArrowDown");
  await expect(box).toHaveValue("draft");
});

test("a DONE is one Ready to check card in the lead's chat: its item, its age, Check it › opens the item's page, where what to test, Looks right and Send back are; once checked a quiet line says what you said", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // The message family's card from Cue (#501): READY TO CHECK, the item, its age; its title; Check it › in the foot.
  const line = page.locator('.dk-chat [data-mg="check"]');
  await expect(line).toHaveCount(1);
  await expect(page.locator(".dk-chat .dk-chk")).toHaveCount(0);
  await expect(line.locator(".mg-tag")).toHaveText("Ready to check");
  await expect(line.locator(".mg-h .mg-id")).toHaveText("Feature #d2");
  await expect(line.locator(".mg-h .mg-id svg.dk-kic")).toHaveCount(1);
  await expect(line.locator(".mg-h .mg-age")).toHaveText(/^\d+[smhd]$/);
  await expect(line.locator(".mg-t")).toHaveText("Quick phrases at the right of Send");
  await expect(line.locator(".mg-f .mg-lk")).toHaveText("Check it ›");
  // No edge stripe, the family's body size, no buttons of its own but the two that open the item.
  await expect(line).toHaveCSS("border-left-width", "0px");
  await expect(line.locator(".mg-t")).toHaveCSS("font-size", "13px");
  await expect(line.locator("[data-desk-act]")).toHaveCount(0);
  // Compact: its title on one line of the body size.
  expect(await line.locator(".mg-t").evaluate((el) => el.getBoundingClientRect().height)).toBeLessThan(23);

  // Not installed yet: nothing to try, so it's waiting for install, on its line, row and page (#130).
  await page.evaluate(() => { state.desk.installed_ms = 1793984280000 - 60000; renderMain(); });
  await expect(line.locator(".mg-tag")).toHaveText("Waiting for install");
  const row = page.locator('.dk-it[data-desk-item="d2"]');
  await expect(statusOf(page, "d2")).toHaveText("Waiting for install");
  await expect(row.locator(".dk2b-chk")).toHaveCount(0);
  // Its page while it waits: says so under what to test, and no Looks right among its buttons (Send back stays).
  const lead = page.locator(".dk-right .foot:not(.held) .composer textarea");
  await lead.fill("a word for the lead");
  await line.locator(".mg-lk").click();
  // What to try, Looks right and Send back: the item's panel (#418).
  const pane = page.locator(".dk-panel[data-side-for]");
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
  await expect(line.locator(".mg-tag")).toHaveText("Ready to check");

  // Check it › opens the item's page: what to test as a checklist, then quiet under it who did it and when (how many
  // files is its Files section's, #710), and that the installed Cue has it; Looks right and Send back.
  // A window wide enough for the panel to show beside the thread (#200).
  await page.setViewportSize({ width: 1512, height: 945 });
  await line.locator(".mg-lk").click();
  await expect(page.locator('.dk-panel[data-side-for="d2"]')).toBeVisible();
  await expect(pane.locator(".dk-ptk li").first()).toBeVisible();
  await expect(info.locator(".dk-chk-w")).toHaveText(/^Done by web-lead · \d+[smhd] ago$/);
  await expect(info.locator(".dk-chk-in")).toContainText("In the installed Cue · installed ");
  // It sits under what to test, the section after You'll see (#418); with the check box at the top saying what to
  // try (#526), it's "Its work", so "What to try" isn't said twice.
  expect(await pane.evaluate((el) => { const w = el.querySelector(".dk-idw"), i = el.querySelector(".dk-chk-info"); return !!(w.compareDocumentPosition(i) & Node.DOCUMENT_POSITION_FOLLOWING); })).toBe(true);
  await expect(pane.locator(".dk-pbox")).toBeVisible();
  await expect(pane.locator(".dk-psec", { has: page.locator(".dk-chk-info") }).locator(".dk-psh")).toHaveText("Its work");
  await expect(pane.locator('.dk-pnow [data-desk-act="looks_right"]')).toHaveText("Close");
  await expect(pane.locator('.dk-acts [data-desk-act="send_back"]')).toHaveText("Send back");
  // A design item's: its mocks in place of installed.
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").checks[0].designs = "https://claude.ai/artifact/mocks"; });
  await expect(info.locator('.dk-chk-in a[data-href="https://claude.ai/artifact/mocks"]')).toHaveText("Open the mocks ↗");
  await info.locator(".dk-chk-in a").click();
  await cue.called("open_link", { url: "https://claude.ai/artifact/mocks" });
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").checks[0].designs = ""; });
  await expect(info.locator(".dk-chk-in")).toContainText("In the installed Cue · installed ");
  // A link in what to test opens.
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").see = "See https://example.com/spec for it"; });
  await pane.locator('.dk-ptk a[data-href="https://example.com/spec"]').click();
  await cue.called("open_link", { url: "https://example.com/spec" });

  // Send back opens a box of its own on the page; with nothing typed its Send asks why, and sends nothing.
  await pane.locator('[data-desk-act="send_back"]').click();
  await expect(page.locator(".dk-right .dk-sb-l")).toHaveText("Why send it back?");
  await page.locator('.dk-board [data-desk-act="back_go"]').click();
  await expect(page.locator(".toast")).toContainText("Say why it goes back first");
  expect((await cue.calls("desk_item_act")).filter((c) => c.args.act === "send_back")).toEqual([]);
  // Its words go to the item's holder only; nothing goes to the lead, and the lead's box is left as it was.
  await page.locator(".dk-right .dk-sb textarea").fill("The arrows still skip one");
  await page.locator('.dk-board [data-desk-act="back_go"]').click();
  await cue.called("desk_item_act", { id: "d2", act: "send_back", text: "The arrows still skip one" });
  expect(await cue.calls("desk_send")).toEqual([]);
  // The page's chat (its holder's, #418) has why it went back, in your words, at once; once the hub has it, its line
  // there says so with how many pictures went too (#410).
  const ichat = page.locator(".dk-right .dk-ichat");
  await expect(ichat.locator(".dk-chk-done")).toContainText("back: “The arrows still skip one”");
  await cue.change((s) => { Object.assign(s.desk.projects.find((x) => x.project === "cue").checks[0], { state: "back", back: "The arrows still skip one", back_pics: 2 }); });
  await expect(ichat.locator(".dk-chk-done")).toContainText("back: “The arrows still skip one” · 2 pictures");
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
  // Sent back (#410): the item's title and what you wrote, in a line; "no note" when you wrote none.
  await cue.change((s) => { Object.assign(s.desk.projects.find((d) => d.project === "cue").checks[0], { state: "back", back: "The arrows still skip one", back_pics: 2 }); });
  const title = await page.evaluate(() => deskItem("cue", "d2").title);
  await expect(said).toHaveText(`You · sent #d2 ${title} back: “The arrows still skip one” · 2 pictures`);
  await cue.change((s) => { Object.assign(s.desk.projects.find((d) => d.project === "cue").checks[0], { back: "", back_pics: 0 }); });
  await expect(said).toHaveText(`You · sent #d2 ${title} back · no note`);
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

test("two send-backs in the same words, each on its own DONE: each shows its own pictures, by when it went, not by its words", async ({ page, cue }) => {
  await cue.open("desk");
  // The first DONE an hour before the second (ms); each sent back a minute after it, in the same words.
  await cue.change((s, [t1, t2]) => {
    const d = s.desk.projects.find((x) => x.project === "cue");
    const k = d.checks[0];
    d.checks = [{ ...k, at: t1, last: false, state: "back", back: "Still off", back_pics: 1 }, { ...k, at: t2, last: true, state: "back", back: "Still off", back_pics: 3 }];
    // Its holder's session gone, its page is its Thread (#418).
    Object.assign(d.items.find((i) => i.id === "d2"), { state: "working", holder: { kind: "session", sid: "gone", name: "cue-old" } });
    const step = (id, at) => ({ project: "cue", item_id: "d2", id, by: "You", sid: "", text: "Still off", why: "sent back to web-lead", at: Math.floor(at / 1000) });
    d.decisions.unshift(step("sbA", t1 + 60000));
    d.decisions.unshift(step("sbB", t2 + 60000));
  }, [1793984280000 - 3600000, 1793984280000]);
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  // Its Thread under its summary, no tab to pick (B2 #666).
  const pane = page.locator(".dk-pg");
  const backs = pane.locator(".dk-tm", { has: page.locator(".ml-back") });
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
  const doneIds = async () => { const o = await listOrder(page); return o.slice(o.indexOf("# DONE TODAY") + 1); };
  const count = (n) => expect.poll(async () => (await doneIds()).length).toBe(n);
  // Done today starts folded (#602, B2 #666): a line with today's count and no rows; opened, the 10 newest, then it
  // loads more as its end comes into view.
  const doneHead = list.locator('[data-desk2b-fold="done"]');
  await expect(doneHead).toHaveAttribute("aria-expanded", "false");
  await expect(doneHead.locator("i")).toHaveText("3");
  await count(0);
  await doneHead.click();
  await expect(doneHead).toHaveAttribute("aria-expanded", "true");
  await count(10);
  await expect(list.locator('[data-desk-done-more="cue"]')).toHaveCount(1);
  // The newest first: today's three, then the newest of the old.
  expect((await doneIds()).slice(0, 5)).toEqual(["d7", "d8", "d9", "o499", "o498"]);
  // To the end: 10 more, and where you were reading stays where it was.
  const anchor = list.locator('[data-desk-item="o495"]');
  // (Its place in the list, by where it's drawn plus how far the list is scrolled: opening Done scrolled it already.)
  const top0 = await anchor.evaluate((e) => e.getBoundingClientRect().top) + await list.evaluate((e) => e.scrollTop);
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
  // B2's item page (#666): its kind and number, then its title.
  await expect(page.locator(".dk-right .dk-pg .dk2b-in")).toHaveText("#d1");
  await expect(page.locator(".dk-right .dk-pg h1")).toHaveText("Bug #d1 · Reply box loses focus after Send");
});

test("a chat opened on the Desk takes the cursor in its box: the lead's, an item's page; not from the keys on the list (#154)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const box = page.locator(".dk-right :is(.dk-box, .foot:not(.held) .composer) textarea[data-text]");
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
  // B2 (#666): the page opens on the left, the list stays at the right, so the keys stay on its row, not the box.
  await expect(page.locator('.dk-items [data-desk-item="d4"]')).toBeFocused();
  await expect(box).not.toBeFocused();
});

test("Looks right is in an item's panel, in reach however far its thread is scrolled; Send back with it (#163, #200)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => { state.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d2").said = Array.from({ length: 30 }, (_, k) => ({ at: Date.now() - (60 - k) * 60000, by: "cue-117", text: `Step ${k}, written out at some length so it takes a line or two.` })); });
  await page.locator('[data-desk-item="d2"]').click();
  // In the panel's NOW (#200), beside the thread: it stays put however far the thread scrolls; Send back with it.
  const pane = page.locator(".dk-right .dk-ipane"), now = page.locator(".dk-panel[data-side-for] .dk-pnow"), top = now.locator('[data-desk-act="looks_right"]');
  await expect(top).toHaveText("Close");
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
  // The row has only Check; Looks right (Close) is on the item's page (B2, #666).
  await page.locator('.dk-items [data-desk-item="d2"]').click();
  const now = page.locator('.dk-panel[data-side-for="d2"] .dk-pnow'), close = now.locator('[data-desk-act="looks_right"]');
  await close.click();
  await expect(now.locator(".dk-soon")).toHaveText("Done · Undo · back to check");
  await expect(page.locator(".toast")).toHaveText("Done · Undo · back to check");
  await page.locator(".toast .toast-undo").click();
  await expect(page.locator(".toast")).toHaveText("Still to check");
  await expect(close).toBeVisible();
  await page.waitForTimeout(3300);
  expect(await cue.calls("desk_looks_right")).toEqual([]);
  // Undo on the item does the same; left alone, it goes.
  await close.click();
  await now.locator('.dk-soon [data-desk-act="looks_undo"]').click();
  await expect(close).toBeVisible();
  await close.click();
  expect(await cue.calls("desk_looks_right")).toEqual([]);
  await cue.called("desk_looks_right", { id: "d2" });
});

test("Waiting on you is never red, with items or at 0; an open item takes the solid highlight with its words in the highlight's ink", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const you = page.locator('.dk-items .dk2b-gh[data-desk2b-group="you"]');
  const deny = await page.evaluate(() => { const t = document.createElement("i"); t.style.color = "var(--deny)"; document.body.append(t); const c = getComputedStyle(t).color; t.remove(); return c; });
  expect(await you.evaluate((e) => getComputedStyle(e).color)).not.toBe(deny);
  await cue.change((st) => { for (const i of st.desk.projects.find((d) => d.project === "cue").items) if (["needs_you", "to_check"].includes(i.state)) i.state = "done"; });
  // At 0 its head goes (B2 draws only groups with something in them).
  await expect(you).toHaveCount(0);
  // The open item takes the list's place (#418): its column at the right, not a lit row.
  await page.locator('[data-desk-item="d3"]').click();
  await expect(page.locator('.dk-panel[data-side-for="d3"]')).toBeVisible();
});

test("an open item's box keeps the cursor: through redraws, and after Enter sends it, as you go on typing", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  const box = page.locator(".dk-right .dk-box textarea");
  await box.click();
  await page.keyboard.type("half a thought");
  const at = await box.boundingBox();
  for (let n = 0; n < 3; n++) await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d6").title += "."; });
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
  await expect(page.locator(".dk-right .dk-pg h1")).toContainText("Tidy the Settings copy");
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
  await expect(page.locator(".dk-right :is(.dk-box, .foot:not(.held) .composer)")).toHaveCount(0);
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
  await expect(page.locator(".dk-right :is(.dk-box, .foot:not(.held) .composer)")).toHaveCount(1);
  // Reply: back to the chat, the cursor in the Desk's box.
  await read.click();
  await page.locator("[data-reader-reply]").click();
  await expect(page.locator(".dk-right :is(.dk-box, .foot:not(.held) .composer) textarea")).toBeFocused();
});

test("Desk approvals send Allow and No for the pending request, from the opened item", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const row = page.locator('.dk-items [data-desk-item="d1"]');
  // The row only says what it asks (B2, #666); Allow and No are on the item's page.
  for (const [index, behavior] of ["allow", "deny"].entries()) {
    const id = `row-approval-${index}`;
    await cue.change((st, id) => {
      const item = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1");
      Object.assign(item.ask, { item_id: id, kind: "permission", text: "Run npm test" });
      st.items.push({ ...st.items.find((i) => i.id === "n-focus-ask"), id, kind: "permission", tool_name: "Bash", tool_input: { command: "npm test" } });
    }, id);
    await expect(statusOf(page, "d1")).toHaveText(/^Asks: Allow /);
    await expect(row.locator("button")).toHaveCount(0);
    await row.click();
    await page.locator(`.dk-right .act-card [data-act="${behavior}"][data-id="${id}"]`).click();
    await cue.called("respond", { id, decision: { behavior } });
    await page.locator(".dk-right .dk-back").click();
  }
});

test("an approval in Waiting on you says in a line or two what it wants; the whole command is in the opened item (#298)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const long = "cd /private/tmp/scratch/l2-i && cat headmac.txt bodylistmac.txt footmac.txt > project/L2IListMac.dc.html && rm bodylistmac.txt headmac.txt footmac.txt && grep -c x-dc project/L2IListMac.dc.html";
  const ask = async (input) => cue.change((st, [long, input]) => {
    const item = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1");
    Object.assign(item.ask, { item_id: "long-approval", kind: "permission", text: long });
    st.items = st.items.filter((i) => i.id !== "long-approval");
    st.items.push({ ...st.items.find((i) => i.id === "n-focus-ask"), id: "long-approval", kind: "permission", tool_name: "Bash", tool_input: { command: long, ...input } });
  }, [long, input]);
  const line = statusOf(page, "d1");
  // Claude's own words for it, when it wrote some.
  await ask({ description: "Join the list mock's parts into one page" });
  await expect(line).toHaveText(/^Asks: Allow join the list mock's parts into one page\? · since /);
  // Else the programs it runs and where.
  await ask({});
  await expect(line).toHaveText(/^Asks: Allow run cat, rm and grep in l2-i\? · since /);
  expect((await line.boundingBox()).height).toBeLessThan(40);
  await page.locator('.dk-items [data-desk-item="d1"]').click();
  await expect(page.locator(".dk-ask")).toContainText("bodylistmac.txt headmac.txt footmac.txt");
});

test("files an item's words name are file chips at the words' size; a command or version stays code (#305)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => { Object.assign(s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2"), { latest: "Moved out of `tests/ui`, changed `ui/desk.js`, ran `npx playwright test`, read `/Users/me/dev/cue/src-tauri/src/hub/desk.rs`; `0.1.22` stays.", holder: { kind: "session", sid: "gone", name: "cue-old" } }); });
  await openDesk(page);
  await page.locator('.dk-items [data-desk-item="d2"]').click();
  const chips = page.locator(".dk-ipane .dk-md .dk-fchip");
  await expect(chips).toHaveText(["tests/ui", "ui/desk.js", "…/src/hub/desk.rs"]);
  await expect(page.locator(".dk-ipane .dk-md code")).toHaveText(["npx playwright test", "0.1.22"]);
  const size = (l) => l.evaluate((el) => parseFloat(getComputedStyle(el).fontSize));
  const words = await size(page.locator(".dk-ipane .dk-md").last());
  for (const l of [chips.first(), page.locator(".dk-ipane .dk-md code").first()]) expect(await size(l)).toBeLessThan(words);
  for (const box of await chips.evaluateAll((els) => els.map((el) => el.getBoundingClientRect().height))) expect(box).toBeLessThan(26);
});

test("an item's question opens at the page's foot and keeps the answer on its page; the row only says it asks", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // A question with nothing suggested: the row says it asks, with no buttons (B2, #666); opened, its foot has the whole card.
  await expect(statusOf(page, "d1")).toHaveText(/^Asks: /);
  await expect(page.locator('.dk-items [data-desk-item="d1"] button')).toHaveCount(0);
  await page.locator('.dk-items [data-desk-item="d1"]').click();
  const chain = page.locator(".dk-pane .dk-chain"), foot = page.locator(".dk-right .dk-ask");
  // The page's section shows only the question; its card is the foot, in the box's place.
  await expect(chain).toContainText(await page.evaluate(() => deskItem("cue", "d1").ask.text));
  await expect(chain.locator("button")).toHaveCount(0);
  await expect(page.locator(".dk-right .dk-box")).toHaveCount(0);
  await foot.locator("[data-pick]", { hasText: "Keep it in the box" }).click();
  await foot.locator('[data-act="submit-answers"]').click();
  await cue.called("respond");
  await cue.change((st) => {
    st.items = st.items.filter((i) => i.id !== "n-focus-ask");
    const d1 = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1");
    d1.state = "working"; d1.ask = null;
  });
  await expect(chain).toContainText("You answered: Keep it in the box");
  await expect(foot).toHaveCount(0);
  expect(await page.evaluate(() => view)).toBe("desk");
  // B2's item page (#666): its kind and number, then its title.
  await expect(page.locator(".dk-right .dk-pg .dk2b-in")).toHaveText("#d1");
  await expect(page.locator(".dk-right .dk-pg h1")).toHaveText("Bug #d1 · Reply box loses focus after Send");
  // A turn that ended on it: its whole last paragraph, and a box to its holder at the foot.
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "site");
    s.state = "waiting";
    s.thread = [...(s.thread || []), { role: "agent", text: "Moved the box.\n\nIf the box stays put, I'll write DONE.\nIf it still jumps, tell me what you did just before.", at_ms: Date.now() }];
    const d1 = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1");
    d1.state = "needs_you"; d1.ask = null; d1.holder = { kind: "session", sid: "site", name: "site-lead" };
  });
  // Its reply is the question (#418, B): it shows once, in its chat, with no box under it telling it again.
  await expect(page.locator(".dk-ichat .cv-agent").last()).toContainText("If it still jumps, tell me what you did just before.");
  await expect(chain).toHaveCount(0);
  const box = page.locator(".dk-right .dk-box textarea");
  await expect(page.locator(".dk-right .dk-box .dk-pre")).toHaveText("To site-lead · Bug #d1");
  await expect(box).toHaveAttribute("placeholder", "Reply box loses…");
  await box.fill("It stays put now");
  await page.locator(".dk-right [data-desk-send]").click();
  // Sent as about this item, so its session answers on it (#176).
  await cue.called("desk_item_act", { id: "d1", act: "note", text: "It stays put now" });
  expect(await page.evaluate(() => deskOf("cue").decisions.find((c) => c.item_id === "d1" && c.delivery)?.delivery)).toMatchObject({ session_id: "site", text: "About #d1 (Reply box loses focus after Send): It stays put now" });
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
  await page.locator(".dk-right .dk-pg h1").waitFor();
  // B2 (#666): Changes under its summary, in place of the old Files tab.
  await page.locator(".dk-right .dk2b-chg").click();
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

test("an item's Changes (the Files tab of before): what's changed so far while it's worked on, with counts, a click opening the diff; none, a line saying so (#149, #666)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  const pane = page.locator(".dk-right .dk-ipane"), chg = pane.locator(".dk2b-chg");
  await expect(chg).toHaveText(/^Changes/);
  await expect(chg).toHaveAttribute("aria-pressed", "false");
  await expect(pane.locator(".dk-inow .dk-fc")).toHaveCount(0);
  await cue.called("desk_item_diff", { id: "d3" });
  const n = await page.evaluate(() => state.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d3").files.length);
  expect(n).toBeGreaterThan(0);
  await expect(chg.locator(".n")).toHaveText(String(n));
  await chg.click();
  await expect(chg).toHaveAttribute("aria-pressed", "true");
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
  await page.locator(".dk-right .dk-back").click();
  await page.locator('[data-desk-item="d3"]').click();
  // Changes stays open for the item as you come back to it.
  await expect(chg).toHaveAttribute("aria-pressed", "true");
  await expect(pane.locator(".dk-sec h3")).toHaveText("Files it's on");
  await expect(pane.locator("button.dk-fc[data-desk-open-file]")).toHaveCount(n);
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d3").files = []; });
  await expect(pane.locator(".dk2b-files .dk-note")).toHaveText("No files on it yet.");
  await expect(chg.locator(".n")).toHaveText("0");
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
  const box = page.locator(".dk-right .foot:not(.held) .composer"), ta = box.locator("textarea"), ghost = box.locator(".sug-line");
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
  // About an item: its phrases (sent by its normal flow, desk-quick.spec) but no suggestion (those words aren't the
  // lead's own next prompt).
  await page.locator('[data-desk-item="d3"]').click();
  await expect(page.locator(".dk-right .dk-box .quick .qp")).toHaveCount(2);
  await expect(page.locator(".dk-right .dk-box .sug-line")).toBeHidden();
});

test("a reply that asks for set words (Reply \"…\") offers them as the suggested reply: the lead's chat, an item's page; else Claude's own (#186)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    s.state = "waiting"; s.suggestion = "take the next item";
    s.thread = [...(s.thread || []), { role: "agent", text: 'It\'s built and its tests pass.\n\nReply "commit and install" and I\'ll finish it.', at_ms: Date.now(), images: [] }];
  });
  await openDesk(page);
  const ta = page.locator(".dk-right :is(.dk-box, .foot:not(.held) .composer) textarea"), ghost = page.locator(".dk-right :is(.dk-box, .foot:not(.held) .composer) .sug-line");
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
  await page.locator('[data-desk-item="d6"]').click();
  const box = page.locator(".dk-right .dk-box textarea");
  await box.fill("Keep the search box centred on narrow windows too");
  await page.locator(".dk-right [data-desk-send]").click();
  await cue.called("desk_item_act", { id: "d6", act: "wrote_lead", text: "Keep the search box centred on narrow windows too" });
  // In its Thread on its page (B2 #666 has no Record tab): yours, once, as you wrote it.
  const step = page.locator(".dk-right .dk-ipane [data-thread-for] .cv-you-text", { hasText: "Keep the search box centred on narrow windows too" });
  await expect(step).toHaveCount(1);
  await expect(step).toHaveText("Keep the search box centred on narrow windows too");
  expect(await page.evaluate(() => [view, deskOpen?.kind, deskOpen?.id])).toEqual(["desk", "item", "d6"]);
});

test("Open on an item's page, or its holder's, opens that session's chat here on the Desk, never the board; ‹ goes back to it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const sid = await page.evaluate(() => state.sessions.find((s) => s.project === "cue" && s.session_id !== "lead").session_id);
  // Not working: on a working session's chat Esc is its Stop first (#494, desk-esc.spec.mjs).
  await cue.change((st, s) => { st.sessions.find((x) => x.session_id === s).state = "waiting"; Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4"), { holder: { kind: "session", sid: s, name: "cue-117" } }); }, sid);
  await page.locator('[data-desk-item="d4"]').click();
  // Open its holder, at the foot of the item's column (#418).
  await page.locator(".dk-panel[data-side-for] .dk-sfoot .btn", { hasText: /^Open / }).click();
  expect(await page.evaluate(() => view)).toBe("desk");
  await expect(page.locator(`.dk-right [data-desk-chat="${sid}"]`)).toBeVisible();
  // ‹ goes back to the item it was opened from; so does Esc.
  await expect(page.locator(".dk-right .dk-ph1 .dk-back")).toHaveText("‹ Bug #d4");
  await page.locator(".dk-right .dk-ph1 .dk-back").click();
  await expect(page.locator(".dk-right .dk-pg h1")).toContainText("Rail dot blinks");
  await page.locator(".dk-panel[data-side-for] .dk-sfoot .btn", { hasText: /^Open / }).click();
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
  // From All sessions (#666), in the lead's chat's place as everything open is: back to All sessions, still there.
  await page.evaluate(() => deskShow(null));
  await openSessions(page);
  await page.evaluate((s) => document.querySelector(`.dk-sessions [data-desk-chat-of="${s}"]`).click(), sid);
  await expect(page.locator(".dk-right .dk-ph1 .dk-back")).toHaveText("‹ All sessions");
  await page.locator(".dk-right .dk-ph1 .dk-back").click();
  await expect(page.locator(".dk-sessions")).toBeVisible();
  await expect(page.locator('.dk-chat[data-desk-chat="cue"]')).toHaveCount(0);
  // The Desk's own lead: its chat, still on the Desk.
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  await page.locator(".dk-right .dk-ph1 .btn", { hasText: "Open its chat" }).click();
  expect(await page.evaluate(() => view)).toBe("desk");
  await expect(page.locator('.dk-chat[data-desk-chat="cue"]')).toBeVisible();
});

test("an item's page is a Thread under Thread, Decisions and Steps tabs: every message in order, a send-back says so on its line (#140, #422)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue"), it = d.items.find((i) => i.id === "d4"), t = it.created < 1e12 ? it.created : it.created / 1000;
    // Its holder's session gone, its page is its Thread (#418).
    it.holder = { kind: "session", sid: "gone", name: "cue-117" };
    it.said = [{ at: (t + 120) * 1000, by: "cue-117", text: "Fixed: the dot checks **who sent** it.", done: true }];
    d.decisions.push({ project: "cue", item_id: "d4", id: "y1", by: "You", sid: "", text: "Still blinks after a reply from someone else", why: "sent back to cue-117", at: t + 240 });
  });
  await page.locator('[data-desk-item="d4"]').click();
  const pane = page.locator(".dk-right .dk-pg");
  // B2 (#666): no tabs; its Thread under its summary, in place, with no session's chat to show.
  await expect(pane.locator(".dk2b-ichd .dk2b-sec")).toHaveText("ITS THREAD");
  const msgs = pane.locator(".dk-thread .dk-tm:not(.rec)");
  // Added first, its reply, then the send-back; the hand-off (a step) sits where its time puts it. A reply that said
  // DONE is a reply, with a record line after it (#276).
  // A message you sent is the chat's bubble with its one line (#422), not a head of its own.
  const who = await msgs.evaluateAll((els) => els.map((e) => (e.classList.contains("dk-tmsg") ? "msg" : e.querySelector(".dk-tw b")?.textContent)));
  expect(who.slice(-3)).toEqual(["You", "cue-117", "msg"]);
  await expect(msgs.nth(who.length - 2).locator(".dk-ttag")).toHaveText("Reply");
  await expect(pane.locator(".dk-thread .dk-tm.rec", { hasText: "Done" })).toHaveText(/Done · cue-117 said Bug #d4 is done/);
  await expect(msgs.nth(who.length - 2).locator("strong")).toHaveText("who sent");
  const back = msgs.last();
  await expect(back.locator(".cv-you-text")).toHaveText("Still blinks after a reply from someone else");
  await expect(back.locator(".msg-line")).toContainText("Delivered to cue-117");
  await expect(back.locator(".ml-back")).toHaveText("sent back");
  await expect(pane.locator(".dk-thread .dk-ttag", { hasText: "Hand-off" })).toHaveCount(0);   // "Gave it…" isn't a hand-off line
  // The answer box is the pane's foot.
  const foot = await page.locator(".dk-right .dk-box").boundingBox(), last = await back.boundingBox();
  expect(foot.y).toBeGreaterThan(last.y);
});

test("the lead unsticking an item shows on its Thread as record lines (#347)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue"), it = d.items.find((i) => i.id === "d4"), t = it.created < 1e12 ? it.created : it.created / 1000;
    const step = (id, text, why, at) => d.decisions.push({ project: "cue", item_id: "d4", id, by: "cue-lead", sid: "lead", text, why, at: t + at, undo: "none" });
    it.holder = { kind: "session", sid: "gone", name: "cue-117" };   // its session gone: its page is its Thread (#418)
    step("u1", "Started #d4 now, without waiting for a copy", "the user needs it", 60);
    step("u2", "Moved #d4 up in the landing line", "the user waits on it", 120);
    step("u3", "Took back #d4's copy", "it's done", 180);
    step("u4", "Closed #d4: its work is on main (ab12cd3)", "it landed by hand", 240);
  });
  await page.locator('[data-desk-item="d4"]').click();
  const recs = page.locator(".dk-right .dk-pg .dk-thread .dk-tm.rec", { hasText: "Unstuck" });
  await expect(recs).toHaveText([
    /Unstuck · cue-lead: Started #d4 now, without waiting for a copy: the user needs it/,
    /Unstuck · cue-lead: Moved #d4 up in the landing line: the user waits on it/,
    /Unstuck · cue-lead: Took back #d4's copy: it's done/,
    /Unstuck · cue-lead: Closed #d4: its work is on main \(ab12cd3\): it landed by hand/,
  ]);
});

test("what you send on an item's page shows in its Thread at once, before it comes back as a step (#140)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  await page.locator(".dk-right .dk-box textarea").fill("Narrow windows too");
  await page.locator(".dk-right [data-desk-send]").click();
  // Its page is its holder's chat (#418): yours in it at once, as you wrote it.
  const mine = page.locator(".dk-right .dk-ichat .cv-you", { hasText: "Narrow windows too" });
  await expect(mine).toHaveCount(1);
  // The lead is at work: Cue holds it till its turn ends, and the line says so, with its controls (#422).
  await expect(mine.locator(".msg-line")).toHaveText("◌ Not sent yet · Cue sends it when its turn ends · Edit · Send now · Cancel");
  // The hub keeps it as a step (#437): still one message.
  await cue.called("desk_item_act", { id: "d3", act: "note", text: "Narrow windows too" });
  await expect(mine).toHaveCount(1);
});

test("an item page's header (back, number, title) stays at the top as its thread scrolls (#142, #200, #666)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // A long thread, so the page scrolls.
  await cue.change((st) => { const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4"); it.said = Array.from({ length: 30 }, (_, n) => ({ at: Date.now() - (30 - n) * 60000, by: "cue-117", text: `Step ${n} of the fix, written out at some length so it takes a line or two.` })); });
  await page.locator('[data-desk-item="d4"]').click();
  // B2 (#666): no tabs; the head with ‹ Items, its number and title sits over the page, which scrolls under it.
  const pane = page.locator(".dk-right .dk-ipane"), head = page.locator(".dk-right .dk-ph"), title = page.locator(".dk-right .dk-ph h1");
  const before = await head.boundingBox(), h1 = await title.boundingBox();
  await pane.evaluate((e) => { e.scrollTop = e.scrollHeight; });
  expect(await pane.evaluate((e) => e.scrollTop)).toBeGreaterThan(200);
  expect(await head.boundingBox()).toEqual(before);
  expect(await title.boundingBox()).toEqual(h1);
  // The page goes under the head, not over it.
  const pb = await pane.boundingBox();
  expect(pb.y).toBeGreaterThanOrEqual(before.y + before.height - 1);
});

test("an item's You'll see is in its panel, ticked once it's ready to check, none before; its Done when isn't shown (#140, #200, #382)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d4"]').click();
  const pane = page.locator(".dk-right .dk-ipane");
  // Not in the thread: the panel's own section.
  await expect(pane.locator(".dk-dwl, .dk-ptk")).toHaveCount(0);
  const sec = page.locator(".dk-panel[data-side-for] .dk-pdw");
  await expect(sec.locator("h4")).toHaveText("You'll see");
  await expect(page.locator(".dk-right")).not.toContainText("desk.spec checks it");
  await expect(sec.locator(".dk-ptk li")).toContainText(["The rail's dot blinks only for a session you wrote to"]);
  await expect(sec.locator(".dk-tk.y")).toHaveCount(0);
  // Ready to check: all ticked.
  await page.locator(".dk-right .dk-back").click();
  await page.locator('[data-desk-item="d2"]').click();
  await expect(sec.locator("h4")).toHaveText("You'll see · 2 of 2");
  await expect(sec.locator(".dk-ptk li").first()).toContainText("Quick phrases sit at the right of Send");
  await expect(sec.locator(".dk-tk.y")).toHaveCount(2);
});

test("an item's thread scrolls to the newest message when one arrives or you send one (#145)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Its holder's session gone, its page is its Thread (#418).
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4").holder = { kind: "session", sid: "gone", name: "cue-117" }; });
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4").said = Array.from({ length: 30 }, (_, k) => ({ at: Date.now() - (60 - k) * 60000, by: "cue-117", text: `Step ${k} of the fix, written out at some length so it takes a line or two.` })); });
  await page.setViewportSize({ width: 1728, height: 1000 });   // its panel beside the chat, not over it (#418)
  await page.locator('[data-desk-item="d4"]').click();
  const pane = page.locator(".dk-right .dk-ipane");
  const inView = async (loc) => {
    // (B2 #666: no tabs pinned over it; the page's own top.)
    const r = await loc.boundingBox(), p = await pane.boundingBox();
    if (!r || !p) return false;   // mid-redraw
    return r.y >= p.y - 1 && r.y + r.height <= p.y + p.height + 1;
  };
  // It opens at its newest message, as a chat does.
  await expect.poll(() => inView(pane.locator(".dk-thread .dk-tm").last())).toBe(true);
  expect(await pane.evaluate((e) => e.scrollTop)).toBeGreaterThan(200);
  // At its end, one arrives: it's in view (#395 option A: the page follows only while you're at its end).
  const reply = (text) => cue.change((st, text) => { const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4"); it.said = [...it.said, { at: Date.now(), by: "cue-117", text }]; }, text);
  await reply("A new reply just came in.");
  const arrived = pane.locator(".dk-thread .dk-tm", { hasText: "A new reply just came in." });
  await expect(arrived).toHaveCount(1);
  await expect.poll(() => inView(arrived)).toBe(true);
  // Scrolled up to read, another arrives: the page stays where you are.
  await pane.evaluate((e) => { e.scrollTop = 0; });
  await reply("Another reply while you read.");
  await expect(pane.locator(".dk-thread .dk-tm", { hasText: "Another reply while you read." })).toHaveCount(1);
  await page.waitForTimeout(200);
  expect(await pane.evaluate((e) => e.scrollTop)).toBe(0);
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
  await deskShowPanel(page);   // B2 hides the panel till Show panel (#666)
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
  await expect(now.locator(".dk-pword")).toHaveText(/^(Working|Idle|Needs you)$/);
  // What waits on you, as the list's WAITS ON YOU has it (B2, #666): #d1's question, then #d2 ready to check.
  await expect(now.locator(".dk-psay")).toContainText("2 items wait on you.");
  await expect(now.locator(".dk-pgo")).toHaveText("Check #d1");
  const waitsSec = pg.locator(".dk-panel .dk-psec", { has: page.locator("h4", { hasText: "Waits on you" }) });
  await expect(waitsSec.locator("h4")).toHaveText("Waits on you · 2");
  await expect(waitsSec.locator(".dk-prow")).toHaveCount(2);
  await expect(waitsSec.locator(".dk-prow").first()).toContainText("#d1");
  await page.screenshot({ path: `${dir}/cue-desk-lead-chat-light.png` });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({ path: `${dir}/cue-desk-lead-chat-dark.png` });
  await page.emulateMedia({ colorScheme: "light" });

  // The review stays available, and clearing the question removes the waiting action.
  await page.evaluate(() => { state.items = state.items.filter((i) => i.id !== "n-focus-ask"); renderMain(); });
  // #d1 leaves; #d2, ready to check, still waits: Check goes to it.
  await expect(now.locator(".dk-pgo")).toHaveText("Check #d2");
  await expect(waitsSec.locator("h4")).toHaveText("Waits on you · 1");
  // Working (its ask answered) with one new item no one has: "Routing #d6"; how many are with sessions.
  await page.evaluate(() => { sessionOf("lead").state = "working"; state.items = state.items.filter((i) => i.session_id !== "lead"); renderMain(); });
  await expect(now.locator(".dk-pword")).toHaveText("Working");
  await expect(now.locator(".dk-psay")).toContainText("Routing #d6 to a session.");
  await expect(now.locator(".dk-psay")).toContainText(/\d+ items? (is|are) with sessions/);
  await expect(page.locator('[data-desk-item="d2"] .dk2b-chk')).toBeVisible();
  // None waits: no Check button, and WAITS ON YOU · 0.
  await page.evaluate(() => { const d = deskOf("cue"); d.items.find((i) => i.id === "d2").state = "done"; renderMain(); });
  await expect(now.locator(".dk-pgo")).toHaveCount(0);
  await expect(waitsSec).toHaveCount(0);

  // A session holding an item: the same header, "Session ·" and its name, Open its session at the right; under it
  // what it's on, its number opening the item; its state in NOW.
  await openSessions(page);   // the sessions list is in All sessions (#666)
  await page.locator('.dk-sessions .dk-srow:not(.crew)', { hasText: "host-lead" }).first().click();
  const sp = page.locator('.dk-right .dk-pg[data-desk-page="session"]');
  await expect(sp).toHaveCount(1);
  await expect(sp.locator("h1 [data-desk-mark]")).toHaveText("Session");
  await expect(sp.locator("h1 .dk-shn")).toHaveText("host-lead");
  await expect(sp.locator(".dk-ph1 [data-desk-session]")).toHaveText("Open its session");
  await expect(sp.locator(".dk-ph3")).toHaveText(/^on #d5 · /);
  await expect(sp.locator(".dk-pnow .dk-pword")).toHaveText(/^(Working|Idle|Needs you)$/);
  // The same type as the Dispatcher's header.
  const font = (l) => l.evaluate((e) => { const c = getComputedStyle(e); return [c.fontSize, c.fontWeight]; });
  await page.evaluate(() => deskShow(null));
  const leadFont = await font(pg.locator("h1"));
  await openSessions(page);   // the sessions list is in All sessions (#666)
  await page.locator('.dk-sessions .dk-srow:not(.crew)', { hasText: "host-lead" }).first().click();
  expect(await font(sp.locator("h1"))).toEqual(leadFont);
  if (process.env.SHOT_DIR) for (const c of ["light", "dark"]) { await page.emulateMedia({ colorScheme: c }); await sp.locator(".dk-ph").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-sess-held-${c}.png` }); }
  await page.emulateMedia({ colorScheme: "light" });
  await sp.locator(".dk-ph3 .dk-sch-t").click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "d5"]);
  await openSessions(page);   // the item took the right card; the sessions list is in All sessions (#666)
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
  // Its page is its holder's chat (#418): yours under the holder's latest words.
  const ichat = page.locator(".dk-right .dk-ichat"), rows = ichat.locator(":scope > [data-key]");
  const latest = await ichat.locator(".cv-agent .cv-text").last().textContent();
  await cue.override("desk_item_act", () => new Promise((r) => { window.__release = r; }));
  await page.locator(".dk-right .dk-box textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  await expect(ichat.locator(".cv-you").last()).toContainText("Check the narrow window too");
  expect(await ichat.evaluate((el, t) => { const ks = [...el.querySelectorAll(":scope > [data-key]")]; const you = ks.findLastIndex((k) => k.textContent.includes("Check the narrow window too")); const msg = ks.findLastIndex((k) => k.textContent.includes(t.trim().slice(0, 30))); return you > msg && msg >= 0; }, latest)).toBe(true);
  // Its holder at work: its "now" is last, under your message, as it answers it (#165).
  await cue.change((st) => { const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4"); const s = st.sessions.find((x) => x.session_id === it.holder.sid); it.state = "working"; it.now_on = true; if (s) s.state = "working"; });
  await expect(ichat.locator('[data-key="where"]')).toHaveCount(1);
  await page.locator(".dk-right .dk-box textarea").fill("And the wide one");
  await page.locator(".dk-right [data-desk-send]").click();
  await expect(rows.last()).toHaveAttribute("data-key", "where");
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
  await expect(lead).toHaveText("DeskFeature Desk›");
  // A quiet line on the card's own ground, not a light pill (#260): no fill, no border, the name in on-bg.
  const look = await lead.evaluate((el) => { const cs = getComputedStyle(el); return { bg: cs.backgroundColor, bw: cs.borderTopWidth }; });
  expect(look.bg).toBe("rgba(0, 0, 0, 0)");
  expect(look.bw).toBe("0px");
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
  await openSessions(page);   // the Desk's name heads All sessions (#666)
  await expect(page.locator(".dk-sh b")).toHaveText("Feature Desk");
});

test("Board Sessions: Desk sessions sit under their own fold line at the foot, open until you fold it (#262)", async ({ page, cue }) => {
  await cue.open("desk");
  await page.evaluate(() => { try { localStorage.removeItem("cue.deskShut"); } catch {} });
  await page.locator('.switch-view [data-view="board"]').click();
  const col = page.locator(".sec-sessions .sec-body");
  const fold = col.locator("[data-fold-desk]");
  await expect(fold).toHaveCount(1);
  const deskSids = await col.evaluate((el) => [...el.querySelectorAll("[data-session]")].map((n) => n.dataset.session).filter((sid) => deskWorksFor(sid)));
  expect(deskSids.length).toBeGreaterThan(0);
  await expect(fold).toHaveCount(1);
  await expect(fold).toContainText(`DESK SESSIONS · ${deskSids.length}`);
  await expect(fold).toHaveAttribute("aria-expanded", "true");
  await expect(fold.locator("svg.dk-kic")).toHaveCount(1);
  // Open: every Desk session's card comes after the fold line, none above it.
  const order = await col.evaluate((el) => [...el.querySelectorAll("[data-fold-desk], [data-session]")].map((n) => n.dataset.session || "FOLD"));
  const at = order.indexOf("FOLD");
  for (const sid of deskSids) expect(order.indexOf(sid)).toBeGreaterThan(at);
  // Fold: their cards leave, the line stays, and it's remembered.
  await fold.click();
  await expect(fold).toHaveAttribute("aria-expanded", "false");
  for (const sid of deskSids) await expect(col.locator(`[data-session="${sid}"]`)).toHaveCount(0);
  expect(await page.evaluate(() => localStorage.getItem("cue.deskShut"))).toBe("1");
  await expect(page.locator("[title]", { has: fold })).toHaveCount(0);
  await fold.click();
  for (const sid of deskSids) await expect(col.locator(`[data-session="${sid}"]`)).toHaveCount(1);
});

test("Board Waiting: a Desk session's your-turn row folds with Desk at the foot; its asks stay in the list (#262)", async ({ page, cue }) => {
  await cue.open("desk");
  await page.evaluate(() => { try { localStorage.removeItem("cue.deskShut"); } catch {} });
  await cue.change((st) => {
    const ask = st.items.find((i) => i.session_id === "lead");
    st.items.push({ ...ask, id: "n-site-turn", kind: "waiting", session_id: "site", tool_name: "", tool_input: {}, message: "Done: the link works." });
  });
  await page.locator('.switch-view [data-view="board"]').click();
  const col = page.locator(".sec-waiting .sec-body");
  const fold = col.locator("[data-fold-desk]");
  await expect(fold).toHaveCount(1);
  await expect(fold).toContainText("DESK SESSIONS · 1");
  await expect(fold).toContainText("your turn");
  const order = await col.evaluate((el) => [...el.querySelectorAll("[data-fold-desk], .nrow")].map((n) => n.matches("[data-fold-desk]") ? "FOLD" : n.textContent.includes("site-lead") || n.querySelector("[data-session='site'], [data-id='n-site-turn']") ? "SITE" : "OTHER"));
  expect(order.indexOf("FOLD")).toBeLessThan(order.lastIndexOf("SITE"));
  expect(order.indexOf("OTHER")).toBeLessThan(order.indexOf("FOLD"));   // the lead's ask stays up in the list
  // One fold for both columns: folding here folds Sessions' Desk line too.
  await fold.click();
  await expect(fold).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator(".sec-sessions [data-fold-desk]")).toHaveAttribute("aria-expanded", "false");
  expect(await col.evaluate((el) => el.textContent.includes("Done: the link works."))).toBe(false);
  await expect(col.locator(".nrow").first()).toBeVisible();
  await fold.click();
  await expect(fold).toHaveAttribute("aria-expanded", "true");
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

test("tasks, bugs and features say their question on the row and are answered on the item page", async ({ page, cue }) => {
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
        kind: "question", status: "pending", tool_input: { questions: [{ question, options: [{ label: "Compact (Recommended)" }, { label: "Spacious" }] }] }, created_ms: Date.now() });
    }
  });
  // Each row says its question in its status line, no buttons (B2, #666); answered on the item's page.
  for (const id of ["d1", "d2", "d3"]) {
    await expect(statusOf(page, id)).toHaveText(/^Asks: Which layout should we use\? · since \d\d:\d\d$/);
    await expect(page.locator(`.dk-items [data-desk-item="${id}"] button`)).toHaveCount(0);
  }
  await page.locator('.dk-items [data-desk-item="d1"]').click();
  const foot = page.locator(".dk-right .dk-ask");
  await foot.locator("[data-pick]", { hasText: "Compact" }).click();
  await foot.locator('[data-act="submit-answers"]').click();
  const sent = await cue.called("respond");
  expect(sent.args.decision).toEqual({ behavior: "allow", answers: { "Which layout should we use?": "Compact (Recommended)" } });
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
  // Its question is its holder's reply, in the item's chat once (#418): no box under it saying it again.
  const right = page.locator(".dk-right");
  await expect(right.locator(".dk-ichat .cv-agent").last()).toContainText("Which + did you mean?");
  // Once in the page's own column (its chat and foot; B2's item page, #666), not again in a box at the foot.
  await expect(right.locator(".dk2b-ichat, .dk-pg > :not(.dk-panel, .dk-ph, .dk-ipane)").getByText("Which + did you mean?")).toHaveCount(1);
  await expect(right.locator(".dk-pane .dk-chain")).toHaveCount(0);
  await expect(right.locator(".dk-ichat .cv-agent").last()).not.toContainText("DECIDED");
  await expect(right).not.toContainText("It asks you");
});

test("an item whose new session couldn't start says so, with why and Retry, not stuck in its queue (#172)", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  await page.evaluate(() => {
    state.desk.projects.find((x) => x.project === "cue").items.push({ id: "f1", project: "cue", kind: "bug", title: "The footer overlaps", state: "new", created: Date.now(), updated: Date.now(), holder: null, from: "you", start_failed: "the folder is gone" });
    renderMain();
  });
  await expect(statusOf(page, "f1")).toHaveText("Couldn't start");
  await page.locator('[data-desk-item="f1"]').click();
  const pane = page.locator(".dk-right .dk-pg");   // the item's page, its panel beside its chat (#418)
  await expect(pane).toContainText("Couldn't start");
  await expect(pane).toContainText("Cue couldn't start a new session for it: the folder is gone");
  await openMore(pane);
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
      "You'll see: (1) the canvas opens from a hosted session (2) it says why when it can't; light and dark. Installed and committed.",
      "Done when: host.spec canvas test passes",
      "Why you: you know the host side",
      "Fixes and next steps for it stay in #d5: do them, or say them in your reply; don't add new items for them. Add one only for something truly separate, with why in one line: ITEM: <bug|feature|task> | <title> | done when: <…> | why: <how it's separate from #d5>",
      "If you put its designs on a canvas, or move them, say DESIGNS: #d5 <canvas link> on a line of its own.",
      "Say DONE: #d5 on a line of its own when it's done.",
    ].join("\n");
    sessionOf("host").thread.push({ at_ms: Date.now() - 60_000, role: "you", text: words });
    deskShow({ kind: "chat", sid: "host" });
  });
  // The hand-off is one of the Desk's lines (#688, option D): the chat has its count line, the card is in Activity.
  const chat = page.locator('.dk-right [data-desk-chat="host"]');
  await expect(chat.locator(".mg-hand")).toHaveCount(0);
  await chat.locator(".dk-cnt-b").last().click();
  const card = page.locator(".dk-moves .dk-mv-ln .mg-hand");
  await expect(card).toHaveCount(1);
  await expect(card.locator(".mg-h")).toHaveText(/^web-lead\s*Hand-off\s*Bug #d5/);
  await expect(card.locator(".dk-hand-t")).toHaveText("Hosted sessions can't open the canvas");
  await expect(card.locator(".dk-dwl li")).toHaveText(["the canvas opens from a hosted session", "it says why when it can't", "light and dark", "Installed and committed"]);
  await expect(card.locator(".dk-hand-w")).toHaveText("you know the host side");
  // Its Done when is the session's check, in its message only (#382).
  await expect(card).not.toContainText("host.spec");
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
  await card.locator(".mg-id[data-desk-hand]").click();
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
  // Its page is its holder's chat (#418); other items' turns fold (desk-item-chat.spec.mjs).
  const pane = page.locator(".dk-right .dk-ipane");
  // Its session's turn ended on it: what you write on its page goes to its session as about it, and shows in its
  // thread as you wrote it.
  await cue.change((st) => {
    const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5");
    st.sessions.find((x) => x.session_id === it.holder.sid).state = "waiting";
    Object.assign(it, { state: "needs_you", now_on: true });
  });
  await page.locator(".dk-right .dk-box textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  await cue.called("desk_item_act", { id: "d5", act: "note", text: "Check the narrow window too" });
  expect(await page.evaluate(() => deskOf("cue").decisions.find((c) => c.item_id === "d5" && c.delivery)?.delivery.text)).toMatch(/^About #d5 \(.+\): Check the narrow window too$/);
  await expect(pane.locator(".dk-ichat .cv-you").last()).toContainText("Check the narrow window too");
  // Its session on it now: its working row comes back.
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").state = "working"; st.sessions.find((x) => x.session_id === "host").state = "working"; });
  await expect(pane.locator('.dk-ichat [data-key="where"]')).toHaveCount(1);
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
  const head = page.locator(".dk-panel[data-side-for] .dk-pnow");
  await expect(head.locator(".dk-pacts a.dk-dlink")).toHaveText("Open the mocks ↗");
  // Under the sentence, not over it.
  const box = async (l) => l.evaluate((e) => { const r = e.getBoundingClientRect(); return [r.top, r.bottom]; });
  expect((await box(head.locator(".dk-pacts a.dk-dlink")))[0]).toBeGreaterThanOrEqual((await box(head.locator(".dk-psay")))[1]);
  await expect(head.locator(".dk-pickl")).toHaveText("Mocks · open the canvas · Pick one for working items: D, E or F.");
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    if (process.env.SHOT_DIR) await page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-178-${theme}.png` });
  }
  // Picked (no reply asks for one): no pick line, only the mocks (#182, #436); no mocks: no link, in the app.
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").said = [{ at: Date.now(), by: "host-lead", text: "Built E. DONE: #d5" }]; });
  await expect(head.locator(".dk-pickl")).toHaveText("Mocks · open the canvas");
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").designs = ""; });
  await expect(head.locator("a.dk-dlink")).toHaveCount(0);
  await expect(head.locator(".dk-pickl")).toHaveCount(0);
  await expect(head.locator(".dk-pbox .dk-ckl")).toHaveText(/^(Try|Nothing said what to try): /);
});

test("a pick on an item's mocks: Pick B closes it, its new build item is linked both ways; a pick again makes no other (#281)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5");
    Object.assign(it, { state: "to_check", title: "Mocks: working items", designs: "https://claude.ai/artifact/MOCKS", said: [{ at: Date.now(), by: "host-lead", text: "Three on the canvas.\nPick one for working items: A, B or C." }] });
  });
  await page.locator('[data-desk-item="d5"]').click();
  const head = page.locator(".dk-panel[data-side-for] .dk-pnow");
  const picks = head.locator('[data-desk-act="pick"]');
  await expect(picks).toHaveText(["Pick", "Pick", "Pick"]);
  await expect(head.locator(".dk-opts .dk-optk")).toHaveText(["A", "B", "C"]);
  await expect(head.locator("[title]")).toHaveCount(0);
  await picks.nth(1).click();
  await cue.called("desk_item_act", { id: "d5", act: "pick", text: "B" });
  const items = () => page.evaluate(() => CueBridge.world.state.desk.projects.find((x) => x.project === "cue").items);
  // Picked: done, no more picks, and which item builds it.
  await expect(head.locator(".dk-pword")).toHaveText("Done");
  const n = (await items()).find((i) => i.id === "d5").built_as;
  await expect(picks).toHaveCount(0);
  await expect(head.locator(".dk-built")).toHaveText(`You picked B on #d5; #${n} builds it`);
  // Its link opens the build item, whose page links back to the mocks.
  await head.locator(`a[data-desk-item="${n}"]`).click();
  await expect(page.locator(".dk-right .dk-pg h1")).toContainText("Build: working items (option B)");
  const back = page.locator(".dk-panel[data-side-for] .dk-pnow .dk-built");
  await expect(back).toHaveText("From your pick on #d5");
  await back.locator('a[data-desk-item="d5"]').click();
  await expect(page.locator(".dk-panel[data-side-for] .dk-pnow .dk-built")).toHaveText(`You picked B on #d5; #${n} builds it`);
  // A pick again (the button is gone, the hub refuses any other): no second build item, and it says which.
  const before = (await items()).length;
  await page.evaluate(() => deskAct("d5", "pick", { dataset: { on: "C" } }));
  await expect(page.locator(".toast").last()).toContainText(`Option B is picked already: it's being built as #${n}`);
  expect((await items()).length).toBe(before);
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    if (process.env.SHOT_DIR) await page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-281-${theme}.png` });
  }
});
