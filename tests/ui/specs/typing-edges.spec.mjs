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
  await expect(page.locator(".active-pane .cv-you.pending").last()).toContainText("waiting for it to take this");
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
  await expect(page.locator(".active-pane .cv-you.held")).toContainText("Couldn't send it: tmux pane is gone");
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
  await expect(page.locator(".active-pane .cv-you.held")).toContainText("Not sent yet · it goes when this turn ends");
});

test("Cue's own kept crew message is Cue's bubble in the future tense, not yours, and Send now sends it", async ({ cue, page }) => {
  await openWorking(page, "eng");
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "eng").held = { role: "you", text: 'Send your teammate docs its review\'s findings with SendMessage (to: "docs") and let it fix them; don\'t fix them yourself:\n1. a.md:1 — fix', at_ms: Date.now(), images: [] };
  });
  const line = page.locator(".active-pane .cv-you.cue-you").last();
  await expect(line).toContainText("Will send docs's findings back to it");
  await expect(line).toContainText("Cue · not sent yet · it goes when this turn ends");
  await expect(page.locator(".active-pane .cv-you.held")).toHaveCount(0);
  await line.locator('button[data-act="held-now"]').click();
  await cue.called("send_held_now", { sessionId: "eng" });
});
