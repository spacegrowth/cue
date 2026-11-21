// ⌘↵ in the Desk lead's chat box does what it does in a session's chat (#630): with the lead at work, what you typed
// goes to it now instead of being kept in Cue until its turn ends; with nothing typed, the message Cue kept for it goes
// now. ↵ still sends (kept while the lead works) and ⇧↵ still makes a new line.
import { test, expect, openDesk } from "./cue.mjs";

const sends = (cue) => cue.calls("desk_send").then((c) => c.map((x) => ({ text: x.args.text, now: !!x.args.now })));

test("⌘↵ in the lead's chat box sends now, as in a session's chat; ↵ and ⇧↵ as before (#630)", async ({ page, cue }) => {
  await cue.open("desk");
  // The lead at work on something.
  await cue.change((st) => {
    Object.assign(st.sessions.find((x) => x.session_id === "lead"), { state: "working", doing: "Reading footer.js…", doing_ms: Date.now(), since_ms: Date.now() });
  });
  await openDesk(page);
  const box = page.locator(".dk-box textarea");
  await box.click();
  await page.keyboard.type("Stop, the footer is fine");
  await page.keyboard.press("Meta+Enter");
  await expect.poll(() => sends(cue)).toEqual([{ text: "Stop, the footer is fine", now: true }]);
  await expect(box).toHaveValue("");
  // ⇧↵ a new line, nothing sent; ↵ sends both lines, kept while the lead works (not now).
  await box.click();
  await page.keyboard.type("First line");
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("second line");
  await expect(box).toHaveValue("First line\nsecond line");
  expect(await sends(cue)).toHaveLength(1);
  await page.keyboard.press("Enter");
  await expect.poll(() => sends(cue)).toEqual([{ text: "Stop, the footer is fine", now: true }, { text: "First line\nsecond line", now: false }]);
  await expect(box).toHaveValue("");
  // Cue kept that one for the lead: ⌘↵ in the empty box sends it now, as the Send now button does.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").held = { role: "you", text: "First line\nsecond line", at_ms: Date.now(), images: [] }; });
  await box.click();
  await page.keyboard.press("Meta+Enter");
  await cue.called("send_held_now", { sessionId: "lead" });
  expect(await sends(cue)).toHaveLength(2);
});
