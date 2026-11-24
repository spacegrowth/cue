// The Desk: Add session, the lead's chat (its past, ↑ ↓, DONE lines, Read), an opened item (its session's chat with
// its bar, or its bare page; questions, approvals), and the Desk's marks on the Board.
import { test, expect, pick, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";
import { openSessions, statusOf, openMore } from "./desk.mjs";

/** A session of cue's that isn't on its Desk ("spare", holding nothing). */
const spare = (cue) => cue.change((st) => st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", thread: [] }));
/** The left card's groups and rows, as "# group" and session ids. */
const leftRows = (page) => page.locator(".dk-sessions .dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-sdiv") ? "—" : el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}` : el.dataset.deskChatOf || el.dataset.deskCrew).filter(Boolean));
/** An opened item's page in the middle (#748): its session's chat with its bar, or (no session) its bare page. */
const itemPage = (page) => page.locator(".dk-c > .dk-right .dkn-sp");
/** Scrolls the part of `loc` (or what holds it) that scrolls to `y`; its scrollTop after. */
const scrollTo = (loc, y) => loc.evaluate((el, y) => { let e = el; while (e && e.scrollHeight <= e.clientHeight + 1) e = e.parentElement; if (!e) return 0; e.scrollTop = y; return e.scrollTop; }, y);

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
  // The Desk's sessions at work are in its sessions column at the left (#738). All sessions takes the lead's chat's
  // place, so the column is looked at back on the lead's chat, Esc closing the list.
  const workerInSide = page.locator('.dkn-side [data-desk-chat-of="spare"]');
  const backToLead = async () => { await page.keyboard.press("Escape"); await expect(page.locator(".dk-sessions")).toHaveCount(0); };
  const reopen = async () => { await openSessions(page); await add.click(); await expect(picker).toBeVisible(); };
  await expect(page.locator('.dkn-side [data-desk-chat-of="host"]')).toBeVisible();
  await expect(workerInSide).toHaveCount(0);
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
  await expect(workerInSide).toHaveCount(1);
  await reopen();
  await spareBox.uncheck();
  await expect.poll(() => leftRows(page)).toEqual(["# Working", "host"]);
  await page.keyboard.press("Escape");
  await backToLead();
  await expect(workerInSide).toHaveCount(0);
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

// #789 (22a2d7c9a8): Desk moves left the chats; a DONE's Ready to check card is in the Desk's Activity tab, not the
// lead's chat, and so is the quiet line once it's checked.
test("a DONE is one Ready to check card in the Desk's Activity, none in the lead's chat: its item, its age, Check it › opens the item, whose bar has what to try, Looks right and Send back; once checked a quiet line says what you said", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // None in the lead's chat (#789).
  await expect(page.locator('.dk-chat [data-mg="check"]')).toHaveCount(0);
  await expect(page.locator(".dk-chat .dk-chk, .dk-chat .dk-chk-done")).toHaveCount(0);
  await page.evaluate(() => deskOpenActivity());
  // The message family's card from Cue (#501): READY TO CHECK, the item, its age; its title; Check it › in the foot.
  const line = page.locator('.dk-right .dk-moves [data-mg="check"]');
  await expect(line).toHaveCount(1);
  await expect(page.locator(".dk-right .dk-moves .dk-chk")).toHaveCount(0);
  await expect(line.locator(".mg-tag")).toHaveText("Ready to check");
  await expect(line.locator(".mg-h .mg-id")).toHaveText("Feature #d2");
  await expect(line.locator(".mg-h .mg-id svg.dk-kic")).toHaveCount(1);
  await expect(line.locator(".mg-h .mg-age")).toHaveText(/^\d+[smhd]$/);
  await expect(line.locator(".mg-t")).toHaveText("Quick phrases at the right of Send");
  await expect(line.locator(".mg-f .mg-lk")).toHaveText("Check it ›");
  // No edge stripe, the family's body size, no buttons of its own but the two that open the item.
  await expect(line).toHaveCSS("border-left-width", "0px");
  await expect(line.locator(".mg-t")).toHaveCSS("font-size", "14px");
  await expect(line.locator("[data-desk-act]")).toHaveCount(0);
  // Compact: its title on one line of the body size.
  expect(await line.locator(".mg-t").evaluate((el) => el.getBoundingClientRect().height)).toBeLessThan(23);

  // Not installed yet: nothing to try, so it's waiting for install, on its line, row and bar (#130).
  await page.evaluate(() => { state.desk.installed_ms = 1793984280000 - 60000; renderMain(); });
  await expect(line.locator(".mg-tag")).toHaveText("Waiting for install");
  const row = page.locator('.dkn-row[data-desk-item="d2"]');
  await expect(statusOf(page, "d2")).toHaveText("Waiting for install");
  await expect(row.locator(".dk-ok")).toHaveCount(0);
  const lead = page.locator(".dk-right .foot:not(.held) .composer textarea");
  const leadTab = page.locator('.dk-right .dk-tab[data-desk-tab="lead"]');
  await leadTab.click();
  await lead.fill("a word for the lead");
  await page.evaluate(() => deskOpenActivity());
  await line.locator(".mg-lk").click();
  // It opens its holder's chat with its bar (#744): while it waits, no Looks right there.
  const bar = itemPage(page).locator(".dkn-ib");
  await expect(bar).toHaveCount(1);
  await expect(bar).not.toHaveClass(/\bcheck\b/);
  await expect(bar.locator('[data-desk-act="looks_right"]')).toHaveCount(0);
  // Closing it, the lead's box is back as you left it.
  await page.evaluate(() => deskShow(null));
  await leadTab.click();
  await expect(lead).toHaveValue("a word for the lead");
  await page.evaluate(() => deskOpenActivity());
  // Installed before the DONE but after its work last changed (you install, then say DONE): it's in, too (#115).
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").checks[0].edited = 1793984280000 - 120000; });
  await page.evaluate(() => { state.desk.installed_ms = 1793984280000 - 60000; renderMain(); });
  await expect(line.locator(".mg-tag")).toHaveText("Ready to check");

  // Check it › opens the item: its bar has what to try, Looks right and Send back.
  await line.locator(".mg-lk").click();
  await expect(bar).toHaveClass(/\bcheck\b/);
  await expect(bar.locator('[data-desk-act="looks_right"]')).toHaveText("Looks right");
  await expect(bar.locator('[data-desk-act="send_back"]')).toHaveText("Send back");
  await expect(bar.locator(".dkn-ibsee")).toHaveText(/^Try: \S/);

  // Send back opens a box of its own in the bar; with nothing typed its Send asks why, and sends nothing.
  await bar.locator('[data-desk-act="send_back"]').click();
  await expect(page.locator(".dk-right .dkn-ibback .dk-sb-l")).toHaveText("Why send it back?");
  await page.locator('.dk-board [data-desk-act="back_go"]').click();
  await expect(page.locator(".toast")).toContainText("Say why it goes back first");
  expect((await cue.calls("desk_item_act")).filter((c) => c.args.act === "send_back")).toEqual([]);
  // Its words go to the item's holder only; nothing goes to the lead, and the lead's box is left as it was.
  await page.locator(".dk-right .dkn-ibback textarea").fill("The arrows still skip one");
  await page.locator('.dk-board [data-desk-act="back_go"]').click();
  await cue.called("desk_item_act", { id: "d2", act: "send_back", text: "The arrows still skip one" });
  expect(await cue.calls("desk_send")).toEqual([]);
  await page.evaluate(() => deskShow(null));
  await leadTab.click();
  await expect(lead).toHaveValue("a word for the lead");

  // Once checked, Activity says what you said, quiet and centred; the last one's Undo after it (#789: not the chat).
  await cue.change((s) => { const d = s.desk.projects.find((x) => x.project === "cue"); Object.assign(d.items.find((i) => i.id === "d2"), { state: "to_check" }); d.checks[0].state = "done"; });
  await expect(page.locator(".dk-chat .dk-chk-done")).toHaveCount(0);
  await page.evaluate(() => deskOpenActivity());
  const said = page.locator(".dk-right .dk-moves .dk-chk-done");
  await expect(line).toHaveCount(0);
  await expect(said).toHaveText("You · looked at #d2 and said it looks right · Undo · back to check");
  await expect(said.locator("svg.dk-kic")).toHaveCount(1);
  await expect(said.locator(".dk-cl-t")).toHaveCSS("text-align", "center");
  // Sent back (#410): the item's title and what you wrote, in a line; "no note" when you wrote none.
  await cue.change((s) => { Object.assign(s.desk.projects.find((d) => d.project === "cue").checks[0], { state: "back", back: "The arrows still skip one", back_pics: 2, back_by: "You" }); });
  const title = await page.evaluate(() => deskItem("cue", "d2").title);
  await expect(said).toHaveText(`You · sent #d2 ${title} back: “The arrows still skip one” · 2 pictures`);
  await cue.change((s) => { Object.assign(s.desk.projects.find((d) => d.project === "cue").checks[0], { back: "", back_pics: 0 }); });
  await expect(said).toHaveText(`You · sent #d2 ${title} back · no note`);
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

// #779: #778 was finishing (Cue landing it) and nobody had pressed Send back, yet the chat said "You · sent #778 … back
// · no note". A DONE past Ready to check shows under whoever moved it: Cue while it finishes, Cue or the lead when they
// sent it back, You only for your own Send back. Since #789 (22a2d7c9a8) these lines are in the Desk's Activity, not the
// lead's chat.
test("a DONE past Ready to check shows under who moved it: Cue while it finishes, Cue or the lead when they sent it back, You only for your Send back", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator('.dk-chat [data-mg="check"]')).toHaveCount(0);
  await page.evaluate(() => deskOpenActivity());
  await expect(page.locator('.dk-right .dk-moves [data-mg="check"]')).toHaveCount(1);
  const title = await page.evaluate(() => deskItem("cue", "d2").title);
  const said = page.locator(".dk-right .dk-moves .dk-chk-done");
  const set = (c) => cue.change((s, c) => { Object.assign(s.desk.projects.find((d) => d.project === "cue").checks[0], { back: "", back_pics: 0, back_by: "", ...c }); }, c);
  // Finishing, nobody sent it back: Cue is landing it, and no "You", no "sent back", no "no note".
  await set({ state: "finishing" });
  await expect(said).toHaveText(`Cue · finishing #d2 ${title}`);
  // Cue sent it back (its landing failed): under Cue, with Cue's why.
  await set({ state: "working", back: "its install failed", back_by: "Cue" });
  await expect(said).toHaveText(`Cue · sent #d2 ${title} back: “its install failed”`);
  // The lead routed it back: under the lead.
  await set({ state: "working", back: "the footer is left", back_by: "web-lead" });
  await expect(said).toHaveText(`web-lead · sent #d2 ${title} back: “the footer is left”`);
  // Your own Send back: under You.
  await set({ state: "working", back: "The arrows still skip one", back_by: "You" });
  await expect(said).toHaveText(`You · sent #d2 ${title} back: “The arrows still skip one”`);
  // Sent back before Cue kept who did it: it only says it went back, under nobody.
  await set({ state: "working", back: "old words" });
  await expect(said).toHaveText(`#d2 ${title} went back: “old words”`);
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
  // It opens #d1: its row lit, its holder's chat with its bar in the middle (#744).
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "d1"]);
  await expect(page.locator('.dkn-card .dkn-row.on[data-desk-item="d1"]')).toHaveCount(1);
  await expect(itemPage(page).locator(".dkn-ib")).toHaveCount(1);
});

