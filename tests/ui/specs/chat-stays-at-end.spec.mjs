// A chat at its end stays at its true end (#855): through a reply coming in, the status line under it changing ("Waiting
// on you for your reply · 1m") and the box under it growing as you type, its last line is always whole above the box,
// in every frame, with no jump. Cue's window is WebKit, where the scroll of Cue putting a chat at its end is told a
// frame late: measured then, after the box had grown a line, the chat looked scrolled away and stopped following. So
// these run in WebKit too (playwright.config.mjs). A chat you scrolled up in stays where you put it.
import { test, expect, openDesk } from "./cue.mjs";

const words = (n) => `${"A line of words. ".repeat(n)}`;
/** Session `sid` at work, with a long thread: its chat scrolls. */
const longThread = (st, sid) => {
  const s = st.sessions.find((x) => x.session_id === sid);
  Object.assign(s, { state: "working", since_ms: Date.now() - 30000, doing: "Reading files", doing_ms: Date.now() - 5000 });
  s.thread = s.thread || [];
  for (let n = 1; n <= 30; n++) s.thread.push({ role: n % 2 ? "you" : "assistant", text: `Earlier message ${n}\n\n${"A line of words. ".repeat(20)}`, at_ms: Date.now() - (40 - n) * 60000 });
};
/** From now, every frame: how far chat `sel` is from its end (px). */
const watchEnd = (page, sel) => page.evaluate((sel) => {
  cancelAnimationFrame(window.__endRaf);
  window.__ends = [];
  const tick = () => {
    const c = document.querySelector(sel);
    if (c) window.__ends.push(Math.round(c.scrollHeight - c.scrollTop - c.clientHeight));
    window.__endRaf = requestAnimationFrame(tick);
  };
  tick();
}, sel);
/** The farthest chat was from its end in any frame since watchEnd. */
const farthest = (page) => page.evaluate(() => Math.max(0, ...window.__ends));
/** The chat's last line is whole above its bottom edge (the box under it). */
const lastWhole = (chat) => chat.evaluate((c) => {
  const r = c.getBoundingClientRect(), pad = parseFloat(getComputedStyle(c).paddingBottom) || 0;
  return c.lastElementChild.getBoundingClientRect().bottom <= r.bottom - pad + 1;
});
/** Session `sid` replies, and a moment later (its own update) it's waiting on you: the status line changes under it. */
const replyThenWait = async (page, cue, sid, text) => {
  await cue.change((st, [sid, text]) => { st.sessions.find((x) => x.session_id === sid).thread.push({ role: "assistant", text, at_ms: Date.now() }); }, [sid, text]);
  await page.waitForTimeout(150);
  await cue.change((st, sid) => { Object.assign(st.sessions.find((x) => x.session_id === sid), { state: "waiting", since_ms: Date.now() - 60000 }); }, sid);
};
/** Five more lines typed in `box`: it grows a line at a time. */
const typeLines = async (page, box) => {
  await box.click();
  await page.keyboard.type("First line");
  for (let i = 1; i <= 5; i++) { await page.keyboard.press("Shift+Enter"); await page.keyboard.type(`Line ${i + 1}`); }
};

