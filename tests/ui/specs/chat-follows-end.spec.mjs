// A chat at its end follows what comes in after it was drawn (#795): a reply growing in place, a picture or a block
// laying out late, a question card taking the box's place under it. Measured after the new content lays out, in the
// lead's chat, a session's chat on the Desk and the Board's chat alike (ui/box/end.js). Scrolled up to read: where you
// are stays put.
import { test, expect, openDesk } from "./cue.mjs";

const atEnd = (c) => c.evaluate((el) => el.scrollHeight - el.clientHeight - el.scrollTop < 4);
/** The end of chat `c` lays out `px` taller with no redraw (words still being written, a picture loading): a style
 *  sheet, so a redraw of the chat keeps it. Returns the chat's height before. */
const grow = (c, px = 400) => c.evaluate((el, px) => {
  const sel = el.matches("[data-desk-chat]") ? `.dk-chat[data-desk-chat="${el.dataset.deskChat}"]` : "#app [data-chat]";
  const st = document.getElementById("late-test") || document.head.appendChild(Object.assign(document.createElement("style"), { id: "late-test" }));
  const was = el.scrollHeight;
  st.textContent += `${sel} > :last-child { padding-bottom: ${(st.dataset.px = +(st.dataset.px || 0) + px)}px; }\n`;
  return was;
}, px);
/** Chat `c` laid out taller than `was`: what grew is in it. */
const grewFrom = async (c, was) => expect.poll(() => c.evaluate((el) => el.scrollHeight)).toBeGreaterThan(was + 100);
/** You scroll `c` up to its middle (a wheel of yours, then where it lands). */
const readUp = (c) => c.evaluate((el) => {
  el.dispatchEvent(new WheelEvent("wheel", { bubbles: true, deltaY: -100 }));
  el.scrollTop = Math.floor((el.scrollHeight - el.clientHeight) / 2);
  el.dispatchEvent(new Event("scroll"));
  return el.scrollTop;
});
const longThread = (st, sid) => {
  const s = st.sessions.find((x) => x.session_id === sid);
  s.state = "working";
  for (let n = 1; n <= 30; n++) s.thread.push({ role: n % 2 ? "you" : "assistant", text: `Earlier message ${n}\n\n${"A line of words. ".repeat(20)}`, at_ms: Date.now() - (40 - n) * 60000 });
};

// Laptop size, and Cue's smallest window.
for (const [w, h] of [[1440, 900], [900, 600]]) {
  for (const scheme of ["light", "dark"]) {
    test.describe(`${w}×${h} ${scheme}`, () => {
      test.use({ viewport: { width: w, height: h }, colorScheme: scheme });

      test("the lead's chat follows a reply that grows after it's drawn, and a question that comes under it", async ({ page, cue }) => {
        await cue.open("desk");
        await cue.change(longThread, "lead");
        await openDesk(page);
        const chat = page.locator(".dk-right .dk-chat");
        await expect(chat).toContainText("Earlier message 30");
        await expect.poll(() => atEnd(chat)).toBe(true);
        // The reply grows where it is, with no redraw: the chat goes with it.
        await grewFrom(chat, await grow(chat));
        await expect.poll(() => atEnd(chat)).toBe(true);
        // A reply arrives, then lays out taller a moment later.
        await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").thread.push({ role: "assistant", text: `Here's where it stands. ${"More words. ".repeat(30)}`, at_ms: Date.now() }); });
        await expect(chat).toContainText("Here's where it stands.");
        await grewFrom(chat, await grow(chat, 300));
        await expect.poll(() => atEnd(chat)).toBe(true);
        // The lead asks: its card takes the box's place under the chat. The chat's end stays in view above it, and the
        // card isn't cut off, its every choice on screen, even when the card grows after it's drawn.
        await cue.change((st) => {
          st.sessions.find((s) => s.session_id === "lead").state = "deciding";
          st.items.push({ ...st.items[0], id: "q-late", session_id: "lead", kind: "question", agent_id: "", agent_name: "", desk: true, hosted: false, status: "pending", tool_name: "AskUserQuestion",
            tool_input: { questions: [{ question: "Which layout for the footer?", header: "", multiSelect: false, options: ["One row", "Two rows", "Stacked", "Hidden"].map((label) => ({ label, description: "What follows if you pick it, in a line." })) }] } });
        });
        const card = page.locator(".dk-right .dk-ask");
        await expect(card).toContainText("Which layout for the footer?");
        await page.addStyleTag({ content: ".dk-right .dk-ask { padding-bottom: 60px; }" });
        await expect.poll(() => atEnd(chat)).toBe(true);
        // The card starts at its top, its question whole, even with the cursor in the box held under it. At laptop size
        // every choice and its buttons show; in the smallest window the first choice does, the rest a scroll inside it.
        await expect(card.locator(".qtext")).toBeInViewport({ ratio: 1 });
        expect(await card.locator(".act-card").evaluate((el) => el.scrollTop)).toBe(0);
        const shown = w >= 1440 ? ["One row", "Two rows", "Stacked", "Hidden", "Decline"] : ["One row"];
        for (const label of shown) await expect(card.getByText(label, { exact: true })).toBeInViewport({ ratio: 1 });
        await expect(card.locator(".foot.held textarea")).toBeInViewport({ ratio: 1 });
      });

      test("scrolled up in the lead's chat, what comes in doesn't move you", async ({ page, cue }) => {
        await cue.open("desk");
        await cue.change(longThread, "lead");
        await openDesk(page);
        const chat = page.locator(".dk-right .dk-chat");
        await expect.poll(() => atEnd(chat)).toBe(true);
        const mid = await readUp(chat);
        await grow(chat);
        await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").thread.push({ role: "assistant", text: "A new reply while you read.", at_ms: Date.now() }); });
        await expect(chat).toContainText("A new reply while you read.");
        await grow(chat, 200);
        await page.waitForTimeout(400);
        expect(await chat.evaluate((el) => el.scrollTop)).toBe(mid);
      });

      test("a session's chat opened on the Desk follows what grows in it after it's drawn", async ({ page, cue }) => {
        await cue.open("desk");
        await cue.change(longThread, "host");
        await openDesk(page);
        await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
        const chat = page.locator('.dk-right [data-desk-chat="host"]');
        await expect(chat).toContainText("Earlier message 30");
        await expect.poll(() => atEnd(chat)).toBe(true);
        await grewFrom(chat, await grow(chat));
        await expect.poll(() => atEnd(chat)).toBe(true);
        const mid = await readUp(chat);
        await grow(chat, 200);
        await page.waitForTimeout(400);
        expect(await chat.evaluate((el) => el.scrollTop)).toBe(mid);
      });
    });
  }
}

test("the Board's chat follows what grows in it after it's drawn, and stays put while you read", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await cue.open("board-waiting");
  const chat = page.locator("#app [data-chat]").first();
  await chat.waitFor();
  // Long enough to scroll.
  await page.addStyleTag({ content: "#app [data-chat] > :first-child { padding-top: 3000px; }" });
  await chat.evaluate((el) => { el.scrollTop = el.scrollHeight; el.dispatchEvent(new Event("scroll")); });
  await expect.poll(() => atEnd(chat)).toBe(true);
  await grewFrom(chat, await grow(chat));
  await expect.poll(() => atEnd(chat)).toBe(true);
  const mid = await readUp(chat);
  await grow(chat, 200);
  await page.waitForTimeout(400);
  expect(await chat.evaluate((el) => el.scrollTop)).toBe(mid);
});