test("a chat opened on the Desk takes the cursor in its box: the lead's, an item's; not from the keys on the list (#154)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const box = page.locator(".dk-right :is(.dk-box, .foot:not(.held) .composer) textarea[data-text]");
  await expect(box).toBeFocused();
  await page.locator('[data-desk-item="d2"]').click();
  await expect(itemPage(page)).toBeVisible();
  await expect(box).toBeFocused();
  // Back on the lead's chat, then off the Desk and back: its box again.
  await page.locator(".dk-right .dk-back").first().click();
  await expect(itemPage(page)).toHaveCount(0);
  await expect(box).toBeFocused();
  // Enter on a row in the list opens it and leaves the keys on the list.
  await page.locator('[data-desk-item="d4"]').focus();
  await page.keyboard.press("Enter");
  await expect(itemPage(page)).toBeVisible();
  // The item opens in the middle, the list stays at the right, so the keys stay on its row, not the box.
  await expect(page.locator('.dk-items [data-desk-item="d4"]')).toBeFocused();
  await expect(box).not.toBeFocused();
});

test("Looks right is in an item's bar over its chat, in reach however far the chat is scrolled; Send back with it (#163, #200)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dkn-card .dkn-row[data-desk-item="d2"] .dkn-tt').click();
  // In the bar over its chat (#748): it stays put however far the chat scrolls; Send back with it.
  const bar = itemPage(page).locator(".dkn-ib.check"), top = bar.locator('[data-desk-act="looks_right"]');
  const chat = itemPage(page).locator('[data-desk-chat="lead"]');
  await expect(top).toHaveText("Looks right");
  await expect(chat.locator('[data-desk-act="looks_right"]')).toHaveCount(0);
  await expect(bar.locator('[data-desk-act="send_back"]')).toBeVisible();
  const at = await top.boundingBox();
  for (const y of [0, 600, 99999]) {
    const got = await scrollTo(chat, y);
    if (y) expect(got).toBeGreaterThan(0);
    expect(await top.boundingBox()).toEqual(at);
  }
  // Clicked there, its few seconds to take it back are there too.
  await top.click();
  await expect(bar.locator(".dk-soon")).toHaveText("Done · Undo · back to check");
});

