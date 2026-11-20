// The Desk lead's ASK lines in its chat (#427): never raw "ASK: …" text. Cue files each as a question, which the chat
// shows once as a small card (the question, then your answer beside it); the pop-up is where you answer, and Cue's
// "Your ASK…The answer…" message back to the lead isn't drawn a second time. The same for an ASK about an item.
import { test, expect, openDesk } from "./cue.mjs";

/** Entries added to the lead's thread, as the hub notes them, a millisecond apart. */
const push = (page, entries) => page.evaluate((es) => {
  const th = sessionOf("lead").thread, t = Date.now();
  es.forEach((e, n) => th.push({ at_ms: t + n, ...e }));
  renderMain();
}, entries);

for (const [what, line, about] of [["an ASK", "ASK: Phrases left or right of Send? | choices: Left / Right", ""], ["an ASK about an item", "- **ASK:** #d2 | Phrases left or right of Send? | choices: Left / Right", " about #d2"]]) {
  test(`${what}: one small card with the question, then your answer beside it; never the raw line`, async ({ page, cue }) => {
    await cue.open("desk");
    await openDesk(page);
    const chat = page.locator(".dk-chat");
    await push(page, [
      { role: "agent", text: `Two ways to place them.\n\n${line}` },
      { role: "agent", text: "asked: Phrases left or right of Send?" },
    ]);
    await expect(chat.locator(".cv-agent").last().locator(".cv-text")).toHaveText("Two ways to place them.");
    await expect(chat).not.toContainText("ASK:");
    await expect(chat.locator(".cv-ask")).toHaveCount(1);
    await expect(chat.locator(".cv-ask .cv-ask-q")).toHaveText("Phrases left or right of Send?");
    await expect(chat.locator(".cv-ask .cv-ask-ans")).toHaveCount(0);
    // Answered: beside the question on its card; Cue's message telling the lead isn't a second copy.
    await push(page, [
      { role: "you", text: "answered: Left", decision: true },
      { role: "you", text: `Your ASK${about}: Phrases left or right of Send?\nThe answer: Left` },
    ]);
    await expect(chat.locator(".cv-ask")).toHaveCount(1);
    await expect(chat.locator(".cv-ask .cv-ask-ans")).toHaveText(/Answered: Left/);
    await expect(chat.getByText("Phrases left or right of Send?")).toHaveCount(1);
    await expect(chat).not.toContainText("Your ASK");
    await expect(chat).not.toContainText("ASK:");
  });
}

test("a line that only mentions ASK in its words stays as it is", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await push(page, [{ role: "agent", text: "I'll ASK you about it later." }]);
  await expect(page.locator(".dk-chat .cv-agent").last().locator(".cv-text")).toHaveText("I'll ASK you about it later.");
});
