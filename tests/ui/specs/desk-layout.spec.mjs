// The Desk's layout (#738, desk/c.js; the only one since #748): the Desk's sessions at the left on the green (the lead
// on top, a heading per machine), the lead's chat with a panel of only Now and Activity, Items as one card (Waiting on
// you and Working, six steps a row) with Up next and Done today side by side under it, each folding to its heading; no
// count of its own in the header. An item opens its session's chat with its bar of steps, or (no session yet) its bar
// over what it was added with: there's no other item page.
import { test, expect, openDesk, deskAllSessions } from "./cue.mjs";
import { stampSteps } from "./desk-steps.mjs";

const open = async (page, cue, size = { width: 1800, height: 1095 }) => {
  await page.setViewportSize(size);
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-board.dk-c")).toHaveCount(1);
};

test("the three columns, always: no switch in Desk settings, no tabs, no List | Cards", async ({ page, cue }) => {
  await open(page, cue);
  const [s, c, i] = await Promise.all([".dk-c > .dkn-side", ".dk-c > .dk-right", ".dk-c > .dkn-items"].map((q) => page.locator(q).boundingBox()));
  expect(s.x + s.width).toBeLessThan(c.x);
  expect(c.x + c.width).toBeLessThan(i.x);
  await expect(page.locator(".dk-c .dk2b-tabs, .dk-c .dkc-sw")).toHaveCount(0);
  // Desk settings has no switch of layouts.
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator(".top .hv-menu [data-hv-desk-set]").click();
  await page.locator(".ld-dialog").waitFor();
  await expect(page.locator(".ld-dialog .dk2-sw, .ld-dialog [data-desk-c-try], .ld-dialog [data-desk2-try]")).toHaveCount(0);
  await page.keyboard.press("Escape");
  // Turned on before, the old switches' keys change nothing.
  await page.evaluate(() => { localStorage.setItem("cue.deskC", "0"); localStorage.setItem("cue.deskV2", "1"); });
  await page.reload();
  await page.locator('#app [data-view="board"]').waitFor();
  await openDesk(page);
  await expect(page.locator(".dk-board.dk-c")).toHaveCount(1);
  await expect(page.locator(".dk-board.dk-v2")).toHaveCount(0);
});

test("the sessions column: the lead on top under a line, a heading per machine, a click opens a chat", async ({ page, cue }) => {
  await open(page, cue);
  const side = page.locator(".dkn-side");
  await expect(side.locator(".dkn-lead")).toContainText("web-lead");
  await expect(side.locator(".dkn-lead")).toHaveClass(/\bon\b/);
  await expect(side.locator(".dkn-rule")).toHaveCount(1);
  await expect(side.locator(".dkn-mg").first()).toHaveText(/^THIS MAC· \d+ (of \d+ )?busy$/);
  const host = side.locator('[data-desk-chat-of="host"]');
  await expect(host).toContainText("#d5 · Building");
  await host.click();
  await expect(host).toHaveClass(/\bon\b/);
  await expect(side.locator(".dkn-lead")).not.toHaveClass(/\bon\b/);
  // Its item is lit in the list, and its chat is in the middle: the Board's top bar for it, no panel (#744).
  await expect(page.locator('.dkn-card .dkn-row.on[data-desk-item="d5"]')).toHaveCount(1);
  await expect(page.locator(".dk-c > .dk-right .dkn-sp .ap-head")).toContainText("host-lead");
  await expect(page.locator(".dk-c > .dk-right .dk-panel")).toHaveCount(0);
  // The lead's block brings its chat back.
  await side.locator(".dkn-lead").click();
  await expect(side.locator(".dkn-lead")).toHaveClass(/\bon\b/);
  await expect(page.locator(".dk-c > .dk2b-chat")).toHaveCount(1);
});

// 692dc421: the lead's chat has no panel (no Now in a panel, no Hide panel); the strip, always shown, carries the count.
test("one count: the lead's strip says Blocked on you and Waiting on you together, with no count in the header; the lead's chat has no panel", async ({ page, cue }) => {
  await open(page, cue);
  await expect(page.locator(".top")).not.toContainText("wait on you");
  // Blocked on you (absent while nothing is) and Waiting on you: the strip counts both.
  const counts = await page.locator('.dkn-card .dkn-gh:is([data-desk2b-group="you"], [data-desk2b-group="blocked"]) > span').allTextContents();
  expect(counts.length).toBeGreaterThan(0);
  const n = String(counts.reduce((a, c) => a + Number(c.trim()), 0));
  // The lead's aside is the strip only: no Now of its own, none of the panel's sections showing, no Hide panel.
  await expect(page.locator(".dk-c .dk2b-chat .dk-pnow")).toHaveCount(0);
  await expect(page.locator(".dk-c .dk2b-chat .dk-panel .dk-psec:visible")).toHaveCount(0);
  await expect(page.locator(".dk-c .dk2b-chat [data-desk-panel]")).toHaveCount(0);
  await expect(page.locator(".dk2b-chat .dk2b-wn")).toHaveText(`${n} wait on you`);
  // The first that waits on you, #d1, opens its session's chat (#744).
  await page.locator('.dkn-card .dkn-row[data-desk-item="d1"] .dkn-tt').click();
  await expect(page.locator('.dkn-card .dkn-row.on[data-desk-item="d1"]')).toHaveCount(1);
  await expect(page.locator('.dk-c > .dk-right .dkn-sp[data-desk-page="session"] .dkn-ib')).toHaveCount(1);
  // Back to the lead's chat (an item has its back again, #813): the strip says the same number.
  await page.locator(".dk-c .dk-right .dkn-sback .dk-back").click();
  await expect(page.locator(".dk2b-chat .dk2b-wn")).toHaveText(`${n} wait on you`);
});

