// What the lead does on the Desk (#360): anything that needs you, its own asks too, waits in the Dispatcher's
// panel until it's answered; what it did on its own (closed, started, moved up, took back a copy, answered a
// session) is a line in its chat as it happens, and its last few show in the panel for a while, then drop off.
import { test, expect } from "./cue.mjs";
import { NOW } from "./clock.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
const s = (ms) => Math.floor(ms / 1000);
const act = (id, item, act, text, why, at) => ({ id, project: "cue", item_id: item, by: "web-lead", sid: "lead", text, why, at: s(at), undo: "none", act });

test("the lead's own moves show in its chat and, for a while, in the panel; its ask waits there till answered (#360)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1728, height: 1000 });
  await cue.open("desk");
  await openDesk(page);
  const acts = [
    act("a1", "d4", "start", "Started #d4 now, without waiting for a copy", "the user needs it now", NOW - 3 * 60 * 1000),
    act("a2", "d3", "land_first", "Moved #d3 up in the landing line", "the user waits on it", NOW - 60 * 1000),
    act("a3", "d8", "close", "Closed #d8: its work is on main (ab12cd3)", "it landed by hand", NOW - 2 * 60 * 60 * 1000),
  ];
  await cue.change((st, { acts, at }) => {
    st.desk.projects.find((p) => p.project === "cue").decisions.push(...acts);
    const ask = st.items.find((i) => i.id === "n-focus-ask");
    st.items.push({ ...ask, id: "n-lead-ask", agent_id: "", agent_name: "", created_ms: at,
      tool_input: { questions: [{ question: "Push the release now?", header: "Push", multiSelect: false, options: [{ label: "Yes" }, { label: "Not yet" }] }] } });
  }, { acts, at: NOW - 60 * 1000 });
  // In its chat, as it happened: each one, the item's number opening it, and why; the old one too.
  const chat = page.locator(".dk-chat");
  await expect(chat.locator(".dk-act")).toHaveCount(3);
  await expect(chat.locator('.dk-act[data-key="act:a2"]')).toContainText("web-lead · Moved #d3 up in the landing line · the user waits on it");
  await expect(chat.locator('.dk-act[data-key="act:a3"]')).toContainText("Closed #d8: its work is on main (ab12cd3)");
  // In the panel: only the last ones, newest first; the close two hours ago has dropped off.
  const panel = page.locator(".dk-right .dk-panel");
  const did = panel.locator(".dk-psec", { hasText: "Just did" }).locator(".dk-pdid");
  await expect(did).toHaveCount(2);
  await expect(did.first()).toContainText("Moved #d3 up in the landing line · the user waits on it");
  await expect(did.nth(1)).toContainText("Started #d4 now");
  // The lead's own ask waits on you there, first, until it's answered.
  const waits = panel.locator(".dk-psec:not(.dk-pnow)", { hasText: "Waits on you" });
  await expect(waits.locator(".dk-psh")).toHaveText("Waits on you · 2");
  await expect(waits.locator(".dk-prow").first()).toContainText("web-lead");
  await expect(panel.locator(".dk-pnow")).toContainText("It asks you something");
  // Its item number opens the item.
  await did.first().locator('[data-desk-chk-item="d3"]').click();
  await expect(page.locator('.dk-right .dk-pg[data-desk-page="item"]')).toBeVisible();
  // Answered: it no longer waits on you.
  await cue.change((st) => { st.items = st.items.filter((i) => i.id !== "n-lead-ask"); });
  await page.locator(".dk-right .dk-back").first().click();
  await expect(page.locator(".dk-right .dk-panel .dk-psec:not(.dk-pnow)", { hasText: "Waits on you" }).locator(".dk-psh")).toHaveText("Waits on you · 1");
  // On a small window the panel folds into the Now line: what waits and the newest move are in it.
  await page.setViewportSize({ width: 1100, height: 800 });
  await expect(page.locator(".dk-right .dk-pnow .dk-pfc")).toContainText("1 waiting");
  await expect(page.locator(".dk-right .dk-pnow .dk-pfc")).toContainText("Just did: Moved #d3 up in the landing line");
});
