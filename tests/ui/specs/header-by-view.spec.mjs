// The header's area after + New and the waiting count follows the view (#666, app/head_view.js): Board and Read the
// projects; Desk the Desks (chips, and a Desks menu with New Desk… and Desk settings…); an add-on's
// view what its head() gives, else nothing.
import { test, expect, openDesk } from "./cue.mjs";

const area = (page) => page.locator(".top .hchips");
const goView = (page, v) => page.locator(`.switch-view [data-view="${v}"]`).click();

test("Board and Read show the projects with no label before them; the menu still has All projects", async ({ cue, page }) => {
  await cue.open("many-sessions");
  for (const v of ["board", "read"]) {
    await goView(page, v);
    await expect(area(page).locator(".hv-lbl")).toHaveCount(0);
    await expect(area(page).locator('[data-pj="toggle"]')).toHaveAttribute("aria-label", /^Projects, /);
    await expect(area(page).locator('[data-pj="toggle"]')).toHaveCount(1);
  }
  await goView(page, "board");
  await page.locator('[data-pj="toggle"]').click();
  await expect(page.locator('.pj-pop [data-pj="pick"][data-project=""]')).toContainText("All projects");
});

test("Desk: the Desks as chips, no projects pill; a chip switches Desk; the open one brings its lead's chat back", async ({ cue, page }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(area(page).locator(".hv-lbl")).toHaveCount(0);
  await expect(area(page).locator('[data-hv-menu="desks"]')).toHaveAttribute("aria-label", /^Desks/);
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

test("the Desks menu: a long Desk name stops before its count, fades at its end and slides only while its row is lit (#240)", async ({ cue, page }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "website").name = "Website launch checklist and the whole pricing page redo, start to end"; });
  await page.locator('.top [data-hv-menu="desks"]').click();
  const menu = page.locator(".top .hv-menu"), long = menu.locator('.hv-row[data-hv-desk="website"]');
  // The menu doesn't grow to the name: it stops at 360px.
  expect((await menu.boundingBox()).width).toBeLessThanOrEqual(360);
  const fit = await long.evaluate((r) => {
    const nm = r.querySelector(".hv-rn"), rs = r.querySelector(".hv-rs");
    return { right: nm.getBoundingClientRect().right, rs: rs.getBoundingClientRect().left, rsFits: rs.scrollWidth <= rs.clientWidth, over: nm.firstElementChild.scrollWidth > nm.clientWidth, mask: getComputedStyle(nm).maskImage || getComputedStyle(nm).webkitMaskImage };
  });
  expect(fit.right).toBeLessThanOrEqual(fit.rs);   // never over the count
  expect(fit.rsFits).toBe(true);                   // and the count is never squeezed
  expect(fit.over).toBe(true);
  expect(fit.mask).toContain("gradient");
  // (Tests run with animations off: the slide's rule is there, and it takes the name only while its row is lit.)
  expect(await page.evaluate(() => [...document.styleSheets].some((sh) => { try { return [...sh.cssRules].some((r) => r.cssText?.includes("hv-rn-slide") && r.selectorText?.includes(".at")); } catch { return false; } }))).toBe(true);
  const slides = () => long.locator(".hv-rn > span").evaluate((el) => el.matches(".hv-row:is(.at, :hover) .hv-rn > span"));
  await page.mouse.move(0, 0);
  expect(await slides()).toBe(await long.evaluate((r) => r.classList.contains("at")));
  await long.hover();
  expect(await slides()).toBe(true);
  expect(await menu.locator("[title]").count()).toBe(0);
});

test("an add-on's view shows what its head() gives there, and nothing when it has none", async ({ cue, page }) => {
  await cue.open("many-sessions");
  await page.evaluate(() => {
    addOn.views.push({ id: "loops", label: () => "Loops", html: () => `<div class="board">loops page</div>`, head: () => `<button class="hchip">All loops 4</button>` });
    addOn.views.push({ id: "plain", label: () => "Plain", html: () => `<div class="board">plain page</div>` });
    renderMain();
  });
  await goView(page, "loops");
  await expect(area(page).locator(".hv-area")).toHaveText("All loops 4");
  await expect(area(page).locator('[data-pj="toggle"]')).toHaveCount(0);
  await goView(page, "plain");
  await expect(area(page).locator(".hv-area, .hv-lbl, [data-pj]")).toHaveCount(0);
  await expect(area(page).locator('[data-chip-list="waiting"]')).toHaveCount(1);
});

test("Desk chips: the Desks menu first, then the pinned Desks in the order pinned; opening one never moves them", async ({ cue, page }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const base = st.desk.projects.find((x) => x.project === "website");
    for (const p of ["alpha", "beta", "gamma"]) st.desk.projects.push({ ...structuredClone(base), id: p, project: p, name: p, items: [], decisions: [], waits: [], circles: [], checks: [], kept: [] });
  });
  await openDesk(page);
  const ids = () => area(page).locator(".hv-chip[data-hv-desk]").evaluateAll((els) => els.map((e) => e.dataset.hvDesk));
  // Nothing pinned yet: the first three Desks by name, after the Desks menu.
  await expect(area(page).locator(".hv-chip").first()).toHaveAttribute("data-hv-menu", "desks");
  expect(await ids()).toEqual(["alpha", "beta", "cue"]);
  // Opening Desks, by chip or from the menu, leaves the chips where they are.
  await area(page).locator('.hv-chip[data-hv-desk="beta"]').click();
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("beta");
  expect(await ids()).toEqual(["alpha", "beta", "cue"]);
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator('.top .hv-menu [data-hv-desk="gamma"]').click();
  await expect.poll(() => page.evaluate(() => deskSelection())).toBe("gamma");
  expect(await ids()).toEqual(["alpha", "beta", "cue"]);
  // gamma isn't pinned: the Desks menu's chip names it, lit.
  await expect(area(page).locator('.hv-open')).toContainText("gamma");
  await expect(area(page).locator('.hv-open')).toHaveClass(/\bhot\b/);
  // Its pin in the menu puts it last among the chips; alpha's takes alpha off. The menu stays open.
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator('.top .hv-menu [data-hv-pin="gamma"]').click();
  await page.locator('.top .hv-menu [data-hv-pin="alpha"]').click();
  await expect(page.locator(".top .hv-menu")).toHaveCount(1);
  await expect(page.locator('.top .hv-menu [data-hv-pin="gamma"]')).toHaveAttribute("aria-pressed", "true");
  expect(await ids()).toEqual(["beta", "cue", "gamma"]);
  await expect(area(page).locator('.hv-open')).toHaveText("Desks");
  // Kept across a reload.
  await expect.poll(() => page.evaluate(() => localStorage.getItem("cue.deskPins"))).toBe(JSON.stringify(["beta", "cue", "gamma"]));
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