test("Items: a row per item with its strip, the step it's at striped; ready to check is a ✓, asking is red words, no buttons", async ({ page, cue }) => {
  await open(page, cue);
  const card = page.locator(".dkn-card");
  const d2 = card.locator('.dkn-row[data-desk-item="d2"]');
  await expect(d2.locator(".dkn-ks .dkn-kp")).toHaveCount(6);
  await expect(d2.locator(".dkn-kp.done")).toHaveCount(5);
  await expect(d2.locator(".dkn-kp.cur")).toHaveCount(1);
  // The step it's at in the Board's waiting stripes.
  const bg = await d2.locator(".dkn-kp.cur i").evaluate((el) => getComputedStyle(el).backgroundImage);
  expect(bg).toContain("repeating-linear-gradient");
  // No number column: the title on top, the kind's icon and number at the head of the line under it (#744).
  await expect(card.locator(".dkn-th > span")).toHaveText(["Item", "Progress", "Now"]);
  expect(await d2.evaluate((el) => el.firstElementChild.className)).toBe("dkn-t");
  await expect(d2.locator(".dkn-tt")).toHaveText("Quick phrases at the right of Send");
  await expect(d2.locator(".dkn-sub")).toHaveText("#d2 · web-lead");
  await expect(d2.locator(".dkn-sub > .dkn-id:first-child .dkn-k")).toHaveAttribute("aria-label", "feature");
  await expect(card.locator('.dkn-row[data-desk-item="d1"] .dkn-sub .dkn-k')).toHaveAttribute("aria-label", "bug");
  await expect(card.locator(".dkn-row")).not.toContainText([/\b(Bug|Feature|Task)\b/]);
  // Now, right-aligned in its 136px: ready to check is the words and a round ✓; asking, just red words.
  const now = d2.locator(".dkn-now");
  expect(await now.evaluate((el) => [Math.round(el.getBoundingClientRect().width), getComputedStyle(el).justifyContent])).toEqual([136, "flex-end"]);
  await expect(now.locator(".dkn-nw")).toHaveText("Ready to check");
  await expect(now.locator('.dk-ok[data-desk-act="looks_right"]')).toHaveAttribute("aria-label", "Looks right");
  const ask = card.locator('.dkn-row[data-desk-item="d1"] .dkn-now');
  await expect(ask).toHaveText("Asks you");
  await expect(ask.locator(".dkn-nask")).toHaveCSS("color", await page.evaluate(() => { const p = document.createElement("i"); p.style.color = "var(--deny)"; document.body.append(p); const c = getComputedStyle(p).color; p.remove(); return c; }));
  await expect(card.locator("[data-desk-chk-item], .dkn-now .btn")).toHaveCount(0);
  await expect(card).not.toContainText(/Choose…|\bCheck\b/);
  // The row opens it: its session's chat.
  await card.locator('.dkn-row[data-desk-item="d1"] .dkn-tt').click();
  await expect(page.locator('.dkn-card .dkn-row.on[data-desk-item="d1"]')).toHaveCount(1);
  await expect(page.locator(".dk-c > .dk-right .dkn-sp")).toHaveCount(1);
});

test("a machine pill only when it isn't This Mac, at the end of the line; Up next and Done today put the icon and number first", async ({ page, cue }) => {
  await open(page, cue);
  await cue.change((st) => {
    const its = st.desk.projects.find((d) => d.project === "cue").items;
    its.find((i) => i.id === "d2").machine = "This Mac";
    its.find((i) => i.id === "d3").machine = "studio";
    its.find((i) => i.id === "d4").machine = "studio";
  });
  const card = page.locator(".dkn-card");
  await expect(card.locator('.dkn-row[data-desk-item="d3"] .dkn-sub')).toHaveText("#d3 · web-lead → search-centre · studio");
  await expect(card.locator('.dkn-row[data-desk-item="d3"] .dkn-sub > :last-child')).toHaveClass("mach");
  await expect(card.locator('.dkn-row[data-desk-item="d2"] .dkn-sub .mach')).toHaveCount(0);
  await expect(card.locator('.dkn-row[data-desk-item="d2"] .dkn-sub')).not.toContainText("This Mac");
  const d4 = page.locator('[data-dkn-drawer="next"] .dkn-nx[data-desk-item="d4"]');
  await expect(d4.locator(".dkn-a1")).toHaveText("Rail dot blinks for a reply you didn't send");
  await expect(d4.locator(".dkn-a2 > :first-child")).toHaveClass("dkn-id");
  await expect(d4.locator(".dkn-a2 > .dkn-id")).toHaveText("#d4");
  await expect(d4.locator(".dkn-a2 > .dkn-id .dkn-k")).toHaveAttribute("aria-label", "bug");
  await expect(d4.locator(".dkn-a2 .mach")).toHaveText("studio");
  const d7 = page.locator('[data-dkn-drawer="done"] .dkn-nx[data-desk-item="d7"]');
  await expect(d7.locator(".dkn-a2 > :first-child .dkn-k")).toHaveAttribute("aria-label", "task");
  await expect(d7.locator(".dkn-a2 .mach")).toHaveCount(0);
});

