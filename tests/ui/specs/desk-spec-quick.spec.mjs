// Desk spec (#498): quick phrases in the Desk's chats. Sources: #128 (the lead's chat has quick phrases over its box;
// #840: only "Go ahead", and an item's page none), #477 (one click sends the phrase at once as its own message, repeat clicks never stack
// it, a draft stays untouched; every chat with phrases), #488 (on an item to check a phrase is a note to its holder, the
// item stays to check). The scenario's phrases: Commit, Go ahead, Think hard.
import { test, expect, openDesk } from "./cue.mjs";

const box = (page) => page.locator(".dk-right :is(.dkn-spm, .foot:not(.held) .composer) textarea");
const phrase = (page, p) => page.locator(`.dk-right :is(.dkn-spm, .foot:not(.held) .composer) .quick .qp[data-phrase="${p}"]`);

// #128, then #840: the lead's chat shows only "Go ahead" of the phrases, over its box.
test("the lead's chat shows only Go ahead of the quick phrases, over its box", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-right .foot:not(.held) .composer .quick .qp")).toHaveText(["Go ahead"]);
  const over = await page.evaluate(() => { const q = document.querySelector(".dk-right .foot:not(.held) .composer .quick"), row = document.querySelector(".dk-right .foot:not(.held) .composer .field-row"); return row.getBoundingClientRect().top + 1 - q.getBoundingClientRect().bottom; });
  expect(over).toBeGreaterThanOrEqual(0);
});

// #477 in the lead's chat: sent at once to the lead, as its own message; the draft stays; a double click sends it once.
test("the lead's chat: a phrase goes at once on its own, the draft stays, a double click sends it once", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await box(page).fill("and the footer");
  await phrase(page, "Go ahead").dblclick();
  await cue.called("desk_send", { project: "cue", text: "Go ahead" });
  await page.waitForTimeout(300);
  expect((await cue.calls("desk_send")).map((c) => c.args.text)).toEqual(["Go ahead"]);
  await expect(box(page)).toHaveValue("and the footer");
});

// #477 on a session's page on the Desk: the phrase goes to that session, the draft stays, never stacked in the box.
test("a session's page: a phrase goes to that session at once, the draft stays, a double click sends it once", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.sessions.find((s) => s.session_id === "host").state = "waiting"; });
  await openDesk(page);
  await page.locator('.dkn-side [data-desk-chat-of="host"]').click();
  await expect(page.locator('.dk-right .dkn-sp[data-desk-page="session"]')).toHaveCount(1);
  await box(page).fill("the canvas, then");
  await phrase(page, "Commit").dblclick();
  await cue.called("send_to_session", { sessionId: "host", text: "Commit" });
  await page.waitForTimeout(300);
  expect((await cue.calls("send_to_session")).map((c) => c.args.text)).toEqual(["Commit"]);
  await expect(box(page)).toHaveValue("the canvas, then");
});

// #840: an item's page has no quick phrases, whether its holder is at work or it's to check.
for (const id of ["d5", "d2"]) {
  test(`item ${id}'s page has no quick phrases over its box`, async ({ page, cue }) => {
    await cue.open("desk");
    await openDesk(page);
    await page.locator(`.dkn-items [data-desk-item="${id}"]`).click();
    await expect(box(page)).toBeVisible();
    await expect(page.locator(".dk-right :is(.dkn-spm, .foot:not(.held) .composer) .quick")).toHaveCount(0);
  });
}
