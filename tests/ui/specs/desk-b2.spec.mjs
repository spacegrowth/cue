// The Desk's lead's chat card and what opens in its place (B2 #666, kept on the one layout #738/#748): Now and Waits
// on you in one strip over the chat (the Working pill unfolds the sessions at work, a chip per item that waits, Check);
// an item opening in the chat's place with back to the lead's chat, Items staying beside it; the lead's panel; dark.
import { test, expect, openDesk, deskShowStrip } from "./cue.mjs";
import { listOrder, statusOf, openDone } from "./desk.mjs";

const chat = (page) => page.locator(".dk-card.dk2b-chat");
const strip = (page) => chat(page).locator(".dk2b-strip");
// Nothing waits on you: the strip shows the Working pill in place of what waits (#666).
const noWaits = (cue) => cue.change((st) => {
  const d = st.desk.projects.find((x) => x.project === "cue");
  for (const i of d.items) if (i.state === "needs_you" || i.state === "to_check") Object.assign(i, { state: "queued", ask: null });
  st.items = st.items.filter((r) => !(r.kind === "question" || r.kind === "permission"));
});

test("the lead's chat card in the middle, the list beside it; no Desk-switching row on the page", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1800, height: 1095 });
  await cue.open("desk");
  await openDesk(page);
  const [c, l] = await Promise.all([chat(page).boundingBox(), page.locator(".dk-c > .dkn-items").boundingBox()]);
  expect(l.x).toBeGreaterThan(c.x + c.width);
  // No Desk-switching row or pills on the page itself.
  await expect(page.locator(".dk-board .dk-strip, .dk-board .dk2-desks")).toHaveCount(0);
});

test("the Working pill unfolds the sessions at work and folds them again", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await noWaits(cue);
  await deskShowStrip(page);
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

test("the strip says how many wait on you, a chip each, and Check opens the first in the chat's place", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskShowStrip(page);
  await expect(strip(page).locator(".dk2b-wn")).toHaveText(/^\d+ waits? on you$/);
  // One or the other: with something waiting, no Working pill.
  await expect(strip(page).locator("[data-desk2b-work]")).toHaveCount(0);
  const chips = strip(page).locator(".dk2b-chip[data-desk-chk-item]");
  await expect(chips.first()).toHaveText(/^#d\d+$/);
  const check = strip(page).locator(".dk2b-check");
  const id = await check.getAttribute("data-desk-chk-item");
  await expect(check).toHaveText(`Check #${id}`);
  const last = await chips.last().getAttribute("data-desk-chk-item");
  await check.click();
  const open = page.locator(".dk-card.dk-right.dk2b-open");
  await expect(open.locator("[data-desk-page] .dkn-ib")).toHaveCount(1);
  // The list stays beside it, its row lit, the lead's chat gone till back.
  await expect(page.locator(".dk-c > .dkn-items")).toHaveCount(1);
  await expect(chat(page)).toHaveCount(0);
  await expect(page.locator(`.dkn-items .dkn-row.on[data-desk-item="${id}"]`)).toHaveCount(1);
  // A chip opens its own item (back to the chat first, where the strip is).
  await open.locator(".dk-back").first().click();
  await strip(page).locator(`.dk2b-chip[data-desk-chk-item="${last}"]`).click();
  await expect(page.locator(`.dkn-items .dkn-row.on[data-desk-item="${last}"]`)).toHaveCount(1);
  await expect(open.locator("[data-desk-page] .dkn-ib")).toHaveCount(1);
});

test("an item queued for a session says so in Up next", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const p = st.desk.projects.find((x) => x.project === "cue"), d6 = p.items.find((i) => i.id === "d6");
    p.items.push({ ...d6, id: "q1", title: "Queued for host", state: "queued", holder: null, next_for: { kind: "session", sid: "host", name: "host-lead" } });
  });
  await openDesk(page);
  await expect(page.locator('[data-dkn-drawer="next"] .dkn-nx[data-desk-item="q1"]')).toHaveCount(1);
  await expect(statusOf(page, "q1")).toHaveText("Next for host-lead");
});

