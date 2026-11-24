// Cue's notices to the Desk's lead (#816): an item finished, looks stuck, is queued, a line was refused… are Cue's, never
// your words. None shows in the lead's chat, and none as "You": each is in Activity under Cue, for its item. The lead's
// answer to a turn of only notices goes to Activity under that notice; one that acts (a ROUTE, a NOTE…) stays in the chat
// with the notices one quiet line over it. A turn with your words in it stays as it is (#812).
import { test, expect, openDesk } from "./cue.mjs";

const SUMMARY = '<desk project="cue">\nDesk lead: web-lead (you)\nNeeds routing:\nNone.\n</desk>';
const STUCK = "#d2 (Quick phrases at the right of Send) looks stuck: cue-fb has shown no new output for 33 minutes while marked working on it. Its last step: Writing its reply.\nStep in: look at its chat, then nudge it (NOTE: #d2 | …), PAUSE: #d2 or ROUTE it on; only if the owner must decide, ASK: #d2 | <question> | choices: … Cue tells you again only if it goes quiet again after moving.";
const QUEUED = "Queued: #d3 is next for cue-789 (busy with #d2); it goes to a fresh session at 23:23 if cue-789 is still busy.";
const FINISHED = "#d2 (Quick phrases at the right of Send) finished: it's ready for the owner's check. Nothing to do unless it gets stuck.\n\n(Desk changes 1358–1359)";

/** The lead's thread gets each [you, reply] pair: Cue's message (the Desk's summary first), then the lead's answer. */
const feed = (page, pairs) => page.evaluate((pairs) => {
  const lead = sessionOf("lead");
  let at = Date.now() - 600000;
  for (const [you, reply] of pairs) {
    lead.thread.push({ at_ms: at, role: "you", text: you });
    if (reply) lead.thread.push({ at_ms: at + 12000, role: "agent", text: reply });
    at += 60000;
  }
  renderMain();
}, pairs);

/** How many bubbles of yours the lead's chat shows. */
const yours = (page) => page.locator(".dk-chat .cv-you-text").count();

const activity = async (page) => {
  await page.evaluate(() => { deskMovesShown.set("cue", 200); deskOpenActivity(); });
  return page.locator(".dk-moves");
};

test("a stuck notice and a notice-only turn: neither is in the chat or yours; both are Cue's in Activity, the lead's answer under its notice", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const mine = await yours(page);
  await feed(page, [
    [`${SUMMARY}\n\n${STUCK}`, "I'll give cue-fb ten more minutes; it's writing its reply."],
    [`${SUMMARY}\n\n${QUEUED}`, "Fine, #d3 waits for cue-789."],
  ]);
  const chat = page.locator(".dk-chat");
  for (const words of ["looks stuck", "Step in", "Queued", "ten more minutes", "#d3 waits for cue-789"]) await expect(chat).not.toContainText(words);
  await expect(chat.locator(".cv-you-text")).toHaveCount(mine);

  const moves = await activity(page);
  const stuck = moves.locator(".dk-mv", { hasText: "looks stuck" });
  await expect(stuck).toHaveCount(1);
  await expect(stuck).not.toContainText("You");
  await expect(stuck.locator(".dk-ans")).toContainText("ten more minutes");
  const queued = moves.locator(".dk-mv", { hasText: "Queued" });
  await expect(queued).toHaveCount(1);
  await expect(queued.locator(".dk-ans")).toContainText("#d3 waits for cue-789");
  // Each is Cue's, filed under its item.
  const metas = await page.evaluate(() => { const d = deskOf("cue"); return deskActivity(d).filter((r) => r.line?.e?.role === "you").map((r) => deskActMeta(r, d)); });
  const stuckMeta = metas.find((m) => m.text.includes("looks stuck"));
  expect(stuckMeta).toMatchObject({ who: "Cue", item: "d2" });
  expect(metas.find((m) => m.text.includes("busy with"))).toMatchObject({ who: "Cue", item: "d3" });
  await expect(page.locator(".dk-moves [title], .dk-chat [title]")).toHaveCount(0);
});

test("the lead's answer that acts on a notice stays in the chat, the notice one quiet line over it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const mine = await yours(page);
  await feed(page, [[`${SUMMARY}\n\n${FINISHED}`, "Good, I'll ask its holder to tidy up first.\n\nNOTE: #d2 | Remove the old phrase list before it's checked."]]);
  const chat = page.locator(".dk-chat");
  const reply = chat.locator(".cv-agent", { hasText: "ask its holder to tidy up" });
  await expect(reply).toHaveCount(1);
  const quiet = reply.locator(".dk-quiet");
  await expect(quiet).toHaveCount(1);
  await expect(quiet).toContainText("Cue");
  await expect(quiet).toContainText("#d2");
  await expect(chat.locator(".cv-you-text")).toHaveCount(mine);
  await expect(chat).not.toContainText("Nothing to do unless it gets stuck");
  // The notice itself is in Activity, as Cue's.
  const moves = await activity(page);
  await expect(moves.locator(".dk-mv", { hasText: "finished" }).first()).toBeVisible();
});

test("a turn with your words in it stays in the chat as yours, with the lead's answer", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await feed(page, [
    [`${SUMMARY}\n\n${STUCK}`, "Waiting on cue-fb."],
    [`${SUMMARY}\n\nare we moving ?`, "Yes, two items landed."],
  ]);
  const chat = page.locator(".dk-chat");
  await expect(chat.locator(".cv-you-text", { hasText: "are we moving ?" })).toHaveCount(1);
  await expect(chat).toContainText("Yes, two items landed.");
  await expect(chat).not.toContainText("Waiting on cue-fb.");
});
