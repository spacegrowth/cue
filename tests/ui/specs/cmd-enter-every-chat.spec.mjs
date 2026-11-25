// ⌘↵ sends now in every chat's box (#861), from one key handling (box/send.js boxEnter): a session's chat on the Board,
// an item's page, a crew member's, a session's page and the lead's on the Desk. While the session a box writes to is at
// work, a quiet "⌘↵ send now" sits beside its Send; idle, it doesn't.
import { test, expect, openDesk } from "./cue.mjs";

const box = (page) => page.locator(".dk-right .foot:not(.held) .composer textarea");
const hint = (page) => page.locator(".dk-right .foot:not(.held) .composer .now-hint");
const toSession = (cue) => cue.calls("send_to_session").then((c) => c.map((x) => ({ sid: x.args.sessionId, text: x.args.text, now: !!x.args.now })));
const itemActs = (cue) => cue.calls("desk_item_act").then((c) => c.map((x) => ({ id: x.args.id, act: x.args.act, text: x.args.text })));

test("a session's chat on the Board: ⌘↵ sends now, ↵ queues, and the hint shows only while it works", async ({ page, cue }) => {
  await cue.open("board-waiting");
  await page.locator('.nrow[data-big="n1"]').click();
  const ta = page.locator(".active-pane .composer textarea"), h = page.locator(".active-pane .composer .now-hint");
  const sid = await ta.getAttribute("data-text").then((k) => k.slice(2));
  await cue.change((st, sid) => { st.sessions.find((x) => x.session_id === sid).state = "waiting"; }, sid);
  await expect(ta).toHaveCount(1);
  await expect(h).toHaveCount(0);
  // At work again: its finished turn's card is gone.
  await cue.change((st, sid) => { st.sessions.find((x) => x.session_id === sid).state = "working"; st.items = st.items.filter((i) => !(i.session_id === sid && i.kind === "waiting")); }, sid);
  await expect(h).toHaveText("⌘↵ send now");
  await expect(h).not.toHaveAttribute("title", /.*/);
  await ta.click();
  await page.keyboard.type("Stop and look at the footer");
  await page.keyboard.press("Meta+Enter");
  await expect.poll(() => toSession(cue)).toEqual([{ sid, text: "Stop and look at the footer", now: true }]);
  await ta.fill("then the header");
  await page.keyboard.press("Enter");
  await expect.poll(() => toSession(cue).then((c) => c[1])).toEqual({ sid, text: "then the header", now: false });
});

test("an item's page: ⌘↵ sends your note to its holder at work now; ↵ as a note that waits", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dkn-items [data-desk-item="d5"]').first().click();
  await expect(hint(page)).toHaveText("⌘↵ send now");
  await box(page).click();
  await page.keyboard.type("Check the narrow window too");
  await page.keyboard.press("Meta+Enter");
  await expect.poll(() => itemActs(cue)).toEqual([{ id: "d5", act: "note_now", text: "Check the narrow window too" }]);
  await expect(box(page)).toHaveValue("");
  await page.keyboard.type("and the dark one");
  await page.keyboard.press("Enter");
  await expect.poll(() => itemActs(cue).then((c) => c[1])).toEqual({ id: "d5", act: "note", text: "and the dark one" });
});

test("a crew member's page: ⌘↵ sends to its lead now", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => deskShow({ kind: "crew", lead: "lead", agent: "id-search" }));
  await expect(hint(page)).toHaveText("⌘↵ send now");
  await box(page).click();
  await page.keyboard.type("keep the field narrow");
  await page.keyboard.press("Meta+Enter");
  await expect.poll(() => toSession(cue)).toEqual([{ sid: "lead", text: "About search-centre: keep the field narrow", now: true }]);
});

test("a session's page on the Desk: ⌘↵ sends now; with nothing typed, the message Cue kept goes now", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
  await expect(hint(page)).toHaveText("⌘↵ send now");
  await box(page).click();
  await page.keyboard.type("try the other port");
  await page.keyboard.press("Meta+Enter");
  await expect.poll(() => toSession(cue)).toEqual([{ sid: "host", text: "try the other port", now: true }]);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").held = { role: "you", text: "and the logs", at_ms: Date.now(), images: [] }; });
  await box(page).click();
  await page.keyboard.press("Meta+Enter");
  await cue.called("send_held_now", { sessionId: "host" });
  expect(await toSession(cue)).toHaveLength(1);
});

test("the hint: the lead's box while it works, none for an idle session", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(hint(page)).toHaveText("⌘↵ send now");
  await page.evaluate(() => deskShow({ kind: "chat", sid: "site" }));
  await expect(box(page)).toHaveCount(1);
  await expect(hint(page)).toHaveCount(0);
});
