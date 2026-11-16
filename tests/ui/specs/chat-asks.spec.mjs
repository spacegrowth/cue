// An ask the agent made (the hub notes "<Tool>: <summary>" in its thread, then your answer as yours: hub/asks.rs) drawn
// in the chat as an ask: what it asked, the command in a code block (folded when long), your answer beside it.
import { test, expect } from "./cue.mjs";

const ASK_CMD = "cat >> notes.md <<'EOF'\n## Router\n- split in two\n- partial fills next\n- tests pass\nEOF";

/** eng's chat open (from its Inbox brief's Open chat). */
const openEngChat = async (page) => {
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('[data-inbox-fold="run"]').click();
  await page.locator('.ib-row[data-inbox-open="eng"]').click();
  await page.locator('.inbox-page[data-inbox-page="eng"] [data-inbox-full="eng"]').first().click();
  return page.locator(".active-pane");
};

test("an ask in the chat: Asked to run, the command in a code block with Allowed beside it, no paragraph of it", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st, cmd) => {
    const t0 = Date.now() - 600000;
    st.sessions.find((x) => x.session_id === "eng").thread = [
      { role: "you", text: "Refactor the order router", at_ms: t0 },
      { role: "agent", text: "Split the router in two; writing the notes now.", at_ms: t0 + 60000 },
      { role: "agent", text: `Bash: ${cmd}`, at_ms: t0 + 70000 },
      { role: "you", text: "allowed", at_ms: t0 + 80000 },
    ];
  }, ASK_CMD);
  const chat = await openEngChat(page);
  const ask = chat.locator(".cv-ask");
  await expect(ask).toHaveCount(1);
  await expect(ask.locator(".cv-ask-head")).toHaveText("Asked to run");
  await expect(ask.locator(".cv-ask-ans")).toContainText("Allowed");
  // Folded to its first lines; Show all opens it whole.
  await expect(ask.locator("pre.cv-ask-code")).toHaveText("$ cat >> notes.md <<'EOF'\n## Router\n- split in two\n- partial fills next");
  await ask.locator(".cv-more").click();
  await expect(chat.locator(".cv-ask pre.cv-ask-code")).toHaveText(`$ ${ASK_CMD}`);
  // Not its words: no reply paragraph holds the command, and "allowed" is no bubble of yours.
  await expect(chat.locator(".cv-agent")).toHaveCount(1);
  await expect(chat.locator(".cv-agent .cv-text")).toHaveText("Split the router in two; writing the notes now.");
  await expect(chat.locator(".cv-agent .cv-text")).not.toContainText("cat >>");
  await expect(chat.locator(".cv-you")).toHaveCount(1);
  await expect(chat.locator(".cv-you")).toContainText("Refactor the order router");
});

test("a denied ask says so, and an edit names its file; a reply that merely starts with a word and a colon stays a reply", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => {
    const t0 = Date.now() - 600000;
    st.sessions.find((x) => x.session_id === "eng").thread = [
      { role: "you", text: "Clean up", at_ms: t0 },
      { role: "agent", text: "Bash: rm -rf build", at_ms: t0 + 10000 },
      { role: "you", text: "denied", at_ms: t0 + 20000 },
      { role: "agent", text: "Edit: Edit /src/router.rs", at_ms: t0 + 30000 },
      { role: "you", text: "allowed (always)", at_ms: t0 + 40000 },
      { role: "agent", text: "Read: the router is now two files.", at_ms: t0 + 50000 },
    ];
  });
  const chat = await openEngChat(page);
  await expect(chat.locator(".cv-ask")).toHaveCount(2);
  await expect(chat.locator(".cv-ask").nth(0).locator(".cv-ask-ans.no")).toContainText("Denied");
  await expect(chat.locator(".cv-ask").nth(1).locator(".cv-ask-head")).toHaveText("Asked to edit router.rs");
  await expect(chat.locator(".cv-ask").nth(1).locator(".cv-ask-ans")).toContainText("Allowed always");
  await expect(chat.locator(".cv-agent .cv-text")).toHaveText("Read: the router is now two files.");
  await expect(chat.locator(".cv-you")).toHaveCount(1);
});

