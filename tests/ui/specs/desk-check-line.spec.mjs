// Items to check say where they are, in one plain line (#436): in the app since its install with what to try, waiting
// for the install, or mocks with the canvas to open. On the Desk row and the item page; it changes by itself as
// the install finishes.
import { test, expect, openDesk } from "./cue.mjs";

const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
const patchCheck = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").checks.find((c) => c.id === k), p); }, [id, patch]);
const clock = (page, ms) => page.evaluate((t) => clockAt(t), ms);

test("an item installed says In the app since its install, and what to try, on its row and its page (#436)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const at = await page.evaluate(() => now() - 30 * 60000);
  await patchItem(cue, "d2", { finish: { step: "checked", at, commit: "c71e0b4" } });
  // Its session said nothing of what to try: Cue's words, its You'll see (#526).
  const since = `In the app since ${await clock(page, at)}`;
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row.locator(".dk2b-ts")).toHaveText(`${since} · Try: Quick phrases sit at the right of Send; More opens the rest. · Cue's words`);
  // Said once: no You'll see line under it as well.
  await expect(row.locator(".dk-see")).toHaveCount(0);
  await row.click();
  await expect(page.locator(".dk-panel[data-side-for] .dk-pnow .dk-ckl")).toHaveText("Try: Quick phrases sit at the right of Send; More opens the rest.");
  await expect(page.locator(".dk-panel[data-side-for] .dk-pnow .dk-cks")).toContainText(since.replace(/^In/, "in"));
  await expect(page.locator(".dk-right [title]")).toHaveCount(0);
});

test("an item not in the installed app yet says Waiting for install, and turns to In the app when the install is done (#436, #666)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Its work changed after the running Cue was installed: nothing to try yet.
  await patchCheck(cue, "d2", { edited: 1793984400000 + 60000, at: 1793984400000 + 60000 });
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  // B2's row says it short, with no You'll see line (#666): that's on its page.
  await expect(row.locator(".dk2b-ts")).toHaveText("Waiting for install");
  await expect(row.locator(".dk2b-chk")).toHaveCount(0);
  // The install finishes: the hub says so, and the line changes with nothing pressed.
  const at = await page.evaluate(() => now());
  await patchItem(cue, "d2", { finish: { step: "checked", at, commit: "c71e0b4" } });
  await expect(row.locator(".dk2b-ts")).toHaveText(`In the app since ${await clock(page, at)} · Try: Quick phrases sit at the right of Send; More opens the rest. · Cue's words`);
});

test("a mocks item says Mocks · open the canvas, the link opening it, on its row and its page (#436)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Mocks, notes or plans that changed nothing the app runs (#408), its canvas among what it made.
  await patchItem(cue, "d2", { finish: { step: "checked", at: Date.now(), commit: "c71e0b4", mocks: true }, made: ["https://claude.ai/artifact/abc"] });
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row.locator(".dk2b-ts")).toHaveText("Mocks · open the canvas");
  await expect(row.locator('.dk2b-ts a[data-href="https://claude.ai/artifact/abc"]')).toHaveText("open the canvas");
  // The link opens the canvas, not the item.
  await row.locator(".dk2b-ts a").click();
  await cue.called("open_link", { url: "https://claude.ai/artifact/abc" });
  await expect(row).not.toHaveClass(/\bon\b/);
  await row.click();
  await expect(page.locator(".dk-panel[data-side-for] .dk-pnow .dk-pickl")).toHaveText("Mocks · open the canvas");
  // Its DESIGNS link, when it gave one, is the canvas.
  await patchItem(cue, "d2", { finish: null, made: [], designs: "https://claude.ai/artifact/xyz" });
  await expect(page.locator('.dk-panel[data-side-for] .dk-pnow .dk-pickl a[data-href="https://claude.ai/artifact/xyz"]')).toHaveText("open the canvas");
});

