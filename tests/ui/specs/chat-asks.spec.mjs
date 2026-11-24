// An ask the agent made (the hub notes "<Tool>: <summary>" in its thread, then your answer as yours: hub/asks.rs) drawn
// in the chat once: a tool's ask settled is one quiet line (how you answered, what it asked, its command's first line);
// one still waiting is the card under the chat, not drawn in it (#801). A question stays a card with your answer.
import { test, expect } from "./cue.mjs";

const ASK_CMD = "cat >> notes.md <<'EOF'\n## Router\n- split in two\n- partial fills next\n- tests pass\nEOF";

/** eng's chat open on the Board. */
const openEngChat = async (page) => {
  await page.evaluate(() => setActive(null, "eng"));
  return page.locator(".active-pane");
};

test("an answered ask in the chat is one quiet line: Allowed, Asked to run, the command's first line; no card, no paragraph of it", async ({ cue, page }) => {
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
  await expect(ask).toHaveClass(/ask-done/);
  await expect(ask.locator(".ask-said")).toHaveText("Allowed");
  await expect(ask.locator(".ask-head")).toHaveText("Asked to run");
  await expect(ask.locator(".ask-line")).toHaveText("cat >> notes.md <<'EOF'");
  // One line, not a card: no red border, no tag, no code block.
  await expect(chat.locator(".mg.ask")).toHaveCount(0);
  const box = await ask.boundingBox();
  expect(box.height).toBeLessThan(40);
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
  await expect(chat.locator(".cv-ask").nth(0)).toHaveClass(/\bno\b/);
  await expect(chat.locator(".cv-ask").nth(0).locator(".ask-said")).toHaveText("Denied");
  await expect(chat.locator(".cv-ask").nth(0).locator(".sent-check")).toHaveText("✕");
  await expect(chat.locator(".cv-ask").nth(1).locator(".ask-head")).toHaveText("Asked to edit router.rs");
  await expect(chat.locator(".cv-ask").nth(1).locator(".ask-said")).toHaveText("Allowed always");
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
  await expect(chat.locator(".cv-ask").nth(0).locator(".ask-said")).toHaveText("Not answered here");
  await expect(chat.locator(".cv-you")).toHaveCount(2);
  await expect(chat.locator(".cv-you").nth(1)).toContainText("allowed the plan, but keep build");
  await expect(chat.locator(".cv-ask").nth(1).locator(".ask-said")).toHaveText("Allowed");
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
  await expect(chat.locator(".cv-ask").nth(0).locator(".ask-head")).toHaveText("Asked to update its to-do list");
  await expect(chat.locator(".cv-ask").nth(0).locator(".ask-said")).toHaveText("In the terminal");
  await expect(chat.locator(".cv-ask").nth(1).locator(".ask-head")).toHaveText("Asked to run");
  await expect(chat.locator(".cv-ask").nth(1).locator(".ask-said")).toHaveText("Not answered here");
  // The "allowed" at the top (its ask gone) is no message of yours, but a decision line; "in the morning", typed, is yours.
  await expect(chat.locator(".cv-result:not(.cv-ask)")).toHaveCount(1);
  await expect(chat.locator(".cv-result:not(.cv-ask)")).toContainText("Allowed");
  await expect(chat.locator(".cv-you")).toHaveCount(2);
  await expect(chat.locator(".cv-you").nth(0)).toContainText("Plan the week");
  await expect(chat.locator(".cv-you").nth(1)).toContainText("in the morning");
  // Read and read-aloud step through its replies only.
  expect(await page.evaluate(() => readerReplies(null, state.sessions.find((x) => x.session_id === "eng")).map((r) => r.text))).toEqual(["The plan is in place."]);
});