test("a message you typed after an unanswered ask, starting with a decision word, stays your message: not its answer", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => {
    const t0 = Date.now() - 600000;
    st.sessions.find((x) => x.session_id === "eng").thread = [
      { role: "you", text: "Clean up", at_ms: t0 },
      { role: "agent", text: "Bash: rm -rf build", at_ms: t0 + 10000 },
      { role: "you", text: "allowed the plan, but keep build", at_ms: t0 + 20000 },
      { role: "agent", text: "Bash: cargo test", at_ms: t0 + 30000 },
      { role: "you", text: "allowed", decision: true, at_ms: t0 + 40000 },
    ];
  });
  const chat = await openEngChat(page);
  await expect(chat.locator(".cv-ask")).toHaveCount(2);
  await expect(chat.locator(".cv-ask").nth(0).locator(".cv-ask-ans")).toHaveCount(0);
  await expect(chat.locator(".cv-you")).toHaveCount(2);
  await expect(chat.locator(".cv-you").nth(1)).toContainText("allowed the plan, but keep build");
  await expect(chat.locator(".cv-ask").nth(1).locator(".cv-ask-ans")).toContainText("Allowed");
});

test("any tool's ask is one, a Bash ask with no command too; an answer whose ask scrolled out isn't yours; Read skips asks", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => {
    const t0 = Date.now() - 600000;
    st.sessions.find((x) => x.session_id === "eng").thread = [
      { role: "you", text: "allowed", at_ms: t0 },
      { role: "you", text: "Plan the week", at_ms: t0 + 5000 },
      { role: "agent", text: "TodoWrite: TodoWrite", at_ms: t0 + 10000 },
      { role: "you", text: "in the terminal", at_ms: t0 + 20000 },
      { role: "agent", text: "Bash:", at_ms: t0 + 30000 },
      { role: "you", text: "in the morning", at_ms: t0 + 40000 },
      { role: "agent", text: "The plan is in place.", at_ms: t0 + 50000 },
    ];
  });
  const chat = await openEngChat(page);
  await expect(chat.locator(".cv-ask")).toHaveCount(2);
  await expect(chat.locator(".cv-ask").nth(0).locator(".cv-ask-head")).toHaveText("Asked to update its to-do list");
  await expect(chat.locator(".cv-ask").nth(0).locator(".cv-ask-ans")).toContainText("In the terminal");
  await expect(chat.locator(".cv-ask").nth(1).locator(".cv-ask-head")).toHaveText("Asked to run");
  await expect(chat.locator(".cv-ask").nth(1).locator(".cv-ask-ans")).toHaveCount(0);
  // The "allowed" at the top (its ask gone) is no message of yours, but a decision line; "in the morning", typed, is yours.
  await expect(chat.locator(".cv-result")).toHaveCount(1);
  await expect(chat.locator(".cv-result")).toContainText("Allowed");
  await expect(chat.locator(".cv-you")).toHaveCount(2);
  await expect(chat.locator(".cv-you").nth(0)).toContainText("Plan the week");
  await expect(chat.locator(".cv-you").nth(1)).toContainText("in the morning");
  // Read and read-aloud step through its replies only.
  expect(await page.evaluate(() => readerReplies(null, state.sessions.find((x) => x.session_id === "eng")).map((r) => r.text))).toEqual(["The plan is in place."]);
});

test("a terminal dialog's choice (no ask before it) is a decision line: what you picked, not a message of yours", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await cue.change((st) => {
    const t0 = Date.now() - 600000;
    st.sessions.find((x) => x.session_id === "eng").thread = [
      { role: "you", text: "/model", at_ms: t0 },
      { role: "you", text: "chose 1. Opus", at_ms: t0 + 10000 },
      { role: "you", text: "Refactor the order router", at_ms: t0 + 20000 },
      { role: "agent", text: "On it.", at_ms: t0 + 30000 },
    ];
  });
  const chat = await openEngChat(page);
  await expect(chat.locator(".cv-result")).toHaveCount(1);
  await expect(chat.locator(".cv-result")).toContainText("Chose 1. Opus");
  await expect(chat.locator(".cv-result .sent-check")).toHaveText("✓");
  await expect(chat.locator(".cv-you")).toHaveCount(2);
  await expect(chat.locator(".cv-you")).not.toContainText(["chose 1. Opus"]);
  // In time order: after /model, before your next message.
  const order = await chat.evaluate((c) => [...c.querySelectorAll(".cv-you, .cv-result")].map((el) => (el.matches(".cv-result") ? "decision" : el.textContent.includes("/model") ? "model" : "you")));
  expect(order).toEqual(["model", "decision", "you"]);
});