test("the ✓: Done · Undo where it was for 1.2 s, the row otherwise as the others; then it folds shut and goes, Undo working till then (#791)", async ({ page, cue }) => {
  await open(page, cue);
  await cue.override("desk_looks_right", ({ id }) => {
    const it = CueBridge.world.state.desk.projects[0].items.find((i) => i.id === id);
    Object.assign(it, { state: "done", done_at: Math.floor(Date.now() / 1000) });
    CueBridge.emit("state", CueBridge.world.snapshot());
    return { commit: "", files: 0, waiting: [] };
  });
  // The clock stopped, so the 1.2 s run only as runFor moves them (a slow run's own seconds don't).
  await page.clock.pauseAt(await page.evaluate(() => Date.now() + 500));
  const card = page.locator(".dkn-card");
  const ids = () => card.locator(".dkn-row[data-desk-item]").evaluateAll((els) => els.map((e) => e.dataset.deskItem));
  const order = await ids();
  const d2 = card.locator('.dkn-row[data-desk-item="d2"]'), at = await d2.boundingBox();
  const look = (loc) => loc.evaluate((el) => [getComputedStyle(el).backgroundColor, getComputedStyle(el.querySelector(".dkn-t")).opacity]);
  const quiet = await look(d2);
  await d2.locator(".dk-ok").click();
  await expect(d2).toHaveClass(/\bsoon\b/);
  await expect(d2.locator(".dk-soon")).toHaveText("Done · Undo");
  // No fill, no greying: it looks as it did, its place and size the same, and the click didn't open it.
  expect(await look(d2)).toEqual(quiet);
  await expect(d2.locator(".dk-ok")).toHaveCount(0);
  expect(await ids()).toEqual(order);
  expect(await d2.boundingBox()).toEqual(at);
  await expect(d2).not.toHaveClass(/\bon\b/);
  // Undo: back to its ✓, nothing sent.
  await d2.locator('.dk-soon [data-desk-act="looks_undo"]').click();
  await expect(d2.locator(".dk-ok")).toHaveCount(1);
  await expect(d2).not.toHaveClass(/\bsoon\b/);
  await page.clock.runFor(5000);
  expect(await cue.calls("desk_looks_right")).toEqual([]);
  // Left alone: still Done · Undo just before 1.2 s; then it folds shut, and Undo while it folds still takes it back.
  await d2.locator(".dk-ok").click();
  await page.clock.runFor(1100);
  await expect(d2).not.toHaveClass(/\bdk-leave\b/);
  await page.clock.runFor(200);
  await expect(d2).toHaveClass(/\bdk-leave\b/);
  await d2.locator('.dk-soon [data-desk-act="looks_undo"]').click();
  await expect(d2).not.toHaveClass(/\b(dk-leave|soon)\b/);
  await page.clock.runFor(1000);
  expect(await cue.calls("desk_looks_right")).toEqual([]);
  // Once more, left alone: sent only once the fold has ended; then it's gone, and in Done today.
  await d2.locator(".dk-ok").click();
  await page.clock.runFor(1300);
  expect(await cue.calls("desk_looks_right")).toEqual([]);
  await page.clock.runFor(400);
  await cue.called("desk_looks_right", { id: "d2" });
  await expect(d2).toHaveCount(0);
  await expect(page.locator('[data-dkn-drawer="done"] .dkn-nx[data-desk-item="d2"]')).toHaveCount(1);
});

