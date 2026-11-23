// A Desk chat has no switch (#685's Conversation only | Everything, taken out by #688 option D): the Desk's own lines
// leave the chat, each run of them between your turns one count line opening Activity, where each is drawn with its
// kind's sign. Lines meant for you (How to check, If you pick…) stay in the chat with their signs.
import { test, expect, openDesk } from "./cue.mjs";

const push = (page, sid, e) => page.evaluate(([sid, e]) => { sessionOf(sid).thread.push({ at_ms: Date.now(), ...e }); renderMain(); }, [sid, e]);
/** No switch anywhere in the window, and no old folded run. */
const noSwitch = async (page) => {
  for (const w of ["Conversation only", "Everything"]) await expect(page.getByRole("button", { name: w, exact: true })).toHaveCount(0);
  await expect(page.locator('[data-act="chat-mode"], [data-act="desk-run"], .dk-right .seg2, .dk-run')).toHaveCount(0);
};

for (const scheme of ["light", "dark"]) {
  test(`the lead's chat: no switch; its Desk lines are one count line, each in Activity with its sign (${scheme})`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await openDesk(page);
    await push(page, "lead", { role: "agent", text: "I'll hand these out now.\n\nROUTE: #d2 -> web-lead | why: it knows the header\nDONE: #d3\nDECIDED: kept it small — one change at a time" });
    await push(page, "lead", { role: "agent", text: "ROUTE: #d4 -> new | why: nobody is free" });
    await push(page, "lead", { role: "agent", text: "Both are moving. Anything else?" });
    await noSwitch(page);
    await expect(page.locator("[title]")).toHaveCount(0);
    const chat = page.locator(".dk-chat");
    await expect(chat).toContainText("I'll hand these out now.");
    await expect(chat).toContainText("Both are moving. Anything else?");
    await expect(chat.locator(".dk-ln, .decided")).toHaveCount(0);
    await expect(chat.locator(".dk-cnt").last()).toHaveText(/^4 Desk updates · gave #d2 and #d4, #d3 done, decided 1 · Activity ›$/);
    // Still so after a redraw.
    await page.evaluate(() => renderMain());
    await noSwitch(page);
    await expect(chat.locator(".dk-ln, .decided")).toHaveCount(0);
    // In Activity: every line, each with its kind's sign; a DONE's sign filled; the decision shown.
    await chat.locator(".dk-cnt-b").last().click();
    const act = page.locator(".dk-moves");
    await expect(act.locator(".dk-ln.dk-sgl", { hasText: "Gave" })).toHaveCount(2);
    await expect(act.locator(".decided")).toBeVisible();
    const done = act.locator('.dk-sg[data-sg="done"]');
    await expect(done).toHaveCount(1);
    expect(await done.evaluate((el) => getComputedStyle(el).backgroundColor)).not.toBe(await act.locator('.dk-sg[data-sg="route"]').first().evaluate((el) => getComputedStyle(el).backgroundColor));
  });
}

test("Cue's messages and the lead's lines next to each other are one count line; your words and the lead's stay", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const t = Date.now();
  await cue.change((st, { t }) => {
    st.sessions.find((x) => x.session_id === "lead").thread.push(
      { role: "you", text: "Put the search box at the top please", at_ms: t - 50000 },
      { role: "agent", text: "On it.", at_ms: t - 40000 },
      { role: "agent", text: "ROUTE: #d2 -> web-lead", at_ms: t - 30000 },
      { role: "you", text: `<desk id="cue" name="cue" project="cue">\nItems: none.\n</desk>\n\nRobin sent #d3 (Search centre) back to its holder: "the box still jumps".\n\n(Desk changes 7)`, at_ms: t - 20000 },
      { role: "agent", text: "That's all for now.", at_ms: t - 10000 });
  }, { t });
  const chat = page.locator(".dk-chat");
  for (const w of ["Put the search box at the top please", "On it.", "That's all for now."]) await expect(chat).toContainText(w);
  await noSwitch(page);
  const run = chat.locator(".dk-cnt").filter({ hasText: "gave #d2" });
  await expect(run).toHaveCount(1);
  await expect(run).toHaveText(/^2 Desk updates · gave #d2, 1 from Cue · Activity ›$/);
  await expect(chat.locator(".dk-cue, [data-dk=\"route\"]")).toHaveCount(0);
  // Both in Activity.
  await run.locator(".dk-cnt-b").click();
  await expect(page.locator(".dk-moves .dk-cue")).toHaveCount(1);
  await expect(page.locator('.dk-moves [data-dk="route"]')).toHaveCount(1);
});