test("the current step's dots move only while a session works on it right now (#729)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // The tests' page holds every animation still (load.js ?still=1): let this one move.
  await page.evaluate(() => [...document.head.querySelectorAll("style")].find((s) => s.textContent.includes("animation:none!important"))?.remove());
  const cur = (id) => page.locator(`.dkn-items .dkn-row[data-desk-item="${id}"] .dkn-kp.cur`);
  const anim = (loc) => loc.evaluate((el) => getComputedStyle(el).animationName);
  // At work: its step striped and marching.
  await expect(cur("d5")).toHaveClass(/\blive\b/);
  expect(await anim(cur("d5").locator("i"))).toBe("dkn-march");
  // Ready to check: striped, still.
  await expect(cur("d2")).toHaveCount(1);
  await expect(cur("d2")).not.toHaveClass(/\blive\b/);
  expect(await anim(cur("d2").locator("i"))).toBe("none");
  // Its bar the same; a redraw keeps it in time with the clock rather than starting it over.
  await page.locator('.dkn-items [data-desk-item="d5"] .dkn-tt').click();
  const step = page.locator(".dk2b-open .dkn-ib .dkn-kp.cur.live i");
  await expect(step).toHaveCount(1);
  expect(await anim(step)).toBe("dkn-march");
  await cue.change(() => {});
  await expect.poll(() => step.evaluate((el) => el.getAnimations()[0]?.startTime)).toBe(0);
});

test("a working session opens its item in the chat's place, with back to the lead's chat", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await noWaits(cue);
  await deskShowStrip(page);
  const lead = await chat(page).locator(".dk-ph").textContent();
  await strip(page).locator("[data-desk2b-work]").click();
  await strip(page).locator(".dk2b-wrow", { hasText: "host-lead" }).click();
  const open = page.locator(".dk-card.dk-right.dk2b-open");
  await expect(page.locator('.dkn-items .dkn-row.on[data-desk-item="d5"]')).toHaveCount(1);
  // Its chat with its bar of steps over it.
  await expect(open.locator(".dkn-ib .dkn-kp")).toHaveCount(6);
  await expect(open.locator('.dk-chat[data-desk-chat="host"]')).toHaveCount(1);
  await open.locator(".dkn-sback .dk-back").click();
  await expect(page.locator(".dk2b-open")).toHaveCount(0);
  await expect(chat(page).locator(".dk-ph")).toHaveText(lead);
  await expect(page.locator(".dk-c > .dkn-items")).toHaveCount(1);
});

test("opening the lead's chat (⌘K) with an item open goes back to the lead's chat, never a second copy", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dkn-items [data-desk-item="d2"] .dkn-tt').click();
  await expect(page.locator('.dkn-items .dkn-row.on[data-desk-item="d2"]')).toHaveCount(1);
  await expect(page.locator(".dk-card.dk2b-open")).toHaveCount(1);
  await page.keyboard.press("Meta+k");
  await page.locator('[data-text="find-live"]').pressSequentially("web-lead");
  await page.locator('.lv-pop .lv-row[data-sid="lead"]').click();
  await expect(page.locator(".lv-pop")).toHaveCount(0);
  await expect(page.locator(".dk2b-open")).toHaveCount(0);
  await expect(page.locator(".dk-chat[data-desk-chat]")).toHaveCount(1);
  await expect(chat(page).locator(".dk-chat[data-desk-chat]")).toHaveCount(1);
  await expect(page.locator(".dk-c > .dkn-items")).toHaveCount(1);
});

