// The Desk page as option B2 (#666): the lead's chat card and the item list side by side, the chat as wide as the
// Board's (878px at 1800×1095); Now and Waits on you in one strip over the chat (the Working pill unfolds the sessions
// at work, a chip per item that waits, Check); the list's tabs by group and its long groups showing their first 3; an
// item opening on the right in the list's place with ‹ Items N back, the chat staying the lead's; Show panel; dark.
import { test, expect, openDesk } from "./cue.mjs";
import { listOrder } from "./desk.mjs";

const chat = (page) => page.locator(".dk-card.dk2b-chat");
const strip = (page) => chat(page).locator(".dk2b-strip");

test("the lead's chat card is as wide as the Board's at 1800×1095, the list beside it", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1800, height: 1095 });
  await cue.open("desk");
  await openDesk(page);
  const [c, l] = await Promise.all([chat(page).boundingBox(), page.locator(".dk-card.dk-items").boundingBox()]);
  expect(Math.round(c.width)).toBe(878);
  expect(l.x).toBeGreaterThan(c.x + c.width);
  // No Desk-switching row or pills on the page itself.
  await expect(page.locator(".dk-board .dk-strip, .dk-board .dk2-desks")).toHaveCount(0);
});

test("the Working pill unfolds the sessions at work and folds them again", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const pill = strip(page).locator("[data-desk2b-work]");
  // One session at work in this Desk (host-lead on #d5).
  await expect(pill).toHaveText(/Working 1/);
  await expect(pill).toHaveAttribute("aria-expanded", "false");
  await expect(strip(page).locator(".dk2b-wlist")).toHaveCount(0);
  await pill.click();
  await expect(pill).toHaveAttribute("aria-expanded", "true");
  const rows = strip(page).locator(".dk2b-wrow");
  await expect(rows).toHaveCount(1);
  await expect(rows.filter({ hasText: "host-lead" })).toContainText("#d5");
  await expect(rows.filter({ hasText: "host-lead" }).locator(".dk2b-trk i")).toHaveCount(6);
  await expect(strip(page).locator(".dk2b-alls")).toHaveText(/^All \d+ sessions? ›$/);
  await strip(page).locator("[data-desk2b-work]").click();
  await expect(strip(page).locator(".dk2b-wlist")).toHaveCount(0);
  await expect(strip(page).locator("[data-desk2b-work]")).toHaveAttribute("aria-expanded", "false");
});

test("the strip says how many wait on you, a chip each, and Check opens the first on the right", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(strip(page).locator(".dk2b-wn")).toHaveText(/^\d+ waits? on you$/);
  const chips = strip(page).locator(".dk2b-chip[data-desk-chk-item]");
  await expect(chips.first()).toHaveText(/^#d\d+$/);
  const check = strip(page).locator(".dk2b-check");
  const id = await check.getAttribute("data-desk-chk-item");
  await expect(check).toHaveText(`Check #${id}`);
  await check.click();
  const side = page.locator(".dk-card.dk-right.dk2b-side");
  await expect(side.locator(`[data-desk-page="item"]`)).toHaveCount(1);
  await expect(side.locator(".dk-ph")).toContainText(`#${id}`);
  await expect(page.locator(".dk-card.dk-items")).toHaveCount(0);
  // A chip opens its own item.
  await chips.last().click();
  const last = await chips.last().getAttribute("data-desk-chk-item");
  await expect(side.locator(".dk-ph")).toContainText(`#${last}`);
});

