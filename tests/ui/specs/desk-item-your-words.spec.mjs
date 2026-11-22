// What you write on an item's page shows as your own words alone (#605): in the item's chat and in its session's own
// chat, never Cue's words around them (the note's "About #id (title): ", a question's "Answer it…" and CHECK line, where
// its copy is, a wait for one). The session still gets all of it. Messages sent that way before show the same.
import { test, expect, openDesk } from "./cue.mjs";

const openItem = async (page, id) => {
  await openDesk(page);
  await page.locator(`[data-desk-item="${id}"]`).first().click();
};
const TITLE = "Hosted sessions can't open the canvas";
/** Bubble `b`'s words, its item head (the kind, number and title over them) left out. */
const wordsOf = (b) => b.evaluate((el) => { const c = el.cloneNode(true); c.querySelector(".dk-about")?.remove(); return c.innerText.trim(); });
/** The session's own chat, outside the Desk: its card under Sessions opened. */
const openSession = async (page, sid) => {
  await page.locator('.switch-view [data-view="board"]').click();
  await page.locator(`.working[data-session="${sid}"]`).first().click();
  return page.locator(".active-pane");
};
const host = (cue, patch) => cue.change((st, patch) => Object.assign(st.sessions.find((x) => x.session_id === "host"), patch), patch);
const item = (cue, patch) => cue.change((st, patch) => Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5"), patch), patch);

test("a note sent from an item's page shows only your words, in its chat and the session's own chat", async ({ page, cue }) => {
  await cue.open("desk");
  await host(cue, { state: "waiting", thread: [] });
  await openItem(page, "d5");
  await page.locator(".dk-right .dk-box textarea").fill("Try it in a hosted session too");
  await page.locator(".dk-right [data-desk-send]").click();
  await cue.called("desk_item_act", { id: "d5", act: "note", text: "Try it in a hosted session too" });
  // The session got Cue's whole message (the window hears of it after the call, as from the hub).
  await expect.poll(() => page.evaluate(() => deskOf("cue").decisions.find((c) => c.item_id === "d5" && c.delivery)?.delivery.text)).toBe(`About #d5 (${TITLE}): Try it in a hosted session too`);
  const mine = page.locator(".dk-right .dk-ichat .cv-you .cv-you-text").filter({ hasText: "Try it in a hosted session too" });
  await expect(mine).toHaveCount(1);
  expect(await wordsOf(mine)).toBe("Try it in a hosted session too");
  await expect(mine.locator(".dk-about")).toContainText(TITLE);
  const own = (await openSession(page, "host")).locator(".cv-you .cv-you-text").filter({ hasText: "Try it in a hosted session too" });
  await expect(own).toHaveCount(1);
  expect(await wordsOf(own)).toBe("Try it in a hosted session too");
  await expect(own).not.toContainText("About #d5");
});

test("a question sent from an item's page shows only your question, never Cue's Answer it and CHECK words", async ({ page, cue }) => {
  await cue.open("desk");
  await host(cue, { state: "waiting", thread: [] });
  await item(cue, { state: "to_check", now_on: false });
  await openItem(page, "d5");
  await page.locator(".dk-right .dk-box textarea").fill("Does it open on the second try too?");
  await page.locator(".dk-right [data-desk-send]").click();
  await cue.called("desk_item_act", { id: "d5", act: "note", text: "Does it open on the second try too?" });
  // What the session got, once the window hears of it (after the call, as from the hub).
  const delivered = () => page.evaluate(() => deskOf("cue").decisions.find((c) => c.item_id === "d5" && c.delivery)?.delivery.text);
  await expect.poll(delivered).toBeDefined();
  const sent = await delivered();
  expect(sent).toMatch(/^A question about Desk item #d5 \(.+\), which is with the user to check: Does it open on the second try too\?\nAnswer it\./);
  expect(sent).toContain("CHECK: #d5");
  for (const where of [page.locator(".dk-right .dk-ichat"), null]) {
    const chat = where || (await openSession(page, "host"));
    const mine = chat.locator(".cv-you .cv-you-text").filter({ hasText: "Does it open on the second try too?" });
    await expect(mine).toHaveCount(1);
    expect(await wordsOf(mine)).toBe("Does it open on the second try too?");
    await expect(chat).not.toContainText("Answer it.");
    await expect(chat).not.toContainText("CHECK: #d5");
    await expect(chat).not.toContainText("A question about Desk item");
  }
});

test("older messages sent with Cue's words after them show only yours, light and dark, no hover text", async ({ page, cue }) => {
  await cue.open("desk");
  const at = Date.now() - 600000;
  await host(cue, { state: "waiting", thread: [
    { role: "you", text: `About #d5 (${TITLE}): make the canvas open at once\nWork in its own copy of the repository: /work/cue/.claude/worktrees/desk-2 (branch desk/d5). Change files only there, never in the main checkout or another folder, make no worktree or branch of your own, and don't merge or push: at your DONE, Cue lands everything in it on the main line as one commit. Cue stops an edit or git command outside it.`, at_ms: at },
    { role: "agent", text: "On it.", at_ms: at + 60000 },
    { role: "you", text: `About #d5 (${TITLE}): and keep the old window\nIt waits for a free copy of /work/cue: every copy is in use. Don't change any files until Cue sends you where its copy is; reading, planning and answering now are fine.`, at_ms: at + 120000 },
    { role: "agent", text: "Waiting for the copy.", at_ms: at + 180000 },
    { role: "you", text: `A question about Desk item #d5 (${TITLE}), which is done: why two windows?\nAnswer it. If you change code for it, say DONE: #d5 once that's done; if you don't, the item stays as it was.\nWith your DONE, on a line of its own, write the one line shown to whoever checks it: CHECK: #d5 | Try: <…>, close it.`, at_ms: at + 240000 },
    { role: "agent", text: "One is the preview.", at_ms: at + 300000 },
  ] });
  const want = ["make the canvas open at once", "and keep the old window", "why two windows?"];
  await openItem(page, "d5");
  const check = async (chat) => {
    const mine = chat.locator(".cv-you .cv-you-text");
    await expect(mine).toHaveCount(3);
    expect(await Promise.all((await mine.all()).map(wordsOf))).toEqual(want);
    for (const cue of ["Work in its own copy", "It waits for a free copy", "Answer it.", "About #d5"]) await expect(chat).not.toContainText(cue);
    expect(await chat.locator(".cv-you [title]").count()).toBe(0);
  };
  await check(page.locator(".dk-right .dk-ichat"));
  const own = await openSession(page, "host");
  await check(own);
  for (const c of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: c });
    await check(own);
    if (process.env.SHOT_DIR) await own.screenshot({ path: `${process.env.SHOT_DIR}/your-words-${c}.png` });
  }
});
