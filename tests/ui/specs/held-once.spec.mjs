// A message sent while its session is busy shows once (#439): Cue keeps it, and the session's state can say so before
// the send itself returns. Its kept bubble ("Not sent yet") is the only copy then, never also a Sending one, and the
// same bubble turns into the sent message once it goes. In a session's chat, the Desk lead's chat and an item's page.
import { test, expect, openDesk } from "./cue.mjs";

/** `cmd` answered as the hub does for a busy session: kept (in the state at once), its answer held back till
 *  window.__go(). `sidOf`: the session it goes to, from its args; `textOf`: what's kept. */
const keepThenWait = (cue, cmd, sidOf, textOf) => cue.override(cmd, (args, [sidSrc, textSrc, lead]) => {
  const sid = (0, eval)(`(${sidSrc})`)(args), text = (0, eval)(`(${textSrc})`)(args);
  const s = CueBridge.world.state.sessions.find((x) => x.session_id === sid);
  s.state = "working";
  s.held = { role: "you", text: s.held ? `${s.held.text}\n\n${text}` : text, at_ms: s.held?.at_ms || Date.now(), images: [] };
  CueBridge.emit("state", CueBridge.world.snapshot());
  return new Promise((r) => { window.__go = () => r(lead ? { sid } : "kept in Cue: it goes when this turn ends"); });
}, [sidOf.toString(), textOf.toString(), cmd === "desk_send"]);
/** Session `sid`'s turn ends: its kept message goes and is in its thread. */
const goes = (cue, sid) => cue.change((st, sid) => {
  const s = st.sessions.find((x) => x.session_id === sid);
  (s.thread ||= []).push({ role: "you", text: s.held.text, at_ms: s.held.at_ms, images: [] });
  s.held = null; s.state = "waiting";
}, sid);
/** Every bubble of yours in `pane` with `text`: exactly one, held, then the same one delivered. */
async function once(page, cue, pane, text, sid) {
  const mine = pane.locator(".cv-you", { hasText: text });
  await expect(mine).toHaveCount(1);
  await expect(mine.locator(".msg-line")).toContainText("Not sent yet");
  await expect(mine.locator(".msg-line")).toContainText("Edit · Send now");
  await expect(pane.locator(".cv-you.msg-sending")).toHaveCount(0);
  // The send returns: still one.
  await page.evaluate(() => window.__go?.());
  await expect(mine).toHaveCount(1);
  await expect(pane.locator(".cv-you.msg-sending")).toHaveCount(0);
  // It goes: the same one, now sent.
  await goes(cue, sid);
  await expect(mine.locator(".msg-line")).toContainText(/^You · .+ ago/);
  await expect(mine).toHaveCount(1);
}

test("a session's chat: a message to it while it works shows once, and goes as that one", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await keepThenWait(cue, "send_to_session", (a) => a.sessionId, (a) => a.text);
  await page.locator('.working[data-session="eng"]').first().click();
  const box = page.locator('.active-pane textarea[data-text="s:eng"]');
  await box.fill("use the new pool");
  await box.press("Enter");
  await once(page, cue, page.locator(".active-pane"), "use the new pool", "eng");
});

test("the Desk lead's chat: a message to it while it works shows once, and goes as that one", async ({ cue, page }) => {
  await cue.open("desk");
  await openDesk(page);
  const lead = await page.evaluate(() => deskOf("cue").lead.sid);
  await keepThenWait(cue, "desk_send", () => deskOf("cue").lead.sid, (a) => `<desk project="cue">\n4 items.\n</desk>\n\n${a.text}`);
  await page.evaluate(() => { deskOpen = null; deskTab = "lead"; renderMain(); });
  const box = page.locator(".dk-right .foot:not(.held) .composer textarea");
  await box.fill("And the footer too");
  await box.press("Enter");
  await once(page, cue, page.locator(".dk-chat"), "And the footer too", lead);
});

test("an item's page: a note to its busy holder (the hub keeps it, hub/desk_act.rs note) shows once, and goes as that one", async ({ cue, page }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  const sid = await page.evaluate(() => { const h = deskItem("cue", "d3").holder; return h.lead_sid || h.sid; });
  const box = page.locator(".dk-right .dk-box textarea");
  await box.fill("Narrow windows too");
  await box.press("Enter");
  await once(page, cue, page.locator(".dk-right .dk-ichat"), "Narrow windows too", sid);
});