test("Waiting on you and Working fold to their headings with a click, remembered with the drawers", async ({ page, cue }) => {
  await open(page, cue);
  const card = page.locator(".dkn-card");
  const you = card.locator('.dkn-gh[data-desk-c-fold="you"]'), work = card.locator('.dkn-gh[data-desk-c-fold="working"]');
  await expect(you).toHaveAttribute("aria-expanded", "true");
  // The heading keeps its dot, count and plain line, and no fold arrow (a click on it still folds).
  await expect(you.locator(".dkn-gdot")).toHaveCount(1);
  await expect(you.locator(".dkn-fa")).toHaveCount(0);
  await you.click();
  await expect(you).toHaveAttribute("aria-expanded", "false");
  await expect(you.locator("> span")).toHaveText("2");
  await expect(you.locator(".dkn-gsay")).toHaveText("done · ready for you to check");
  await expect(card.locator('.dkn-row[data-desk-item="d1"], .dkn-row[data-desk-item="d2"]')).toHaveCount(0);
  await expect(card.locator('.dkn-row[data-desk-item="d3"]')).toHaveCount(1);
  expect(JSON.parse(await page.evaluate(() => localStorage.getItem("cue.deskCFolds")))).toContain("you");
  await work.click();
  await expect(card.locator(".dkn-row[data-desk-item]")).toHaveCount(0);
  await page.reload();
  await page.locator('#app [data-view="board"]').waitFor();
  await openDesk(page).catch(() => {});
  await expect(page.locator('.dkn-card .dkn-gh[data-desk-c-fold="you"]')).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator('.dkn-card .dkn-gh[data-desk-c-fold="working"]')).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator(".dkn-card .dkn-row[data-desk-item]")).toHaveCount(0);
  await page.locator('.dkn-card .dkn-gh[data-desk-c-fold="you"]').click();
  await expect(page.locator('.dkn-card .dkn-row[data-desk-item="d2"]')).toHaveCount(1);
  await expect(page.locator('.dkn-card .dkn-row[data-desk-item="d3"]')).toHaveCount(0);
});

test("a selected row fills the card edge to edge; the card fills its column, Up next and Done today a 35% share under it", async ({ page, cue }) => {
  await open(page, cue);
  const card = page.locator(".dkn-card");
  await card.locator('.dkn-row[data-desk-item="d3"]').click();
  const row = card.locator('.dkn-row.on[data-desk-item="d3"]');
  const [r, b] = await Promise.all([row.boundingBox(), card.locator(".dkn-body").evaluate((el) => { const x = el.getBoundingClientRect(); return { x: x.x, w: el.clientWidth }; })]);
  expect(Math.round(r.x)).toBe(Math.round(b.x));
  expect(Math.round(r.width)).toBe(Math.round(b.w));
  // Few rows: the card still reaches down to Up next and Done today, which take 35% of the column.
  const [col, c, low] = await Promise.all([".dk-c > .dkn-items", ".dkn-card", ".dkn-low"].map((q) => page.locator(q).boundingBox()));
  expect(Math.abs(low.height - col.height * 0.35)).toBeLessThan(2);
  expect(Math.abs(c.y + c.height + 18 - low.y)).toBeLessThan(2);
  // Both folded: the card takes the room.
  await page.locator('[data-dkn-drawer="next"] [data-desk-c-fold]').click();
  await page.locator('[data-dkn-drawer="done"] [data-desk-c-fold]').click();
  await expect.poll(async () => (await page.locator(".dkn-card").boundingBox()).height).toBeGreaterThan(c.height + low.height * 0.6);
});

