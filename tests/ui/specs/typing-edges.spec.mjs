// Typing to sessions when something goes wrong, as the window shows it: a send or reply that fails comes back to
// its box, a message Cue is still watching or that didn't go through says so (with Send again), a kept message
// that couldn't be sent says why, ⌘↵ with nothing typed sends the kept one, and blank or " /" messages.
import { test, expect } from "./cue.mjs";

const box = (page, sid) => page.locator(`.active-pane textarea[data-text="s:${sid}"]`);
const openCard = (page, id) => page.locator(`.nrow[data-big="${id}"]`).click();
const openWorking = (page, sid) => page.locator(`.working[data-session="${sid}"]`).first().click();
/** Session `sid`'s last message of yours, changed by `fields` (as the hub's state says it after a look). */
const markLastYours = (cue, sid, fields) => cue.change((st, [sid, fields]) => {
  const s = st.sessions.find((x) => x.session_id === sid);
  Object.assign(s.thread.filter((e) => e.role === "you").pop(), fields);
}, [sid, fields]);

test.beforeEach(async ({ cue }) => { await cue.open("board-waiting"); });

test("A reply that fails comes back to its box and says why", async ({ cue, page }) => {
  await cue.override("reply", () => { throw "tmux pane %1 is gone"; });
  await openCard(page, "n1");
  await box(page, "relay").pressSequentially("carry it over");
  await box(page, "relay").press("Enter");
  await expect(page.locator(".toast")).toContainText("Couldn't send: tmux pane %1 is gone");
  await expect(box(page, "relay")).toHaveValue("carry it over");
  await expect(page.locator('.nrow[data-big="n1"]')).toHaveCount(1);
});

test("⌘↵ that fails keeps the message in the box", async ({ cue, page }) => {
  await cue.override("send_to_session", () => { throw "tmux pane is gone"; });
  await openWorking(page, "alerts");
  await box(page, "alerts").pressSequentially("stop, check the retries");
  await box(page, "alerts").press("Meta+Enter");
  await expect(page.locator(".toast")).toContainText("Couldn't send: tmux pane is gone");
  await expect(box(page, "alerts")).toHaveValue("stop, check the retries");
});

test("Only spaces and newlines: Enter sends nothing to a working session", async ({ cue, page }) => {
  await openWorking(page, "eng");
  await box(page, "eng").pressSequentially("   ");
  await box(page, "eng").press("Shift+Enter");
  await box(page, "eng").press("Enter");
  await expect(page.locator(".active-pane .btn.send")).toBeDisabled();
  expect(await cue.calls("send_to_session")).toEqual([]);
});

test("A message that starts with \" /\" goes with its space, as a message", async ({ cue, page }) => {
  await openWorking(page, "eng");
  await box(page, "eng").pressSequentially(" /reomte is a typo");
  await box(page, "eng").press("Enter");
  expect((await cue.called("send_to_session", { sessionId: "eng" })).args.text).toBe(" /reomte is a typo");
});

test("A message Cue is still watching says it's waiting for it to take it", async ({ cue, page }) => {
  await openCard(page, "n1");
  await markLastYours(cue, "relay", { pending: true, at_ms: Date.now() });
  await expect(page.locator(".active-pane .cv-you.pending").last()).toContainText("◌ Sending…");
});

test("A message that didn't go through offers Send again, which sends it", async ({ cue, page }) => {
  await openCard(page, "n1");
  await markLastYours(cue, "relay", { unsent: true });
  const again = page.locator('.active-pane .cv-you.unsent button[data-act="resend"]').last();
  const text = await again.getAttribute("data-resend");
  await again.click();
  expect((await cue.called("send_to_session", { sessionId: "relay" })).args).toMatchObject({ text, now: false });
});

test("A kept message that couldn't be sent says why", async ({ cue, page }) => {
  await openWorking(page, "eng");
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "eng").held = { role: "you", text: "use the new pool", at_ms: Date.now(), images: [], unsent: true, delivery: [{ at_ms: Date.now(), what: "couldn't send: tmux pane is gone" }] };
  });
  await expect(page.locator(".active-pane .cv-you.held")).toContainText("! Couldn't send · tmux pane is gone");
});

test("⌘↵ with nothing typed sends the kept message now", async ({ cue, page }) => {
  await openWorking(page, "eng");
  await box(page, "eng").pressSequentially("use the new pool");
  await box(page, "eng").press("Enter");
  await expect(page.locator(".active-pane .cv-you.held")).toContainText("use the new pool");
  await box(page, "eng").press("Meta+Enter");
  await cue.called("send_held_now", { sessionId: "eng" });
  await expect(page.locator(".active-pane .cv-you.held")).toHaveCount(0);
});

test("Your kept message says it isn't sent yet", async ({ cue, page }) => {
  await openWorking(page, "eng");
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "eng").held = { role: "you", text: "use the new pool", at_ms: Date.now(), images: [] };
  });
  await expect(page.locator(".active-pane .cv-you.held")).toContainText("◌ Not sent yet · Cue sends it when its turn ends");
});