test("Looks right waits a moment with \"Done · Undo\" on the item and in a toast; Undo then leaves it to check, nothing sent", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Looks right in the opened item's bar (#748).
  await page.locator('.dk-items [data-desk-item="d2"] .dkn-tt').click();
  const bar = itemPage(page).locator(".dkn-ib"), close = bar.locator('[data-desk-act="looks_right"]');
  await close.click();
  await expect(bar.locator(".dk-soon")).toHaveText("Done · Undo · back to check");
  await expect(page.locator(".toast")).toHaveText("Done · Undo · back to check");
  await page.locator(".toast .toast-undo").click();
  await expect(page.locator(".toast")).toHaveText("Still to check");
  await expect(close).toBeVisible();
  await page.waitForTimeout(3300);
  expect(await cue.calls("desk_looks_right")).toEqual([]);
  // Undo on the item does the same; left alone, it goes.
  await close.click();
  await bar.locator('.dk-soon [data-desk-act="looks_undo"]').click();
  await expect(close).toBeVisible();
  await close.click();
  expect(await cue.calls("desk_looks_right")).toEqual([]);
  await cue.called("desk_looks_right", { id: "d2" });
});

test("Waiting on you is never red, with items or at 0; an open item takes the solid highlight with its words in the highlight's ink", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const you = page.locator('.dkn-card .dkn-gh[data-desk2b-group="you"]');
  const deny = await page.evaluate(() => { const t = document.createElement("i"); t.style.color = "var(--deny)"; document.body.append(t); const c = getComputedStyle(t).color; t.remove(); return c; });
  expect(await you.evaluate((e) => getComputedStyle(e).color)).not.toBe(deny);
  await cue.change((st) => { for (const i of st.desk.projects.find((d) => d.project === "cue").items) if (["needs_you", "to_check"].includes(i.state)) i.state = "done"; });
  // At 0 its head goes (only groups with something in them are drawn).
  await expect(you).toHaveCount(0);
  // The open item: its row lit, its page in the middle.
  await page.locator('[data-desk-item="d3"]').click();
  await expect(page.locator('.dkn-card .dkn-row.on[data-desk-item="d3"]')).toHaveCount(1);
  await expect(itemPage(page)).toBeVisible();
});

