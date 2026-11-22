// Desk spec (#498): Enter sends, a message sent to a busy session shows as kept and comes back into the box it went from
// (Edit or Esc), with its words intact and nothing typed since lost; Esc closes what's open before it stops anything.
// Sources: #494 (Esc pulls back a kept message, Esc twice stops a working session, on the lead's chat, an item's page and
// a session's page), #422 (Edit on a kept message, back into this page's box), #437 (an item's box keeps what you typed).
import { test, expect, openDesk, deskAllSessions, deskShowPanel } from "./cue.mjs";

/** desk_send as the hub answers it for a busy lead (ipc/desk.rs deliver + hub/send.rs keep): the Desk's summary block,
 *  a blank line, your words, kept in Cue till its turn ends. */
const leadKeeps = (cue) => cue.override("desk_send", (args) => {
  const w = CueBridge.world, d = w.state.desk.projects.find((x) => x.project === args.project), s = w.state.sessions.find((x) => x.session_id === d.lead.sid);
  const said = `<desk project="${args.project}">\n${d.items.length} items.\n</desk>\n\n${args.text.trim()}`;
  s.held = { role: "you", text: s.held ? `${s.held.text}\n\n${said}` : said, at_ms: Date.now(), images: [] };
  CueBridge.emit("state", w.snapshot());
  return { sid: s.session_id };
});

// #494 + "nothing typed is ever lost": Enter sends the lead's box; while the lead works its message is kept, and Esc
// brings back your words alone (not the Desk summary Cue put before them) ahead of what you've typed since.
test("the lead's chat: Enter sends; kept while it works; Esc brings back just your words, before what you typed since", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await leadKeeps(cue);
  const box = page.locator(".dk-right .dk-box textarea");
  await box.fill("use the new pool");
  await box.press("Enter");
  await cue.called("desk_send", { project: "cue", text: "use the new pool" });
  await expect(box).toHaveValue("");
  // Shown in the lead's chat as kept, not sent yet.
  const kept = page.locator(".dk-chat .cv-you", { hasText: "use the new pool" });
  await expect(kept).toHaveCount(1);
  await expect(kept.locator(".msg-line")).toContainText("Not sent yet");
  await box.fill("and the logs");
  await page.keyboard.press("Escape");
  await cue.called("unhold_message", { sessionId: "lead" });
  await expect(box).toHaveValue("use the new pool\n\nand the logs");
  await expect(box).toBeFocused();
  // Nothing to stop it but Esc twice: the box keeps every word.
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
});

// #422: Edit on the kept message in the lead's chat takes it back into the lead's box on the Desk, the one it went from.
test("the lead's chat: Edit on a kept message puts your words back in the lead's box on the Desk", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await leadKeeps(cue);
  const box = page.locator(".dk-right .dk-box textarea");
  await box.fill("rename the header");
  await box.press("Enter");
  const kept = page.locator(".dk-chat .cv-you", { hasText: "rename the header" });
  await expect(kept.locator(".msg-line")).toContainText("Not sent yet");
  await box.fill("first this");
  await kept.locator(".msg-line button", { hasText: "Edit" }).click();
  await cue.called("unhold_message", { sessionId: "lead" });
  await expect(box).toHaveValue("rename the header\n\nfirst this");
  await expect(page.locator(".dk-chat .cv-you", { hasText: "rename the header" })).toHaveCount(0);
});

// #494 on a session's page: what you send to it while it works is kept; Esc brings it back into that page's box, ahead
// of your draft; Edit does the same (#422).
test("a session's page: kept while it works, Edit brings it back into that page's box, draft kept after it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await page.locator(".dk-sessions .dk-srow:not(.crew)", { hasText: "host-lead" }).first().click();
  await expect(page.locator('.dk-right .dk-pg[data-desk-page="session"]')).toHaveCount(1);
  const box = page.locator(".dk-right .dk-box textarea");
  await box.fill("check the canvas port");
  await box.press("Enter");
  await cue.called("send_to_session", { sessionId: "host", text: "check the canvas port" });
  await expect(box).toHaveValue("");
  const kept = page.locator(".dk-right .cv-you", { hasText: "check the canvas port" });
  await expect(kept.locator(".msg-line")).toContainText("Not sent yet");
  await box.fill("then the logs");
  await kept.locator(".msg-line button", { hasText: "Edit" }).click();
  await cue.called("unhold_message", { sessionId: "host" });
  await expect(box).toHaveValue("check the canvas port\n\nthen the logs");
  await expect(page.locator('.dk-right .dk-pg[data-desk-page="session"]')).toHaveCount(1);
});

// #494 / #422 on an item's page: Esc takes back the note kept for its busy holder, without the "About #…" Cue added,
// ahead of a draft typed since.
test("an item's page: Esc takes back the note kept for its busy holder, your words alone, the draft after them", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  const box = page.locator(".dk-right .dk-box textarea");
  await box.fill("try the other port");
  await box.press("Enter");
  await cue.called("desk_item_act", { id: "d5", act: "note", text: "try the other port" });
  await expect(box).toHaveValue("");
  await expect(page.locator(".dk-right .dk-ichat .cv-you", { hasText: "try the other port" }).locator(".msg-line")).toContainText(/Waiting: .+ your note goes in next/);
  await box.fill("and 8080");
  await page.keyboard.press("Escape");
  await cue.called("unhold_message", { sessionId: "host" });
  await expect(box).toHaveValue("try the other port\n\nand 8080");
  await expect(page.locator(".dk-right [data-desk-page]")).toHaveCount(1);
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
});

// "Nothing typed is ever lost" + Esc closes what's open: on a page whose session isn't working and keeps nothing, Esc
// with words in the box leaves the page and the words alone; with the box empty it closes the page.
test("Esc with words in a page's box keeps the page and the words; empty, it closes the page", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.sessions.find((s) => s.session_id === "host").state = "waiting"; });
  await openDesk(page);
  await deskAllSessions(page);
  await page.locator(".dk-sessions .dk-srow:not(.crew)", { hasText: "host-lead" }).first().click();
  const sp = page.locator('.dk-right .dk-pg[data-desk-page="session"]');
  await expect(sp).toHaveCount(1);
  const box = page.locator(".dk-right .dk-box textarea");
  await box.fill("half a thought");
  await box.focus();
  await page.keyboard.press("Escape");
  await expect(sp).toHaveCount(1);
  await expect(box).toHaveValue("half a thought");
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
  expect(await cue.calls("send_to_session")).toHaveLength(0);
  // Back on the page later, the words are still there.
  await box.fill("");
  await page.keyboard.press("Escape");
  await expect(sp).toHaveCount(0);
});

// #494: Esc twice stops the working holder of an item's page; one Esc alone never does, and the page stays open.
test("an item's page: one Esc doesn't stop its working holder, the second within a moment does; the page stays", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  await page.keyboard.press("Escape");
  await page.waitForTimeout(200);
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
  await expect(page.locator(".dk-right [data-desk-page]")).toHaveCount(1);
  await page.keyboard.press("Escape");
  await cue.called("interrupt_session", { sessionId: "host" });
  await expect(page.locator(".dk-right [data-desk-page]")).toHaveCount(1);
});
