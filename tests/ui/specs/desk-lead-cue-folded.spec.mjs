// Cue's long messages to a Desk's lead show folded in its chat (#616): Away's rules, what you sent back, the Desk's
// changes each show as one short line saying what it is, and a click on it opens Cue's full text under it, as + on a
// voice message does. The lead still gets the full text. Your own messages and the lead's replies stay as they are.
// Light and dark, no hover text.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";

const BLOCK = `<desk id="cue" name="cue" project="cue">\nItems: none.\n</desk>\n\n`;
const AWAY = "Robin is about to go away. Go through every open item (working, waiting, to check) and settle everything you can under these rules, writing a DECIDED line for each call (DECIDED: <what you chose> — <why>). You may decide alone:\n- route new items to a session\n- send back what fails its check\nAnswer or close any ASK you can settle yourself. Leave at most 5 questions that truly need Robin, as ASK lines, most important first: any still open as Away goes on comes back to you to decide. Then stop.";
const CHANGES = `#d5 (Hosted sessions can't open the canvas) finished: it's ready for Robin's check. Nothing to do unless it gets stuck.\n\nRobin sent #d3 (Search centre) back to its holder: "the box still jumps".\n\n#d3 (Search centre): the user's words, sent back to its holder: "the box still jumps".\n\n(Desk changes 4–6)`;
const SENT_BACK = `Robin sent #d3 (Search centre) back to its holder: "the box still jumps when I type".\n\n(Desk changes 7)`;
const YOURS = "Put the search box at the top of the page please";
const REPLY = "Routed it to the session that has search.";

for (const scheme of ["light", "dark"]) {
  test(`Cue's long messages to the lead fold to one line each, opening on a click (${scheme}) (#616)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1500, height: 950 });
    await cue.open("desk");
    await openDesk(page);
    await cue.change((st, { msgs }) => {
      const lead = st.sessions.find((x) => x.session_id === "lead");
      lead.thread.push(...msgs);
    }, { msgs: [
      { role: "you", text: BLOCK + AWAY, at_ms: NOW - 50000 },
      { role: "you", text: BLOCK + CHANGES, at_ms: NOW - 40000 },
      { role: "you", text: BLOCK + SENT_BACK, at_ms: NOW - 30000 },
      { role: "you", text: BLOCK + YOURS, at_ms: NOW - 20000 },
      { role: "agent", text: REPLY, at_ms: NOW - 10000 },
    ] });
    const chat = page.locator(".dk-chat");
    const lines = chat.locator(".dk-cue");
    await expect(lines).toHaveCount(3);
    // One short line each, saying what it is; none of Cue's long words show till it's opened.
    await expect(lines.nth(0)).toContainText("Away");
    await expect(lines.nth(0)).toContainText("Settle what you can before Away");
    await expect(lines.nth(1)).toContainText("Desk changes");
    await expect(lines.nth(1)).toContainText("#d5 finished · #d3 sent back");
    await expect(lines.nth(2)).toContainText("You sent it back: “the box still jumps when I type”");
    // Found and measured in one go: a redraw replaces the chat, and a locator can find a line in one call and measure it
    // in the next, which a redraw can land between.
    const tall = await page.evaluate(() => [...document.querySelectorAll(".dk-chat .dk-cue")].map((l) => l.getBoundingClientRect().height));
    expect(tall).toHaveLength(3);
    for (const h of tall) expect(h).toBeLessThan(70);
    await expect(chat).not.toContainText("Go through every open item");
    await expect(chat).not.toContainText("Nothing to do unless it gets stuck");
    await expect(chat).not.toContainText("(Desk changes");
    if (process.env.SHOT_DIR) await chat.screenshot({ path: `${process.env.SHOT_DIR}/desk-lead-cue-folded-${scheme}-closed.png` });
    // A click opens Cue's full text under its line, word for word; another folds it again.
    await lines.nth(0).locator(".dk-cue-w").click();
    await expect(lines.nth(0).locator(".mg-full")).toHaveText(AWAY);
    await expect(lines.nth(0).locator(".dk-cue-t")).toHaveAttribute("aria-expanded", "true");
    if (process.env.SHOT_DIR) await chat.screenshot({ path: `${process.env.SHOT_DIR}/desk-lead-cue-folded-${scheme}.png` });
    await lines.nth(0).locator(".dk-cue-t").click();
    await expect(lines.nth(0).locator(".mg-full")).toHaveCount(0);
    // Your own message and the lead's reply stay as they were.
    await expect(chat.locator(".cv-you .cv-you-text").filter({ hasText: YOURS })).toHaveCount(1);
    await expect(chat).toContainText(REPLY);
    await expect(chat.locator(".dk-cue").filter({ hasText: YOURS })).toHaveCount(0);
    // Cue's line uses the theme's colours: its words aren't the bubble's, and nothing has hover text.
    const [ink, want] = await page.evaluate(() => {
      const w = document.querySelectorAll(".dk-chat .dk-cue .dk-cue-w")[1], d = document.createElement("i");
      d.style.color = "var(--ink)"; document.body.append(d);
      const v = [getComputedStyle(w).color, getComputedStyle(d).color]; d.remove(); return v;
    });
    expect(ink).toBe(want);
    await expect(page.locator("[title]")).toHaveCount(0);
    // The lead still got the full text: the session's thread holds it as sent.
    const sent = await page.evaluate(() => state.sessions.find((x) => x.session_id === "lead").thread.map((e) => e.text));
    expect(sent.some((t) => t.endsWith(AWAY))).toBe(true);
  });
}
