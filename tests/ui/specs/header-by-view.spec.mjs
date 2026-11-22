// The header's area after + New and the waiting count follows the view (#666, app/head_view.js): Board and Read the
// projects; Desk the Desks (chips, and a Desks menu with New Desk… and Desk settings…); Inbox a Show filter; an add-on's
// view what its head() gives, else nothing.
import { test, expect, openDesk } from "./cue.mjs";

const area = (page) => page.locator(".top .hchips");
const goView = (page, v) => page.locator(`.switch-view [data-view="${v}"]`).click();

test("Board and Read show the projects under a Projects label; the menu still has All projects", async ({ cue, page }) => {
  await cue.open("many-sessions");
  for (const v of ["board", "read"]) {
    await goView(page, v);
    await expect(area(page).locator(".hv-lbl")).toHaveText("Projects");
    await expect(area(page).locator('[data-pj="toggle"]')).toHaveCount(1);
  }
  await goView(page, "board");
  await page.locator('[data-pj="toggle"]').click();
  await expect(page.locator('.pj-pop [data-pj="pick"][data-project=""]')).toContainText("All projects");
});

test("Desk: the Desks as chips, no projects pill; a chip switches Desk; the open one brings its lead's chat back", async ({ cue, page }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(area(page).locator(".hv-lbl")).toHaveText("Desks");
  await expect(area(page).locator('[data-pj="toggle"]')).toHaveCount(0);
  const chips = area(page).locator(".hv-chip[data-hv-desk]");
  await expect(chips).toHaveCount(2);
  await expect(chips.first()).toHaveAttribute("data-hv-desk", "cue");
  await expect(chips.first()).toHaveClass(/\bhot\b/);
  // Another Desk's chip opens it.
  await area(page).locator('.hv-chip[data-hv-desk="website"]').click();
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("website");
  await expect(area(page).locator('.hv-chip[data-hv-desk="website"]')).toHaveClass(/\bhot\b/);
  // Back on cue, an item open: its chip brings the lead's chat back.
  await area(page).locator('.hv-chip[data-hv-desk="cue"]').click();
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("cue");
  await page.locator(".dk-items .dk-it:visible").first().click();
  await expect.poll(() => page.evaluate(() => deskOpen?.kind)).toBe("item");
  await area(page).locator('.hv-chip[data-hv-desk="cue"]').click();
  await expect.poll(() => page.evaluate(() => [deskOpen, deskTab, deskSelection()])).toEqual([null, "lead", "cue"]);
});

