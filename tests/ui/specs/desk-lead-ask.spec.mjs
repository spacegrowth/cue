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
    await expect(chat.locator(".cv-ask .ask-q")).toHaveText("Phrases left or right of Send?");
    await expect(chat.locator(".cv-ask .mg-st")).toHaveCount(0);
    // Answered: beside the question on its card; Cue's message telling the lead isn't a second copy.
    await push(page, [
      { role: "you", text: "answered: Left", decision: true },
      { role: "you", text: `Your ASK${about}: Phrases left or right of Send?\nThe answer: Left` },
    ]);
    await expect(chat.locator(".cv-ask")).toHaveCount(1);
    await expect(chat.locator(".cv-ask .mg-st")).toHaveText(/Answered: Left/);
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

// #651: your own words typed on Cue's push card ("what does it mean? what do i do") were taken as the answer: the card
// closed, nothing was pushed, and nobody answered you. They aren't a yes or a no: the card stays open for its Yes or No,
// and the words go to the lead, who answers in its chat.
test("words typed on the push card go to the lead and the card stays open for its Yes or No (#651)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const q = "Update from GitHub, then upload 87 landings? Yes rebases main onto the newest on GitHub, checks them for private words, then pushes. If a private word turns up, nothing is uploaded.";
  await cue.change((st, q) => {
    st.sessions.find((s) => s.session_id === "lead").state = "deciding";
    st.items.push({ ...st.items[0], id: "n-push", agent_id: "", agent_name: "", desk: true, hosted: false,
      tool_input: { questions: [{ question: q, header: "", multiSelect: false, options: [{ label: "Yes", description: "" }, { label: "No", description: "" }] }], desk_push: { desk: "cue", top: "/work/cue", ahead: 87, starts: [] } } });
  }, q);
  const ask = page.locator(".dk-right .dk-ask");
  // #675: the card says plainly what Yes does: the rebase, how many landings, the private-word check.
  await expect(ask).toContainText(q);
  await expect(ask).not.toContainText("ahead of the shared copy");
  await ask.locator('[data-act="redirect"]').click();
  const box = ask.locator('textarea[data-text="n-push"]');
  await box.fill("what does it mean? what do i do");
  await box.press("Enter");
  await cue.called("respond", { id: "n-push", decision: { behavior: "allow", answers: { [q]: "what does it mean? what do i do" } } });
  // Delivered to the lead, as Cue's card in its chat with your words.
  const said = page.locator(".dk-chat", { hasText: "what does it mean? what do i do" });
  await expect(said).toHaveCount(1);
  await expect(page.locator(".dk-chat")).toContainText("Your words");
  // The card is still open and answerable: its Yes and No, not greyed out.
  await expect(ask).toContainText(q);
  const yes = ask.locator("[data-pick]", { hasText: "Yes" });
  await yes.click();
  const send = ask.locator('[data-act="submit-answers"]');
  await expect(send).toBeEnabled();
  await send.click();
  await expect.poll(async () => (await cue.calls("respond")).filter((c) => c.args.id === "n-push").length).toBe(2);
  await expect(page.locator(".dk-right .dk-ask")).toHaveCount(0);
});
