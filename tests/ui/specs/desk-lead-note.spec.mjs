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
  // The Desk's own (#688): out of the chat, a count line saying it; in Activity the message family's card (#501):
  // who → to, Note, the item, then the note.
  const chat = page.locator(".dk-chat");
  await expect(chat.locator('[data-mg="note"]')).toHaveCount(0);
  await expect(chat.locator(".dk-cnt").last()).toHaveText(/^1 Desk update · a note on #d2 · Activity ›$/);
  await expect(chat).not.toContainText("NOTE:");
  await expect(chat).not.toContainText("**");
  await page.evaluate(() => deskOpenActivity());
  const card = page.locator('.dk-moves [data-mg="note"]');
  await expect(card).toHaveCount(1);
  await expect(card.locator(".mg-to")).toHaveText("→ cue-387");
  await expect(card.locator(".mg-tag")).toHaveText("Note");
  await expect(card.locator(".mg-id")).toHaveText("Feature #d2");
  await expect(card.locator(".mg-t")).toHaveText("The phrases go left of Send; keep | as it is in commands.");
  await expect(card).not.toContainText("NOTE:");
  await expect(card).not.toContainText("**");
  await expect(card.locator("[title]")).toHaveCount(0);
  if (process.env.SHOT_DIR) await card.screenshot({ path: `${process.env.SHOT_DIR}/note-card.png` });
  // Its item number opens the item.
  await card.locator('[data-desk-chk-item="d2"]').click();
  await expect(page.locator(".dk-right .dkn-sp .dkn-ib")).toBeVisible();
  await expect(page.locator('.dkn-card .dkn-row.on[data-desk-item="d2"]')).toHaveCount(1);
});

test("a NOTE Cue has no record of yet names the item's holder; one the hub wouldn't read stays as written", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await push(page, "NOTE: #d2 | Check the small window too.");
  await expect(page.locator(".dk-chat .dk-cnt").last()).toHaveText(/^1 Desk update · a note on #d2 · Activity ›$/);
  await page.evaluate(() => deskOpenActivity());
  const card = page.locator('.dk-moves [data-mg="note"]');
  await expect(card.locator(".mg-to")).toHaveText(/^→ \S+$/);
  await expect(card.locator(".mg-id")).toHaveText(/^(Bug|Feature|Task) #d2$/);
  // Back to the chat: a NOTE the hub wouldn't read is the lead's words, as written.
  await page.evaluate(() => { deskTab = "lead"; renderMain(); });
  await push(page, "NOTE: no item here");
  await expect(page.locator(".dk-chat .cv-agent").last()).toContainText("NOTE: no item here");
});