test("rows name no session; Give to… and the queue's words stay on their line", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const p = st.desk.projects.find((x) => x.project === "cue"), d6 = p.items.find((i) => i.id === "d6");
    // One queued for a session by name, one waiting for install with its holder, one held by a crew member.
    p.items.push({ ...d6, id: "q1", title: "Queued for host", state: "queued", holder: null, next_for: { kind: "session", sid: "host", name: "host-lead" } });
    p.items.push({ ...d6, id: "c1", title: "Crew one", state: "working", holder: { kind: "crew", lead_sid: "host", name: "mate-one" } });
  });
  await openDesk(page);
  // Every session's name, its holder's and a crew member's: none in any row of the list.
  const names = await page.evaluate(() => [...new Set([...state.sessions.map((s) => s.name), ...deskOf("cue").items.flatMap((i) => i.holder ? [i.holder.name] : [])])].filter(Boolean));
  expect(names).toContain("host-lead");
  expect(names).toContain("mate-one");
  // Every group in full, so no row is left out.
  for (const b of await page.locator("[data-desk2b-all]").all()) await b.click();
  const subs = await page.locator(".dk-items .dk-it .dk-who").allTextContents();
  expect(subs.length).toBeGreaterThan(3);
  for (const t of subs) for (const n of names.filter((n) => n !== "host-lead")) expect(t, `a row's line names ${n}`).not.toContain(n);
  // The holder's name is gone from a working row and a held one; the queue's words, which name who it's next for, stay.
  for (const id of ["d5", "c1"]) await expect(page.locator(`.dk-items [data-desk-item="${id}"] .dk-who`)).not.toContainText(/host-lead|mate-one/);
  await expect(page.locator('.dk-items [data-desk-item="q1"] .dk-who')).toHaveText("next for host-lead");
  // A row nobody holds keeps Give to….
  await expect(page.locator(".dk-items .dk-it [data-desk-give]").first()).toBeVisible();
});

test("the list's tabs show only their groups; a long group shows 3 and Show all N", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    for (const n of [1, 2, 3, 4]) d.items.push({ id: `q${n}`, title: `Queued thing ${n}`, state: "queued", kind: "feature", created_ms: st.now_ms - n * 60000 });
  });
  await openDesk(page);
  const tabs = page.locator(".dk-items .dk2b-tabs .dk2b-tab");
  await expect(tabs.first()).toHaveText(/^All \d+$/);
  await expect(tabs.first()).toHaveClass(/\bon\b/);
  const list = page.locator(".dk-items .dk-scroll");
  // The queue (d4, d6 and the four added) shows its first 3 and Show all 6.
  const all = list.locator('[data-desk2b-all="queued"]');
  await expect(all).toHaveText("Show all 6");
  const rowsIn = () => list.locator(".dk-it:visible").evaluateAll((els) => els.map((e) => e.dataset.deskItem).filter((x) => /^(d4|d6|q\d)$/.test(x)));
  expect((await rowsIn()).length).toBe(3);
  await all.click();
  await expect(list.locator('[data-desk2b-all="queued"]')).toHaveCount(0);
  expect((await rowsIn()).length).toBe(6);
  // Up next: only the queue, whole; Waits on you: no queue.
  await tabs.filter({ hasText: "Up next" }).click();
  await expect(tabs.filter({ hasText: "Up next" })).toHaveClass(/\bon\b/);
  await expect(list.locator('.dk-it[data-desk-item="q4"]')).toBeVisible();
  await expect(list.locator('.dk-it[data-desk-item="d3"]')).toHaveCount(0);
  await tabs.filter({ hasText: "Waits on you" }).click();
  await expect(list.locator('.dk-it[data-desk-item="q1"]')).toHaveCount(0);
  await expect(list.locator('.dk-it[data-desk-item="d1"]')).toBeVisible();
  await tabs.filter({ hasText: /^All/ }).click();
  await expect(list.locator('.dk-it[data-desk-item="d3"]')).toBeVisible();
});

test("a working session opens its item on the right with ‹ Items N back; the chat stays the lead's", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const lead = await chat(page).locator(".dk-ph").textContent();
  await strip(page).locator("[data-desk2b-work]").click();
  await strip(page).locator(".dk2b-wrow", { hasText: "host-lead" }).click();
  const side = page.locator(".dk-card.dk-right.dk2b-side");
  await expect(side.locator(".dk-ph")).toContainText("#d5");
  await expect(strip(page).locator(".dk2b-wrow.on")).toContainText("host-lead");
  // Its progress, who has it, and its tabs still there.
  await expect(side.locator(".dk2b-step")).toHaveCount(6);
  await expect(side.locator(".dk2b-ik")).toHaveText(["Who", "Doing now", "Tests", "Next"]);
  await expect(side.locator(".dk2b-iacts")).toContainText("Changes");
  await expect(chat(page).locator(".dk-ph")).toHaveText(lead);
  await expect(chat(page)).not.toHaveClass(/dk-right/);
  const back = side.locator(".dk2b-items");
  await expect(back).toHaveText(/^‹ Items \d+$/);
  await back.click();
  await expect(page.locator(".dk-card.dk-items")).toHaveCount(1);
  await expect(page.locator(".dk2b-side")).toHaveCount(0);
  await expect(chat(page)).toHaveClass(/dk-right/);
});

