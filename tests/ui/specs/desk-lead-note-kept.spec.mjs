// The Desk lead's note to a busy session (#905): never in that session's chat, kept, queued or sent; the Desk's
// Activity tab lists it, with its item and when. Your own words kept for a busy session still show "Not sent yet".
import { test, expect, openDesk } from "./cue.mjs";

const NOTE = "Note from the Desk lead on Desk item #d5 (Header): make the footer match the header too.";
const WAIT = "Not sent yet · Cue sends it when its turn ends";

/** host busy, with `held` kept for it (and `queued`, when given); the lead's NOTE in its reply, as the hub noted it. */
const busy = (cue, held, queued = null) => cue.change((st, [held, queued, NOTE]) => {
  const s = st.sessions.find((x) => x.session_id === "host");
  s.state = "working";
  s.held = held && { role: "you", at_ms: Date.now() - 20000, images: [], ...held };
  if (queued) s.queued = { role: "you", at_ms: Date.now() - 10000, images: [], ...queued };
  const d = st.desk.projects.find((p) => p.project === "cue");
  d.decisions.push({ id: "n905", project: "cue", item_id: "d5", by: "web-lead", sid: "lead", text: "make the footer match the header too.", why: "note to host-lead", at: Math.floor(Date.now() / 1000) - 30, undo: "none", act: "" });
  st.sessions.find((x) => x.session_id === "lead").thread.push({ role: "agent", at_ms: Date.now() - 30000, text: "Noted.\n\nNOTE: #d5 | make the footer match the header too." });
}, [held, queued, NOTE]);

const hostChat = async (page) => {
  await page.locator('[data-desk-item="d5"]').first().click();
  return page.locator('.dk-right .dkn-sp[data-desk-page="session"] [data-desk-chat="host"]');
};

test("a lead's note kept for its busy holder is not in the holder's chat, on the Desk or the Board; Activity lists it", async ({ page, cue }) => {
  await cue.open("desk");
  await busy(cue, { text: NOTE, cue_parts: [NOTE] });
  await openDesk(page);
  const chat = await hostChat(page);
  await expect(chat.locator(".cv-you-text").first()).toBeAttached();
  await expect(chat).not.toContainText("make the footer match");
  await expect(chat.getByText("Not sent yet")).toHaveCount(0);
  await expect(chat.locator(".cue-quiet")).toHaveCount(0);
  // The Board's chat of the same session: the same.
  await page.evaluate(() => setActive(null, "host"));
  const board = page.locator(".active-pane");
  await expect(board).not.toContainText("make the footer match");
  await expect(board.getByText("Not sent yet")).toHaveCount(0);
  // The Desk's Activity: the note, to its holder, its item, and when.
  await openDesk(page);
  await page.evaluate(() => deskOpenActivity());
  const card = page.locator('.dk-moves [data-mg="note"]');
  await expect(card).toHaveCount(1);
  await expect(card.locator(".mg-id")).toContainText("#d5");
  await expect(card.locator(".mg-t")).toHaveText("make the footer match the header too.");
  await expect(page.locator(".dk-moves").locator("[title]")).toHaveCount(0);
});

test("your words kept with a lead's note: your bubble alone, Not sent yet; the note isn't drawn", async ({ page, cue }) => {
  await cue.open("desk");
  await busy(cue, { text: `also check the dark theme\n\n${NOTE}`, cue_parts: [NOTE] });
  await openDesk(page);
  const chat = await hostChat(page);
  const held = chat.locator(".cv-you.msg-held");
  await expect(held).toHaveCount(1);
  await expect(held.locator(".cv-you-text")).toHaveText("also check the dark theme");
  await expect(held.locator(".ml-words")).toHaveText(WAIT);
  await expect(chat).not.toContainText("make the footer match");
});

test("a lead's note queued in its holder is not in the chat; your note from the item's page still is", async ({ page, cue }) => {
  await cue.open("desk");
  const title = await page.evaluate(() => state.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5").title);
  const yours = `About #d5 (${title}): make the button bigger`;
  await busy(cue, { text: yours, cue_parts: [yours] }, { text: NOTE, cue: true, kind: "note" });
  await openDesk(page);
  const chat = await hostChat(page);
  await expect(chat.locator(".cv-you.msg-queued")).toHaveCount(0);
  await expect(chat).not.toContainText("make the footer match");
  // Your note from the item's page stays in the chat as before (#836: Cue's line that it goes when the turn ends).
  await expect(chat).toContainText("make the button bigger");
  await expect(chat).toContainText("goes when its turn ends");
});