test("Cancel on a kept message drops it without sending it", async ({ cue, page }) => {
  await openWorking(page, "eng");
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "eng").held = { role: "you", text: "use the new pool", at_ms: Date.now(), images: [] };
  });
  const held = page.locator(".active-pane .cv-you.held");
  await expect(held).toContainText("Not sent yet · Cue sends it when its turn ends · Edit · Send now · Cancel");
  if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/chat-held-cancel.png` });
  await held.locator('button[data-act="held-x"]').click();
  await cue.called("unhold_message", { sessionId: "eng" });
  await expect(page.locator(".toast")).toContainText("It won't be sent");
  await expect(held).toHaveCount(0);
  await expect(box(page, "eng")).toHaveValue("");
  expect(await cue.calls("send_held_now")).toHaveLength(0);
});

test("Cancel that loses the race with its turn's end says it had already gone", async ({ cue, page }) => {
  await openWorking(page, "eng");
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "eng").held = { role: "you", text: "use the new pool", at_ms: Date.now(), images: [] };
  });
  // Its turn ended and Cue sent it just before the click reached the hub: nothing kept to take back.
  await cue.override("unhold_message", () => null);
  await page.locator('.active-pane .cv-you.held button[data-act="held-x"]').click();
  await expect(page.locator(".toast")).toContainText("Too late: it had already gone");
});

test("In a narrow pane a kept message's actions stay on one line", async ({ cue, page }) => {
  await page.setViewportSize({ width: 760, height: 800 });
  await openWorking(page, "eng");
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "eng").held = { role: "you", text: "use the new pool", at_ms: Date.now(), images: [] };
  });
  const acts = page.locator(".active-pane .cv-you.held .msg-line .ml-acts");
  await expect(acts).toContainText("Edit · Send now · Cancel");
  const lines = await acts.evaluate((el) => el.getClientRects().length);
  expect(lines).toBe(1);
  if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/chat-held-narrow.png` });
});

test("A kept message stays where you wrote it: drawn in time order, and its line turns Delivered in place", async ({ cue, page }) => {
  await openWorking(page, "eng");
  const t = await page.evaluate(() => Date.now());
  await cue.change((st, t) => {
    const s = st.sessions.find((x) => x.session_id === "eng");
    s.thread = [
      { role: "you", text: "Fix the retry loop.", at_ms: t - 600000, delivery: [{ at_ms: t - 599000, what: "taken: it started a turn" }] },
      { role: "agent", text: "Looking at the retry loop now.", at_ms: t - 300000 },
    ];
    s.held = { role: "you", text: "Run the queue tests too.", at_ms: t - 200000, images: [] };
    s.thread.push({ role: "agent", text: "Found it: the backoff never resets.", at_ms: t - 100000 });
  }, t);
  const chat = page.locator(".active-pane [data-chat]");
  const order = () => chat.locator(".cv-you .cv-you-text, .cv-agent .cv-text").allInnerTexts();
  const written = ["Fix the retry loop.", "Looking at the retry loop now.", "Run the queue tests too.", "Found it: the backoff never resets."];
  await expect.poll(order).toEqual(written);
  const bubble = chat.locator(`[data-key="y:${t - 200000}"]`);
  await expect(bubble.locator(".msg-line")).toContainText("Not sent yet");
  if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/chat-held-in-place.png` });
  await bubble.evaluate((el) => { el._same = true; });   // the same element after: a property (a redraw sets its attributes)
  // Its turn ends and Cue sends it: the thread's copy (sent now) says when you wrote it (the hub's written_ms).
  await cue.change((st, t) => {
    const s = st.sessions.find((x) => x.session_id === "eng");
    delete s.held;
    s.thread.push({ role: "you", text: "Run the queue tests too.", at_ms: t - 1000, written_ms: t - 200000, delivery: [{ at_ms: t - 900, what: "taken: it started a turn" }] });
  }, t);
  await expect(bubble.locator(".msg-line")).toContainText("Delivered to");
  await expect.poll(order).toEqual(written);
  expect(await bubble.evaluate((el) => el._same === true), "the same bubble, its line changed in place").toBe(true);
  if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/chat-held-taken.png` });
});

test("Cue's own kept crew message is Cue's bubble in the future tense, not yours, and Send now sends it", async ({ cue, page }) => {
  await openWorking(page, "eng");
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "eng").held = { role: "you", text: 'Send your teammate docs its review\'s findings with SendMessage (to: "docs") and let it fix them; don\'t fix them yourself:\n1. a.md:1 — fix', at_ms: Date.now(), images: [] };
  });
  const line = page.locator(".active-pane .cv-you.cue-you").last();
  await expect(line).toContainText("Will send docs's findings back to it");
  await expect(line).toContainText("From Cue · Not sent yet · Cue sends it when its turn ends");
  await expect(page.locator(".active-pane .cv-you.held")).toHaveCount(0);
  await line.locator('button[data-act="held-now"]').click();
  await cue.called("send_held_now", { sessionId: "eng" });
});

test("↑ in a long wrapped message moves up a row first; only from the top row does it bring back what you sent", async ({ cue, page }) => {
  await openCard(page, "n1");
  const b = box(page, "relay");
  const long = "check the retries on every queue ".repeat(12).trim();
  await b.fill(long);
  await b.evaluate((ta) => ta.setSelectionRange(ta.value.length, ta.value.length));
  await b.press("ArrowUp");
  await expect(b).toHaveValue(long);
  expect(await b.evaluate((ta) => ta.selectionStart)).toBeLessThan(long.length);
  await b.evaluate((ta) => ta.setSelectionRange(3, 3));
  await b.press("ArrowUp");
  await expect(b).not.toHaveValue(long);
  await b.press("ArrowDown");
  await expect(b).toHaveValue(long);
  await b.evaluate((ta) => ta.setSelectionRange(ta.value.length - 3, ta.value.length - 3));
  await b.press("ArrowDown");
  await expect(b).toHaveValue(long);
});
