// The new Desk page (#628, desk/v2.js): "Try the new Desk page" in Desk settings turns it on and off, off by default
// and remembered. On it: no sessions column; the Desks as pills in its head with Desk settings, a drawer; Chats ▾ on
// the lead's chat; List | Progress in the Items head; the groups in their order; an item's page led by its steps, who
// is on it and its buttons; a session's chat with ‹ Lead chat and Its item ›.
import { test, expect, openDesk } from "./cue.mjs";
import { listOrder, openSessions } from "./desk.mjs";

/** Turn the new page on from today's Desk settings. */
async function turnOn(page) {
  // Desk settings… in the header's Desks menu (today's page has no settings line of its own, #666).
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator(".top .hv-menu [data-hv-desk-set]").click();
  const sw = page.locator(".ld-dialog [data-desk2-try]");
  await expect(sw).toHaveAttribute("aria-checked", "false");
  await sw.click();
  await expect(page.locator(".dk-board.dk-v2")).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(page.locator(".ld-dialog")).toHaveCount(0);
}

test("Today's Desk page is the default", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-board")).toHaveCount(1);
  await expect(page.locator(".dk-board.dk-v2")).toHaveCount(0);
  // Today's page is B2 (#666): the lead's chat at the left and the item list beside it, no sessions column; the
  // sessions open on the left, in the chat's place, from All N sessions under the Working pill.
  await expect(page.locator(".dk-board.dk-b2")).toHaveCount(1);
  const [c, l] = await Promise.all([page.locator(".dk-board > .dk-card.dk2b-chat").boundingBox(), page.locator(".dk-board > .dk-card.dk-items").boundingBox()]);
  expect(l.x).toBeGreaterThan(c.x + c.width);
  await expect(page.locator(".dk-sessions")).toHaveCount(0);
  // No Progress toggle in the B2 list head (#666).
  await expect(page.locator(".dk-items [data-desk-prog-toggle]")).toHaveCount(0);
  // The sessions open in the lead's chat's place on the left; the item list stays on the right.
  await openSessions(page);
  await expect(page.locator(".dk-board > .dk2b-chat")).toHaveCount(0);
  await expect(page.locator(".dk-board > .dk-items")).toHaveCount(1);
  const [s, l2] = await Promise.all([page.locator(".dk-board > .dk-sessions").boundingBox(), page.locator(".dk-board > .dk-items").boundingBox()]);
  expect(l2.x).toBeGreaterThan(s.x + s.width - 1);
});

test("The switch in Desk settings flips the page, and it's remembered", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await turnOn(page);
  await expect(page.locator(".dk-board.dk-b2")).toHaveCount(0);
  await page.reload();
  await openDesk(page);
  await expect(page.locator(".dk-board.dk-v2")).toHaveCount(1);
  // Off again from the new page's drawer: today's page, and still off after a reload.
  await page.locator("[data-desk2-set]").click();
  const sw = page.locator(".dk2-drawer [data-desk2-try]");
  await expect(sw).toHaveAttribute("aria-checked", "true");
  await sw.click();
  await expect(page.locator(".dk-board.dk-v2")).toHaveCount(0);
  await expect(page.locator(".dk-board.dk-b2")).toHaveCount(1);
  await page.reload();
  await openDesk(page);
  await expect(page.locator(".dk-board.dk-b2")).toHaveCount(1);
});

