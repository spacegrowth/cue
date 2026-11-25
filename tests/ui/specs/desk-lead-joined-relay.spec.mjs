// Your words to the Desk's lead that went in one message with a session's words for it (#812): you wrote while the
// lead was busy, a session's TO LEAD came in the same wait, and Cue sent both when its turn ended (sessions/
// kept_message.rs hold). The lead's chat shows your bubble with only your words and the lead's answer under it; the
// session's words are a TO LEAD card in Activity, as a session's words on their own are (#789).
import { test, expect, openDesk } from "./cue.mjs";

const SUMMARY = '<desk project="cue">\nDesk lead: web-lead (you)\nNeeds routing:\nNone.\n</desk>';
const relay = (who, id, title, words) => `From ${who} on #${id} (${title}):\n${words}\nAnswer it with NOTE: #${id} | <your words>.`;
const RELAY = relay("cue-790", "d2", "Away is just on or off", "is stuck in a loop: Cue keeps sending it back. Please close it by hand.");
const REPLY = "Yes, we're moving. Two items landed in the last 15 minutes, and six more are in progress.";

/** The lead's thread gets `you` (as Cue sent it, the Desk's summary first) and then the lead's reply to it. */
const feed = (page, you) => page.evaluate(([you, reply]) => {
  const lead = sessionOf("lead"), at = Date.now() - 60000;
  lead.thread.push({ at_ms: at, role: "you", text: you }, { at_ms: at + 12000, role: "agent", text: reply });
  renderMain();
}, [you, REPLY]);

for (const [name, words] of [["your words first", `are we moving ?\n\n${RELAY}`], ["the session's words first", `${RELAY}\n\nare we moving ?`]]) {
  test(`your words and a session's in one message (${name}): your bubble with only yours, the lead's answer under it`, async ({ page, cue }) => {
    await cue.open("desk");
    await openDesk(page);
    await feed(page, `${SUMMARY}\n\n${words}`);
    const chat = page.locator(".dk-chat");
    const mine = chat.locator(".cv-you-text", { hasText: "are we moving ?" });
    await expect(mine).toHaveCount(1);
    // Only your words: the Desk's summary the lead got with them shows nowhere (#882).
    await expect(mine).toHaveText("are we moving ?");
    // Nothing of the session's words, or Cue's line for the lead, in the chat.
    await expect(chat).not.toContainText("stuck in a loop");
    await expect(chat).not.toContainText("Answer it with NOTE");
    // The lead's answer is the next message after yours.
    const next = await page.evaluate(() => {
      const you = [...document.querySelectorAll(".dk-chat .cv-you-text")].find((el) => el.textContent.includes("are we moving ?"));
      const row = you.closest(".dk-chat > *");
      let n = row.nextElementSibling;
      while (n && !n.textContent.trim()) n = n.nextElementSibling;
      return n?.textContent || "";
    });
    expect(next).toContain("Yes, we're moving.");
    await expect(page.locator(".dk-chat [title]")).toHaveCount(0);
    // The session's words are in Activity, as its card.
    await page.evaluate(() => deskOpenActivity());
    const card = page.locator('.dk-moves [data-mg="from"]');
    await expect(card).toHaveCount(1);
    await expect(card).toContainText("cue-790");
    await expect(card).toContainText("stuck in a loop");
    await expect(card).not.toContainText("are we moving");
    await expect(card).not.toContainText("Answer it with NOTE");
  });
}

test("two sessions' words in one message: a TO LEAD card each in Activity, the lead's answer under them, nothing in the chat", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const two = `${RELAY}\n\n${relay("cue-796", "d3", "Build steps", "is paused on tests.")}`;
  await feed(page, `${SUMMARY}\n\n${two}`);
  await expect(page.locator(".dk-chat")).not.toContainText("paused on tests");
  // A turn of only sessions' words for the lead (#816): its answer goes to Activity too, under them.
  await expect(page.locator(".dk-chat")).not.toContainText("Yes, we're moving.");
  await page.evaluate(() => deskOpenActivity());
  const cards = page.locator('.dk-moves [data-mg="from"]');
  await expect(cards).toHaveCount(2);
  await expect(cards.filter({ hasText: "cue-796" })).toContainText("is paused on tests.");
  await expect(cards.filter({ hasText: "cue-790" })).not.toContainText("paused on tests");
  await expect(page.locator(".dk-moves .dk-ans")).toContainText("Yes, we're moving.");
});
