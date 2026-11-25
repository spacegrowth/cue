// Each Desk on the Board as its lead's card (#825, option B of #823): while anything on a Desk waits on you, one card
// in Waiting with its counts, the ask with its buttons and the items to check; with nothing waiting, one line under
// Sessions. Its sessions get no cards of their own on the Board.
import { test, expect } from "./cue.mjs";

const T = 1_800_000_000_000;
const board = (page) => page.locator('.switch-view [data-view="board"]').click();
const waiting = (page) => page.locator(".sec-waiting .sec-body");
const sessions = (page) => page.locator(".sec-sessions .sec-body");
const deskCard = (page, p) => waiting(page).locator(`.dkw[data-dkw="${p}"]`);

/** The cue Desk's host session asks to run a command, for item d5. */
const hostAsks = (cue) => cue.change((st, T) => {
  st.items.push({ id: "perm", session_id: "host", project: "cue", kind: "permission", status: "pending", tool_name: "Bash", tool_input: { command: "cargo test --lib desk" }, suggestions: [], created_ms: T });
  const d5 = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5");
  Object.assign(d5, { state: "needs_you", ask: { item_id: "perm", sid: "host", kind: "permission", text: "cargo test --lib desk" } });
}, T);

test("each Desk with something waiting is one card in Waiting: its lead, its counts, the ask and the items to check (#825)", async ({ page, cue }) => {
  await cue.open("desk");
  await hostAsks(cue);
  await board(page);
  const card = deskCard(page, "cue");
  await expect(card).toHaveCount(1);
  await expect(card.locator(".nrow-who .proj")).toHaveText("web-lead");
  await expect(card.locator(".dkw-tag")).toHaveText(/Desk · /);
  await expect(card.locator(".dkw-tag svg.dk-kic")).toHaveCount(1);
  await expect(card.locator(".dkw-cnt")).toContainText("to check");
  // The newest ask first, with who asks, for which item, and its buttons.
  const ask = card.locator(".dkw-ask");
  await expect(ask).toHaveCount(1);
  await expect(ask.locator(".dkw-from")).toContainText("host-lead");
  await expect(ask.locator(".dkw-from")).toContainText("#d5");
  await expect(ask.locator('[data-act="allow"]')).toBeVisible();
  await expect(ask.locator('[data-act="deny"]')).toBeVisible();
  // Then the lead's question and d2 to check, a line each.
  const rows = card.locator(".dkw-ir");
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(0)).toContainText("web-lead");
  await expect(rows.nth(1)).toContainText("#d2");
  await expect(rows.nth(1)).toContainText("to check");
  await expect(card.locator(".dkw-ih")).toHaveText(/Waits on you · 3/i);
  // The website Desk has only something to check: its own card, no ask in it.
  await expect(deskCard(page, "website")).toHaveCount(1);
  await expect(deskCard(page, "website").locator(".dkw-ask")).toHaveCount(0);
  await expect(deskCard(page, "website").locator(".dkw-ir")).toContainText("#w1");
  // No Desk session has a card of its own anywhere on the Board's columns.
  for (const sid of ["host", "lead", "site"]) {
    await expect(sessions(page).locator(`[data-session="${sid}"]`)).toHaveCount(0);
  }
  await expect(waiting(page).locator('.nrow[data-big="perm"], .nrow[data-big="n-focus-ask"]')).toHaveCount(0);
  await expect(page.locator("[title]", { has: card })).toHaveCount(0);
});

test("the ask answers from inside the Desk's card; the card's other parts open the Desk (#825)", async ({ page, cue }) => {
  await cue.open("desk");
  await hostAsks(cue);
  await board(page);
  const card = deskCard(page, "cue");
  await card.locator('.dkw-ask [data-act="allow"]').click();
  await expect.poll(() => cue.calls("respond"), { timeout: 1500 }).toHaveLength(1);
  expect((await cue.calls("respond"))[0].args).toEqual({ id: "perm", decision: { behavior: "allow" } });
  await expect(page.locator('.switch-view [data-view="board"]')).toHaveClass(/on/);   // answered here, still on the Board
  // An item's line opens it on its Desk.
  await card.locator(".dkw-ir", { hasText: "#d2" }).click();
  await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/on/);
});