test("opening the lead's chat (⌘K) leaves what's on the right: the lead's chat is at the left, never a second copy", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  const side = page.locator(".dk-card.dk-right.dk2b-side");
  await expect(side.locator(".dk-ph")).toContainText("#d2");
  await page.keyboard.press("Meta+k");
  await page.locator('[data-text="find-live"]').pressSequentially("web-lead");
  await page.locator('.lv-pop .lv-row[data-sid="lead"]').click();
  await expect(page.locator(".lv-pop")).toHaveCount(0);
  await expect(side.locator(".dk-ph")).toContainText("#d2");
  await expect(page.locator(".dk-chat[data-desk-chat]")).toHaveCount(1);
  await expect(chat(page).locator(".dk-chat[data-desk-chat]")).toHaveCount(1);
  // With nothing open, the list stays.
  await side.locator(".dk2b-items").click();
  await expect(page.locator(".dk-card.dk-items")).toHaveCount(1);
  await page.keyboard.press("Meta+k");
  await page.locator('[data-text="find-live"]').fill("web-lead");
  await page.locator('.lv-pop .lv-row[data-sid="lead"]').click();
  await expect(page.locator(".lv-pop")).toHaveCount(0);
  await expect(page.locator(".dk-card.dk-items")).toHaveCount(1);
  await expect(page.locator(".dk2b-side")).toHaveCount(0);
  await expect(page.locator(".dk-chat[data-desk-chat]")).toHaveCount(1);
});

test("Show panel brings the full panel back beside the lead's chat; Hide panel puts the strip back", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1800, height: 1095 });
  await cue.open("desk");
  await openDesk(page);
  const tog = chat(page).locator('[data-desk-panel="dispatcher-shown"]');
  await expect(tog).toHaveText("Show panel");
  await expect(chat(page).locator(".dk-pnow")).toHaveCount(0);
  await tog.click();
  await expect(chat(page).locator('[data-desk-panel="dispatcher-shown"]')).toHaveText("Hide panel");
  await expect(chat(page).locator(".dk2b-strip")).toHaveCount(0);
  await expect(chat(page).locator(".dk-panel .dk-pnow")).toBeVisible();
  await chat(page).locator('[data-desk-panel="dispatcher-shown"]').click();
  await expect(chat(page).locator(".dk2b-strip")).toBeVisible();
});

test("dark theme draws the page in its own colors", async ({ page, cue }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await cue.open("desk");
  await openDesk(page);
  await strip(page).locator("[data-desk2b-work]").click();
  // The pill's redraw done first; each colour then read off the element on the page at that moment (a handle taken
  // before a redraw is a detached element, whose computed colours are empty).
  await expect(strip(page).locator(".dk2b-wlist")).toBeVisible();
  const css = (sel, prop) => page.evaluate(([sel, prop]) => getComputedStyle(document.querySelector(sel))[prop], [sel, prop]);
  const lum = (c) => { const [r, g, b] = c.match(/[\d.]+/g).map(Number); return (r + g + b) / 3; };
  expect(lum(await css(".dk-card.dk2b-chat", "backgroundColor"))).toBeLessThan(90);
  expect(lum(await css(".dk-card.dk-items", "backgroundColor"))).toBeLessThan(90);
  expect(lum(await css(".dk-items .dk2b-tab:not(.on)", "color"))).toBeGreaterThan(150);
});

test("opening an item and going back leaves every row where it was", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const before = await listOrder(page);
  for (const id of ["d4", "d2", "d3"]) {
    const row = page.locator(`.dk-items [data-desk-item="${id}"]`);
    await row.scrollIntoViewIfNeeded();
    const top = (await row.boundingBox()).y;
    await row.click();
    await expect(page.locator(`.dk-right .dk-panel[data-side-for="${id}"]`)).toBeVisible();
    await page.locator(".dk-right .dk2b-items").click();
    await expect(page.locator(".dk-items")).toHaveCount(1);
    expect(await listOrder(page)).toEqual(before);
    expect((await page.locator(`.dk-items [data-desk-item="${id}"]`).boundingBox()).y).toBe(top);
  }
});