// #801: the hook lost Cue mid-wait and asked again, so the thread holds the same ask twice, while its card is up.
test("one request asked twice shows once: the card under the chat, none in it; answered, one quiet line, light and dark, wide and small", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const cmd = "cd /x/eng && rm tests/probe.spec.mjs && git status --short";
  await cue.change((st, cmd) => {
    const t0 = Date.now() - 600000;
    const s = st.sessions.find((x) => x.session_id === "eng");
    s.state = "deciding";
    s.thread = [
      { role: "you", text: "Tidy up the probe", at_ms: t0 },
      { role: "agent", text: "Removing the probe spec.", at_ms: t0 + 10000 },
      { role: "agent", text: `Bash: ${cmd}`, at_ms: t0 + 20000 },
      { role: "agent", text: `Bash: ${cmd}`, at_ms: t0 + 260000 },
    ];
    st.items.push({ id: "n801", kind: "permission", harness: "claude", session_id: "eng", cwd: s.cwd, project: s.project, tool_name: "Bash", tool_input: { command: cmd }, suggestions: [], context: [], created_ms: t0 + 260000, status: "pending", outcome: "" });
  }, cmd);
  const chat = await openEngChat(page);
  await expect(page.locator(".act-card")).toHaveCount(1);
  await expect(page.locator(".act-card")).toContainText("wants to run");
  await expect(chat.locator(".cv-ask")).toHaveCount(0);
  await expect(chat.locator(".mg.ask")).toHaveCount(0);
  await expect(chat.locator(".cv-agent")).toHaveCount(1);
  // Answered: the card goes, and the request is one quiet line.
  await cue.change((st) => {
    st.items = st.items.filter((i) => i.id !== "n801");
    const s = st.sessions.find((x) => x.session_id === "eng");
    s.state = "working";
    s.thread.push({ role: "you", text: "allowed", decision: true, at_ms: s.thread.at(-1).at_ms + 30000 });
  });
  await expect(page.locator(".act-card")).toHaveCount(0);
  await expect(chat.locator(".cv-ask")).toHaveCount(1);
  await expect(chat.locator(".cv-ask .ask-said")).toHaveText("Allowed");
  await expect(chat.locator(".mg.ask")).toHaveCount(0);
  for (const scheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: scheme });
    for (const [width, height] of [[1440, 900], [900, 600]]) {
      await page.setViewportSize({ width, height });
      const ask = chat.locator(".cv-ask");
      const [box, pane] = await Promise.all([ask.boundingBox(), chat.boundingBox()]);
      expect(box.height).toBeLessThan(40);
      expect(box.x + box.width).toBeLessThanOrEqual(pane.x + pane.width + 1);
      expect(await chat.evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
      if (process.env.SHOT_DIR) await ask.screenshot({ path: `${process.env.SHOT_DIR}/801-${scheme}-${width}.png` });
    }
  }
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

test("an answered question wraps as text with its answer under it, inside the pane, at narrow and wide widths", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const q = "Should the Desk keep the lead apart from the sessions list at the top of the left column, or fold it in with the others?";
  const a = "answered: Keep it apart at the top, and never show its crew there, not even the ones that finished and were committed long ago";
  await cue.change((st, [q, a]) => {
    const t0 = Date.now() - 600000;
    st.sessions.find((x) => x.session_id === "eng").thread = [
      { role: "you", text: "Lay out the Desk", at_ms: t0 },
      { role: "agent", text: `asked: ${q}`, at_ms: t0 + 60000 },
      { role: "you", text: a, at_ms: t0 + 70000 },
    ];
  }, [q, a]);
  const chat = await openEngChat(page);
  for (const width of [720, 1500]) {
    await page.setViewportSize({ width, height: 900 });
    const ask = chat.locator(".cv-ask");
    await expect(ask.locator(".mg-st")).toContainText("Keep it apart");
    const [box, qb, ab] = await Promise.all([ask.boundingBox(), ask.locator(".ask-q").boundingBox(), ask.locator(".mg-st").boundingBox()]);
    // The question as text, not a letter a line; the answer under it; both inside the ask's own width.
    expect(qb.width).toBeGreaterThan(box.width / 2);
    expect(ab.y).toBeGreaterThanOrEqual(qb.y + qb.height - 1);
    expect(qb.x + qb.width).toBeLessThanOrEqual(box.x + box.width + 1);
    expect(ab.x + ab.width).toBeLessThanOrEqual(box.x + box.width + 1);
    expect(await chat.evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
  }
});
