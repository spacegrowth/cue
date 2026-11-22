// Desk spec (#498): "Waiting on you", what waits on you on the Desk. Sources: #182 (an item waits on you only with a
// clear ask: Answer / Pick / Try it), #475 (a session's question puts its item in Waiting on you until you answer it),
// the Dispatcher panel's "Waits on you" (#200, #392: what waits on you, each opening its item), held steps (a tool
// approval on an item's session waits on you as the item's ask, #299/#431), and answered asks leave the list.
import { test, expect, openDesk, deskShowPanel } from "./cue.mjs";
import { listOrder } from "./desk.mjs";

// Wide enough that the Dispatcher's panel stands beside its chat (narrower, it folds into the Now line).
test.use({ viewport: { width: 1728, height: 1000 } });
/** The ids in the list's WAITS ON YOU group (B2, #666), sorted (its order is #182's, checked elsewhere). */
const waitingIds = async (page) => { const o = await listOrder(page), at = o.indexOf("# WAITS ON YOU"); if (at < 0) return []; const end = o.findIndex((x, n) => n > at && x.startsWith("# ")); return o.slice(at + 1, end < 0 ? undefined : end).sort(); };
const panelWaits = (page) => page.locator(".dk-right .dk-panel .dk-psec:not(.dk-pnow)", { hasText: "Waits on you" });

// The panel lists what the list's group has, each row opening its item's page. B2 (#666): WAITS ON YOU holds the
// questions and what's ready to check (d2), as the strip's count does.
test("the lead's panel lists what waits on you, the same as the list's group, and a row opens its item", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskShowPanel(page);   // the lead's panel, behind Show panel
  await expect.poll(() => waitingIds(page)).toEqual(["d1", "d2"]);
  const rows = panelWaits(page).locator("[data-desk-chk-item]");
  await expect.poll(() => rows.evaluateAll((els) => els.map((e) => e.dataset.deskChkItem).sort())).toEqual(["d1", "d2"]);
  await expect(panelWaits(page).locator(".dk-psh")).toHaveText("Waits on you · 2", { ignoreCase: true });
  await expect(rows.first()).toHaveAttribute("data-desk-chk-item", "d1");
  await expect(rows.first()).toContainText("Asks you something");
  await expect(panelWaits(page).locator('[data-desk-chk-item="d2"]')).toContainText("Ready to check");
  await rows.first().click();
  await expect(page.locator('.dk-right .dk-pg[data-desk-page="item"]')).toBeVisible();
  expect(await page.evaluate(() => deskOpen && `${deskOpen.kind}:${deskOpen.id}`)).toBe("item:d1");
});

// #475: an answered ask leaves Waiting on you: the list's group, its count and the panel.
test("answering an item's question on its page takes it off Waiting on you: the group, its count and the panel", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const you = page.locator('.dk-items [data-desk2b-group="you"]');
  await expect(you).toHaveText("WAITS ON YOU 2");
  await page.locator('.dk-items [data-desk-item="d1"]').click();
  await page.locator(".dk-right .dk-ask .qc-opt", { hasText: "Keep it in the box" }).click();
  await page.locator('.dk-right .dk-ask [data-act="submit-answers"]').click();
  await cue.called("respond", { id: "n-focus-ask" });
  await expect.poll(() => waitingIds(page)).toEqual(["d2"]);
  await expect(you).toHaveText("WAITS ON YOU 1");
  await page.evaluate(() => { deskOpen = null; deskTab = "lead"; renderMain(); });
  await deskShowPanel(page);
  await expect(panelWaits(page).locator('[data-desk-chk-item="d1"]')).toHaveCount(0);
  await expect(panelWaits(page).locator('[data-desk-chk-item="d2"]')).toHaveCount(1);
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
  await deskShowPanel(page);
  await expect.poll(() => waitingIds(page)).toEqual(["d1", "d2", "d5"]);
  await expect(panelWaits(page).locator('[data-desk-chk-item="d5"]')).toContainText("Asks to use a tool");
  // B2: rows carry no answer buttons (asserted gone); Allow is on the item's page.
  await expect(page.locator('.dk-items [data-desk-item="d5"] [data-desk-decision]')).toHaveCount(0);
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  await page.locator('.dk-right [data-act="allow"][data-id="perm-d5"]').click();
  await cue.called("respond", { id: "perm-d5" });
  await expect.poll(() => waitingIds(page)).toEqual(["d1", "d2"]);
  await page.evaluate(() => { deskOpen = null; deskTab = "lead"; renderMain(); });
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
  await deskShowPanel(page);
  await expect.poll(() => waitingIds(page)).toEqual(["d1", "d2"]);
  await expect(panelWaits(page).locator('[data-desk-chk-item="d5"]')).toHaveCount(0);
});
