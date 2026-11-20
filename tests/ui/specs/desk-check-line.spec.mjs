// Items to check say where they are, in one plain line (#436): in the app since its install with what to try, landing
// with the next install, or mocks with the canvas to open. On the Desk row and the item page; it changes by itself as
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
  await expect(row.locator(".dk-h")).toHaveText(`${since} · Try: Quick phrases sit at the right of Send; More opens the rest. · Cue's words`);
  // Said once: no You'll see line under it as well.
  await expect(row.locator(".dk-see")).toHaveCount(0);
  await row.locator(".dk-t").click();
  await expect(page.locator(".dk-panel[data-side-for] .dk-pnow .dk-ckl")).toHaveText("Try: Quick phrases sit at the right of Send; More opens the rest.");
  await expect(page.locator(".dk-panel[data-side-for] .dk-pnow .dk-cks")).toContainText(since.replace(/^In/, "in"));
  await expect(page.locator(".dk-right [title]")).toHaveCount(0);
});

test("an item not in the installed app yet says Lands with the next install, and turns to In the app when the install is done (#436)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Its work changed after the running Cue was installed: nothing to try yet.
  await patchCheck(cue, "d2", { edited: 1793984400000 + 60000, at: 1793984400000 + 60000 });
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row.locator(".dk-h")).toHaveText("Lands with the next install");
  await expect(row.locator(".dk-see")).toHaveText("You'll see: Quick phrases sit at the right of Send; More opens the rest.");
  // The install finishes: the hub says so, and the line changes with nothing pressed.
  const at = await page.evaluate(() => now());
  await patchItem(cue, "d2", { finish: { step: "checked", at, commit: "c71e0b4" } });
  await expect(row.locator(".dk-h")).toHaveText(`In the app since ${await clock(page, at)} · Try: Quick phrases sit at the right of Send; More opens the rest. · Cue's words`);
});

test("a mocks item says Mocks · open the canvas, the link opening it, on its row and its page (#436)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Mocks, notes or plans that changed nothing the app runs (#408), its canvas among what it made.
  await patchItem(cue, "d2", { finish: { step: "checked", at: Date.now(), commit: "c71e0b4", mocks: true }, made: ["https://claude.ai/artifact/abc"] });
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row.locator(".dk-h")).toHaveText("Mocks · open the canvas");
  await expect(row.locator('.dk-h a[data-href="https://claude.ai/artifact/abc"]')).toHaveText("open the canvas");
  // The link opens the canvas, not the item.
  await row.locator(".dk-h a").click();
  await cue.called("open_link", { url: "https://claude.ai/artifact/abc" });
  await expect(row).not.toHaveClass(/\bon\b/);
  await row.locator(".dk-t").click();
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
  await expect(row.locator(".dk-h")).toHaveText("2 changes not landed yet · send it back so they land");
  await row.locator(".dk-t").click();
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
  await expect(row.locator(".dk-h")).toHaveText(`${since} · Try: open any item waiting on you; the top line says what to do`);
  await expect(row.locator(".dk-h b")).toHaveText("Try:");
  await expect(row.locator(".dk-see")).toHaveCount(0);
  await row.locator(".dk-t").click();
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

test("a Nothing to see line from its session, on its row and in its panel's box (#526)", async ({ page, cue }) => {
  const { since, row } = await openChecked(page, cue, { check: { kind: "nothing", text: "tests passed, close it" } });
  await expect(row.locator(".dk-h")).toHaveText(`${since} · Nothing to see: tests passed, close it`);
  await expect(row.locator(".dk-h b")).toHaveText("Nothing to see:");
  await row.locator(".dk-t").click();
  const b = box(page);
  await expect(b.locator(".dk-ckl")).toHaveText("Nothing to see: tests passed, close it");
  await expect(b.locator(".dk-ckl b")).toHaveText("Nothing to see:");
  await expect(b.locator(".dk-cks")).toHaveText(`web-lead said this when it finished · ${since.replace(/^In/, "in")}`);
  await expect(b.locator(".dk-pacts button")).toHaveText(["Close", "Send back"]);
});

test("no line from its session: Cue's words, its You'll see to try, else that nothing said what to try (#526)", async ({ page, cue }) => {
  const { since, row } = await openChecked(page, cue, {});
  await expect(row.locator(".dk-h")).toHaveText(`${since} · Try: Quick phrases sit at the right of Send; More opens the rest. · Cue's words`);
  await row.locator(".dk-t").click();
  const b = box(page);
  await expect(b.locator(".dk-ckl")).toHaveText("Try: Quick phrases sit at the right of Send; More opens the rest.");
  await expect(b.locator(".dk-cks")).toHaveText(`Cue's words: web-lead didn't say what to try, so this is its You'll see · ${since.replace(/^In/, "in")}`);
  // No You'll see either.
  await patchItem(cue, "d2", { see: "" });
  await expect(row.locator(".dk-h")).toHaveText(`${since} · Nothing said what to try: Send back to ask its session · Cue's words`);
  await expect(b.locator(".dk-ckl")).toHaveText("Nothing said what to try: Send back to ask its session");
  await expect(b.locator(".dk-cks")).toHaveText(`Cue's words: web-lead didn't say what to try · ${since.replace(/^In/, "in")}`);
  await expect(b.locator(".dk-pacts button")).toHaveText(["Close", "Send back"]);
});

test("the box in dark: the line and its note readable on the box (#526)", async ({ page, cue }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  const { row } = await openChecked(page, cue, { check: { kind: "try", text: "open any item" } });
  await row.locator(".dk-t").click();
  const b = box(page);
  await expect(b.locator(".dk-ckl")).toHaveText("Try: open any item");
  // A filled box, on the panel's own color, with its line in the dark theme's ink.
  const [bg, under, ink] = await b.evaluate((el) => [getComputedStyle(el).backgroundColor, getComputedStyle(el.closest(".dk-panel")).backgroundColor, getComputedStyle(el.querySelector(".dk-ckl")).color]);
  expect(bg).not.toBe("rgba(0, 0, 0, 0)");
  expect(bg).not.toBe(under);
  expect(ink).toBe("rgb(233, 236, 223)");
});