test("a click on the Desk's card opens its lead's chat on the Board, ready to type; its answers and item links do their own (#850)", async ({ page, cue }) => {
  await cue.open("desk");
  await hostAsks(cue);
  await board(page);
  const card = deskCard(page, "cue");
  const onDesk = page.locator('.switch-view [data-view="desk"]');
  // A hand over the card, its item links and its rows; no underline on an item's name.
  for (const el of [card, card.locator(".dkw-cnt"), card.locator(".dkw-for"), card.locator(".dkw-ir").nth(1)]) expect(await el.evaluate((e) => getComputedStyle(e).cursor)).toBe("pointer");
  await card.locator(".dkw-ir").nth(1).hover();
  expect(await card.locator(".dkw-ir").nth(1).locator(".dkw-tt").evaluate((e) => getComputedStyle(e).textDecorationLine)).toBe("none");
  await card.locator(".dkw-for").hover();
  expect(await card.locator(".dkw-for").evaluate((e) => getComputedStyle(e).textDecorationLine)).toBe("none");
  // An item link opens that item, not the lead's chat.
  await card.locator(".dkw-ir", { hasText: "#d2" }).click();
  await expect(onDesk).toHaveClass(/on/);
  expect(await page.evaluate(() => deskOpen?.kind === "item" && deskOpen.id)).toBe("d2");
  // Back on the Board, Allow answers in place.
  await board(page);
  await card.locator('.dkw-ask [data-act="allow"]').click();
  await expect.poll(() => cue.calls("respond"), { timeout: 1500 }).toHaveLength(1);
  await expect(page.locator('.switch-view [data-view="board"]')).toHaveClass(/on/);
  // The card itself (its head, its counts) opens its lead's chat in the Board's chat pane, as a session's card does,
  // its box ready; the Board stays.
  for (const part of [".nrow-who .proj", ".dkw-cnt"]) {
    await board(page);
    await page.evaluate(() => { active = null; });
    await card.locator(part).click();
    await expect(page.locator('.switch-view [data-view="board"]')).toHaveClass(/on/);
    expect(await page.evaluate(() => active?.sid)).toBe("lead");
    await expect.poll(() => page.evaluate(() => document.activeElement?.matches("textarea[data-text]") || false)).toBe(true);
  }
});

test("the Desk's card is picked out in place while its lead's chat is open, and lets go when another card opens (#918)", async ({ page, cue }) => {
  await cue.open("desk");
  await hostAsks(cue);
  // A session of a project with no Desk, so it has a card of its own on the Board.
  await cue.change((st) => {
    const site = st.sessions.find((x) => x.session_id === "site");
    st.sessions.push({ ...site, session_id: "solo", project: "solo", name: "solo", state: "idle", thread: [] });
  });
  await board(page);
  const card = deskCard(page, "cue");
  const other = page.locator('.sec-body .nrow[data-session="solo"], .sec-body [data-session="solo"]').first();
  await other.click();
  await expect(other).toHaveClass(/(^| )on( |$)/);
  await expect(card).not.toHaveClass(/(^| )on( |$)/);
  const at = () => card.evaluate((c) => [...c.parentElement.children].indexOf(c));
  const before = await at();
  const fill = () => card.evaluate((c) => getComputedStyle(c).backgroundColor);
  const plain = await fill();
  await card.locator(".nrow-who .proj").click();
  await expect(card).toHaveClass(/(^| )on( |$)/);
  expect(await fill()).not.toBe(plain);
  expect(await at()).toBe(before);
  await expect(other).not.toHaveClass(/(^| )on( |$)/);
  // Another card opened: the highlight moves to it.
  await other.click();
  await expect(card).not.toHaveClass(/(^| )on( |$)/);
  await expect(other).toHaveClass(/(^| )on( |$)/);
});

test("a Desk with nothing waiting is one line under Sessions that opens it; it comes back as a card when something waits (#825)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    st.items = st.items.filter((i) => i.session_id !== "lead");
    for (const d of st.desk.projects) for (const i of d.items) if (i.state === "needs_you" || i.state === "to_check") { i.state = "working"; i.ask = null; }
  });
  await board(page);
  await expect(waiting(page).locator(".dkw")).toHaveCount(0);
  const line = sessions(page).locator('.dkw-line[data-desk-hand="cue"]');
  await expect(line).toHaveCount(1);
  await expect(line.locator(".dkw-nm")).toHaveText("web-lead");
  await expect(line).toContainText("at work");
  await expect(sessions(page).locator(".dkw-sub")).toContainText("DESKS");
  for (const sid of ["host", "lead", "site"]) await expect(sessions(page).locator(`[data-session="${sid}"]`)).toHaveCount(0);
  await hostAsks(cue);
  await expect(line).toHaveCount(0);
  await expect(deskCard(page, "cue")).toHaveCount(1);
  await cue.change((st) => { st.items = st.items.filter((i) => i.id !== "perm"); const d5 = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5"); d5.state = "working"; d5.ask = null; });
  await expect(line).toHaveCount(1);
  await line.click();
  await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/on/);
  await expect(page.locator('[data-desk-chat="cue"]').first()).toBeVisible();
});

