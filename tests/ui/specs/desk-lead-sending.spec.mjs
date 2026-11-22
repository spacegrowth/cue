// A message on its way to an idle Desk lead (#587): Sending… under it, and nothing under that saying the lead waits on
// your reply; once its session has it, Delivered, and where the lead is shows again.
import { test, expect, openDesk } from "./cue.mjs";

test("while your message to an idle lead is on its way, nothing says the lead waits on your reply", async ({ page, cue }) => {
  await cue.open("desk");
  // The lead idle, waiting on you for nothing but your reply.
  await cue.change((st) => {
    st.items = st.items.filter((i) => i.session_id !== "lead");
    Object.assign(st.sessions.find((x) => x.session_id === "lead"), { state: "waiting", doing: "", since_ms: Date.now() - 60000 });
  });
  await openDesk(page);
  const live = page.locator(".dk-chat .cv-status");
  await expect(live.locator(".cv-what")).toHaveText("Waiting on you for your reply");
  // The send doesn't come back until the test lets it.
  await page.evaluate(() => { window.letGo = null; });
  await cue.override("desk_send", () => new Promise((r) => { window.letGo = () => r({ sid: "lead" }); }));
  await page.locator(".dk-right .foot:not(.held) .composer textarea").fill("Is the footer done?");
  await page.locator(".dk-right .foot:not(.held) .composer textarea").press("Enter");
  await cue.called("desk_send", { project: "cue", text: "Is the footer done?" });
  const mine = page.locator(".dk-chat .cv-you.msg-sending", { hasText: "Is the footer done?" });
  await expect(mine).toHaveCount(1);
  await expect(page.locator(".dk-chat")).not.toContainText("Waiting on you for your reply");
  // It's in the lead's thread (after the Desk's summary), and it's at work on it: Delivered, its working line back.
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    s.thread.push({ role: "you", text: '<desk project="cue">\n4 items.\n</desk>\n\nIs the footer done?', at_ms: Date.now() });
    Object.assign(s, { state: "working", doing: "Reading footer.js…", doing_ms: Date.now(), since_ms: Date.now() });
  });
  await page.evaluate(() => window.letGo());
  await expect(page.locator(".dk-chat .msg-sending")).toHaveCount(0);
  await expect(page.locator(".dk-chat .cv-you", { hasText: "Is the footer done?" }).locator(".msg-line")).toContainText("Delivered");
  await expect(live).toContainText("Reading footer.js");
});