test("Desk: a dot on a Desk that waits on you; the Desks menu lists every Desk with what it has going, New Desk… and Desk settings…", async ({ cue, page }) => {
  await cue.open("desk");
  await openDesk(page);
  const going = await page.evaluate(() => deskSorted().map((d) => [d.project, hvGoingText(hvDeskGoing(d))]));
  await page.locator('.top [data-hv-menu="desks"]').click();
  const rows = page.locator(".top .hv-menu .hv-row");
  for (const [p, text] of going) {
    const row = page.locator(`.top .hv-menu [data-hv-desk="${p}"]`);
    await expect(row).toContainText(text);
    await expect(row.locator(".hv-dot.ask")).toHaveCount(/wait/.test(text) ? 1 : 0);
    await expect(area(page).locator(`.hv-chip[data-hv-desk="${p}"] .hv-dot.ask`)).toHaveCount(/wait/.test(text) ? 1 : 0);
  }
  expect(going.some(([, t]) => /waits? on you/.test(t)), "a Desk in the scenario waits on you").toBe(true);
  await expect(rows.nth(going.length)).toHaveText("All Desks");
  await expect(rows.nth(going.length + 1)).toHaveText("New Desk…");
  await expect(rows.nth(going.length + 2)).toHaveText("Desk settings…");
  await expect(page.locator('.top .hv-menu .hv-row.on')).toHaveAttribute("data-hv-desk", "cue");
  // Esc closes it; a click elsewhere too.
  await page.keyboard.press("Escape");
  await expect(page.locator(".top .hv-menu")).toHaveCount(0);
  // A Desk picked in the menu opens it, and the menu closes.
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator('.top .hv-menu [data-hv-desk="website"]').click();
  await expect(page.locator(".top .hv-menu")).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("website");
  // All Desks opens the page of every Desk; a Desk picked there opens it again.
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator(".top .hv-menu [data-hv-desk-home]").click();
  await expect(page.locator(".dk-desks .dk-desk").first()).toBeVisible();
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("");
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator('.top .hv-menu [data-hv-desk="website"]').click();
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("website");
  // New Desk… opens the New Desk sheet.
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator(".top .hv-menu [data-hv-new-desk]").click();
  await expect(page.locator(".nd")).toBeVisible();
  await page.locator(".nd .nd-x").click();
  await expect(page.locator(".nd")).toHaveCount(0);
  // Desk settings… opens that Desk's settings (a dialog): the Desk page's own when it has them (deskSettingsOpen, how
  // its work lands), else Desk settings' drawer (desk/v2.js).
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator(".top .hv-menu [data-hv-desk-set]").click();
  await expect(page.locator(".ld-dialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator(".ld-dialog")).toHaveCount(0);
  await page.evaluate(() => { window.__set = null; window.deskSettingsOpen = (p) => { window.__set = p; }; });
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator(".top .hv-menu [data-hv-desk-set]").click();
  await expect.poll(() => page.evaluate(() => window.__set)).toBe("website");
});

test("Inbox: Show filters the list to one project, each with how many need you; All shows every one", async ({ cue, page }) => {
  await cue.open("desk-inbox");
  await goView(page, "inbox");
  await expect(area(page).locator(".hv-lbl")).toHaveText("Show");
  await expect(area(page).locator('[data-pj="toggle"]')).toHaveCount(0);
  const by = await page.evaluate(() => inboxCountsBy());
  expect(by.length, "the scenario's asks are in more than one project").toBeGreaterThan(1);
  const total = await page.evaluate(() => inboxCountAll());
  const all = area(page).locator('[data-hv-inbox=""]');
  await expect(all).toHaveClass(/\bon\b/);
  await expect(all.locator("em")).toHaveText(String(total));
  await expect(page.locator('.switch-view [data-view="inbox"] .ib-count')).toHaveText(String(total));
  const [p, n] = by[0];
  await expect(area(page).locator(`[data-hv-inbox="${p}"] em`)).toHaveText(String(n));
  const rowsProjects = () => page.evaluate(() => [...document.querySelectorAll(".inbox .ib-row[data-inbox-open]")].map((el) => sessionOf(el.dataset.inboxOpen)?.project));
  expect(new Set(await rowsProjects()).size).toBeGreaterThan(1);
  await area(page).locator(`[data-hv-inbox="${p}"]`).click();
  await expect(area(page).locator(`[data-hv-inbox="${p}"]`)).toHaveClass(/\bon\b/);
  await expect.poll(async () => [...new Set(await rowsProjects())]).toEqual([p]);
  // The switch's count stays every project's; the Board's projects aren't touched.
  await expect(page.locator('.switch-view [data-view="inbox"] .ib-count')).toHaveText(String(total));
  expect(await page.evaluate(() => projFilter)).toBe("");
  await all.click();
  await expect.poll(async () => new Set(await rowsProjects()).size).toBeGreaterThan(1);
});

test("Inbox: past four projects the rest are under More, with their counts; one picked there shows as its own part", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await goView(page, "inbox");
  const by = await page.evaluate(() => inboxCountsBy());
  expect(by.length, "the scenario has asks in more than four projects").toBeGreaterThan(4);
  await expect(area(page).locator(".hv-segs > [data-hv-inbox]")).toHaveCount(5);   // All and four
  await area(page).locator('[data-hv-menu="inbox"]').click();
  const rest = by.slice(4);
  const rows = page.locator(".top .hv-menu [data-hv-inbox]");
  expect(await rows.evaluateAll((els) => els.map((el) => [el.dataset.hvInbox, +el.querySelector(".hv-rs").textContent]))).toEqual(rest);
  await rows.first().click();
  await expect(page.locator(".top .hv-menu")).toHaveCount(0);
  await expect(area(page).locator(`.hv-segs > [data-hv-inbox="${rest[0][0]}"]`)).toHaveClass(/\bon\b/);
  expect(await page.evaluate(() => inboxProj)).toBe(rest[0][0]);
});

test("an add-on's view shows what its head() gives there, and nothing when it has none", async ({ cue, page }) => {
  await cue.open("many-sessions");
  await page.evaluate(() => {
    addOn.views.push({ id: "loops", label: () => "Loops", html: () => `<div class="board">loops page</div>`, head: () => `${hvLabel("Loops")}<button class="hchip">All loops 4</button>` });
    addOn.views.push({ id: "plain", label: () => "Plain", html: () => `<div class="board">plain page</div>` });
    renderMain();
  });
  await goView(page, "loops");
  await expect(area(page).locator(".hv-area")).toContainText("LoopsAll loops 4");
  await expect(area(page).locator('[data-pj="toggle"]')).toHaveCount(0);
  await goView(page, "plain");
  await expect(area(page).locator(".hv-area, .hv-lbl, [data-pj]")).toHaveCount(0);
  await expect(area(page).locator('[data-chip-list="waiting"]')).toHaveCount(1);
});

test("Desk chips: after the open one and those waiting on you, the Desks opened last, newest first", async ({ cue, page }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const base = st.desk.projects.find((x) => x.project === "website");
    for (const p of ["alpha", "beta", "gamma"]) st.desk.projects.push({ ...structuredClone(base), id: p, project: p, name: p, items: [], decisions: [], waits: [], circles: [], checks: [], kept: [] });
  });
  await openDesk(page);
  const waiting = await page.evaluate(() => deskSorted().filter((d) => hvDeskGoing(d).waits).map((d) => d.project));
  expect(waiting.length, "at most one Desk waits on you, so the Desks opened last decide the rest").toBeLessThanOrEqual(1);
  for (const p of ["gamma", "alpha", "beta"]) {
    await page.locator('.top [data-hv-menu="desks"]').click();
    await page.locator(`.top .hv-menu [data-hv-desk="${p}"]`).click();
    await expect.poll(() => page.evaluate(() => deskSelection())).toBe(p);
  }
  // Now on beta, alpha and gamma opened before it.
  const want = [...new Set(["beta", ...waiting, "alpha", "gamma"])].slice(0, 3);
  await expect(area(page).locator(".hv-chip[data-hv-desk]")).toHaveCount(3);
  expect(await area(page).locator(".hv-chip[data-hv-desk]").evaluateAll((els) => els.map((e) => e.dataset.hvDesk))).toEqual(want);
});

