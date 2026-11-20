// The Desk lead's NOTE lines in its chat (#449): never raw "NOTE: #418 | …". Each is a small card saying who it went
// to and which item ("Note to cue-387 · Feature #418"), the message under it in plain text.
import { test, expect, openDesk } from "./cue.mjs";

const push = (page, text) => page.evaluate((t) => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: t }); renderMain(); }, text);

test("a NOTE is a small card: who it went to and which item, the message under it; no raw NOTE line", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const words = "The **phrases** go left of Send; keep | as it is in commands.";
  await cue.change((st, { words }) => {
    const d = st.desk.projects.find((p) => p.project === "cue"), it = d.items.find((i) => i.id === "d2");
    it.kind = "feature";
    d.decisions.push({ id: "n1", project: "cue", item_id: "d2", by: "web-lead", sid: "lead", text: words, why: "note to cue-387", at: Math.floor(Date.now() / 1000), undo: "none", act: "" });
  }, { words });
  await push(page, `Sent a note.\n\n- **NOTE:** #d2 | ${words}`);
  const chat = page.locator(".dk-chat"), card = chat.locator(".dk-note");
  await expect(card).toHaveCount(1);
  await expect(card.locator(".dk-note-h")).toHaveText(/^Note to cue-387 · Feature #d2$/);
  await expect(card.locator(".dk-note-t")).toHaveText("The phrases go left of Send; keep | as it is in commands.");
  await expect(chat).not.toContainText("NOTE:");
  await expect(chat).not.toContainText("**");
  await expect(card.locator("[title]")).toHaveCount(0);
  if (process.env.SHOT_DIR) await card.screenshot({ path: `${process.env.SHOT_DIR}/note-card.png` });
  // Its item number opens the item.
  await card.locator('[data-desk-chk-item="d2"]').click();
  await expect(page.locator('.dk-right .dk-pg[data-desk-page="item"]')).toBeVisible();
});

test("a NOTE Cue has no record of yet names the item's holder; one the hub wouldn't read stays as written", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await push(page, "NOTE: #d2 | Check the small window too.");
  const card = page.locator(".dk-chat .dk-note");
  await expect(card.locator(".dk-note-h")).toHaveText(/^Note to \S+ · (Bug|Feature|Task) #d2$/);
  await push(page, "NOTE: no item here");
  await expect(page.locator(".dk-chat .cv-agent").last()).toContainText("NOTE: no item here");
});
