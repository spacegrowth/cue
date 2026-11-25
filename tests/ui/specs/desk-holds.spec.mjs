// Holds on a repository's code (#540): every hold on the Desk's repository, its own or another Desk's, is one quiet
// line under the Desk's header, "cue #509 holds hub/desk_*, ui/desk_* till it lands". No hover text, no stripe.
import { test, expect, openDesk, deskAllSessions, deskShowPanel } from "./cue.mjs";

test("a hold on this Desk's code shows as a quiet line under its header, and goes when it ends (#540)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  const holds = page.locator(".dk-strip .dk-holds");
  await expect(holds).toHaveCount(0);
  await cue.change((st) => {
    st.desk.projects.find((d) => d.project === "cue").holds = [
      { item: "509", desk: "cue", desk_name: "cue", area: ["src-tauri/src/hub/desk_*.rs", "ui/desk_*.js"], till: "it lands", why: "the refactor moves these files", mine: true },
      { item: "533", desk: "loops", desk_name: "loops", area: ["ui/desk/thread.js"], till: "5pm", why: "", mine: false },
    ];
  });
  await expect(holds.locator(".dk-hold")).toHaveText(["cue #509 holds hub/desk_*, ui/desk_* till it lands", "loops #533 holds ui/desk/thread.js till 5pm"]);
  await expect(holds.locator("[title]")).toHaveCount(0);
  // Quiet: the header's muted colour, as its counts line (#932: the Desk's header on the panel).
  const look = await holds.locator(".dk-hold").first().evaluate((e) => { const s = getComputedStyle(e); return { border: s.borderLeftWidth, color: s.color, quiet: getComputedStyle(document.querySelector(".dk-strip .dk-sub")).color, ink: getComputedStyle(document.querySelector(".dk-strip .dkh-n b")).color }; });
  expect(look.border).toBe("0px");
  expect(look.color).toBe(look.quiet);
  expect(look.color).not.toBe(look.ink);
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").holds = []; });
  await expect(holds).toHaveCount(0);
});
