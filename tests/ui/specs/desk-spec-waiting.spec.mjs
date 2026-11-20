// Desk spec (#498): "Waiting on you", what waits on you on the Desk. Sources: #182 (an item waits on you only with a
// clear ask: Answer / Pick / Try it), #475 (a session's question puts its item in Waiting on you until you answer it),
// the Dispatcher panel's "Waits on you" (#200, #392: what waits on you, each opening its item), held steps (a tool
// approval on an item's session waits on you as the item's ask, #299/#431), and answered asks leave the list.
import { test, expect, openDesk } from "./cue.mjs";

// Wide enough that the Dispatcher's panel stands beside its chat (narrower, it folds into the Now line).
test.use({ viewport: { width: 1728, height: 1000 } });
/** The ids in the list's Waiting on you group, sorted (its order is #182's, checked elsewhere). */
const waitingIds = (page) => page.locator('[data-desk-gbody="you"] [data-desk-item]').evaluateAll((els) => els.map((e) => e.dataset.deskItem).sort());
const panelWaits = (page) => page.locator(".dk-right .dk-panel .dk-psec:not(.dk-pnow)", { hasText: "Waits on you" });

// The panel lists what the list's group has, each row opening its item's page.
test("the lead's panel lists what waits on you, the same as the list's group, and a row opens its item", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect.poll(() => waitingIds(page)).toEqual(["d1"]);
  const rows = panelWaits(page).locator("[data-desk-chk-item]");
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toHaveAttribute("data-desk-chk-item", "d1");
  await expect(rows.first()).toContainText("Asks you something");
  await rows.first().click();
  await expect(page.locator('.dk-right .dk-pg[data-desk-page="item"]')).toBeVisible();
  expect(await page.evaluate(() => deskOpen && `${deskOpen.kind}:${deskOpen.id}`)).toBe("item:d1");
});

// #475: an answered ask leaves Waiting on you: the list's group, its count and the panel.
test("answering an item's question on its page takes it off Waiting on you: the group, its count and the panel", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const you = page.locator(".dk-items .dk-gh.you");
  await expect(you).toHaveText("Waiting on you 1");
  await page.locator('[data-desk-gbody="you"] [data-desk-item="d1"]').click();
  await page.locator(".dk-right .dk-ask .qc-opt", { hasText: "Keep it in the box" }).click();
  await page.locator('.dk-right .dk-ask [data-act="submit-answers"]').click();
  await cue.called("respond", { id: "n-focus-ask" });
  await expect.poll(() => waitingIds(page)).toEqual([]);
  await page.evaluate(() => { deskOpen = null; deskTab = "lead"; renderMain(); });
  await expect(panelWaits(page).locator("[data-desk-chk-item]")).toHaveCount(0);
});

// A held step: a tool approval on the session holding an item waits on you as that item's ask, in the group and the
// panel ("Asks to use a tool"); allowed, it leaves both.
test("a tool approval on an item's session waits on you as that item, and leaves once allowed", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    st.sessions.find((s) => s.session_id === "host").state = "deciding";
    st.items.push({ id: "perm-d5", session_id: "host", harness: "claude", hosted: true, project: "cue", kind: "permission", status: "pending", tool_name: "Bash", tool_input: { command: "npm test" }, suggestions: [], created_ms: Date.now(), thread: [{ role: "you", text: "Desk item #d5 (bug): Hosted sessions can't open the canvas" }] });
  });
  await openDesk(page);
  await expect.poll(() => waitingIds(page)).toEqual(["d1", "d5"]);
  await expect(panelWaits(page).locator('[data-desk-chk-item="d5"]')).toContainText("Asks to use a tool");
  await page.locator('.dk-items [data-desk-item="d5"] [data-desk-decision="allow"]').click();
  await cue.called("respond", { id: "perm-d5" });
  await expect.poll(() => waitingIds(page)).toEqual(["d1"]);
  await expect(panelWaits(page).locator('[data-desk-chk-item="d5"]')).toHaveCount(0);
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
  await expect.poll(() => waitingIds(page)).toEqual(["d1"]);
  await expect(panelWaits(page).locator('[data-desk-chk-item="d5"]')).toHaveCount(0);
});