// Each chat: how to open it at its end, with a long thread for session `sid`.
const CHATS = {
  "the lead's chat": { sid: "lead", chat: ".dk-right .dk-chat", box: ".dk-right textarea", async open(page, cue) {
    await cue.open("desk"); await cue.change(longThread, "lead"); await openDesk(page);
  } },
  "an item's chat": { sid: "host", chat: '.dk-right .dkn-sp [data-desk-chat="host"]', box: ".dk-right .dkn-sp textarea", async open(page, cue) {
    await cue.open("desk"); await cue.change(longThread, "host");
    await cue.change((st) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "working", now_on: true, said: [], latest: "" }); });
    await openDesk(page);
    await page.locator('.dkn-items [data-desk-item="d5"]').first().click();
  } },
  "a session's chat on the Desk": { sid: "host", chat: '.dk-right [data-desk-chat="host"]', box: ".dk-right textarea", async open(page, cue) {
    await cue.open("desk"); await cue.change(longThread, "host"); await openDesk(page);
    await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
  } },
  // The session the Board opens with, whichever that is at the window's size; only sessions at their prompt asking
  // (no question or permission card in its box's place).
  "a session's chat on the Board": { chat: "#app .active-pane [data-chat]", async open(page, cue) {
    await cue.open("board-waiting");
    await cue.change((st) => { st.items = st.items.filter((i) => i.kind === "waiting"); });
    await expect(page.locator(".active-pane textarea[data-text]")).toHaveCount(1);
    const sid = (await page.locator("#app .active-pane [data-chat-for]").getAttribute("data-chat-for")).slice(2);
    await cue.change(longThread, sid);
    return sid;
  } },
  "a crew lead's chat on its page": { sid: "lead", chat: ".crew-page [data-chat]", box: '.crew-page textarea[data-text="s:lead"]', async open(page, cue) {
    await cue.open("world-team");
    await page.locator('.nrow[data-big="n1793984390004"]').click();
    await page.locator('.crew-tray [data-cp-open="lead"]').click();
    await cue.change(longThread, "lead");
  } },
};

for (const [w, h] of [[1440, 900], [900, 600]]) {
  for (const scheme of ["light", "dark"]) {
    test.describe(`${w}×${h} ${scheme}`, () => {
      test.use({ viewport: { width: w, height: h }, colorScheme: scheme });
      for (const [name, c] of Object.entries(CHATS)) {
        test(`${name} stays at its end through a reply, its status line and the box growing`, async ({ page, cue }) => {
          const sid = (await c.open(page, cue)) || c.sid, box = c.box || `.active-pane textarea[data-text="s:${sid}"]`;
          const chat = page.locator(c.chat).first();
          // Its long thread drawn (29: the Board shows a working turn by its step, not the reply it's on).
          await expect(chat).toContainText("Earlier message 29");
          await expect.poll(() => chat.evaluate((el) => el.scrollHeight - el.scrollTop - el.clientHeight)).toBeLessThan(2);
          await watchEnd(page, c.chat);
          await replyThenWait(page, cue, sid, `Here's where it stands. ${words(30)}`);
          await expect(chat.locator(".cv-status").last()).toContainText("Waiting on you");
          await typeLines(page, page.locator(box).first());
          // Another reply while the box is tall.
          await cue.change((st, sid) => { st.sessions.find((x) => x.session_id === sid).state = "working"; }, sid);
          await replyThenWait(page, cue, sid, `And one more thing. ${words(30)}`);
          await expect(chat).toContainText("And one more thing.");
          await page.waitForTimeout(400);
          expect(await farthest(page), "never short of its end, in any frame").toBeLessThan(2);
          expect(await lastWhole(chat), "its last line whole above the box").toBe(true);
        });
      }

      test("an item's chat scrolled up stays where you put it as replies come and the box grows", async ({ page, cue }) => {
        const c = CHATS["an item's chat"];
        await c.open(page, cue);
        const chat = page.locator(c.chat).first();
        await expect(chat).toContainText("Earlier message 30");
        await expect.poll(() => chat.evaluate((el) => el.scrollHeight - el.scrollTop - el.clientHeight)).toBeLessThan(2);
        await chat.hover();
        await page.mouse.wheel(0, -600);
        await expect.poll(() => chat.evaluate((el) => el.scrollHeight - el.scrollTop - el.clientHeight)).toBeGreaterThan(300);
        await page.waitForTimeout(300);
        const top = await chat.evaluate((el) => el.scrollTop);
        await replyThenWait(page, cue, c.sid, `Here's where it stands. ${words(30)}`);
        await expect(chat).toContainText("Here's where it stands.");
        await typeLines(page, page.locator(c.box).first());
        await page.waitForTimeout(400);
        expect(await chat.evaluate((el) => el.scrollTop)).toBe(top);
      });
    });
  }
}
