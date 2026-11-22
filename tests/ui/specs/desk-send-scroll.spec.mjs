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
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("Is the footer done?");
  await page.locator(".dk-right .foot:not(.held) .composer textarea").press("Enter");
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

/** The lead idle, with a long chat, its Desk open at the chat's end. */
const longChat = async (page, cue) => {
  await cue.open("desk");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    s.state = "waiting";
    for (let n = 1; n <= 30; n++) s.thread.push({ role: n % 2 ? "you" : "assistant", text: `Earlier message ${n}\n\n${"A line of words. ".repeat(20)}`, at_ms: Date.now() - (40 - n) * 60000 });
  });
  await openDesk(page);
  const chat = page.locator(".dk-right .dk-chat");
  await expect.poll(() => atEnd(chat)).toBe(true);
  return chat;
};
const leadBox = (page) => page.locator(".dk-right .foot:not(.held) .composer textarea");

test("typing a long message in the lead's box keeps the chat's end in view, and sending ends there (#704)", async ({ page, cue }) => {
  const chat = await longChat(page, cue);
  await leadBox(page).click();
  for (let i = 0; i < 10; i++) { await page.keyboard.type(`Line ${i} of what I want to say`); if (i < 9) await page.keyboard.press("Shift+Enter"); }
  // The box grew over the chat: the chat moved up with it, its last message still in full view.
  await expect.poll(() => atEnd(chat)).toBe(true);
  await page.keyboard.press("Enter");
  await cue.called("desk_send", { project: "cue" });
  await expect(chat.locator(".cv-you", { hasText: "Line 9 of what I want to say" })).toBeInViewport();
  await expect.poll(() => atEnd(chat)).toBe(true);
});

test("a / command sent from the lead's box, scrolled up, brings its chat to the end (#704)", async ({ page, cue }) => {
  const chat = await longChat(page, cue);
  await chat.evaluate((el) => { el._youAt = Date.now(); el.scrollTop = 200; el.dispatchEvent(new Event("scroll")); });
  await expect.poll(() => page.evaluate(() => deskChatEnd)).toBe(false);
  await leadBox(page).fill("/compact");
  await leadBox(page).press("Enter");
  await cue.called("send_to_session", { sessionId: "lead" });
  await expect.poll(() => atEnd(chat)).toBe(true);
  expect(await page.evaluate(() => deskChatEnd)).toBe(true);
});

test("a scroll right after the chat was put at its end, not from you, doesn't stop it following (#704)", async ({ page, cue }) => {
  const chat = await longChat(page, cue);
  await leadBox(page).fill("Is the footer done?");
  await leadBox(page).press("Enter");
  await cue.called("desk_send", { project: "cue", text: "Is the footer done?" });
  // The page settling under it moves it a little off its end, with no wheel or touch of yours.
  await chat.evaluate((el) => { el.scrollTop -= 120; el.dispatchEvent(new Event("scroll")); });
  expect(await page.evaluate(() => deskChatEnd)).toBe(true);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").thread.push({ role: "assistant", text: `Yes. ${"The footer is done. ".repeat(40)}`, at_ms: Date.now() }); });
  await expect(chat).toContainText("The footer is done.");
  await expect.poll(() => atEnd(chat)).toBe(true);
});