test("a Desk with no lead: the Desks menu still opens All sessions, renames the Desk and opens its settings", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const c = (key, label) => ({ key, label, hint: "", ok: true, why: "" });
    Object.assign(st.desk.projects.find((x) => x.project === "cue"), { lead: null, lands: {
      project: "cue", place: "folder", why: "", summary: "shared folder",
      set: { where: "shared", how: "owner", check: "none", after: "nothing", push: "ask", at_once: 3 },
      parts: [{ key: "where", label: "Where an item works", now: "shared", choices: [c("shared", "Shared folder")] }],
      at_once: { min: 1, max: 9 }, steps: [],
    } });
  });
  await openDesk(page);
  // No lead: who runs it is asked in the chat card, so there's no strip and no All N sessions there.
  await expect(page.locator(".dk-board .dk-pick")).toBeVisible();
  await expect(page.locator(".dk2b-strip")).toHaveCount(0);
  const menu = page.locator(".top .hv-menu");
  await page.locator('.top [data-hv-menu="desks"]').click();
  await menu.locator("[data-hv-desk-sessions]").click();
  await expect(menu).toHaveCount(0);
  await expect(page.locator(".dk-sessions")).toBeVisible();
  await expect(page.locator(".dk-sessions .dk-lcard")).toHaveCount(0);
  await expect(page.locator(".dk-sessions [data-desk-chat-of]").first()).toBeVisible();
  // Rename: the Desk's name in a box over the sessions, selected.
  await page.locator(".dk-sessions .dk2b-items").click();
  await expect(page.locator(".dk-sessions")).toHaveCount(0);
  await page.locator('.top [data-hv-menu="desks"]').click();
  await menu.locator("[data-hv-desk-ren]").click();
  await expect(page.locator(".dk-sessions .pj-name-in")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.locator(".dk-sessions .pj-name-in")).toHaveCount(0);
  // Desk settings…: its settings dialog.
  await page.locator('.top [data-hv-menu="desks"]').click();
  await menu.locator("[data-hv-desk-set]").click();
  await expect(page.locator(".ld-dialog")).toBeVisible();
});

// ---------- two chats on one page (the lead's at the left, what's open on the right) ----------

test("Earlier messages on the right chat moves that chat, never the lead's at the left", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1400, height: 520 });
  await cue.open("desk");
  await openDesk(page);
  const lead = chat(page).locator(".dk-chat");
  const before = await lead.evaluate((c) => { c.scrollTop = c.scrollHeight; return c.scrollTop; });
  expect(before, "the lead's chat scrolls, so a move on it would show").toBeGreaterThan(0);
  await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
  const right = page.locator('.dk-right .dk-chat[data-desk-chat="host"]');
  await cue.override("session_log_page", ({ sessionId, beforeMs }) => sessionId !== "host" ? [] :
    Array.from({ length: 12 }, (_, i) => ({ role: i % 2 ? "agent" : "user", text: `Earlier line ${i} of the host's chat, long enough to take some room.`, at_ms: beforeMs - 60000 + i * 1000, images: [] })));
  await right.locator('[data-act="load-older"]').click();
  await expect(right.getByText("Earlier line 0 of the host's chat")).toHaveCount(1);
  await expect.poll(() => right.evaluate((c) => c.scrollTop), "the host's chat goes to its top").toBe(0);
  expect(await lead.evaluate((c) => c.scrollTop)).toBe(before);
});

test("Esc in the lead's box is the lead's: the page on the right stays, Esc twice stops the lead, not that item's holder", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.sessions.find((s) => s.session_id === "lead").state = "working"; });
  await openDesk(page);
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  const side = page.locator(".dk-card.dk-right.dk2b-side");
  await expect(side.locator(".dk-ph")).toContainText("#d5");
  await chat(page).locator("textarea[data-text]").focus();
  await page.keyboard.press("Escape");
  await expect(side.locator(".dk-ph")).toContainText("#d5");
  await page.keyboard.press("Escape");
  await expect.poll(() => cue.calls("interrupt_session")).toHaveLength(1);
  expect(JSON.stringify(await cue.calls("interrupt_session"))).toContain('"lead"');
  await expect(side.locator(".dk-ph")).toContainText("#d5");
});

