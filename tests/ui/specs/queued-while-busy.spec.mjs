// #531: a message typed to a session mid-turn sat under a dashed bubble saying "Sending…" for as long as the turn ran,
// as if stuck. Its agent takes it in when it's free: it says Queued. Sending is for one still on its way.
import { test, expect } from "./cue.mjs";

const openCard = (page, id) => page.locator(`.nrow[data-big="${id}"]`).click();
/** Session `sid`: `state`, and your last message in it typed but not seen taken yet. */
const busyWithYours = (cue, sid, state) => cue.change((st, [sid, state]) => {
  const s = st.sessions.find((x) => x.session_id === sid);
  s.state = state;
  Object.assign(s.thread.filter((e) => e.role === "you").pop(), { pending: true, at_ms: Date.now() });
}, [sid, state]);

test("a message to a session mid-turn says Queued, not Sending (#531)", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await openCard(page, "n1");
  await busyWithYours(cue, "relay", "working");
  const mine = page.locator(".active-pane .cv-you.pending").last();
  await expect(mine).toHaveClass(/msg-queued/);
  await expect(mine.locator(".msg-line")).toHaveText("◌ Queued · goes in when it's free");
  await expect(mine.locator(".msg-line button")).toHaveCount(0);
  // Its turn over and Cue still hasn't seen it taken: that's Sending again.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "relay").state = "waiting"; });
  await expect(mine).toHaveClass(/msg-sending/);
  await expect(mine.locator(".msg-line")).toContainText("◌ Sending…");
});