test("\"↓ N more\" scrolls its list on even when the Desk redraws just as it starts (#749)", async ({ page, cue }) => {
  await open(page, cue, { width: 1440, height: 900 });
  await cue.change((st) => {
    const its = st.desk.projects.find((d) => d.project === "cue").items;
    const d3 = its.find((i) => i.id === "d3"), d4 = its.find((i) => i.id === "d4");
    for (let k = 0; k < 14; k++) its.push({ ...d3, id: `w${k}`, title: `Working item ${k}` });
    for (let k = 0; k < 10; k++) its.push({ ...d4, id: `q${k}`, title: `Queued item ${k}`, depends_on: [] });
  });
  for (const name of ["items", "next"]) {
    const list = page.locator(`[data-dkn-list="${name}"]`);
    await expect(list.locator("xpath=following-sibling::*[1]")).toHaveText(/^↓ \d+ more$/);
    // The click, and an update drawing the window afresh in the same moment, before the list has moved at all.
    await page.evaluate((n) => { document.querySelector(`[data-desk-c-all="${n}"]`).click(); renderMain(); }, name);
    await expect.poll(() => list.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
  }
});

/** A list's rows, into its bands (a band's heading counts as a row): as desk/c.js deskCFitLists cuts them. */
const ROWS = ":scope > :not(.dkn-band), :scope > .dkn-band > .dkn-gh, :scope > .dkn-band > .dkn-bb > *";
/** Each list ends on a whole row: none cut by its edge. */
const wholeRows = (list) => list.evaluate((el, q) => {
  const edge = el.getBoundingClientRect().bottom;
  return [...el.querySelectorAll(q)].filter((r) => { const b = r.getBoundingClientRect(); return b.top < edge - 1 && b.bottom > edge + 1; }).map((r) => r.dataset.deskItem || r.className);
}, ROWS);
test("a list that overflows ends on a whole row with \"↓ N more\" under it; a click scrolls on, at its end the line keeps its room", async ({ page, cue }) => {
  await open(page, cue, { width: 1440, height: 900 });
  await cue.change((st) => {
    const its = st.desk.projects.find((d) => d.project === "cue").items;
    const d3 = its.find((i) => i.id === "d3"), d4 = its.find((i) => i.id === "d4");
    for (let k = 0; k < 14; k++) its.push({ ...d3, id: `w${k}`, title: `Working item ${k}` });
    for (let k = 0; k < 10; k++) its.push({ ...d4, id: `q${k}`, title: `Queued item ${k}`, depends_on: [] });
  });
  await expect(page.locator('.dkn-card .dkn-row[data-desk-item="w13"]')).toHaveCount(1);
  // Working (#d3, #d5 and 14 more) shows its first 4 rows and scrolls within, "↓ 12 more" under it.
  const wl = page.locator('.dkn-card .dkn-b-working [data-dkn-list="working"]');
  await expect.poll(() => wl.evaluate((el) => { const e = el.getBoundingClientRect().bottom; return [...el.children].filter((r) => r.getBoundingClientRect().bottom <= e + 1).length; })).toBe(4);
  await expect(page.locator(".dkn-card .dkn-b-working .dkn-wmore")).toHaveText("↓ 12 more");
  for (const name of ["items", "next"]) {
    const list = page.locator(`[data-dkn-list="${name}"]`), more = list.locator("xpath=following-sibling::*[1]");
    await expect(more).toHaveClass(/\bdkn-more\b/);
    await expect(more).toBeVisible();
    await expect(more).toHaveText(/^↓ \d+ more$/);
    // The lists are cut while this line is still empty: it's as tall without its words as with them, or they'd cut the last row.
    expect(await more.evaluate((el) => { const full = el.offsetHeight, t = el.textContent; el.textContent = ""; const empty = el.offsetHeight; el.textContent = t; return empty - full; })).toBe(0);
    expect(await wholeRows(list), name).toEqual([]);
    // Its count is the rows below the edge.
    const below = await list.evaluate((el) => { const e = el.getBoundingClientRect().bottom; return [...el.querySelectorAll(":scope > .dk-it, :scope > .dkn-band > .dkn-bb > .dk-it")].filter((r) => r.getBoundingClientRect().bottom > e + 1).length; });
    expect(below).toBeGreaterThan(0);
    await expect(more).toHaveText(`↓ ${below} more`);
    // A click scrolls a page on; at the end it says nothing but keeps its place.
    await more.click();
    await expect.poll(() => list.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
    const h = (await more.boundingBox()).height;
    await list.evaluate((el) => { el.scrollTop = el.scrollHeight; });
    await expect(more).toHaveClass(/\bend\b/);
    expect(await more.evaluate((el) => [getComputedStyle(el).visibility, Math.round(el.getBoundingClientRect().height)])).toEqual(["hidden", Math.round(h)]);
  }
  // Done today fits: no line under it.
  await expect(page.locator('[data-dkn-drawer="done"] .dkn-more')).toBeHidden();
});

test("an item a session holds opens that session's chat: back, its top bar, the item, its steps, Chat, Files and Activity; no panel", async ({ page, cue }) => {
  await open(page, cue);
  await page.locator('.dkn-card .dkn-row[data-desk-item="d5"]').click();
  const sp = page.locator('.dk-c > .dk-right .dkn-sp[data-desk-page="session"]');
  await expect(sp.locator(".dkn-sback .dk-back")).toHaveCount(1);
  await expect(sp.locator(".ap-head")).toContainText("host-lead");
  await expect(sp.locator(".dkn-smeta")).toHaveText("on #d5 · Hosted sessions can't open the canvas");
  // Its steps; working, nothing to press.
  await expect(sp.locator(".dkn-ib .dkn-ks .dkn-kp")).toHaveCount(6);
  await expect(sp.locator(".dkn-ib [data-desk-act]")).toHaveCount(0);
  await expect(sp.locator(".dkn-ib")).not.toHaveClass(/\bcheck\b/);
  // Its parts in order, then the chat and the box.
  expect(await sp.evaluate((el) => [...el.children].map((c) => c.className.split(" ")[0]))).toEqual(["dkn-sback", "ap-head", "dkn-smeta", "dkn-ib", "dkn-stabs", "dkn-spm"]);
  // #789 (22a2d7c9) gave an item's page an Activity tab, for the Desk's moves its chat no longer shows.
  await expect(sp.locator(".dkn-stab")).toHaveText(["Chat", /^Files \d+$/, "Activity"]);
  await expect(sp.locator('.dkn-stab[data-desk-c-stab="chat"]')).toHaveAttribute("aria-selected", "true");
  await expect(sp.locator('.dkn-spm [data-desk-chat="host"]')).toHaveCount(1);
  await expect(sp.locator(".dkn-spm textarea")).toHaveCount(1);
  await expect(page.locator(".dk-c > .dk-right .dk-panel")).toHaveCount(0);
  await expect(page.locator(".dk-c > .dk-right")).not.toContainText("Open its session");
  // Files: its tab shows what it changed instead of the chat.
  await sp.locator('.dkn-stab[data-desk-c-stab="files"]').click();
  await expect(sp.locator('.dkn-stab[data-desk-c-stab="files"]')).toHaveAttribute("aria-selected", "true");
  await expect(sp.locator(".dkn-sfiles")).toBeVisible();
  await expect(sp.locator('[data-desk-chat="host"]')).toHaveCount(0);
  await sp.locator('.dkn-stab[data-desk-c-stab="chat"]').click();
  await expect(sp.locator('[data-desk-chat="host"]')).toHaveCount(1);
  // Ready to check: Looks right, Send back and what to try in its bar.
  await page.locator('.dkn-card .dkn-row[data-desk-item="d2"] .dkn-tt').click();
  const bar = page.locator(".dk-c > .dk-right .dkn-sp .dkn-ib.check");
  await expect(bar.locator('[data-desk-act="looks_right"]')).toHaveText("Looks right");
  await expect(bar.locator('[data-desk-act="send_back"]')).toHaveText("Send back");
  await expect(bar.locator(".dkn-ibsee")).toHaveText(/^Try: \S/);
  // Send back: why, in its own box in the bar, in place of the buttons.
  await bar.locator('[data-desk-act="send_back"]').click();
  await expect(page.locator(".dk-c > .dk-right .dkn-ib .dkn-ibback textarea")).toBeFocused();
  await expect(page.locator('.dk-c > .dk-right .dkn-ib [data-desk-act="looks_right"]')).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(page.locator(".dk-c > .dk-right .dkn-ib .dkn-ibback")).toHaveCount(0);
  // An item no session has: the same frame, its bar over what it was added with; no tabs, no panel.
  await page.locator('[data-dkn-drawer="next"] .dkn-nx[data-desk-item="d6"]').click();
  const bare = page.locator('.dk-c > .dk-right .dkn-sp[data-desk-page="item"]');
  await expect(bare.locator(".dkn-ih1")).toContainText("#d6");
  await expect(bare.locator(".dkn-ib .dkn-ks .dkn-kp")).toHaveCount(6);
  await expect(bare.locator(".dkn-ibare .dk-sec h3", { hasText: "Added with" })).toHaveCount(1);
  await expect(bare.locator('.dkn-ib [data-desk-act="drop"]')).toHaveText("Drop this item");
  await expect(bare.locator(".dkn-spm textarea")).toHaveCount(1);
  await expect(page.locator(".dk-c > .dk-right .dk-panel, .dk-c > .dk-right .dkn-stabs")).toHaveCount(0);
  // Nowhere an item page of the old kind: no Thread, Record or Decisions tabs.
  await expect(page.locator(".dk-ipane, .dk-itabs, [data-desk-itab], .dk2b-ipage")).toHaveCount(0);
});

test("in a session's chat on the new layout its lines for Cue never show as written: IF PICKED one quiet line, CHECK left out, no cards (#833)", async ({ page, cue }) => {
  await open(page, cue);
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "host").thread.push({ role: "agent", at_ms: Date.now(), text: [
      "Built it and the tests pass.",
      "",
      "IF PICKED: #d5 | B | task | a task follows | You'll see: the switch | done when: it's there",
      "CHECK: #d5 | Where: the Desk | Do: open a chat | Expect: a switch in its head",
    ].join("\n") });
  });
  await page.locator('.dkn-side [data-desk-chat-of="host"]').click();
  const chat = page.locator('.dk-c > .dk-right .dkn-sp [data-desk-chat="host"]');
  await expect(chat).toContainText("Built it and the tests pass.");
  await expect(chat.locator(".cue-pick")).toHaveText("1 option to pick from");
  await expect(chat).not.toContainText(/IF PICKED|CHECK:|Where: the Desk/);
  await expect(chat.locator(".dk-sg, .dk-ln")).toHaveCount(0);
  await expect(chat).not.toContainText(/If you pick B|How to check/);
});

