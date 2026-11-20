// Esc on the Desk (#494), as on the Board (app/keys.js escSession): a message Cue kept for the session the Desk has
// open comes back into its box, and Esc twice stops that session while it works. In the lead's chat, on an item's page
// (its holder's) and on a session's page. Desk panels still close first.
import { test, expect, openDesk } from "./cue.mjs";

const hold = (cue, sid, text) => cue.change((st, [sid, text]) => { st.sessions.find((s) => s.session_id === sid).held = { text, images: [], at_ms: Date.now() }; }, [sid, text]);

test("the lead's chat: Esc brings back a kept message into its box, then Esc twice stops the lead", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await hold(cue, "lead", "use the new pool");
  const box = page.locator(".dk-box textarea");
  await page.keyboard.press("Escape");
  await cue.called("unhold_message", { sessionId: "lead" });
  await expect(box).toHaveValue("use the new pool");
  await expect(box).toBeFocused();
  // Nothing kept now: once arms Stop, twice stops it; the lead's chat stays.
  await page.keyboard.press("Escape");
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
  await page.keyboard.press("Escape");
  await cue.called("interrupt_session", { sessionId: "lead" });
  await expect(box).toHaveValue("use the new pool");
});

test("an item's page: Esc brings back what Cue kept for its holder without the About line, Esc twice stops its holder", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  const title = await page.evaluate(() => deskItem("cue", "d5").title);
  await hold(cue, "host", `About #d5 (${title}): try the other port`);
  const box = page.locator(".dk-right .dk-box textarea");
  await page.keyboard.press("Escape");
  await cue.called("unhold_message", { sessionId: "host" });
  await expect(box).toHaveValue("try the other port");
  await expect(page.locator(".dk-right [data-desk-page]")).toHaveCount(1);
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await cue.called("interrupt_session", { sessionId: "host" });
  expect((await cue.calls("interrupt_session")).map((c) => c.args.sessionId)).toEqual(["host"]);
});

test("a session's page: Esc twice stops it; once it's stopped, Esc goes back as before", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator(".dk-sessions .dk-srow:not(.crew)", { hasText: "host-lead" }).first().click();
  const sp = page.locator('.dk-right .dk-pg[data-desk-page="session"]');
  await expect(sp).toHaveCount(1);
  await hold(cue, "host", "and the logs");
  await page.keyboard.press("Escape");
  await cue.called("unhold_message", { sessionId: "host" });
  await expect(page.locator(".dk-right .dk-box textarea")).toHaveValue("and the logs");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await cue.called("interrupt_session", { sessionId: "host" });
  // Stopped (waiting now) and nothing kept: Esc is the Desk's again, out of the box first, then the page closes.
  await expect.poll(() => page.evaluate(() => sessionOf("host").state)).toBe("waiting");
  await page.locator(".dk-right .dk-box textarea").fill("");
  await page.keyboard.press("Escape");
  await expect(sp).toHaveCount(0);
});

test("Desk panels close before Esc reaches the session: the Give to… menu, the switcher", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => { deskGiveFor = { id: "d6" }; renderMain(); });
  expect(await page.evaluate(() => deskGiveFor?.id)).toBe("d6");
  await page.keyboard.press("Escape");
  expect(await page.evaluate(() => deskGiveFor)).toBeNull();
  await page.keyboard.press("Escape");
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
  await page.evaluate(() => { deskSwitchOpen = true; renderMain(); });
  await page.keyboard.press("Escape");
  expect(await page.evaluate(() => deskSwitchOpen)).toBe(false);
  expect(await cue.calls("interrupt_session")).toHaveLength(0);
});
