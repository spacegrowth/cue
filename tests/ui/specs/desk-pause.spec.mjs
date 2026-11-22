// Pause (#366): a Pause button in the Items head pauses the whole Desk (desk_pause); while paused, "Paused" shows by
// the Items title and the button reads Resume.
import { test, expect, openDesk } from "./cue.mjs";


test("Pause pauses the Desk; while paused it shows Paused and Resume", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const btn = page.locator(".dk-items .dk2b-ih .dk-pz");
  await expect(btn).toHaveText("Pause");
  await expect(page.locator(".dk-items .dk-pz-tag")).toHaveCount(0);
  await btn.click();
  await cue.called("desk_pause", { desk: "cue", on: true });
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").paused = { on: true, since: Date.now(), by: "You", held: 0 }; });
  await expect(page.locator(".dk-items .dk-pz-tag")).toHaveText("Paused");
  await expect(btn).toHaveText("Resume");
  await btn.click();
  await cue.called("desk_pause", { desk: "cue", on: false });
});