test("Up next and Done today sit side by side on the green and fold to their headings, remembered", async ({ page, cue }) => {
  await open(page, cue);
  const next = page.locator('[data-dkn-drawer="next"]'), done = page.locator('[data-dkn-drawer="done"]');
  const [a, b] = await Promise.all([next.boundingBox(), done.boundingBox()]);
  expect(Math.round(a.y)).toBe(Math.round(b.y));
  expect(a.x + a.width).toBeLessThan(b.x);
  // On the green: no card under them.
  expect(await next.evaluate((el) => getComputedStyle(el.closest(".dkn-low")).backgroundColor)).toBe("rgba(0, 0, 0, 0)");
  await expect(next.locator(".dkn-nx")).toHaveCount(2);
  await expect(next.locator(".dkn-dh")).toContainText("drag to reorder");
  const head = next.locator("[data-desk-c-fold]");
  await head.click();
  await expect(head).toHaveAttribute("aria-expanded", "false");
  // Folded, its rows stay in the page (so it can slide open again) but out of sight.
  await expect(next.locator(".dkn-nx").first()).toBeHidden();
  await expect(next.locator(".dkn-nx").nth(1)).toBeHidden();
  await page.reload();
  await page.locator('#app [data-view="board"]').waitFor();
  await openDesk(page).catch(() => {});
  await expect(page.locator('[data-dkn-drawer="next"] [data-desk-c-fold]')).toHaveAttribute("aria-expanded", "false");
  await page.locator('[data-dkn-drawer="next"] [data-desk-c-fold]').click();
  await expect(page.locator('[data-dkn-drawer="next"] .dkn-nx')).toHaveCount(2);
  await expect(page.locator('[data-dkn-drawer="next"] .dkn-nx').first()).toBeVisible();
});

