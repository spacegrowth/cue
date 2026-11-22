// The Desk opens with asks in play (#279): a session asking you something reads "waiting on you"; one whose turn
// just finished doesn't. 89466073 put the Desk's use of actionableRequest (ui/core/state.js) on main without it, and
// the Desk threw on every draw.
import { test, expect, openDesk } from "./cue.mjs";

test("the Desk opens with a pending ask and a finished turn: only the ask reads waiting on you (#279)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const site = st.sessions.find((x) => x.session_id === "site");
    st.sessions.push({ ...site, session_id: "asker", project: "cue", name: "asker", state: "deciding", thread: [] },
      { ...site, session_id: "done1", project: "cue", name: "done1", state: "idle", thread: [] });
    const ask = st.items.find((i) => i.id === "n-focus-ask");
    st.items.push({ ...ask, id: "n-asker", session_id: "asker" }, { ...ask, id: "n-done1", session_id: "done1", kind: "waiting" });
    st.desk.projects.find((d) => d.project === "cue").members.push("asker", "done1");
  });
  // B2 (#666): the sessions list opens on the right from the strip's Working ▾, then All N sessions ›.
  await page.locator("[data-desk2b-work]").click();
  await page.locator("[data-desk2b-sessions]").click();
  const left = page.locator(".dk-sessions");
  await expect(left.locator('[data-desk-chat-of="asker"] .dk-st')).toHaveText("needs you");
  await expect(left.locator('[data-desk-chat-of="done1"] .dk-st')).not.toHaveText("needs you");
});
