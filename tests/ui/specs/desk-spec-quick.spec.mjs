// Desk spec (#498): quick phrases in the Desk's chats. Sources: #128 (the lead's chat has the same quick phrases as a
// session's chat, over its box), #477 (one click sends the phrase at once as its own message, repeat clicks never stack
// it, a draft stays untouched; every chat with phrases), #488 (on an item to check a phrase is a note to its holder, the
// item stays to check). The scenario's phrases: Commit, Go ahead, Think hard.
import { test, expect, openDesk } from "./cue.mjs";

const box = (page) => page.locator(".dk-right :is(.dkn-spm, .foot:not(.held) .composer) textarea");
const phrase = (page, p) => page.locator(`.dk-right :is(.dkn-spm, .foot:not(.held) .composer) .quick .qp[data-phrase="${p}"]`);

// #128: the same phrases a session's chat shows (the shared quick row), over the lead's box.
test("the lead's chat shows a session chat's quick phrases, over its box", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const chat = await page.evaluate(() => { const el = document.createElement("div"); el.innerHTML = quickRow(""); return [...el.querySelectorAll(".qp")].map((b) => b.textContent.trim()); });
  expect(chat.length).toBeGreaterThan(0);
  await expect(page.locator(".dk-right .foot:not(.held) .composer .quick .qp")).toHaveText(chat);
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

// #477 + #437 on an item its holder is working on: the phrase is a note to its holder, kept while it's busy and shown
// as waiting on the item's page; the draft stays.
test("an item its holder is working on: a phrase is a note to the holder, shown as waiting, the draft untouched", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dkn-items [data-desk-item="d5"]').click();
  await box(page).fill("the port is");
  await phrase(page, "Think hard").click();
  await cue.called("desk_item_act", { id: "d5", act: "note", text: "Think hard" });
  await expect(page.locator('.dk-right [data-desk-chat="host"] .cv-you', { hasText: "Think hard" }).locator(".msg-line")).toContainText("Not sent yet · Cue sends it when its turn ends");
  await expect(box(page)).toHaveValue("the port is");
  expect((await cue.calls("desk_item_act")).filter((c) => c.args.act === "send_back")).toEqual([]);
});

// #488 with a draft: on an item to check the phrase is a note ("phrase", never send_back), the item stays to check,
// and what you typed stays in the box.
test("an item to check: a phrase with a draft typed goes as a note, the item stays to check, the draft stays", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dkn-items [data-desk-item="d2"]').click();
  await box(page).fill("still 6px");
  await phrase(page, "Commit").click();
  await cue.called("desk_item_act", { id: "d2", act: "phrase", text: "Commit" });
  await page.waitForTimeout(300);
  expect((await cue.calls("desk_item_act")).filter((c) => c.args.act === "send_back")).toEqual([]);
  expect(await page.evaluate(() => deskItem("cue", "d2").state)).toBe("to_check");
  await expect(box(page)).toHaveValue("still 6px");
});
