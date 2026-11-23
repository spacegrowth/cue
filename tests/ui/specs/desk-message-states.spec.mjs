// Your own messages in every Desk chat are drawn by the board's chat code (#688): the same bubble, the item's head over
// your words, the board's line for each state (sending, queued, kept, delivered, couldn't send) and its Send again
// (no by-the-way offer under a kept one, #731). A send-back ("Not done") is yours too, never Cue's card. What you sent from the Desk's box
// sits among the chat's own rows, one list with its kept and queued messages.
import { test, expect, openDesk } from "./cue.mjs";

const HELD = "Not sent yet · Cue sends it when its turn ends";
const backText = (id, title, words) => `Desk item #${id} (${title}) isn't done: ${words}\nIts fixes stay in #${id}; don't add new items for them. Say DONE: #${id} when it is.`;
const openItem = async (page, id) => {
  await openDesk(page);
  await page.locator(`[data-desk-item="${id}"]`).first().click();
};
const busyHost = (cue, field, text) => cue.change((st, [field, text]) => {
  const s = st.sessions.find((x) => x.session_id === "host");
  s.state = "working";
  s[field] = { role: "you", text: text.replace("TITLE", st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5").title), at_ms: Date.now(), images: [] };
}, [field, text]);

test("a note kept for the item's busy holder reads as on the board: its head, the board's words, Edit · Send now · Cancel", async ({ page, cue }) => {
  await cue.open("desk");
  await busyHost(cue, "held", "About #d5 (TITLE): make the button bigger");
  await openItem(page, "d5");
  const held = page.locator('.dk-right .dkn-sp[data-desk-page="session"] [data-desk-chat="host"] .cv-you.msg-held');
  await expect(held).toHaveCount(1);
  await expect(held.locator(".dk-about")).toContainText("#d5");
  await expect(held.locator(".cv-you-text")).toContainText("make the button bigger");
  await expect(held.locator(".ml-words")).toHaveText(HELD);
  await expect(held.locator(".ml-acts")).toContainText("Edit · Send now · Cancel");
  await expect(page.locator(".dk-right")).not.toContainText("is finishing something");
  await expect(held).not.toContainText("by the way");
  await expect(held).not.toContainText("Keep it queued");
});

test("Not done is your own bubble in the item's chat, in every state, never Cue's card", async ({ page, cue }) => {
  await cue.open("desk");
  await busyHost(cue, "held", backText("d5", "TITLE", "the colour is off"));
  await openItem(page, "d5");
  const ichat = page.locator('.dk-right .dkn-sp[data-desk-page="session"] [data-desk-chat="host"]');
  const mine = ichat.locator(".cv-you.mg-you", { hasText: "the colour is off" });
  // Kept in Cue.
  await expect(mine).toHaveCount(1);
  await expect(mine.locator(".dk-about")).toContainText("#d5");
  await expect(mine.locator(".ml-words")).toHaveText(HELD);
  await expect(ichat.locator(".cue-msg")).toHaveCount(0);
  // Queued in its agent.
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "host"); s.queued = s.held; s.held = null; });
  await expect(mine.locator(".msg-line")).toHaveClass(/ml-queued/);
  await expect(mine.locator(".ml-words")).toHaveText("Queued for host-lead · it reads this after its current step");
  // Typed, Cue hasn't seen it take it yet.
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "host"); s.thread = [...(s.thread || []), { ...s.queued, pending: true }]; s.queued = null; s.state = "idle"; });
  await expect(mine.locator(".msg-line")).toHaveClass(/ml-sending/);
  // Delivered.
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "host"); s.thread.at(-1).pending = false; });
  await expect(mine.locator(".msg-line")).toHaveClass(/ml-delivered/);
  await expect(mine.locator(".ml-words")).toHaveText(/^You · .+ ago$/);
  await expect(mine.locator(".dk-about")).toContainText("#d5");
  // Couldn't go: the board's Send again.
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "host"); s.thread.at(-1).unsent = true; });
  await expect(mine.locator(".msg-line")).toHaveClass(/ml-failed/);
  await expect(mine.locator('[data-act="resend"]')).toHaveText("Send again");
  await expect(ichat.locator(".cue-msg")).toHaveCount(0);
});

test("Not done to the lead is your own bubble in the lead's chat, not Cue's line", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st, backText) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    s.thread = [...(s.thread || []), { role: "you", text: backText.replace("TITLE", st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").title), at_ms: Date.now() }];
  }, backText("d2", "TITLE", "Fix the spacing"));
  await openDesk(page);
  const chat = page.locator(".dk-chat");
  const mine = chat.locator(".cv-you.mg-you", { hasText: "Fix the spacing" });
  await expect(mine).toHaveCount(1);
  await expect(mine.locator(".dk-about")).toContainText("#d2");
  await expect(mine.locator(".cv-you-text")).not.toContainText("isn't done");
  await expect(mine.locator(".msg-line")).toHaveClass(/ml-delivered/);
  await expect(chat.locator(".dk-cue", { hasText: "Fix the spacing" })).toHaveCount(0);
});

test("a timed send on the item's chat has the item's head, not Cue's About line", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "host");
    s.send_at = { text: `About #d5 (${st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5").title}): check it again`, at_ms: Date.now() + 3600000 };
  });
  await openItem(page, "d5");
  const row = page.locator('.dk-right .dkn-sp[data-desk-page="session"] [data-desk-chat="host"] [data-key="send-at"]');
  await expect(row.locator(".dk-about")).toContainText("#d5");
  await expect(row.locator(".cv-you-text")).toContainText("check it again");
  await expect(row).not.toContainText("About #d5");
});

test("a send from the Desk box that didn't go offers the board's Send again in the lead's chat", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.override("desk_send", () => { throw "offline"; });
  // The Desk's own box (the lead's writes to a "desk:" key).
  const box = page.locator('.dk-right textarea[data-text^="desk:"]');
  await box.fill("Footer too");
  await box.press("Enter");
  const bad = page.locator(".dk-chat .cv-you.msg-failed", { hasText: "Footer too" });
  await expect(bad.locator(".msg-line")).toHaveText("! Couldn't send · offline · Send again");
  await cue.override("desk_send", () => ({ sid: "lead" }));
  await bad.getByRole("button", { name: "Send again" }).click();
  await cue.called("desk_send", { project: "cue", text: "Footer too" });
  await expect(page.locator(".dk-chat .msg-failed")).toHaveCount(0);
});

test("what you sent from the Desk box sits among the chat's own rows, once, drawn as the board's outbox", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => { deskSent.set("cue", [{ text: "About #d2 (Quick phrases at the right of Send): bigger please", images: [], at: Date.now(), sent: 0, project: "cue" }]); renderMain(); });
  const chat = page.locator(".dk-chat");
  const row = chat.locator(".cv-you", { hasText: "bigger please" });
  await expect(row).toHaveCount(1);
  await expect(row.locator(".msg-line")).toHaveClass(/ml-sending/);
  await expect(row.locator(".dk-about")).toContainText("#d2");
  await expect(row).toHaveAttribute("data-key", /^p:/);
  // Kept by the lead's session (busy): the chat's own kept row, not a second one.
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    s.state = "working";
    s.held = { role: "you", text: "About #d2 (Quick phrases at the right of Send): bigger please", at_ms: Date.now(), images: [] };
  });
  await expect(row).toHaveCount(1);
  await expect(row.locator(".ml-words")).toHaveText(HELD);
  await expect(row.locator(".dk-about")).toContainText("#d2");
});
