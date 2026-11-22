// A message kept for a busy session waits in line with Edit, Send now and Cancel only (#731): no "Ask it now, by the
// way" or "Keep it queued" under it, and no line explaining them, however long the session has been at its turn.
import { test, expect } from "./cue.mjs";

/** Session `sid` working for `min` minutes, with `text` kept for it. */
const keptFor = (cue, sid, min, text) => cue.change((st, [sid, min, text]) => {
  const s = st.sessions.find((x) => x.session_id === sid), t = now() - min * 60_000;
  Object.assign(s, { state: "working", harness: "claude", since_ms: t, segments: [{ kind: "waiting", start_ms: t - 600_000, end_ms: t }, { kind: "working", start_ms: t, end_ms: null }] });
  s.held = { role: "you", text, at_ms: now() - 12 * 60_000, images: [] };
}, [sid, min, text]);

for (const min of [5, 64]) {
  test(`busy ${min} minutes: the kept message has Edit, Send now and Cancel, and no by-the-way offer`, async ({ cue, page }) => {
    await cue.open("board-waiting");
    await keptFor(cue, "eng", min, "Why did it skip the CSV export?");
    await page.locator('.working[data-session="eng"]').first().click();
    const pane = page.locator(".active-pane"), mine = pane.locator(".cv-you", { hasText: "Why did it skip the CSV export?" });
    await expect(mine.locator(".msg-line")).toContainText("Not sent yet");
    await expect(mine.locator(".msg-line")).toContainText("Edit · Send now · Cancel");
    await expect(pane).not.toContainText("by the way");
    await expect(pane).not.toContainText("Keep it queued");
    await expect(pane).not.toContainText("may wait a while");
    await expect(pane).not.toContainText("A side question gets an answer");
  });
}