test("the lead's panel shows as the page opens; Hide panel puts the strip in its place, Show panel brings it back", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1800, height: 1095 });
  await cue.open("desk");
  await openDesk(page);
  const tog = chat(page).locator('[data-desk-panel="dispatcher-hidden"]');
  await expect(tog).toHaveText("Hide panel");
  await expect(chat(page).locator(".dk-panel .dk-pnow")).toBeVisible();
  await expect(chat(page).locator(".dk-panel .dk-psec", { hasText: "Just did" })).toHaveCount(0);
  await expect(chat(page).locator(".dk2b-strip")).toHaveCount(0);
  await tog.click();
  await expect(chat(page).locator('[data-desk-panel="dispatcher-hidden"]')).toHaveText("Show panel");
  await expect(chat(page).locator(".dk2b-strip")).toBeVisible();
  await chat(page).locator('[data-desk-panel="dispatcher-hidden"]').click();
  await expect(chat(page).locator(".dk-panel .dk-pnow")).toBeVisible();
  // No Conversation only | Everything switch in the lead's tab row: its chat shows everything.
  await expect(chat(page).locator(".dk-ptabs .seg2")).toHaveCount(0);
});

test("dark theme draws the page in its own colors", async ({ page, cue }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await cue.open("desk");
  await openDesk(page);
  await noWaits(cue);
  await deskShowStrip(page);
  await strip(page).locator("[data-desk2b-work]").click();
  // The pill's redraw done first; each colour then read off the element on the page at that moment (a handle taken
  // before a redraw is a detached element, whose computed colours are empty).
  await expect(strip(page).locator(".dk2b-wlist")).toBeVisible();
  const css = (sel, prop) => page.evaluate(([sel, prop]) => getComputedStyle(document.querySelector(sel))[prop], [sel, prop]);
  const lum = (c) => { const [r, g, b] = c.match(/[\d.]+/g).map(Number); return (r + g + b) / 3; };
  expect(lum(await css(".dk-card.dk2b-chat", "backgroundColor"))).toBeLessThan(90);
  expect(lum(await css(".dkn-items .dk-card", "backgroundColor"))).toBeLessThan(90);
  expect(lum(await css(".dkn-items .dkn-row .dkn-tt", "color"))).toBeGreaterThan(150);
});

test("opening an item and going back leaves every row where it was", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const before = await listOrder(page);
  for (const id of ["d4", "d2", "d3"]) {
    const row = page.locator(`.dkn-items [data-desk-item="${id}"]`);
    await row.scrollIntoViewIfNeeded();
    const top = (await row.boundingBox()).y;
    await row.click();
    await expect(page.locator(`.dkn-items [data-desk-item="${id}"].on`)).toHaveCount(1);
    await expect(page.locator(".dk-right.dk2b-open [data-desk-page]")).toHaveCount(1);
    expect(await listOrder(page)).toEqual(before);
    expect((await page.locator(`.dkn-items [data-desk-item="${id}"]`).boundingBox()).y).toBe(top);
    await page.locator(".dk-right .dkn-sback .dk-back").click();
    await expect(page.locator(".dk2b-open")).toHaveCount(0);
    expect(await listOrder(page)).toEqual(before);
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

// ---------- what's open takes the lead's chat's place ----------

test("a session's chat opens in the lead's chat's place, the list beside it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
  await expect(page.locator('.dk-right.dk2b-open .dk-chat[data-desk-chat="host"]')).toHaveCount(1);
  await expect(chat(page)).toHaveCount(0);
  await expect(page.locator(".dk-c > .dkn-items")).toHaveCount(1);
});

test("Esc on an open item goes back to the lead's chat; Esc twice in the lead's box stops the lead", async ({ page, cue }) => {
  await cue.open("desk");
  // #d5's holder waits: a working holder takes the first Esc (Esc twice stops it, esc-order.spec), as on the Board.
  await cue.change((st) => {
    st.sessions.find((s) => s.session_id === "lead").state = "working";
    st.sessions.find((s) => s.session_id === "host").state = "waiting";
  });
  await openDesk(page);
  await page.locator('.dkn-items [data-desk-item="d5"] .dkn-tt').click();
  await expect(page.locator('.dkn-items .dkn-row.on[data-desk-item="d5"]')).toHaveCount(1);
  await expect(page.locator(".dk-card.dk2b-open")).toHaveCount(1);
  // Out of its box, so Esc is the Desk's.
  await page.evaluate(() => document.activeElement?.blur());
  await page.keyboard.press("Escape");
  await expect(page.locator(".dk2b-open")).toHaveCount(0);
  await chat(page).locator("textarea[data-text]").focus();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect.poll(() => cue.calls("interrupt_session")).toHaveLength(1);
  expect(JSON.stringify(await cue.calls("interrupt_session"))).toContain('"lead"');
});

test("a reply's This turn in … on the lead brings that turn into view in the lead's chat, not its end", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1400, height: 520 });
  await cue.open("desk");
  await openDesk(page);
  const lead = chat(page).locator(".dk-chat");
  const first = await lead.evaluate((c) => Number(c.querySelector(".cv-agent[data-at]").dataset.at));
  await page.locator('.dkn-items [data-desk-item="d2"] .dkn-tt').click();
  await expect(page.locator('.dkn-items .dkn-row.on[data-desk-item="d2"]')).toHaveCount(1);
  await expect(page.locator(".dk-card.dk2b-open")).toHaveCount(1);
  await page.evaluate((at) => { deskTurnAt = at; deskChatEnd = false; deskShow({ kind: "chat", sid: "lead", from: deskChatFrom("lead") }); }, first);
  await expect.poll(() => page.evaluate(() => deskTurnAt)).toBe(0);
  await expect(page.locator(".dk2b-open")).toHaveCount(0);
  const where = await lead.evaluate((c) => ({ top: c.scrollTop, end: c.scrollHeight - c.clientHeight }));
  expect(where.end).toBeGreaterThan(0);
  expect(where.top).toBeLessThan(where.end);
});

