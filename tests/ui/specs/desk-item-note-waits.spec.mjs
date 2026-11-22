// A note you send on an item's page while its session is busy (#419): it shows as your words alone with "Waiting:
// host-lead is finishing something, your note goes in next" under it, kept in Cue or queued; once the session takes it,
// the chat's Working line instead. The item leaves Waiting on you when the note goes (the hub puts it back at work): in
// the Items list, the Progress view and the Waiting on you count.
import { test, expect, openDesk } from "./cue.mjs";

const openItem = async (page, id) => {
  await openDesk(page);
  await page.locator(`[data-desk-item="${id}"]`).first().click();
};
const WAIT = "Waiting: host-lead is finishing something, your note goes in next";

test("a note kept for its busy session shows your words and that it goes in next, then the Working line once taken", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "host").state = "working";
    st.sessions.find((x) => x.session_id === "host").held = { role: "you", text: `About #d5 (${st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5").title}): make the button bigger`, at_ms: Date.now(), images: [] };
  });
  await openItem(page, "d5");
  const ichat = page.locator(".dk-right .dk-ichat"), held = ichat.locator(".cv-you.msg-held");
  await expect(held.locator(".cv-you-text")).toHaveText("make the button bigger");
  await expect(held.locator(".ml-words")).toHaveText(WAIT);
  await expect(held).toContainText("Edit");
  await expect(held).toContainText("Send now");
  await expect(ichat).not.toContainText("About #d5");
  // Taken: no waiting line, the chat's Working line.
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "host");
    s.thread = [...(s.thread || []), { role: "you", text: s.held.text, at_ms: Date.now() }];
    delete s.held;
  });
  await expect(ichat.getByText(WAIT)).toHaveCount(0);
  await expect(ichat.locator('[data-key="where"]')).toHaveCount(1);
});

test("a send-back queued in its session shows your words alone, waiting", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "host").state = "working";
    st.sessions.find((x) => x.session_id === "host").queued = { role: "you", text: `Desk item #d5 (${st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5").title}) isn't done: the colour is off\nIts fixes stay in #d5; don't add new items for them. Say DONE: #d5 when it is.`, at_ms: Date.now(), images: [] };
  });
  await openItem(page, "d5");
  const q = page.locator(".dk-right .dk-ichat .cv-you.msg-queued");
  await expect(q.locator(".cv-you-text")).toHaveText("the colour is off");
  await expect(q.locator(".ml-words")).toHaveText(WAIT);
  await expect(q).toContainText("Send now");
});

test("an item your note went to leaves Waiting on you: the list, Progress and the count", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5"), { state: "needs_you" }); });
  await openDesk(page);
  const items = page.locator(".dk-items"), you = items.locator(".dk-gh.you");
  await expect(you).toHaveText(/^Waiting on you \d+$/);
  const before = Number((await you.innerText()).match(/\d+$/)[0]);
  await expect(page.locator('[data-desk-gbody="you"] [data-desk-item="d5"]')).toHaveCount(1);
  await items.locator(".dk-ch [data-desk-prog-toggle]").click();
  const count = page.locator(".dk-prog .dk-grcount"), needs = async () => Number((await count.innerText()).match(/(\d+) needs? you/)?.[1] || 0);
  await expect(count).toBeVisible();
  const needBefore = await needs();
  await page.keyboard.press("Escape");
  // The note went: the hub has it at work again, its session busy with your note held.
  await cue.change((st) => {
    Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5"), { state: "working" });
    st.sessions.find((x) => x.session_id === "host").state = "working";
    st.sessions.find((x) => x.session_id === "host").held = { role: "you", text: `About #d5 (${st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5").title}): yes, and smaller`, at_ms: Date.now(), images: [] };
  });
  await expect(you).toHaveText(`Waiting on you ${before - 1}`);
  await expect(page.locator('[data-desk-gbody="you"] [data-desk-item="d5"]')).toHaveCount(0);
  await items.locator(".dk-ch [data-desk-prog-toggle]").click();
  await expect.poll(needs).toBe(needBefore - 1);
  await expect(page.locator('.dk-prog [data-desk-prog-row="d5"] .dk-grw.you')).toHaveCount(0);
});

test("a send-back waiting for a free working folder says so, not Sending, and the item isn't Waiting on you", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const it = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5");
    Object.assign(it, { state: "queued", tree_wait: true, back_waiting: "The canvas still opens blank" });
  });
  await openItem(page, "d5");
  // Sent back from its page (desk/act.js deskSendBack), the hub keeping it till a working folder is free.
  await page.evaluate(() => { deskPend("d5", "The canvas still opens blank", "back"); renderMain(); });
  const right = page.locator(".dk-right"), ichat = right.locator(".dk-ichat");
  await expect(ichat.locator(".cv-you .ml-words", { hasText: "Waiting: sent back, it starts when a working folder is free" })).toHaveCount(1);
  await expect(ichat).not.toContainText("Sending…");
  // Its row says it waits for a folder, sent back (whether it's Waiting on you is the hub's: desk_copy_wait.rs). In the
  // Items list: ‹ Items first, the item's page took its place (#666).
  await page.locator(".dk-right .dk-ph .dk-back").first().click();
  await expect(page.locator('.dk-items [data-desk-item="d5"]').first()).toContainText("Sent back · waiting for a free working folder");
});
