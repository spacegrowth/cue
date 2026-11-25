// A session's lines for Cue in its reply (core/md.js cueLineHook, #833): no chat shows them as written. IF PICKED is one
// quiet line (how many options, the mocks), DESIGNS an Open the mocks button; DONE, CHECK, TO LEAD and the rest are left out.
import { test, expect, openDesk } from "./cue.mjs";

const TEXT = [
  "Three ways to lay out the header.",
  "",
  "DESIGNS: #829 https://claude.ai/artifact/abc123",
  "IF PICKED: #829 | A | feature | Header A | you'll see: a | done when: b | by: me",
  "IF PICKED: #829 | B | feature | Header B | you'll see: a | done when: b | by: new",
  "- IF PICKED: #829 | C | feature | Header C | you'll see: a | done when: b | by: me",
  "",
  "DECIDED: kept the old colours — they match the board",
  "DECIDED (minor): worked in a copy — the tree was busy",
  "TO LEAD: #830 needs the same fix",
  "DONE: #829",
  "**CHECK:** #829 | Where: Board | Do: click Open | Expect: it opens",
  "ITEM: bug | Something else | you'll see: x | done when: y | why: z",
  "",
  "Which one should I build?",
].join("\n");

test("Cue's lines leave the reply: IF PICKED one quiet line, DESIGNS a button, the rest gone", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const html = await page.evaluate((t) => md(t), TEXT);
  await page.setContent(`<div class="msg">${html}</div>`);
  await expect(page.locator(".cue-pick")).toHaveCount(1);
  await expect(page.locator(".cue-pick")).toHaveText("3 options to pick from");
  await expect(page.locator("a.cue-mocks")).toHaveText("Open the mocks ↗");
  await expect(page.locator("a.cue-mocks")).toHaveAttribute("data-href", "https://claude.ai/artifact/abc123");
  await expect(page.locator(".decided .dec-what")).toHaveText(["kept the old colours"]);
  const words = await page.evaluate(() => document.body.textContent);
  for (const w of ["IF PICKED", "DESIGNS", "DONE", "CHECK", "TO LEAD", "ITEM:", "DECIDED", "minor"]) expect(words).not.toContain(w);
  await expect(page.locator("p")).toHaveText(["Three ways to lay out the header.", "Which one should I build?"]);
});

test("an IF PICKED with no DESIGNS line links its item's mocks when the Desk has them", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const html = await page.evaluate(() => {
    state.desk = { projects: [{ project: "p", items: [{ id: "829", designs: "https://claude.ai/artifact/xyz" }] }] };
    return md("IF PICKED: #829 | A | task | T | you'll see: a | done when: b\nIF PICKED: #829 | A | task | T2 | you'll see: a | done when: b");
  });
  await page.setContent(`<div class="msg">${html}</div>`);
  await expect(page.locator(".cue-pick")).toContainText("1 option to pick from");
  await expect(page.locator(".cue-pick a")).toHaveAttribute("data-href", "https://claude.ai/artifact/xyz");
});

test("lines Cue wouldn't act on, code blocks and files stay as written", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const out = await page.evaluate(() => [
    md("DONE: all of it, tests pass"),
    md("TO LEAD: <your words>"),
    md("```\nDONE: #1\n```"),
    md("DONE: #1", { doc: true }),
  ]);
  expect(out[0]).toContain("DONE: all of it");
  expect(out[1]).toContain("TO LEAD:");
  expect(out[2]).toContain("DONE: #1");
  expect(out[3]).toContain("DONE: #1");
});

// In a session's chat on the Desk, at laptop size and Cue's smallest window, light and dark: the reply reads as words,
// one quiet options line and the mocks button, nothing in capitals for Cue.
for (const [w, h] of [[1440, 900], [900, 600]]) {
  for (const scheme of ["light", "dark"]) {
    test(`a session's reply on the Desk, ${w}×${h} ${scheme}`, async ({ page, cue }) => {
      await page.setViewportSize({ width: w, height: h });
      await page.emulateMedia({ colorScheme: scheme });
      await cue.open("desk");
      await cue.change((st, t) => { const th = st.sessions.find((x) => x.session_id === "host").thread; th.findLast((e) => e.role !== "you").text = t; }, TEXT);
      await openDesk(page);
      await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
      const chat = page.locator('.dk-right [data-desk-chat="host"]');
      await expect(chat).toContainText("Which one should I build?");
      await expect(chat.locator(".cue-pick")).toHaveText("3 options to pick from");
      await expect(chat.locator("a.cue-mocks")).toBeVisible();
      for (const word of ["IF PICKED", "DESIGNS:", "CHECK:", "TO LEAD", "DONE: #"]) await expect(chat).not.toContainText(word);
      if (process.env.CUE_SHOTS) await chat.locator(".cv-agent").last().screenshot({ path: `${process.env.CUE_SHOTS}/${w}-${scheme}.png` });
    });
  }
}