test("an open item's box keeps the cursor: through redraws, and after Enter sends it, as you go on typing", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  const box = page.locator(".dk-right .foot:not(.held) .composer textarea");
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
  await expect(itemPage(page).locator(".dkn-ih1")).toContainText("Tidy the Settings copy");
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
  // The row only says it asks (#738); Allow and No are at the opened item's foot.
  for (const [index, behavior] of ["allow", "deny"].entries()) {
    const id = `row-approval-${index}`;
    await cue.change((st, id) => {
      const item = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1");
      Object.assign(item.ask, { item_id: id, kind: "permission", text: "Run npm test" });
      st.items.push({ ...st.items.find((i) => i.id === "n-focus-ask"), id, kind: "permission", tool_name: "Bash", tool_input: { command: "npm test" } });
    }, id);
    await expect(statusOf(page, "d1")).toHaveText("Needs your OK");
    await expect(row.locator("button")).toHaveCount(0);
    await row.click();
    await page.locator(`.dk-right .act-card [data-act="${behavior}"][data-id="${id}"]`).click();
    await cue.called("respond", { id, decision: { behavior } });
    await page.locator(".dk-right .dkn-sback .dk-back").click();
  }
});

test("an approval in Waiting on you: the whole command is in the opened item (#298)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const long = "cd /private/tmp/scratch/l2-i && cat headmac.txt bodylistmac.txt footmac.txt > project/L2IListMac.dc.html && rm bodylistmac.txt headmac.txt footmac.txt && grep -c x-dc project/L2IListMac.dc.html";
  await cue.change((st, long) => {
    const item = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1");
    Object.assign(item.ask, { item_id: "long-approval", kind: "permission", text: long });
    st.items = st.items.filter((i) => i.id !== "long-approval");
    st.items.push({ ...st.items.find((i) => i.id === "n-focus-ask"), id: "long-approval", kind: "permission", tool_name: "Bash", tool_input: { command: long } });
  }, long);
  await expect(statusOf(page, "d1")).toHaveText("Needs your OK");
  await page.locator('.dk-items [data-desk-item="d1"]').click();
  await expect(page.locator(".dk-right .dk-ask")).toContainText("bodylistmac.txt headmac.txt footmac.txt");
});

