// Items to check say what to do with them (#436, #526): waiting for the install, nothing to check yet; once it's in
// the app, its row is Ready to check with the ✓, and its bar over its chat says what to try, in one line: the line its
// session wrote at DONE ("Try: …", "Nothing to try: …", its steps' Do and where), else Cue's words (its You'll see, or
// that nothing said what to try). It changes by itself as the install finishes.
import { test, expect, openDesk } from "./cue.mjs";
import { statusOf } from "./desk.mjs";

const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
const patchCheck = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").checks.find((c) => c.id === k), p); }, [id, patch]);

/** The bar over the opened item's chat, and its line of what to try. */
const bar = (page) => page.locator(".dk-right .dkn-ib");
const see = (page) => bar(page).locator(".dkn-ibsee");

async function openChecked(page, cue, patch) {
  await cue.open("desk");
  await openDesk(page);
  const at = await page.evaluate(() => now() - 30 * 60000);
  await patchItem(cue, "d2", { finish: { step: "checked", at, commit: "c71e0b4" }, ...patch });
  return { at, row: page.locator('.dk-items [data-desk-item="d2"]') };
}

test("an item installed is Ready to check on its row, and its bar says what to try (#436)", async ({ page, cue }) => {
  const { row } = await openChecked(page, cue, {});
  await expect(statusOf(page, "d2")).toHaveText("Ready to check");
  await expect(row.locator(".dk-ok")).toHaveAttribute("aria-label", "Looks right");
  // Its session said nothing of what to try: its You'll see.
  await row.click();
  await expect(see(page)).toHaveText("Try: Quick phrases sit at the right of Send; More opens the rest.");
  await expect(bar(page).locator("[title]")).toHaveCount(0);
});

test("an item not in the installed app yet says Waiting for install, and turns to Ready to check when the install is done (#436, #666)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Its work changed after the running Cue was installed: nothing to try yet.
  await patchCheck(cue, "d2", { edited: 1793984400000 + 60000, at: 1793984400000 + 60000 });
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(statusOf(page, "d2")).toHaveText("Waiting for install");
  await expect(row.locator(".dk-ok")).toHaveCount(0);
  // The install finishes: the hub says so, and the row changes with nothing pressed.
  const at = await page.evaluate(() => now());
  await patchItem(cue, "d2", { finish: { step: "checked", at, commit: "c71e0b4" } });
  await expect(statusOf(page, "d2")).toHaveText("Ready to check");
  await expect(row.locator(".dk-ok")).toHaveCount(1);
});

test("a Try line from its session: on its bar, with Looks right and Send back (#526)", async ({ page, cue }) => {
  const { row } = await openChecked(page, cue, { check: { kind: "try", text: "open any item waiting on you; the top line says what to do" } });
  await row.click();
  await expect(see(page)).toHaveText("Try: open any item waiting on you; the top line says what to do");
  await expect(see(page).locator("b")).toHaveText("Try:");
  await expect(bar(page).locator(".dkn-ibacts button")).toHaveText(["Looks right", "Send back"]);
  await expect(bar(page).locator("[title]")).toHaveCount(0);
  await bar(page).getByRole("button", { name: "Looks right" }).click();
  await expect(bar(page).locator(".dk-soon")).toContainText("Done");
});

test("a Nothing to try line from its session, on its bar (#526, #668)", async ({ page, cue }) => {
  const { row } = await openChecked(page, cue, { check: { kind: "nothing", text: "tests passed, close it" } });
  await row.click();
  await expect(see(page)).toHaveText("Nothing to try: tests passed, close it");
  await expect(see(page).locator("b")).toHaveText("Nothing to try:");
  await expect(bar(page).locator(".dkn-ibacts button")).toHaveText(["Looks right", "Send back"]);
});

// Three steps someone new can follow (#668): Where, Do, Expect; the bar's one line is the Do and where.
const steps = { kind: "try", text: "pick a machine, close and reopen", where: "Desk settings", do: "pick a machine, close and reopen", expect: "it's still picked" };

test("three steps from its session: the Do and where on its bar (#668)", async ({ page, cue }) => {
  const { row } = await openChecked(page, cue, { check: steps });
  await row.click();
  await expect(see(page)).toHaveText("Try: pick a machine, close and reopen · Desk settings");
  await expect(bar(page).locator(".dkn-ibacts button")).toHaveText(["Looks right", "Send back"]);
});

test("no line from its session: its You'll see to try, else that nothing said what to try (#526)", async ({ page, cue }) => {
  const { row } = await openChecked(page, cue, {});
  await row.click();
  await expect(see(page)).toHaveText("Try: Quick phrases sit at the right of Send; More opens the rest.");
  // No You'll see either.
  await patchItem(cue, "d2", { see: "" });
  await expect(see(page)).toHaveText("Nothing said what to try: Send back to ask its session");
  await expect(bar(page).locator(".dkn-ibacts button")).toHaveText(["Looks right", "Send back"]);
});