test("a laptop's window: the sessions a strip, Items never under 520; the smallest window fits", async ({ page, cue }) => {
  await open(page, cue, { width: 1440, height: 900 });
  const w = (q) => page.locator(q).evaluate((el) => Math.round(el.getBoundingClientRect().width));
  expect(await w(".dk-c > .dkn-side")).toBe(64);
  expect(await w(".dk-c > .dkn-items")).toBeGreaterThanOrEqual(520);
  await expect(page.locator(".dkn-side .dkn-head")).toBeHidden();
  await page.setViewportSize({ width: 900, height: 600 });
  await expect.poll(() => w(".dk-c > .dkn-side")).toBe(64);
  // The cards redraw once they've measured themselves after a resize.
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(900);
  // Narrow, the strip goes on its own line under the title, Now under it.
  const box = (q) => page.locator(`.dkn-row[data-desk-item="d1"] ${q}`).boundingBox();
  const [t, k, n] = await Promise.all([box(".dkn-t"), box(".dkn-ks"), box(".dkn-now")]);
  expect(k.y).toBeGreaterThanOrEqual(t.y + t.height);
  expect(n.y).toBeGreaterThanOrEqual(k.y + k.height);
});

test("progress: a strip on every row, no switch; each piece as long as its step took, its time under it", async ({ page, cue }) => {
  await open(page, cue);
  await stampSteps(page);
  // Strip is the one view: no Strip · Line switch, no line along a row's foot; Item, Progress and Now over the rows.
  await expect(page.locator(".dkn-ih .dkn-lsw, .dkn-ih [data-desk-c-line]")).toHaveCount(0);
  await expect(page.locator(".dkn-card .dkn-th > span")).toHaveText(["Item", "Progress", "Now"]);
  await expect(page.locator(".dkn-card .dkn-tline")).toHaveCount(0);
  const d3 = page.locator('.dkn-card .dkn-row[data-desk-item="d3"]');
  // At its tests: 2m picked up, 21m building, 12m so far at tests (bold), three stubs to come.
  await expect(d3.locator(".dkn-ks")).toHaveAttribute("aria-label", "Step 3 of 6, Tests");
  await expect(d3.locator(".dkn-kt")).toHaveText(["2m", "21m", "12m", "", "", ""]);
  const w = await d3.locator(".dkn-kp").evaluateAll((els) => els.map((el) => Math.round(el.getBoundingClientRect().width)));
  expect(w[1]).toBeGreaterThan(w[2]);
  expect(w[2]).toBeGreaterThan(w[0]);
  expect(w.slice(3)).toEqual([8, 8, 8]);
  expect(w[0]).toBeGreaterThanOrEqual(18);
  await expect(d3.locator(".dkn-kp.cur .dkn-kt")).toHaveCSS("font-weight", "700");
  // The strip's column is 180 wide on every row, so they line up.
  const xs = await page.locator(".dkn-card .dkn-row .dkn-ks").evaluateAll((els) => els.map((el) => [Math.round(el.getBoundingClientRect().x), Math.round(el.getBoundingClientRect().width)]));
  expect(new Set(xs.map(String)).size).toBe(1);
  expect(xs[0][1]).toBe(180);
  // Still the strip after a reload.
  await page.reload();
  await page.locator('#app [data-view="board"]').waitFor();
  await openDesk(page);
  await expect(page.locator(".dkn-card .dkn-ks").first()).toBeVisible();
  await expect(page.locator(".dkn-card .dkn-tline")).toHaveCount(0);
});

test("no tooltips and no stripes down a row's left edge", async ({ page, cue }) => {
  await open(page, cue);
  await stampSteps(page);
  // A stripe down the left: a left border wider than the right, or an inset shadow pushed in from the left only.
  const leftEdge = () => page.locator(".dkn-card .dkn-row").evaluateAll((els) => els.flatMap((el) => [el, ...el.querySelectorAll("*")]).filter((el) => { const c = getComputedStyle(el); return parseFloat(c.borderLeftWidth) > parseFloat(c.borderRightWidth) || /\b[1-9][\d.]*px 0px 0px (0px )?inset/.test(c.boxShadow); }).map((el) => el.className));
  expect(await leftEdge()).toEqual([]);
  await expect(page.locator(".dk-c [title]")).toHaveCount(0);
});