test("more than three things waiting: the rest are one line that opens the Desk (#825)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { for (const i of st.desk.projects.find((d) => d.project === "cue").items) if (["d3", "d4", "d5", "d6"].includes(i.id)) i.state = "to_check"; });
  await board(page);
  const card = deskCard(page, "cue");
  await expect(card.locator(".dkw-ir")).toHaveCount(3);
  await expect(card.locator(".dkw-more")).toHaveText(/^\+ \d+ more · Open the Desk ›$/);
  // A hand over it, never an underline, in either look (#850).
  for (const scheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: scheme });
    for (const el of [card.locator(".dkw-more"), card.locator(".dkw-ir").first()]) {
      await el.hover();
      expect(await el.evaluate((e) => [e, ...e.querySelectorAll("*")].map((x) => getComputedStyle(x).textDecorationLine).filter((t) => t !== "none"))).toEqual([]);
      expect(await el.evaluate((e) => getComputedStyle(e).cursor)).toBe("pointer");
    }
  }
  await card.locator(".dkw-more").click();
  await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/on/);
});

for (const [label, size, scheme] of [["laptop, light", { width: 1440, height: 900 }, "light"], ["smallest window, dark", { width: 900, height: 600 }, "dark"]]) {
  test(`the Desk's card fits its column: ${label} (#825)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize(size);
    await cue.open("desk");
    await hostAsks(cue);
    await board(page);
    const card = deskCard(page, "cue");
    await expect(card).toBeVisible();
    // Nothing in it runs past its edge, and its buttons can be pressed.
    const over = await card.evaluate((el) => [...el.querySelectorAll("*")].filter((c) => c.getBoundingClientRect().right > el.getBoundingClientRect().right + 1).map((c) => c.className));
    expect(over).toEqual([]);
    const allow = card.locator('.dkw-ask [data-act="allow"]');
    await expect(allow).toBeVisible();
    expect((await allow.boundingBox()).width).toBeGreaterThan(40);
    // The ask stands apart from the card in both looks.
    const [cardBg, askBg] = await Promise.all([card.evaluate((el) => getComputedStyle(el).backgroundColor), card.locator(".dkw-ask").evaluate((el) => getComputedStyle(el).backgroundColor)]);
    expect(askBg).not.toBe(cardBg);
    await page.screenshot({ path: `${process.env.CUE_SHOTS || "/tmp"}/desk-card-${scheme}-${size.width}.png` });
  });
}

test("a starred Desk session keeps the Desk's mark before its name, once (#156)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st, T) => { for (const sid of ["host", "site"]) st.sessions.find((s) => s.session_id === sid).starred_ms = T; }, T);
  await page.evaluate(() => { try { localStorage.setItem("cue.drawer", "starred"); } catch {} });
  await board(page);
  const star = (name) => page.locator(".sec-starred [data-session]").filter({ has: page.locator(".tl-name", { hasText: new RegExp(`^${name}$`) }) });
  await expect(star("host-lead").locator(".dk-name-ic svg.dk-kic")).toHaveCount(1);
  await expect(star("site-lead").locator(".dk-name-ic")).toHaveCount(1);
  for (const el of await page.locator(".tl-top, .card-head, .nrow-top").all()) expect(await el.locator(":scope > .dk-name-ic, :scope > * > .dk-name-ic").count()).toBeLessThanOrEqual(1);
  await cue.change((st) => { for (const d of st.desk.projects) { for (const i of d.items) if (i.holder?.sid === "site") i.state = "done"; if (d.lead?.sid === "site") d.lead = null; } });
  await expect(star("site-lead").locator(".dk-name-ic")).toHaveCount(1);
  await cue.change((st) => { for (const d of st.desk.projects) d.members = (d.members || []).filter((sid) => sid !== "site"); });
  await expect(star("site-lead").locator(".dk-name-ic")).toHaveCount(0);
  // Opened, its chat's title has it before the name.
  await star("host-lead").click();
  const title = page.locator(".proj", { hasText: /^host-lead$/ }).last();
  await expect(title.locator("xpath=preceding-sibling::*[1]")).toHaveClass(/dk-name-ic/);
  expect(await page.evaluate(() => deskSessMark("nobody"))).toBe("");
});