test("an item with changes left on its branch says how many aren't landed yet, on its row and its page (#448)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await patchItem(cue, "d2", { finish: { step: "checked", at: await page.evaluate(() => now()), commit: "c71e0b4", unlanded: 2 } });
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row.locator(".dk2b-ts")).toHaveText("2 changes not landed yet · send it back so they land");
  await row.click();
  await expect(page.locator(".dk-panel .dk-pnow .dk-pword")).toHaveText("Not landed yet");
  await expect(page.locator(".dk-panel .dk-pnow .dk-psay")).toHaveText("2 changes not landed yet · send it back so they land");
});

// What to do with an item waiting on you (#526, option B): one line, "Try: …" or "Nothing to see: …", in a box at the
// top of its page's panel with where it came from, Close and Send back; the same line on its row.
const box = (page) => page.locator(".dk-panel[data-side-for] .dk-pnow .dk-pbox");
async function openChecked(page, cue, patch) {
  await cue.open("desk");
  await openDesk(page);
  const at = await page.evaluate(() => now() - 30 * 60000);
  await patchItem(cue, "d2", { finish: { step: "checked", at, commit: "c71e0b4" }, ...patch });
  return { at, since: `In the app since ${await clock(page, at)}`, row: page.locator('.dk-items [data-desk-item="d2"]') };
}

test("a Try line from its session: on its row, and in a box at the top of its panel with who said it, Close and Send back (#526)", async ({ page, cue }) => {
  const { since, row } = await openChecked(page, cue, { check: { kind: "try", text: "open any item waiting on you; the top line says what to do" } });
  await expect(row.locator(".dk2b-ts")).toHaveText(`${since} · Try: open any item waiting on you; the top line says what to do`);
  await expect(row.locator(".dk2b-ts b")).toHaveText("Try:");
  await expect(row.locator(".dk-see")).toHaveCount(0);
  await row.click();
  const b = box(page);
  await expect(b.locator(".dk-pword")).toHaveText("Ready to check");
  await expect(b.locator(".dk-ckl")).toHaveText("Try: open any item waiting on you; the top line says what to do");
  await expect(b.locator(".dk-ckl b")).toHaveText("Try:");
  await expect(b.locator(".dk-cks")).toHaveText(`web-lead said this when it finished · ${since.replace(/^In/, "in")}`);
  // Close then Send back, real buttons, right under it; no Looks right anywhere on the page, and the line said once.
  await expect(b.locator(".dk-pacts button")).toHaveText(["Close", "Send back"]);
  await expect(page.locator(".dk-pg").getByRole("button", { name: "Looks right" })).toHaveCount(0);
  await expect(page.locator(".dk-pg .dk-pickl")).toHaveCount(0);
  await expect(page.locator(".dk-right [title], .dk-pg [title]")).toHaveCount(0);
  // Close does what Looks right did.
  await b.getByRole("button", { name: "Close" }).click();
  await expect(b.locator(".dk-soon")).toContainText("Done");
});

test("a Nothing to try line from its session, on its row and in its panel's box (#526, #668)", async ({ page, cue }) => {
  const { since, row } = await openChecked(page, cue, { check: { kind: "nothing", text: "tests passed, close it" } });
  await expect(row.locator(".dk2b-ts")).toHaveText(`${since} · Nothing to try: tests passed, close it`);
  await expect(row.locator(".dk2b-ts b")).toHaveText("Nothing to try:");
  await row.click();
  const b = box(page);
  await expect(b.locator(".dk-ckl")).toHaveText("Nothing to try: tests passed, close it");
  await expect(b.locator(".dk-ckl b")).toHaveText("Nothing to try:");
  await expect(b.locator(".dk-cks")).toHaveText(`web-lead said this when it finished · ${since.replace(/^In/, "in")}`);
  await expect(b.locator(".dk-pacts button")).toHaveText(["Close", "Send back"]);
  await expect(b.locator(".dk-steps, .dk-show")).toHaveCount(0);
});

// Three steps someone new can follow (#668): Where, Do, Expect, numbered in the box; the row says the Do and where.
const steps = { kind: "try", text: "pick a machine, close and reopen", where: "Desk settings", do: "pick a machine, close and reopen", expect: "it's still picked" };

