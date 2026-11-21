// Talking to a Desk's lead by voice (#617): with the lead's chat open on the Desk, the window tells talking (voice/talk.rs)
// that chat is open, so what you say goes to the lead, not the session that moved last; sent, it shows in the lead's chat
// as yours with the mic, as a voice message to a session does, its Desk block left out. In light and dark, no hover text.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";

/** The chat the window last told talking is open (its "chat-open" event): a session's id, or null. */
const toldOpen = (cue) => cue.calls("event:chat-open").then((c) => c.at(-1)?.args?.sid ?? null);

for (const scheme of ["light", "dark"]) {
  test(`a voice message to the Desk's lead shows in its chat as yours, with the mic (${scheme}) (#617)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1500, height: 950 });
    await cue.open("desk");
    await openDesk(page);
    // The lead's chat is open: talking goes to the lead.
    await expect.poll(() => toldOpen(cue)).toBe("lead");
    // What you said, as the hub has it once it's sent (desk/hub/lead/send.rs: after the Desk's block; talk.rs marks it
    // said by voice).
    const said = "Add an item: the notch should show which Desk is talking";
    await cue.change((st, { said, at }) => {
      const lead = st.sessions.find((x) => x.session_id === "lead");
      lead.thread.push({ role: "you", text: `<desk id="cue" name="cue" project="cue">\nItems: none.\n</desk>\n\n${said}`, at_ms: at, voice: true });
    }, { said, at: NOW - 5000 });
    const row = page.locator(".dk-chat .cv-you-row").last();
    await expect(row.locator(".cv-mic")).toHaveCount(1);
    await expect(row.locator(".cv-you-text").first()).toContainText(said);
    await expect(page.locator(".dk-chat")).not.toContainText("Items: none.");
    await expect(page.locator("[title]")).toHaveCount(0);
    if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/desk-lead-voice-${scheme}.png` });
  });
}

