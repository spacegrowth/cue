// The Desk's WAITING TO INSTALL group (#911): Install now at its top runs the Desk's own steps after landing at once,
// named by them; none while an install runs or once asked, and no group (so no button) on a Desk without such steps.
import { test, expect, openDesk } from "./cue.mjs";
import { groupOf } from "./desk.mjs";

const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
const names = (cue, n) => cue.change((st, v) => { const d = st.desk.projects.find((x) => x.project === "cue"); d.lands = { ...(d.lands || {}), set: { ...(d.lands?.set || {}), after_names: v } }; }, n);

test("Install now on the group waiting to install runs the Desk's steps now; named by them, gone while one runs, none without them (#911)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await names(cue, { building: "Build", installing: "Install", starting: "Restart" });
  await patchItem(cue, "d3", { state: "finishing", finish: { step: "queued", commit: "c71e0b4feed", at: Date.now() } });
  expect(await groupOf(page, "d3")).toBe("WAITING TO INSTALL");
  const bar = page.locator(".dkn-b-install .dkn-gbar");
  const go = bar.locator("[data-desk-install-now]");
  await expect(bar.locator("span")).toHaveText("Build · Install · Restart · runs now instead of waiting");
  await expect(go).toHaveText("Install now");
  await expect(page.locator(".dkn-b-install [title]")).toHaveCount(0);
  for (const theme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: theme });
    await go.scrollIntoViewIfNeeded();
    if (process.env.SHOT_DIR) await page.locator(".dkn-items").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-911-${theme}.png` });
  }

  // Named by the Desk's own steps.
  await names(cue, { building: "Compile", installing: "Deploy", starting: "Start again" });
  await expect(go).toHaveText("Deploy now");
  await names(cue, { building: "Build", installing: "Install", starting: "Restart" });

  // A click asks for it once; once asked, the button goes.
  await go.click();
  await cue.called("desk_install_now", { project: "cue" });
  await expect(go).toHaveCount(0);

  // While an install runs: none.
  await patchItem(cue, "d3", { finish: { step: "queued", commit: "c71e0b4feed", at: Date.now() } });
  await expect(go).toHaveCount(1);
  await cue.change((st) => { st.desk.install = { step: "building", at: Date.now(), began: Date.now(), items: ["d3"] }; });
  await expect(go).toHaveCount(0);
  await cue.change((st) => { st.desk.install = null; });
  await expect(go).toHaveCount(1);

  // A Desk with no steps after landing: no group, no button.
  await names(cue, {});
  await expect(page.locator(".dkn-b-install")).toHaveCount(0);
  await expect(page.locator("[data-desk-install-now]")).toHaveCount(0);
});
