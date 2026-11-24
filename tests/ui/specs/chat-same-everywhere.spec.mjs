// Every chat shows a reply the same way (#832, docs/chat-compare.md round 1): on an item's page and a session's page on
// the Desk, a reply has Read (which opens it) and Send to…, and item numbers and session names in it are links, as on
// the Board's chat, where they open the item on its Desk. btw opens its panel on a session's page. A crew member's
// Activity is its item's moves. Esc twice on a hosted session that hasn't answered yet gives back what you sent.
import { test, expect, openDesk } from "./cue.mjs";

const LONG = "Fixed the rail dot so it only blinks for a session you wrote to, and #d4 is ready to check now; the narrow window was fine as it was, nothing else changed there.";

const reply = (cue, sid, text) => cue.change((st, [sid, text]) => {
  const s = st.sessions.find((x) => x.session_id === sid), at = Date.now();
  s.thread = [{ role: "you", text: "How is the rail dot?", at_ms: at - 60000 }, { role: "agent", text, at_ms: at - 30000 }];
}, [sid, text]);

test("an item's page: a reply has Send to… and Read, Read opens it, and #d4 in it is a link", async ({ page, cue }) => {
  await cue.open("desk");
  await reply(cue, "host", LONG);
  await openDesk(page);
  await page.locator('.dkn-items [data-desk-item="d5"]').first().click();
  const msg = page.locator('.dk-right [data-desk-chat="host"] .cv-agent').last();
  await expect(msg.locator(".cv-fwd")).toHaveCount(1);
  await expect(msg.locator('a[data-desk-chk-item="d4"]')).toHaveCount(1);
  await msg.locator(".rd-pill").click();
  await expect(page.locator(".dk-right .rd-head")).toHaveCount(1);
});

test("a session's page on the Desk: Send to… and links on a reply, and btw opens its panel", async ({ page, cue }) => {
  await cue.open("desk");
  await reply(cue, "site", LONG);
  await openDesk(page);
  await page.evaluate(() => { deskShow({ kind: "chat", sid: "site" }); });
  const msg = page.locator('.dk-right [data-desk-chat="site"] .cv-agent').last();
  await expect(msg.locator(".cv-fwd")).toHaveCount(1);
  await expect(msg.locator('a[data-desk-chk-item="d4"]')).toHaveCount(1);
  await page.locator('.dk-right [data-act="btw"][data-sid="site"]').click();
  await expect(page.locator(".dk-right .btw-pop")).toHaveCount(1);
});

test("the lead's chat still has no Send to…", async ({ page, cue }) => {
  await cue.open("desk");
  await reply(cue, "lead", LONG);
  await openDesk(page);
  await expect(page.locator(".dk-chat .cv-agent").last().locator('a[data-desk-chk-item="d4"]')).toHaveCount(1);
  await expect(page.locator(".dk-chat .cv-fwd")).toHaveCount(0);
});

test("the Board's chat: #d4 in a reply is a link that opens the item on its Desk", async ({ page, cue }) => {
  await cue.open("desk");
  await reply(cue, "site", LONG);
  await page.evaluate(() => { view = "board"; setActive(null, "site"); renderMain(); });
  const link = page.locator('.active-pane .cv-agent a[data-desk-chk-item="d4"]');
  await expect(link).toHaveCount(1);
  await link.click();
  await expect.poll(() => page.evaluate(() => view)).toBe("desk");
  await expect.poll(() => page.evaluate(() => deskOpen?.kind === "item" && deskOpen.id)).toBe("d4");
});

test("Esc twice on a hosted session before it answered: stopped, and what you sent is back in the box", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "site"), at = Date.now();
    Object.assign(s, { state: "working", since_ms: at - 5000, hosted: true });
    s.thread = [{ role: "agent", text: "Done.", at_ms: at - 60000 }, { role: "you", text: "make the footer sticky", at_ms: at - 5000 }];
  });
  await page.evaluate(() => { view = "board"; setActive(null, "site"); renderMain(); });
  const box = page.locator('.active-pane textarea[data-text="s:site"]');
  await box.waitFor();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await cue.called("interrupt_session", { sessionId: "site" });
  await expect(box).toHaveValue("make the footer sticky");
});

test("Esc twice once it has answered: stopped, and nothing comes back into the box", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "site"), at = Date.now();
    Object.assign(s, { state: "working", since_ms: at - 5000, hosted: true });
    s.thread = [{ role: "you", text: "make the footer sticky", at_ms: at - 5000 }, { role: "agent", text: "On it.", at_ms: at - 2000 }];
  });
  await page.evaluate(() => { view = "board"; setActive(null, "site"); renderMain(); });
  const box = page.locator('.active-pane textarea[data-text="s:site"]');
  await box.waitFor();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await cue.called("interrupt_session", { sessionId: "site" });
  await expect(box).toHaveValue("");
});
