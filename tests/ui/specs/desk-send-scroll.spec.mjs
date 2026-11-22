// Sending in the lead's chat goes to its end and follows the reply as it comes (#666): even from higher up, the chat
// shows what you sent at once, and the lead's answer below it, without you scrolling down.
import { test, expect, openDesk } from "./cue.mjs";

const atEnd = (c) => c.evaluate((el) => el.scrollHeight - el.clientHeight - el.scrollTop < 4);

test.use({ viewport: { width: 1800, height: 1095 } });
test("send in the lead's chat: at its end at once, and it follows the reply", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    Object.assign(s, { state: "working", doing: "Reading footer.js…", doing_ms: Date.now(), since_ms: Date.now() });
    for (let n = 1; n <= 30; n++) s.thread.push({ role: n % 2 ? "you" : "assistant", text: `Earlier message ${n}\n\n${"A line of words. ".repeat(20)}`, at_ms: Date.now() - (40 - n) * 60000 });
  });
  await openDesk(page);
  const chat = page.locator(".dk-right .dk-chat");
  await expect(chat).toContainText("Earlier message 30");
  await expect.poll(() => atEnd(chat)).toBe(true);
  // Scrolled up to read, then you send.
  await chat.evaluate((el) => { el.scrollTop = 200; el.dispatchEvent(new Event("scroll")); });
  await page.locator(".dk-right .dk-box textarea").fill("Is the footer done?");
  await page.locator(".dk-right .dk-box textarea").press("Enter");
  await cue.called("desk_send", { project: "cue", text: "Is the footer done?" });
  await expect(chat.locator(".cv-you", { hasText: "Is the footer done?" })).toBeInViewport();
  await expect.poll(() => atEnd(chat)).toBe(true);
  // The lead's reply comes, then grows: the chat stays at its end.
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    s.thread.push({ role: "you", text: '<desk project="cue">\n4 items.\n</desk>\n\nIs the footer done?', at_ms: Date.now() });
    s.thread.push({ role: "assistant", text: `Yes. ${"The footer is done. ".repeat(40)}`, at_ms: Date.now() });
  });
  await expect(chat).toContainText("The footer is done.");
  await expect.poll(() => atEnd(chat)).toBe(true);
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    s.thread.push({ role: "assistant", text: `And one more thing. ${"More words here. ".repeat(40)}`, at_ms: Date.now() });
  });
  await expect(chat).toContainText("And one more thing.");
  await expect.poll(() => atEnd(chat)).toBe(true);
});
