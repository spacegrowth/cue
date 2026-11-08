// A finished card whose session has no recent thread in Cue (it lost track of it a while, then the session came
// back with a turn that said nothing) shows what was said last, from Cue's log, without a click.
import { test, expect } from "./cue.mjs";

test("a finished card with no recent thread shows its last messages from Cue's log", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  const sid = await page.evaluate(() => state.items.find((i) => i.kind === "waiting").session_id);
  await cue.change((st, sid) => {
    const it = st.items.find((i) => i.session_id === sid && i.kind === "waiting");
    Object.assign(it, { message: "", context: [], thread: [] });
    const s = st.sessions.find((x) => x.session_id === sid);
    if (s) s.thread = [];
  }, sid);
  await page.evaluate((sid) => setActive(null, sid), sid);
  await cue.called("session_log_page", { sessionId: sid });
  await expect(page.locator("[data-chat]").getByText(/^Earlier message \d+$/).last()).toBeVisible();
});