test("three steps from its session: numbered Where, Do, Expect in its box, the Do and where on its row, no Show me without a page (#668)", async ({ page, cue }) => {
  const { since, row } = await openChecked(page, cue, { check: steps });
  await expect(row.locator(".dk2b-ts")).toHaveText(`${since} · Try: pick a machine, close and reopen · Desk settings`);
  await row.click();
  const b = box(page);
  await expect(b.locator(".dk-steps li")).toHaveText(["Where Desk settings", "Do pick a machine, close and reopen", "Expect it's still picked"]);
  await expect(b.locator(".dk-steps li b")).toHaveText(["Where", "Do", "Expect"]);
  await expect(b.locator(".dk-ckl")).toHaveCount(0);
  await expect(b.locator(".dk-show")).toHaveCount(0);
  await expect(b.locator(".dk-cks")).toHaveText(`web-lead said this when it finished · ${since.replace(/^In/, "in")}`);
  await expect(b.locator(".dk-pacts button")).toHaveText(["Close", "Send back"]);
  await expect(page.locator(".dk-right [title], .dk-pg [title]")).toHaveCount(0);
});

test("Show me opens the page its steps name (#668)", async ({ page, cue }) => {
  const url = "https://claude.ai/artifact/abc123";
  const { row } = await openChecked(page, cue, { check: { ...steps, where: `the Desk mocks page, ${url}.` } });
  await row.click();
  const show = box(page).locator(".dk-show");
  await expect(show).toHaveText("Show me");
  await expect(show).toHaveAttribute("data-href", url);
});

test("no line from its session: Cue's words, its You'll see to try, else that nothing said what to try (#526)", async ({ page, cue }) => {
  const { since, row } = await openChecked(page, cue, {});
  await expect(row.locator(".dk2b-ts")).toHaveText(`${since} · Try: Quick phrases sit at the right of Send; More opens the rest. · Cue's words`);
  await row.click();
  const b = box(page);
  await expect(b.locator(".dk-ckl")).toHaveText("Try: Quick phrases sit at the right of Send; More opens the rest.");
  await expect(b.locator(".dk-cks")).toHaveText(`Cue's words: web-lead didn't say what to try, so this is its You'll see · ${since.replace(/^In/, "in")}`);
  // No You'll see either.
  await patchItem(cue, "d2", { see: "" });
  await expect(b.locator(".dk-ckl")).toHaveText("Nothing said what to try: Send back to ask its session");
  await expect(b.locator(".dk-cks")).toHaveText(`Cue's words: web-lead didn't say what to try · ${since.replace(/^In/, "in")}`);
  await expect(b.locator(".dk-pacts button")).toHaveText(["Close", "Send back"]);
  // Its row says so too, in the Items list (‹ Items: the item's page took the list's place, #666).
  await page.locator(".dk-right .dk-ph .dk-back").first().click();
  await expect(row.locator(".dk2b-ts")).toHaveText(`${since} · Nothing said what to try: Send back to ask its session · Cue's words`);
});

test("the steps in dark: their words in the dark theme's ink (#668)", async ({ page, cue }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  const { row } = await openChecked(page, cue, { check: steps });
  await row.click();
  const ink = await box(page).locator(".dk-steps").evaluate((el) => getComputedStyle(el).color);
  expect(ink).toBe("rgb(233, 236, 223)");
});

test("the box in dark: the line and its note readable on the box (#526)", async ({ page, cue }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  const { row } = await openChecked(page, cue, { check: { kind: "try", text: "open any item" } });
  await row.click();
  const b = box(page);
  await expect(b.locator(".dk-ckl")).toHaveText("Try: open any item");
  // A filled box, on the panel's own color, with its line in the dark theme's ink.
  const [bg, under, ink] = await b.evaluate((el) => [getComputedStyle(el).backgroundColor, getComputedStyle(el.closest(".dk-panel")).backgroundColor, getComputedStyle(el.querySelector(".dk-ckl")).color]);
  expect(bg).not.toBe("rgba(0, 0, 0, 0)");
  expect(bg).not.toBe(under);
  expect(ink).toBe("rgb(233, 236, 223)");
});
