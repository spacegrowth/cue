// A Desk chat shows each of the Desk's lines with its kind's sign (#685, option A of #676). The Conversation only |
// Everything switch is gone (#666, B2): no chat folds its lines into runs any more, a stored "Conversation only" too.
import { test, expect, openDesk } from "./cue.mjs";

const push = (page, sid, e) => page.evaluate(([sid, e]) => { sessionOf(sid).thread.push({ at_ms: Date.now(), ...e }); renderMain(); }, [sid, e]);

for (const scheme of ["light", "dark"]) {
  test(`the lead's chat shows each line with its sign, a DONE's filled; no Conversation only switch, nothing folds (${scheme})`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await openDesk(page);
    const before = await page.locator('.dk-chat .dk-sg[data-sg="route"]').count();
    await push(page, "lead", { role: "agent", text: "I'll hand these out now.\n\nROUTE: #d2 -> web-lead | why: it knows the header\nDONE: #d3\nDECIDED: kept it small — one change at a time" });
    await push(page, "lead", { role: "agent", text: "ROUTE: #d4 -> new | why: nobody is free" });
    await push(page, "lead", { role: "agent", text: "Both are moving. Anything else?" });
    // The switch is gone, from the chat's head and from anywhere on the page.
    await expect(page.locator('[data-act="chat-mode"]')).toHaveCount(0);
    await expect(page.locator(".seg2", { hasText: "Conversation only" })).toHaveCount(0);
    await expect(page.locator("[title]")).toHaveCount(0);
    const chat = page.locator(".dk-chat");
    // Every line, each with its kind's sign; a DONE's sign filled.
    const first = chat.locator(".cv-agent", { hasText: "I'll hand these out now." });
    await expect(first.locator(".dk-ln.dk-sgl", { hasText: "Gave" })).toHaveCount(1);
    await expect(chat.locator('.dk-sg[data-sg="route"]')).toHaveCount(before + 2);
    const done = first.locator('.dk-sg[data-sg="done"]');
    await expect(done).toHaveCount(1);
    expect(await done.evaluate((el) => getComputedStyle(el).backgroundColor)).not.toBe(await first.locator('.dk-sg[data-sg="route"]').evaluate((el) => getComputedStyle(el).backgroundColor));
    await expect(first.locator(".decided")).toBeVisible();
    await expect(chat).toContainText("Both are moving. Anything else?");
    await expect(chat.locator(".dk-run")).toHaveCount(0);
    // A chat kept as Conversation only from before still shows every line.
    await page.evaluate(() => setChatMode("lead", "talk"));
    await expect(chat.locator(".dk-run")).toHaveCount(0);
    await expect(chat.locator('.dk-sg[data-sg="route"]')).toHaveCount(before + 2);
  });
}

test("Cue's messages and the lead's lines next to each other each show in the lead's chat, with your words and the lead's", async ({ page, cue }) => {
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
  await expect(chat.locator(".dk-run")).toHaveCount(0);
  await expect(chat.locator(".dk-ln.dk-sgl", { hasText: "#d2" })).toHaveCount(1);
  await expect(chat.locator(".dk-cue", { hasText: "the box still jumps" })).toHaveCount(1);
});

test("a session's chat: no switch in its head; its CHECK, IF PICKED and small calls are lines with signs, none folded", async ({ page, cue }) => {
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
  await page.evaluate(() => { setChatMode("host", "talk"); deskShow({ kind: "chat", sid: "host" }); });
  const chat = page.locator('[data-desk-chat="host"]');
  await expect(chat).toContainText("Built it and the tests pass.");
  await expect(page.locator('[data-act="chat-mode"]')).toHaveCount(0);
  for (const raw of ["CHECK:", "IF PICKED:", "DECIDED (minor)"]) await expect(chat).not.toContainText(raw);
  const check = chat.locator(".dk-ln", { has: page.locator('.dk-sg[data-sg="check"]') });
  await expect(check.locator(".mg-lh")).toHaveText(/^How to check \w+ #d5$/);
  await expect(check.locator(".dk-opt-r")).toHaveText(["Where the Desk", "Do open a chat", "Expect a switch in its head"]);
  await expect(chat.locator(".dk-ln", { has: page.locator('.dk-sg[data-sg="if-picked"]') }).locator(".mg-lh")).toHaveText("If you pick B");
  const minor = chat.locator(".dk-ln", { has: page.locator('.dk-sg[data-sg="minor"]') });
  await expect(minor.locator(".mg-lh")).toHaveText("Small call: kept the old name");
  await expect(chat.locator(".dk-run")).toHaveCount(0);
});