test("a reply's This turn in … on the lead brings that turn into view in the lead's chat, not its end", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1400, height: 520 });
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dk-items [data-desk-item="d2"]').click();
  const lead = chat(page).locator(".dk-chat");
  const first = await lead.evaluate((c) => Number(c.querySelector(".cv-agent[data-at]").dataset.at));
  await lead.evaluate((c) => { c.scrollTop = c.scrollHeight; });
  await page.evaluate((at) => { deskTurnAt = at; deskChatEnd = false; deskShow({ kind: "chat", sid: "lead", from: deskChatFrom("lead") }); }, first);
  await expect.poll(() => page.evaluate(() => deskTurnAt)).toBe(0);
  const where = await lead.evaluate((c) => ({ top: c.scrollTop, end: c.scrollHeight - c.clientHeight }));
  expect(where.end).toBeGreaterThan(0);
  expect(where.top).toBeLessThan(where.end);
  await expect(page.locator(".dk-card.dk-right.dk2b-side .dk-ph")).toContainText("#d2");
});

test("a kept item's page with no one holding it has a box of its own, apart from the lead's beside it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { Object.assign(st.desk.projects.find((x) => x.project === "cue"), { kept: [{ id: "d9", title: "Old idea, dropped", project: "cue", repo: "/r/cue", path: "/r/cue-kept/d9", branch: "desk/d9", at: Date.now() - 120000 }] }); });
  await page.locator('.dk-items .dk-kept-t[data-desk-item="d9"]').click();
  const box = page.locator(".dk-card.dk-right.dk2b-side textarea[data-text]"), leadBox = chat(page).locator("textarea[data-text]");
  await expect(box).toHaveCount(1);
  expect(await box.getAttribute("data-text")).not.toBe(await leadBox.getAttribute("data-text"));
  await box.fill("about the kept one");
  await expect(leadBox).toHaveValue("");
});

test("a part kept through a draw is drawn afresh when a box or tick in it went its own way", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const out = await page.evaluate(() => {
    const root = document.createElement("div");
    document.body.append(root);
    const html = `<div class="dk-chat" data-desk-chat="t"></div><span><input class="a" value="drawn"></span><span><input class="b" type="checkbox"></span><span><input class="c" value="same"></span>`;
    root.innerHTML = html;
    const c = root.querySelector(".c");
    root.querySelector(".a").value = "typed";
    root.querySelector(".b").checked = true;
    drawAround(root, html);
    const r = { a: root.querySelector(".a").value, b: root.querySelector(".b").checked, keptSame: root.querySelector(".c") === c };
    root.remove();
    return r;
  });
  expect(out).toEqual({ a: "drawn", b: false, keptSame: true });
});

test("the tabs: an empty one says so; Done today counts only today's; Waits on you counts what the strip says", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.items.push({ id: "y1", title: "Finished yesterday", state: "done", kind: "task", created_ms: st.now_ms - 2 * 86400000, done_at: new Date(st.now_ms - 2 * 86400000).toISOString(), updated: new Date(st.now_ms - 2 * 86400000).toISOString() });
  });
  await openDesk(page);
  const tabs = page.locator(".dk-items .dk2b-tabs .dk2b-tab"), list = page.locator(".dk-items .dk-scroll");
  const waits = Number((await strip(page).locator(".dk2b-wn").textContent()).match(/^\d+/)[0]);
  await expect(tabs.filter({ hasText: "Waits on you" }).locator(".n")).toHaveText(String(waits));
  const today = await page.evaluate(() => deskDoneList(deskOf("cue")).filter(deskDoneToday).map((i) => i.id));
  expect(today).not.toContain("y1");
  if (today.length) {
    await expect(tabs.filter({ hasText: "Done today" }).locator(".n")).toHaveText(String(today.length));
    await tabs.filter({ hasText: "Done today" }).click();
    await expect(list.locator('.dk-it[data-desk-item="y1"]')).toHaveCount(0);
  }
  // None done today: the tab, picked, says so in a line.
  await cue.change((st) => { const d = st.desk.projects.find((x) => x.project === "cue"); for (const i of d.items) if (i.state === "done") i.done_at = i.updated = new Date(st.now_ms - 2 * 86400000).toISOString(); });
  await page.evaluate(() => { deskB2Tab = "done"; renderMain(); });
  await expect(list.locator(".dk-empty")).toHaveText("Nothing done today.");
  await expect(list.locator(".dk-it")).toHaveCount(0);
});