test("the Desks menu: open on a list row's click, that row opens and the menu closes; left open on another view, it takes no keys", async ({ cue, page }) => {
  await cue.open("desk");
  await openDesk(page);
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.locator('.top [data-hv-menu="desks"]').click();
  await expect(page.locator(".top .hv-menu")).toHaveCount(1);
  // A click on the Desk's list: the menu closes and the item opens, in the one click.
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  await expect(page.locator(".top .hv-menu")).toHaveCount(0);
  await expect(page.locator(".dk-card.dk-right .dk-ph")).toContainText("#d5");
  // Enter on a find that matches no Desk: nothing opens, nothing breaks.
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator(".top .hv-find").fill("no desk is called this");
  await expect(page.locator(".top .hv-menu .hv-none")).toHaveText("No Desk matches.");
  await page.keyboard.press("Enter");
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("cue");
  // Left open, then another view: no menu there, and its keys aren't the menu's.
  await page.evaluate(() => { hvMenuSet("desks"); view = "board"; renderMain(); });
  await expect(page.locator(".top .hv-menu")).toHaveCount(0);
  expect(await page.evaluate(() => headViewKey(new KeyboardEvent("keydown", { key: "ArrowDown" })))).toBe(false);
  expect(await page.evaluate(() => hvMenu)).toBe("");
  expect(errors).toEqual([]);
});
