// Desk spec (#498): "Waiting on you", what waits on you on the Desk. Sources: #182 (an item waits on you only with a
// clear ask: Answer / Pick / Try it), #475 (a session's question puts its item in Waiting on you until you answer it),
// the Dispatcher panel's "Waits on you" (#200, #392: what waits on you, each opening its item), held steps (a tool
// approval on an item's session waits on you as the item's ask, #299/#431), and answered asks leave the list.
import { test, expect, openDesk } from "./cue.mjs";
import { listOrder, statusOf } from "./desk.mjs";

// Wide enough that the Dispatcher's panel stands beside its chat (narrower, it folds into the Now line).
test.use({ viewport: { width: 1728, height: 1000 } });
/** The ids in the list's WAITING ON YOU group (#748), sorted (its order is #182's, checked elsewhere). */
const waitingIds = async (page) => { const o = await listOrder(page), at = o.indexOf("# WAITING ON YOU"); if (at < 0) return []; const end = o.findIndex((x, n) => n > at && x.startsWith("# ")); return o.slice(at + 1, end < 0 ? undefined : end).sort(); };

// #475: an answered ask leaves Waiting on you: the list's group, its count and the panel.
test("answering an item's question on its page takes it off Waiting on you: the group and its count", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const you = page.locator('.dkn-card .dkn-gh[data-desk2b-group="you"] > span:not(.dkn-fa)');
  await expect(you).toHaveText("2");
  await page.locator('.dkn-card [data-desk-item="d1"] .dkn-tt').click();
  await page.locator(".dk-right .dk-ask .qc-opt", { hasText: "Keep it in the box" }).click();
  await page.locator('.dk-right .dk-ask [data-act="submit-answers"]').click();
  await cue.called("respond", { id: "n-focus-ask" });
  await expect.poll(() => waitingIds(page)).toEqual(["d2"]);
  await expect(you).toHaveText("1");
});

// A held step: a tool approval on the session holding an item waits on you as that item's ask, in the group ("Asks to use a
// tool"); allowed, it leaves both.
test("a tool approval on an item's session waits on you as that item, and leaves once allowed", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    st.sessions.find((s) => s.session_id === "host").state = "deciding";
    st.items.push({ id: "perm-d5", session_id: "host", harness: "claude", hosted: true, project: "cue", kind: "permission", status: "pending", tool_name: "Bash", tool_input: { command: "npm test" }, suggestions: [], created_ms: Date.now(), thread: [{ role: "you", text: "Desk item #d5 (bug): Hosted sessions can't open the canvas" }] });
  });
  await openDesk(page);
  await expect.poll(() => waitingIds(page)).toEqual(["d1", "d2", "d5"]);
  await expect(statusOf(page, "d5")).toHaveText("Asks you");
  // B2: rows carry no answer buttons (asserted gone); Allow is on the item's page.
  await expect(page.locator('.dkn-card [data-desk-item="d5"] [data-desk-decision]')).toHaveCount(0);
  await page.locator('.dkn-card [data-desk-item="d5"] .dkn-tt').click();
  await page.locator('.dk-right [data-act="allow"][data-id="perm-d5"]').click();
  await cue.called("respond", { id: "perm-d5" });
  await expect.poll(() => waitingIds(page)).toEqual(["d1", "d2"]);
  await expect(statusOf(page, "d5")).not.toHaveText("Asks you");
});

// #182: a session that stopped with no clear ask doesn't wait on you: an item at work whose holder is idle, with no
// question, stays off the group.
test("an item whose session stopped without asking anything isn't waiting on you", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    st.sessions.find((s) => s.session_id === "host").state = "waiting";
    Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5"), { stopped: true, latest: "Checking the box next." });
  });
  await openDesk(page);
  await expect.poll(() => waitingIds(page)).toEqual(["d1", "d2"]);
});