test("The new page: Desk pills, the drawer, Chats, List and Progress, the groups' order", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await turnOn(page);
  // A pill per Desk and + New Desk.
  const pills = page.locator(".dk2-head .dk2-pill[data-desk-go]");
  await expect(pills).toHaveCount(2);
  await expect(page.locator('.dk2-pill.on[data-desk-go="cue"]')).toHaveCount(1);
  await expect(page.locator(".dk2-head [data-desk-new-desk]")).toHaveText("+ New Desk");
  // The groups: Waiting on you, Ready to check, In progress, Landing, Waiting to install, Queue, … in that order.
  const heads = await page.locator(".dk-items [data-desk-group]").evaluateAll((els) => els.map((e) => e.dataset.deskGroup));
  const order = ["you", "check", "working", "landing", "install", "queued", "held", "done"].filter((k) => heads.includes(k));
  expect(heads.filter((k) => order.includes(k))).toEqual(order);
  // Desk settings: a drawer with its sections and the switch; Esc closes it.
  await page.locator("[data-desk2-set]").click();
  const dr = page.locator(".dk-v2 .dk2-drawer");
  await expect(dr.locator(".move-title")).toHaveText("Desk settings");
  for (const s of ["lands", "desk", "sessions", "old", "try"]) await expect(dr.locator(`[data-ld-sec="${s}"]`)).toHaveCount(1);
  await expect(dr.locator('[data-ld-sec="sessions"] .ld-h')).toContainText("Sessions on this Desk · 1 ·");
  await expect(dr.locator("[data-desk-change]")).toHaveText("Hand to another session…");
  await expect(dr.locator(".ld-foot")).toContainText("Changes wait until you press Save");
  await expect(page.locator(".ld-scrim")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(dr).toHaveCount(0);
  // Chats ▾: the lead, the Desk's session, New session, Add a running session.
  await page.locator("[data-desk2-chats]").click();
  const menu = page.locator(".dk2-chats .dk2-menu");
  await expect(menu.locator(".dk2-mn")).toHaveText(["web-lead", await menu.locator("[data-desk-chat-of] .dk2-mn").first().innerText(), "New session", "Add a running session to this Desk…"]);
  await menu.locator("[data-desk-chat-of]").first().click();
  await expect(menu).toHaveCount(0);
  await expect(page.locator('.dk-right [data-desk-page="session"]')).toHaveCount(1);
  await expect(page.locator(".dk-right .dk2-lead")).toHaveText("‹ Lead chat");
  await page.locator(".dk-right .dk2-lead").click();
  await expect(page.locator('.dk-right [data-desk-page="dispatcher"]')).toHaveCount(1);
  // Progress in place of the list, one at a time.
  await page.locator('[data-desk2-mode="progress"]').click();
  await expect(page.locator(".dk-items .dk2-prog")).toHaveCount(1);
  await expect(page.locator(".dk-items .dk-it")).toHaveCount(0);
  await expect(page.locator(".dk-prog")).toHaveCount(0);
  await page.locator('[data-desk2-mode="list"]').click();
  await expect(page.locator(".dk-items .dk2-prog")).toHaveCount(0);
  // Another Desk from its pill.
  await page.locator('.dk2-pill[data-desk-go="website"]').click();
  await expect(page.locator('.dk2-pill.on[data-desk-go="website"]')).toHaveCount(1);
});

test("The new page puts Landing and Waiting to install right under In progress, before the Queue", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await turnOn(page);
  const at = await page.evaluate(() => now());
  await cue.change((st, t) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "finishing", finish: { step: "line", at: t } }); }, at);
  expect(await listOrder(page)).toEqual(["# Waiting on you", "d1", "# Ready to check", "d2", "# In progress", "d3", "# Landing", "d5", "# Queue", "d4", "d6", "# Done", "d7", "d8", "d9"]);
});

test("An item's page leads with its steps, who is on it and its buttons; its chat goes back to the lead", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const now = Date.now();
  await cue.change((st, now) => {
    const d3 = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5");
    d3.steps = [now - 600000, now - 480000, now - 60000, 0, 0, 0];
    d3.act = { now: "Running tests", was: "", at_ms: now - 30000 };
  }, now);
  await turnOn(page);
  await page.locator('.dk-items .dk-it[data-desk-item="d5"]').click();
  const top = page.locator(".dk-right .dk2-top");
  await expect(top.locator(".dk2-sn")).toHaveText(["Picked up", "Building", "Tests", "Commit", "Landing", "To check"]);
  await expect(top.locator(".dk2-step.done")).toHaveCount(2);
  await expect(top.locator(".dk2-step.cur .dk2-sn")).toHaveText("Tests");
  await expect(top.locator(".dk2-who")).toContainText("Who is on it");
  await expect(top.locator(".dk2-who")).toContainText("Tests · Running tests");
  await expect(page.locator(".dk-right .dk-itabs")).toBeHidden();
  // Its header has no old "‹ lead" button beside ‹ Lead chat.
  await expect(page.locator('.dk-right [data-desk-page="item"] .dk-ph1 [data-desk-back]')).toHaveCount(0);
  const acts = top.locator(".dk2-acts > .btn");
  await expect(acts.first()).toHaveText("‹ Lead chat");
  await expect(acts.nth(1)).toHaveText("Open its chat");
  // ⋯: the tabs.
  await top.locator("[data-desk2-more]").click();
  await expect(top.locator(".dk2-menu .dk2-mn")).toHaveText(["Record", "Decisions", "Files"]);
  await top.locator('.dk2-menu [data-desk-itab="record"]').click();
  await expect(top.locator(".dk2-menu")).toHaveCount(0);
  await expect(page.locator(".dk-right .dk-ibody.dk-itab-record")).toHaveCount(1);
  // Its chat: ‹ Lead chat, and Its item › back to it.
  await acts.nth(1).click();
  await expect(page.locator(".dk-right .dk2-lead")).toHaveText("‹ Lead chat");
  await page.locator(".dk-right .dk2-its").click();
  await expect(page.locator(".dk-right .dk2-top")).toHaveCount(1);
  await page.locator(".dk-right .dk2-top .dk2-lead").click();
  await expect(page.locator('.dk-right [data-desk-page="dispatcher"]')).toHaveCount(1);
});