test("a kept item's page with no one holding it has a box of its own in the lead's chat's place", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { Object.assign(st.desk.projects.find((x) => x.project === "cue"), { kept: [{ id: "d9", title: "Old idea, dropped", project: "cue", repo: "/r/cue", path: "/r/cue-kept/d9", branch: "desk/d9", at: Date.now() - 120000 }] }); });
  await page.locator('.dk-items .dk-kept-t[data-desk-item="d9"]').click();
  const box = page.locator(".dk-card.dk2b-open textarea[data-text]");
  await expect(box).toHaveCount(1);
  await expect(chat(page)).toHaveCount(0);
  await box.fill("about the kept one");
  await expect(box).toHaveValue("about the kept one");
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

test("Done today counts only today's, and says so when there are none; Waiting on you counts what the strip says", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.items.push({ id: "y1", title: "Finished yesterday", state: "done", kind: "task", created_ms: st.now_ms - 2 * 86400000, done_at: new Date(st.now_ms - 2 * 86400000).toISOString(), updated: new Date(st.now_ms - 2 * 86400000).toISOString() });
  });
  await openDesk(page);
  await deskShowStrip(page);
  await openDone(page);
  const done = page.locator('.dkn-items [data-dkn-drawer="done"]');
  const waits = Number((await strip(page).locator(".dk2b-wn").textContent()).match(/^\d+/)[0]);
  await expect(page.locator('.dkn-card .dkn-gh[data-desk2b-group="you"] > span:not(.dkn-fa)')).toHaveText(String(waits));
  const today = await page.evaluate(() => deskDoneList(deskOf("cue")).filter(deskDoneToday).map((i) => i.id));
  expect(today).not.toContain("y1");
  if (today.length) {
    await expect(done.locator(".dkn-dh .dkn-dn")).toHaveText(String(today.length));
    await expect(done.locator('.dk-it[data-desk-item="y1"]')).toHaveCount(0);
  }
  // None done today: it says so in a line.
  await cue.change((st) => { const d = st.desk.projects.find((x) => x.project === "cue"); for (const i of d.items) if (i.state === "done") i.done_at = i.updated = new Date(st.now_ms - 2 * 86400000).toISOString(); });
  await expect(done.locator(".dkn-dnone")).toHaveText("Nothing done today.");
  await expect(done.locator(".dk-it")).toHaveCount(0);
});