test("files an item's words name are file chips at the words' size; a command or version stays code (#305)", async ({ page, cue }) => {
  await cue.open("desk");
  // No live session: its page shows what it was added with (#748).
  await cue.change((s) => { Object.assign(s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2"), { words: "Moved out of `tests/ui`, changed `ui/desk.js`, ran `npx playwright test`, read `/Users/me/dev/cue/src-tauri/src/hub/desk.rs`; `0.1.22` stays.", holder: { kind: "session", sid: "gone", name: "cue-old" } }); });
  await openDesk(page);
  await page.locator('.dk-items [data-desk-item="d2"]').click();
  const words = page.locator('.dk-right .dkn-sp[data-desk-page="item"] .dkn-ibare .dk-md');
  const chips = words.locator(".dk-fchip");
  await expect(chips).toHaveText(["tests/ui", "ui/desk.js", "…/src/hub/desk.rs"]);
  await expect(words.locator("code")).toHaveText(["npx playwright test", "0.1.22"]);
  const size = (l) => l.evaluate((el) => parseFloat(getComputedStyle(el).fontSize));
  const at = await size(words.last());
  for (const l of [chips.first(), words.locator("code").first()]) expect(await size(l)).toBeLessThan(at);
  for (const box of await chips.evaluateAll((els) => els.map((el) => el.getBoundingClientRect().height))) expect(box).toBeLessThan(26);
});

test("an item's question opens at the page's foot; the row only says it asks", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // A question with nothing suggested: the row says it asks, with no buttons (#738); opened, its foot has the whole card.
  await expect(statusOf(page, "d1")).toHaveText("Asks you");
  await expect(page.locator('.dk-items [data-desk-item="d1"] button')).toHaveCount(0);
  await page.locator('.dk-items [data-desk-item="d1"]').click();
  const foot = page.locator(".dk-right .dk-ask");
  // Its card is the foot, in the box's place.
  await expect(page.locator(".dk-right .foot:not(.held) .composer")).toHaveCount(0);
  await foot.locator("[data-pick]", { hasText: "Keep it in the box" }).click();
  await foot.locator('[data-act="submit-answers"]').click();
  await cue.called("respond");
  await cue.change((st) => {
    st.items = st.items.filter((i) => i.id !== "n-focus-ask");
    const d1 = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1");
    d1.state = "working"; d1.ask = null;
  });
  await expect(foot).toHaveCount(0);
  expect(await page.evaluate(() => [view, deskOpen?.kind, deskOpen?.id])).toEqual(["desk", "item", "d1"]);
  // A turn that ended on it: its whole last paragraph, and a box to its holder at the foot.
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "site");
    s.state = "waiting";
    s.thread = [...(s.thread || []), { role: "agent", text: "Moved the box.\n\nIf the box stays put, I'll write DONE.\nIf it still jumps, tell me what you did just before.", at_ms: Date.now() }];
    const d1 = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1");
    d1.state = "needs_you"; d1.ask = null; d1.holder = { kind: "session", sid: "site", name: "site-lead" };
  });
  // Its reply is the question (#418, B): it shows once, in its chat (#744).
  await expect(itemPage(page).locator('[data-desk-chat="site"] .cv-agent').last()).toContainText("If it still jumps, tell me what you did just before.");
  const box = page.locator(".dk-right .foot:not(.held) .composer textarea");
  await expect(page.locator(".dk-right .foot:not(.held) .composer .field-to1")).toHaveText("To site-lead");
  await expect(page.locator(".dk-right .foot:not(.held) .composer .field-to2")).toHaveText("#d1");
  await expect(box).toHaveAttribute("placeholder", "Reply box loses…");
  await box.fill("It stays put now");
  await page.locator(".dk-right [data-desk-send]").click();
  // Sent as about this item, so its session answers on it (#176).
  await cue.called("desk_item_act", { id: "d1", act: "note", text: "It stays put now" });
  // The window hears of it shortly after the call, as from the hub.
  await expect.poll(() => page.evaluate(() => deskOf("cue").decisions.find((c) => c.item_id === "d1" && c.delivery)?.delivery)).toMatchObject({ session_id: "site", text: "About #d1 (Reply box loses focus after Send): It stays put now" });
  expect(await page.evaluate(() => [view, deskOpen.kind])).toEqual(["desk", "item"]);
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
  // About an item its session holds: its phrases (sent by its normal flow, desk-quick.spec) but not the lead's
  // suggestion (those words aren't the lead's own next prompt).
  await page.locator('[data-desk-item="d5"]').click();
  await expect(page.locator(".dk-right .foot:not(.held) .composer .field-to1")).toHaveText("To host-lead");
  await expect(page.locator(".dk-right .foot:not(.held) .composer .field-to2")).toHaveText("#d5");
  await expect(page.locator(".dk-right .foot:not(.held) .composer .quick .qp")).toHaveCount(2);
  await expect(page.locator(".dk-right .foot:not(.held) .composer .sug-line")).toBeHidden();
  // Held by a crew member: what you type goes to it through its lead, and its box has no quick phrases.
  await page.locator('[data-desk-item="d3"]').click();
  await expect(page.locator(".dk-right .foot:not(.held) .composer .field-to")).toContainText("search-centre");
  await expect(page.locator(".dk-right .foot:not(.held) .composer .quick .qp")).toHaveCount(0);
  await expect(page.locator(".dk-right .foot:not(.held) .composer .sug-line")).toBeHidden();
});

test("a reply that asks for set words (Reply \"…\") offers them as the suggested reply: the lead's chat, an opened item; else Claude's own (#186)", async ({ page, cue }) => {
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
  // An item ready to check (it goes back to its holder) whose chat's newest message asks (curly quotes too): its words,
  // Tab types them in, Enter sends. Its chat is its holder's (#744): #d2's is the lead's.
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "lead"); s.thread = [...s.thread, { role: "agent", text: "#153 isn't done. What's left is putting it into main.\n\nReply “merge” and I'll finish it.", at_ms: Date.now() + 2, images: [] }]; });
  await page.locator('[data-desk-item="d2"] .dkn-tt').click();
  await expect(itemPage(page)).toBeVisible();
  await expect(ghost).toHaveText("SuggestedmergeTab");
  await ta.focus();
  await page.keyboard.press("Tab");
  await expect(ta).toHaveValue("merge");
  await page.keyboard.press("Enter");
  await cue.called("desk_item_act", { id: "d2", act: "send_back", text: "merge" });
  // Answered: no suggestion till it asks again.
  await expect(ghost).toBeHidden();
});

test("what you write on the page of an item no session has goes to the lead about it; its page stays open (#136)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  const box = page.locator(".dk-right .foot:not(.held) .composer textarea");
  await box.fill("Keep the search box centred on narrow windows too");
  await page.locator(".dk-right [data-desk-send]").click();
  await cue.called("desk_item_act", { id: "d6", act: "wrote_lead", text: "Keep the search box centred on narrow windows too" });
  expect(await page.evaluate(() => [view, deskOpen?.kind, deskOpen?.id])).toEqual(["desk", "item", "d6"]);
  await expect(page.locator('.dk-right .dkn-sp[data-desk-page="item"]')).toBeVisible();
});

