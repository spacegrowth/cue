// The Desk lead's steps (#914): a turn you started shows its steps with the lead's reply under them; a turn Cue
// started (a hand-off ask, "#d5 finished", a full run that failed) goes to Activity whole, its steps with it, so no
// steps sit in the chat with no reply after them. Only the newest turn's steps are open; older ones fold to their chip.
import { test, expect, openDesk } from "./cue.mjs";

const SUMMARY = '<desk project="cue">\nDesk lead: web-lead (you)\nNeeds routing:\nNone.\n</desk>\n\n';
const MINUTE = 60000;

/** The lead's thread and its step feed, one turn per message to it: you, Cue, Cue, you; a step or two before each reply. */
async function leadTurns(cue) {
  await cue.override("session_log_page", () => []);
  await cue.override("transcript_page", () => []);
  const t0 = await cue.page.evaluate(() => Date.now() - 20 * 60000);
  const msgs = [
    { role: "you", text: `${SUMMARY}How is the header going?` },
    { role: "agent", text: "The header is centred now; I sent it back for its check." },
    { role: "you", text: "Hand off: write a short note so a fresh session can carry on from here.", cue: true },
    { role: "agent", text: "The hand-off note is written, and that was the only change." },
    { role: "you", text: "#d5 (Header) finished: it's ready for your check.", cue: true },
    { role: "agent", text: "Main doesn't build since #d5, so I'm sending it back." },
    { role: "you", text: `${SUMMARY}What's left today?` },
    { role: "agent", text: "Only #d6 is left, and it's at work." },
  ].map((m, k) => ({ images: [], ...m, at_ms: t0 + k * MINUTE }));
  await cue.change((st, msgs) => { Object.assign(st.sessions.find((x) => x.session_id === "lead"), { thread: msgs, state: "waiting" }); }, msgs);
  const step = (id, verb, subject, at) => ({ t: "step", id, kind: "run", verb, subject, mono: true, result: "done", bad: false, done: true, at_ms: at });
  const turns = [0, 2, 4, 6].map((k) => ({ at_ms: t0 + k * MINUTE, prompt: "", more: 0, items: [
    step(`a${k}`, "Ran", `git log --oneline -${k + 1}`, t0 + k * MINUTE + 10000),
    step(`b${k}`, "Ran", "git status --short", t0 + k * MINUTE + 20000),
    { t: "say", text: msgs[k + 1].text, at_ms: msgs[k + 1].at_ms },
  ] }));
  await cue.override("session_steps", ({ sessionId }, turns) => (sessionId !== "lead" ? { version: 1, turns: [] } : { version: 1, turns }), turns);
  return turns;
}

test("the lead's chat: steps only for your turns, each with the lead's reply under it; Cue's turns leave with their steps", async ({ page, cue }) => {
  await cue.open("desk");
  await leadTurns(cue);
  await openDesk(page);
  const chat = page.locator(".dk-chat");
  await expect(chat.locator(".cv-agent", { hasText: "Only #d6 is left" })).toHaveCount(1);
  // Each part in order: your words, its steps, its reply; twice. Nothing of Cue's turns.
  const parts = () => chat.evaluate((el) => [...el.querySelectorAll(".cv-you, .stx, .cv-agent")].map((x) => (x.classList.contains("stx") ? "steps" : x.classList.contains("cv-you") ? "you" : "reply")));
  await expect.poll(parts).toEqual(["you", "steps", "reply", "you", "steps", "reply"]);
  await expect(chat).not.toContainText("hand-off note");
  await expect(chat).not.toContainText("sending it back");
  await expect(chat.locator(".stx-line", { hasText: "git log --oneline -3" })).toHaveCount(0);
  await expect(chat.locator(".stx-line", { hasText: "git log --oneline -5" })).toHaveCount(0);
  // The newest turn open with its Show all; the older one folded to its chip.
  const chips = chat.locator(".stx-fold");
  await expect(chips).toHaveCount(2);
  await expect(chips.first()).toHaveAttribute("aria-expanded", "false");
  await expect(chips.first()).toContainText("2 steps");
  await expect(chips.last()).toHaveAttribute("aria-expanded", "true");
  await expect(chat.locator(".stx-all")).toHaveCount(1);
  await expect(chat.locator(".stx-line")).toHaveText([/git log --oneline -7/, /git status --short/]);
});

test("a Desk session at work on Cue's hand-off keeps its steps in view before it answers", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.override("session_log_page", () => []);
  await cue.override("transcript_page", () => []);
  const at = await cue.page.evaluate(() => Date.now() - 60000);
  await cue.change((st, at) => {
    Object.assign(st.sessions.find((x) => x.session_id === "host"), { state: "working", thread: [{ role: "you", text: "Desk item #d5 (feature): Header stays put", cue: true, kind: "brief", images: [], at_ms: at }] });
  }, at);
  await cue.override("session_steps", ({ sessionId }, at) => (sessionId !== "host" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at, prompt: "", more: 0, items: [
    { t: "step", id: "h1", kind: "read", verb: "Read", subject: "header.js", mono: true, result: "40 lines", bad: false, done: true, at_ms: at + 10000 },
  ] }] }), at);
  await openDesk(page);
  await page.locator('[data-desk-item="d5"]').first().click();
  const chat = page.locator(".dk-right .dkn-sp[data-desk-page='session']");
  await expect(chat.locator(".stx-fold")).toContainText("Working · 1 step");
  await expect(chat.locator(".stx-line")).toHaveText([/Read header\.js/]);
});

test("a turn that was the newest folds once a newer turn of yours comes in, in place", async ({ page, cue }) => {
  await cue.open("desk");
  const prev = await leadTurns(cue);
  await openDesk(page);
  const chat = page.locator(".dk-chat");
  const chips = chat.locator(".stx-fold");
  await expect(chips).toHaveCount(2);
  await expect(chips.last()).toHaveAttribute("aria-expanded", "true");
  // You write again; the lead takes a step and answers.
  await cue.page.evaluate(() => {
    const s = sessionOf("lead"), at = Date.now() - 60000;
    s.thread.push({ role: "you", text: "<desk project=\"cue\">\nNone.\n</desk>\n\nAnd tomorrow?", images: [], at_ms: at }, { role: "agent", text: "Tomorrow starts with #d7.", images: [], at_ms: at + 30000 });
    renderMain();
  });
  await cue.override("session_steps", ({ sessionId }, prev) => {
    const at = Date.now() - 60000;
    return sessionId !== "lead" ? { version: 2, turns: [] } : { version: 2, turns: [...prev, { at_ms: at, prompt: "", more: 0, items: [
      { t: "step", id: "z1", kind: "run", verb: "Ran", subject: "git log --oneline -9", mono: true, result: "done", bad: false, done: true, at_ms: at + 10000 },
      { t: "say", text: "Tomorrow starts with #d7.", at_ms: at + 30000 },
    ] }] };
  }, prev);
  await expect(chips).toHaveCount(3, { timeout: 15000 });
  await expect(chips.nth(0)).toHaveAttribute("aria-expanded", "false");
  await expect(chips.nth(1)).toHaveAttribute("aria-expanded", "false");
  await expect(chips.nth(1)).toContainText("2 steps · 2 commands");
  await expect(chips.nth(2)).toHaveAttribute("aria-expanded", "true");
  await expect(chat.locator(".stx-all")).toHaveCount(1);
  await expect(chat.locator(".stx-line")).toHaveText([/git log --oneline -9/]);
});