test("Items in bands: Working, then Waiting on you, each heading with its dot, count and a plain line, stacked as they stick", async ({ page, cue }) => {
  await open(page, cue);
  const heads = page.locator(".dkn-card .dkn-body > .dkn-band > .dkn-gh");
  // Nothing is blocked on you here, so no band for it; #d3 and #d5 at work, #d1 and #d2 wait on you.
  await expect(heads).toHaveText([/^WORKING 2\s*at work · nothing for you$/, /^WAITING ON YOU 2\s*done · ready for you to check$/]);
  await expect(page.locator(".dkn-card .dkn-band > .dkn-gh .dkn-gdot")).toHaveCount(2);
  await expect(page.locator('.dkn-card .dkn-b-working .dkn-row[data-desk-item]')).toHaveCount(2);
  await expect(page.locator('.dkn-card .dkn-b-you .dkn-row[data-desk-item]')).toHaveCount(2);
  // Each heading sticks, the next one under the ones above it (set once the list has measured itself).
  await expect.poll(() => heads.evaluateAll((els) => els.map((el) => [getComputedStyle(el).position, el.style.top === `${els[0] === el ? 0 : els[0].offsetHeight}px`]))).toEqual([["sticky", true], ["sticky", true]]);
});

test("the lead's own ask is Blocked on you, over Working, and its count takes it in", async ({ page, cue }) => {
  await open(page, cue);
  const you = page.locator('.dkn-card .dkn-gh[data-desk2b-group="you"] > span');
  await expect(you).toHaveText("2");
  await expect(page.locator('.dkn-card .dkn-gh[data-desk2b-group="blocked"]')).toHaveCount(0);
  // The lead asks something of its own (not a crew member's ask).
  await cue.change((st) => {
    const a = st.items.find((i) => i.id === "n-focus-ask");
    st.items.push({ ...a, id: "n-lead-ask", agent_id: "", agent_name: "", tool_input: { questions: [{ question: "Ship the phrases today?", header: "Ship", multiSelect: false, options: [{ label: "Yes" }, { label: "No" }] }] } });
  });
  const row = page.locator(".dkn-card .dkn-b-blocked .dkn-row[data-desk-c-lead]");
  await expect(row).toContainText("web-lead");
  await expect(page.locator(".dkn-card .dkn-body > .dkn-band > .dkn-gh")).toHaveText([/^BLOCKED ON YOU 1\s*nothing moves till you answer$/, /^WORKING 2/, /^WAITING ON YOU 2/]);
  await expect(you).toHaveText("2");
});

test("Up next says in full why each item waits", async ({ page, cue }) => {
  await open(page, cue);
  await cue.change((st) => {
    const its = st.desk.projects.find((d) => d.project === "cue").items;
    const d4 = its.find((i) => i.id === "d4");
    // Nobody has them yet (#776: the session is picked as an item starts).
    const { holder, after, ...rest } = d4;
    const add = (id, title, more) => its.push({ ...rest, id, title, ...more });
    add("u1", "After the search fix", { after: "d3" });
    add("u2", "Parked for later", { state: "held" });
    add("u3", "Anyone can take this", {});
  });
  const line = (id, drawer = "next") => page.locator(`[data-dkn-drawer="${drawer}"] .dkn-nx[data-desk-item="${id}"] .dkn-a2`);
  await expect(line("u1")).toContainText("Waits for #d3 to land first");
  // A parked one is in Parked, under Up next (147f6e329e); opened, it says how it starts.
  await page.locator('[data-dkn-drawer="parked"] [data-desk-c-fold]').click();
  await expect(line("u2", "parked")).toBeVisible();
  await expect(line("u2", "parked")).toContainText("Parked: press Start to begin it");
  await expect(line("u3")).toContainText("Waits for a free session");
});

test("the lead's block brings its chat back from All sessions", async ({ page, cue }) => {
  await open(page, cue);
  await page.locator(".dkn-side [data-desk2b-sessions]").click();
  await expect(page.locator(".dk-c .dk-sessions")).toHaveCount(1);
  await page.locator(".dkn-side .dkn-lead").click();
  await expect(page.locator(".dk-c .dk-sessions")).toHaveCount(0);
  await expect(page.locator(".dk-c > .dk2b-chat")).toHaveCount(1);
  await expect(page.locator(".dkn-side .dkn-lead")).toHaveClass(/\bon\b/);
});

test("switch off: dragging the grip with All sessions open moves from its own width", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1800, height: 1095 });
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  const card = page.locator(".board.dk-b2 > .dk-card.dk-sessions");
  const w0 = await card.evaluate((el) => Math.round(el.getBoundingClientRect().width));
  const g = await page.locator('.board.dk-b2 [data-col-grip="items"]').boundingBox();
  const x = g.x + g.width / 2, y = g.y + g.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 20, y, { steps: 4 });
  await page.mouse.move(x + 40, y, { steps: 4 });
  await page.mouse.up();
  await expect.poll(() => card.evaluate((el) => Math.round(el.getBoundingClientRect().width))).toBe(w0 + 40);
});