test("an item opens its holder's chat here on the Desk, never the board; ‹ goes back; from All sessions, back to it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const sid = await page.evaluate(() => state.sessions.find((s) => s.project === "cue" && s.session_id !== "lead").session_id);
  // Not working: on a working session's chat Esc is its Stop first (#494, desk-esc.spec.mjs).
  await cue.change((st, s) => { st.sessions.find((x) => x.session_id === s).state = "waiting"; Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4"), { holder: { kind: "session", sid: s, name: "cue-117" } }); }, sid);
  await page.locator('[data-desk-item="d4"]').click();
  expect(await page.evaluate(() => view)).toBe("desk");
  await expect(page.locator(`.dk-right .dkn-sp [data-desk-chat="${sid}"]`)).toBeVisible();
  // ‹ goes back to the lead's chat; so does Esc.
  await page.locator(".dk-right .dkn-sback .dk-back").click();
  await expect(page.locator(".dk-c > .dk2b-chat")).toHaveCount(1);
  await page.locator('[data-desk-item="d4"]').click();
  await expect(page.locator(`.dk-right .dkn-sp [data-desk-chat="${sid}"]`)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator(".dk-c > .dk2b-chat")).toHaveCount(1);
  // From All sessions (#666), in the lead's chat's place as everything open is: back to All sessions, still there.
  await openSessions(page);
  await page.evaluate((s) => document.querySelector(`.dk-sessions [data-desk-chat-of="${s}"]`).click(), sid);
  await expect(page.locator(".dk-right .dkn-sback .dk-back")).toHaveText("‹ All sessions");
  await page.locator(".dk-right .dkn-sback .dk-back").click();
  await expect(page.locator(".dk-sessions")).toBeVisible();
  expect(await page.evaluate(() => view)).toBe("desk");
});

test("what you send on an opened item shows in its chat at once, before it comes back as a step (#140)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("Narrow windows too");
  await page.locator(".dk-right [data-desk-send]").click();
  // Its page is its holder's chat (#744): yours in it at once, as you wrote it.
  const mine = itemPage(page).locator('[data-desk-chat="lead"] .cv-you', { hasText: "Narrow windows too" });
  await expect(mine).toHaveCount(1);
  // The lead is at work: Cue holds it till its turn ends, and the line says so, with its controls (#422).
  await expect(mine.locator(".msg-line")).toHaveText("◌ Not sent yet · Cue sends it when its turn ends · Edit · Send now · Cancel");
  // The hub keeps it as a step (#437): still one message.
  await cue.called("desk_item_act", { id: "d3", act: "note", text: "Narrow windows too" });
  await expect(mine).toHaveCount(1);
});

test("an opened item's head (back, its session's top bar, its bar) stays at the top as its chat scrolls (#142, #200, #666)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d4"]').click();
  const sp = itemPage(page), back = sp.locator(".dkn-sback"), head = sp.locator(".ap-head"), bar = sp.locator(".dkn-ib");
  const chat = sp.locator('[data-desk-chat="lead"]');
  await expect(chat).toBeVisible();
  const before = await Promise.all([back, head, bar].map((l) => l.boundingBox()));
  expect(await scrollTo(chat, 99999)).toBeGreaterThan(200);
  expect(await Promise.all([back, head, bar].map((l) => l.boundingBox()))).toEqual(before);
  // The chat goes under them, not over them.
  const cb = await sp.locator(".dkn-spm").boundingBox(), b = before[2];
  expect(cb.y).toBeGreaterThanOrEqual(b.y + b.height - 1);
});

test("an item no session has shows its You'll see on its page; its Done when isn't shown (#140, #200, #382)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d4").holder = { kind: "session", sid: "gone", name: "cue-117" }; });
  await page.locator('[data-desk-item="d4"]').click();
  const bare = page.locator('.dk-right .dkn-sp[data-desk-page="item"]');
  const sec = bare.locator(".dk-sec", { has: page.locator(".dkn-isee") });
  await expect(sec.locator("h3")).toHaveText("You'll see");
  await expect(sec.locator(".dkn-isee li")).toContainText(["The rail's dot blinks only for a session you wrote to"]);
  await expect(page.locator(".dk-right")).not.toContainText("desk.spec checks it");
});

