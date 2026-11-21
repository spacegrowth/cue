// A loop the Desk lead drafted (#583, its LOOP: line): its chat shows the move, then the add-on's loop card for the
// draft (Start loop, Edit, Not now), given as the Desk's own (desk:<project>), never as the lead's chat: a loop
// started from it gets a session of its own. Without the add-on, the move's line alone.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";

const draft = { name: "Forecast fixes", project: "weather", folder: "/Users/me/weather", goal: "Make the forecast tests pass", done_when: "every forecast test passes", check: "npm test", decisions: "every", rounds: 4, time_min: 60, spend_usd: 8 };
const move = { id: "l1", project: "cue", item_id: null, by: "web-lead", sid: "lead", text: "Drafted \"Forecast fixes\" for weather, for you to start", why: "", at: Math.floor((NOW - 60 * 1000) / 1000), undo: "none", act: "loop", draft };

test("a loop the lead drafted shows in its chat as the move and the add-on's card for the draft", async ({ page, cue }) => {
  await cue.open("desk");
  // The add-on's card, as a stand-in: what it was given to draw.
  await page.evaluate(() => {
    addOn.reply = (sid, text) => {
      const m = /^```cue-loop\n([\s\S]*)\n```$/.exec(text);
      return { text: m ? "" : text, after: m ? `<div class="lp-test-card" data-sid="${sid}">${m[1].replace(/</g, "&lt;")}</div>` : "" };
    };
  });
  await openDesk(page);
  await cue.change((st, mv) => { st.desk.projects.find((p) => p.project === "cue").decisions.push(mv); }, move);
  const row = page.locator('.dk-chat .dk-act[data-key="act:l1"]');
  await expect(row.locator(".mg-lh")).toHaveText("Drafted \"Forecast fixes\" for weather, for you to start");
  const card = row.locator(".lp-test-card");
  await expect(card).toHaveAttribute("data-sid", "desk:cue");
  expect(JSON.parse(await card.textContent())).toEqual(draft);
});

test("without the add-on a drafted loop is its line alone", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st, mv) => { st.desk.projects.find((p) => p.project === "cue").decisions.push(mv); }, move);
  const row = page.locator('.dk-chat .dk-act[data-key="act:l1"]');
  await expect(row.locator(".mg-lh")).toHaveText("Drafted \"Forecast fixes\" for weather, for you to start");
  await expect(row).not.toContainText("cue-loop");
});
