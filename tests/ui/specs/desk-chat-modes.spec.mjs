// A Desk chat's Conversation only | Everything (#685, option C of #676): Everything shows each of the Desk's lines with
// its kind's sign (option A); Conversation only folds each run of them into one row that opens in place. The switch sits
// in the chat's head, the same in the lead's chat and in a session's, and each chat keeps its own setting.
import { test, expect, openDesk } from "./cue.mjs";

const push = (page, sid, e) => page.evaluate(([sid, e]) => { sessionOf(sid).thread.push({ at_ms: Date.now(), ...e }); renderMain(); }, [sid, e]);

for (const scheme of ["light", "dark"]) {
  test(`the lead's chat: Everything shows each line with its sign; Conversation only folds a run into one row that opens in place (${scheme})`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await openDesk(page);
    const before = await page.locator('.dk-chat .dk-sg[data-sg="route"]').count();
    await push(page, "lead", { role: "agent", text: "I'll hand these out now.\n\nROUTE: #d2 -> web-lead | why: it knows the header\nDONE: #d3\nDECIDED: kept it small — one change at a time" });
    await push(page, "lead", { role: "agent", text: "ROUTE: #d4 -> new | why: nobody is free" });
    await push(page, "lead", { role: "agent", text: "Both are moving. Anything else?" });
    const sw = page.locator(".dk-ptabs .seg2");
    await expect(sw.locator(".seg2-b")).toHaveText(["Conversation only", /^Everything \d+$/]);
    await expect(sw.locator(".seg2-b.on")).toHaveText(/^Everything/);
    await expect(page.locator("[title]")).toHaveCount(0);
    const chat = page.locator(".dk-chat");
    // Everything: every line, each with its kind's sign; a DONE's sign filled.
    const first = chat.locator(".cv-agent", { hasText: "I'll hand these out now." });
    await expect(first.locator(".dk-ln.dk-sgl", { hasText: "Gave" })).toHaveCount(1);
    await expect(chat.locator('.dk-sg[data-sg="route"]')).toHaveCount(before + 2);
    const done = first.locator('.dk-sg[data-sg="done"]');
    await expect(done).toHaveCount(1);
    expect(await done.evaluate((el) => getComputedStyle(el).backgroundColor)).not.toBe(await first.locator('.dk-sg[data-sg="route"]').evaluate((el) => getComputedStyle(el).backgroundColor));
    await expect(chat.locator(".dk-run")).toHaveCount(0);
    // Conversation only: the words stay; the first reply's lines are one row in it; the next reply, nothing but a line,
    // is a row of its own between the words.
    await sw.locator(".seg2-b", { hasText: "Conversation only" }).click();
    await expect(page.locator(".dk-ptabs .seg2-b.on")).toHaveText("Conversation only");
    await expect(chat).toContainText("I'll hand these out now.");
    await expect(chat).toContainText("Both are moving. Anything else?");
    await expect(chat.locator(".dk-ln")).toHaveCount(0);
    const runs = first.locator(".dk-run");
    await expect(runs).toHaveCount(1);
    await expect(runs.nth(0).locator(".dk-run-b")).toHaveText(/^\+3 Desk updates · gave #d2, #d3 done, decided 1$/);
    // The reply of nothing but a line: a row of its own, between the words around it.
    const own = chat.locator(".dk-chat > .dk-run, .dk-run:not(.cv-agent .dk-run)").filter({ hasText: "gave #d4" });
    await expect(own.locator(".dk-run-b")).toHaveText(/^\+1 Desk update · gave #d4$/);
    // A click opens it in place: its lines under it, as Everything draws them; it stays open through a redraw.
    const y = (await runs.nth(0).boundingBox()).y;
    await runs.nth(0).locator(".dk-run-b").click();
    await expect(runs.nth(0)).toHaveClass(/open/);
    await expect(runs.nth(0).locator(".dk-run-b")).toHaveAttribute("aria-expanded", "true");
    await expect(runs.nth(0).locator(".dk-ln.dk-sgl", { hasText: "Gave" })).toHaveCount(1);
    await expect(runs.nth(0).locator(".decided")).toBeVisible();
    expect(Math.abs((await runs.nth(0).boundingBox()).y - y)).toBeLessThan(2);
    await page.evaluate(() => renderMain());
    await expect(runs.nth(0)).toHaveClass(/open/);
    await runs.nth(0).locator(".dk-run-b").click();
    await expect(runs.nth(0).locator(".dk-ln")).toHaveCount(0);
    // Kept for this chat.
    expect(await page.evaluate(() => JSON.parse(localStorage.getItem("cue.chatModes"))["lead"])).toBe("talk");
    await page.locator(".dk-ptabs .seg2-b", { hasText: "Everything" }).click();
    await expect(chat.locator(".dk-run")).toHaveCount(0);
    await expect(chat.locator('.dk-sg[data-sg="route"]')).toHaveCount(before + 2);
  });
}

test("Cue's messages and the lead's lines next to each other fold into one run; your words and the lead's stay out", async ({ page, cue }) => {
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
  await page.evaluate(() => setChatMode("lead", "talk"));
  const chat = page.locator(".dk-chat");
  for (const w of ["Put the search box at the top please", "On it.", "That's all for now."]) await expect(chat).toContainText(w);
  const runs = chat.locator(".dk-run").filter({ hasText: "gave #d2" });
  await expect(runs).toHaveCount(1);
  await expect(runs.locator(".dk-run-b")).toHaveText(/^\+2 Desk updates · gave #d2, 1 from Cue$/);
  await runs.locator(".dk-run-b").click();
  await expect(runs.locator(".dk-cue")).toHaveCount(1);
});

test("a session's chat: the same switch in its head; its CHECK, IF PICKED and small calls are lines with signs, folded in Conversation only", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "host").thread.push({ role: "agent", at_ms: Date.now(), text: [
      "Built it and the tests pass.",
      "",
      "DECIDED (minor): kept the old name — nothing reads it",
      "IF PICKED: B | a task follows | You'll see: the switch",
      "DONE: #d5",
      "CHECK: #d5 | Where: the Desk | Do: open a chat | Expect: a switch in its head",
    ].join("\n") });
  });
  await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
  const pg = page.locator(".dk-right");
  const chat = pg.locator('[data-desk-chat="host"]');
  const sw = pg.locator(".seg2");
  await expect(sw.locator(".seg2-b")).toHaveText(["Conversation only", /^Everything \d+$/]);
  for (const raw of ["CHECK:", "IF PICKED:", "DECIDED (minor)"]) await expect(chat).not.toContainText(raw);
  const check = chat.locator(".dk-ln", { has: page.locator('.dk-sg[data-sg="check"]') });
  await expect(check.locator(".mg-lh")).toHaveText(/^How to check \w+ #d5$/);
  await expect(check.locator(".dk-opt-r")).toHaveText(["Where the Desk", "Do open a chat", "Expect a switch in its head"]);
  await expect(chat.locator(".dk-ln", { has: page.locator('.dk-sg[data-sg="if-picked"]') }).locator(".mg-lh")).toHaveText("If you pick B");
  const minor = chat.locator(".dk-ln", { has: page.locator('.dk-sg[data-sg="minor"]') });
  await expect(minor.locator(".mg-lh")).toHaveText("Small call: kept the old name");
  await sw.locator(".seg2-b", { hasText: "Conversation only" }).click();
  await expect(chat).toContainText("Built it and the tests pass.");
  await expect(chat.locator(".dk-ln")).toHaveCount(0);
  await expect(chat.locator(".dk-run-b")).toHaveText(/^\+4 Desk updates · #d5 done, how to check #d5, a small call, 1 more$/);
  // Only this chat: the lead's still shows everything.
  expect(await page.evaluate(() => [chatTalkOnly("host"), chatTalkOnly("lead")])).toEqual([true, false]);
});