test("the chats' pages: the Dispatcher's NOW (its state, routing, with sessions, Check #…); a session's chat has its top bar and what it's on (#200)", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  await page.setViewportSize({ width: 1800, height: 900 });
  const dir = process.env.SHOT_DIR || process.env.TMPDIR || "/tmp";
  // For the shots: a new item from Cue, and the lead's own lines for it, as its chat shows them.
  await page.evaluate(() => {
    const t = sessionOf("lead").thread;
    t.push({ at_ms: Date.now() - 60000, role: "you", text: '<desk project="cue">\n3 open\n</desk>\n\nNew item #d6 (task): Tidy the Settings copy | done when: every label fits | from you\nRoute each now: ROUTE it to a session, or -> new.' });
    t.push({ at_ms: Date.now(), role: "agent", text: "ROUTE: #d6 -> host-lead | why: it has the Settings page warm\nDONE: #d3\nDESIGNS: #d6 https://example.com/mocks" });
    renderMain();
  });
  // The Dispatcher's page (#200): its header names it; its tabs (its chat, named for the lead, and Decisions) over the
  // chat. 692dc421c7: the lead's chat has no panel (no NOW of its own, no Hide panel), even this wide; the strip over
  // the chat says how many wait on you, a chip each, and Check #… for the first that waits.
  const pg = page.locator('.dk-right .dk-pg[data-desk-page="dispatcher"]');
  await expect(pg.locator("h1")).toHaveText("Dispatcher · web-lead");
  await expect(pg.locator(".dk-ptabs .dk-tab.on")).toHaveText("web-lead");
  await expect(pg.locator(".dk-ptabs")).toContainText("Decisions");
  await expect(pg.locator(".dk-pnow")).toHaveCount(0);
  await expect(pg.locator("[data-desk-panel]")).toHaveCount(0);
  const now = page.locator(".dk2b-chat .dk2b-strip");
  await expect(now).toBeVisible();
  // What waits on you, as Items' WAITING ON YOU has it: #d1's question, then #d2 ready to check.
  await expect(now.locator(".dk2b-wn")).toHaveText("2 wait on you");
  await expect(now.locator(".dk2b-chip[data-desk-chk-item]")).toHaveText(["#d1", "#d2"]);
  await expect(now.locator(".dk2b-check")).toHaveText("Check #d1");
  await page.screenshot({ path: `${dir}/cue-desk-lead-chat-light.png` });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({ path: `${dir}/cue-desk-lead-chat-dark.png` });
  await page.emulateMedia({ colorScheme: "light" });

  // The review stays available, and clearing the question removes the waiting action.
  await page.evaluate(() => { state.items = state.items.filter((i) => i.id !== "n-focus-ask"); renderMain(); });
  // #d1 leaves; #d2, ready to check, still waits: Check goes to it.
  await expect(now.locator(".dk2b-check")).toHaveText("Check #d2");
  // Working (its ask answered): #d2 still waits, on its own (the strip has no "Routing #…" words since 692dc421c7).
  await page.evaluate(() => { sessionOf("lead").state = "working"; state.items = state.items.filter((i) => i.session_id !== "lead"); renderMain(); });
  await expect(now.locator(".dk2b-wn")).toHaveText("1 waits on you");
  await expect(now.locator(".dk2b-check")).toHaveText("Check #d2");
  await expect(page.locator('.dkn-row[data-desk-item="d2"] .dk-ok')).toBeVisible();
  // None waits: no Check button; the strip's Working pill counts the sessions at work instead.
  await page.evaluate(() => { const d = deskOf("cue"); d.items.find((i) => i.id === "d2").state = "done"; renderMain(); });
  await expect(now.locator(".dk2b-check")).toHaveCount(0);
  await expect(now.locator("[data-desk2b-work]")).toHaveText(/^Working \d+/);

  // A session holding an item: its chat with the Board's top bar for it (#744); under it what it's on, its number
  // opening the item.
  await page.locator('.dkn-side [data-desk-chat-of="host"]').click();
  const sp = page.locator('.dk-right .dkn-sp[data-desk-page="session"]');
  await expect(sp).toHaveCount(1);
  await expect(sp.locator(".ap-head")).toContainText("host-lead");
  await expect(sp.locator(".dkn-smeta")).toHaveText(/^on #d5 · /);
  if (process.env.SHOT_DIR) for (const c of ["light", "dark"]) { await page.emulateMedia({ colorScheme: c }); await sp.locator(".ap-head").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-sess-held-${c}.png` }); }
  await page.emulateMedia({ colorScheme: "light" });
  await sp.locator(".dkn-smeta .dk-sch-t").click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "d5"]);
  await page.locator('.dkn-side [data-desk-chat-of="host"]').click();
  await expect(sp).toHaveCount(1);
  await page.screenshot({ path: `${dir}/cue-desk-session-chat-light.png` });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({ path: `${dir}/cue-desk-session-chat-dark.png` });
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

test("on a named Desk, a session's chat names the items it holds on that Desk, not on its folder's Desk", async ({ page, cue }) => {
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
  const meta = page.locator('.dk-right .dkn-sp[data-desk-page="session"] .dkn-smeta');
  await expect(meta).toHaveCount(1);
  await expect(meta.locator(".dk-sch-t")).toHaveText(["#n1"]);
  await expect(meta).toHaveText("on #n1 · Only on Feature A");
  await meta.locator(".dk-sch-t").click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id, deskOpen.project])).toEqual(["item", "n1", "nd1"]);
});

test("a message you send on an opened item is its chat's last, under its holder's latest words (#160); with its holder at work, its now under it (#165)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Its holder between turns, its latest words the last of its chat (#744: an item's chat is its holder's).
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "host");
    s.state = "idle";
    s.thread = [...(s.thread || []), { role: "agent", text: "Moved the search into the header's middle.", at_ms: Date.now() - 60000, images: [] }];
  });
  await page.locator('[data-desk-item="d5"]').click();
  const chat = itemPage(page).locator('[data-desk-chat="host"]'), rows = chat.locator(":scope > [data-key]");
  const latest = await chat.locator(".cv-agent .cv-text").last().textContent();
  await cue.override("desk_item_act", () => new Promise((r) => { window.__release = r; }));
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  await expect(chat.locator(".cv-you").last()).toContainText("Check the narrow window too");
  expect(await chat.evaluate((el, t) => { const ks = [...el.querySelectorAll(":scope > [data-key]")]; const you = ks.findLastIndex((k) => k.textContent.includes("Check the narrow window too")); const msg = ks.findLastIndex((k) => k.textContent.includes(t.trim().slice(0, 30))); return you > msg && msg >= 0; }, latest)).toBe(true);
  // Its holder at work: its "now" is last, under your message, as it answers it (#165).
  await cue.change((st) => { const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"); it.state = "working"; it.now_on = true; st.sessions.find((x) => x.session_id === "host").state = "working"; });
  await expect(chat.locator('[data-key="where"]')).toHaveCount(1);
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("And the wide one");
  await page.locator(".dk-right [data-desk-send]").click();
  await expect(rows.last()).toHaveAttribute("data-key", "where");
  await expect(rows.nth(-2)).toContainText("And the wide one");
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

test("tasks, bugs and features say on the row that they ask, and are answered on the opened item", async ({ page, cue }) => {
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
  // Each row says it asks, no buttons (#738); answered at the opened item's foot.
  for (const id of ["d1", "d2", "d3"]) {
    await expect(statusOf(page, id)).toHaveText("Asks you");
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
  // Its question is its holder's reply, in the item's chat once (#744): no box under it saying it again.
  const right = page.locator(".dk-right"), chat = itemPage(page).locator('[data-desk-chat="site"]');
  await expect(chat.locator(".cv-agent").last()).toContainText("Which + did you mean?");
  await expect(itemPage(page).getByText("Which + did you mean?")).toHaveCount(1);
  await expect(chat.locator(".cv-agent").last()).not.toContainText("DECIDED");
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
  const pane = page.locator('.dk-right .dkn-sp[data-desk-page="item"]');   // no session: its bare page (#748)
  await expect(pane).toContainText("Couldn't start");
  await expect(pane).toContainText("Cue couldn't start a new session for it: the folder is gone");
  await openMore(pane);
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveText("Retry");
  if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-start-failed.png` });
  await pane.locator('[data-desk-act="start_now"]').click();
  await cue.called("desk_item_act", { id: "f1", act: "start_now" });
  // Held by a session once it starts: the words go.
  expect(await page.evaluate(() => deskB2Status({ state: "working", holder: { sid: "x" }, start_failed: "old" })[0])).not.toContain("Couldn't start");
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
  // #789 (22a2d7c9a8): the hand-off left the session's chat (#744, no panel) for Activity: its session's Activity tab
  // has it, as its card, from Cue to the session.
  await expect(page.locator('.dk-right [data-desk-chat="host"]')).toHaveCount(1);
  await expect(page.locator('.dk-right [data-desk-chat="host"] .mg-hand')).toHaveCount(0);
  await expect(page.locator('.dk-right [data-desk-chat="host"] [data-mg="hand"]')).toHaveCount(0);
  await page.locator('.dk-right [data-desk-c-stab="activity"]').click();
  const card = page.locator(".dk-right .dkn-sact .dk-mv-ln .mg-hand");
  await expect(card).toHaveCount(1);
  await expect(card.locator("xpath=following-sibling::*[contains(@class,'dk-act-why')]")).toHaveText("Cue to host-lead");
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
  // Shot where it is on the screen: a locator's own shot finds the card, then shoots it, and a redraw between leaves it gone.
  await card.evaluate((el) => el.scrollIntoView({ block: "nearest" }));
  await page.screenshot({ clip: await card.boundingBox(), path: `${dir}/cue-desk-hand-light.png` });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({ clip: await card.boundingBox(), path: `${dir}/cue-desk-hand-dark.png` });
  await page.emulateMedia({ colorScheme: "light" });
  // Its number opens the item.
  await card.locator(".mg-id[data-desk-hand]").click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "d5"]);
});

test("what you write on an opened item goes to its session as about it and shows in its chat; its working row comes back (#176)", async ({ page, cue }) => {
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
  // Its page is its holder's chat (#744).
  const chat = itemPage(page).locator('[data-desk-chat="host"]');
  // Its session's turn ended on it: what you write on its page goes to its session as about it, and shows in its
  // chat as you wrote it.
  await cue.change((st) => {
    const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5");
    st.sessions.find((x) => x.session_id === it.holder.sid).state = "waiting";
    Object.assign(it, { state: "needs_you", now_on: true });
  });
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  await cue.called("desk_item_act", { id: "d5", act: "note", text: "Check the narrow window too" });
  await expect.poll(() => page.evaluate(() => deskOf("cue").decisions.find((c) => c.item_id === "d5" && c.delivery)?.delivery.text)).toMatch(/^About #d5 \(.+\): Check the narrow window too$/);
  await expect(chat.locator(".cv-you").last()).toContainText("Check the narrow window too");
  // Its session on it now: its working row comes back.
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").state = "working"; st.sessions.find((x) => x.session_id === "host").state = "working"; });
  await expect(chat.locator('[data-key="where"]')).toHaveCount(1);
  for (const theme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: theme });   // as the Mac switches it: a redraw puts Settings → Appearance (system) back over a data-theme set by hand
    if (process.env.SHOT_DIR) await page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-176-${theme}.png` });
  }
});
