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
    // The Desk's own (#688): out of the chat, one count line; Activity has each, newest first.
    await expect(chat.locator(".dk-cue")).toHaveCount(0);
    await expect(chat.locator(".dk-cnt")).toHaveText([/^3 Desk updates · 3 from Cue · Activity ›$/]);
    await expect(chat).not.toContainText("Go through every open item");
    await expect(chat).not.toContainText("Nothing to do unless it gets stuck");
    await expect(chat).not.toContainText("(Desk changes");
    // Your own message and the lead's reply stay as they were.
    await expect(chat.locator(".cv-you .cv-you-text").filter({ hasText: YOURS })).toHaveCount(1);
    await expect(chat).toContainText(REPLY);
    await page.evaluate(() => deskOpenActivity());
    const act = page.locator(".dk-moves");
    const lines = act.locator(".dk-cue");
    await expect(lines).toHaveCount(3);
    // One short line each, saying what it is; none of Cue's long words show till it's opened.
    await expect(lines.nth(2)).toContainText("Away");
    await expect(lines.nth(2)).toContainText("Settle what you can before Away");
    await expect(lines.nth(1)).toContainText("Desk changes");
    await expect(lines.nth(1)).toContainText("#d5 finished · #d3 sent back");
    await expect(lines.nth(0)).toContainText("You sent it back: “the box still jumps when I type”");
    // Found and measured in one go: a redraw replaces the chat, and a locator can find a line in one call and measure it
    // in the next, which a redraw can land between.
    const tall = await page.evaluate(() => [...document.querySelectorAll(".dk-moves .dk-cue")].map((l) => l.getBoundingClientRect().height));
    expect(tall).toHaveLength(3);
    for (const h of tall) expect(h).toBeLessThan(70);
    await expect(act).not.toContainText("Go through every open item");
    await expect(act).not.toContainText("Nothing to do unless it gets stuck");
    if (process.env.SHOT_DIR) await act.screenshot({ path: `${process.env.SHOT_DIR}/desk-lead-cue-folded-${scheme}-closed.png` });
    // A click opens what matters in it under its line (#625): what happens next, not Cue's rules for the lead; another
    // folds it again.
    await lines.nth(2).locator(".dk-cue-w").click();
    await expect(lines.nth(2).locator(".mg-full")).toContainText("The lead settles what it can; at most 5 questions wait for you");
    await expect(lines.nth(2).locator(".mg-full")).not.toContainText("Go through every open item");
    await expect(lines.nth(2).locator(".dk-cue-t")).toHaveAttribute("aria-expanded", "true");
    if (process.env.SHOT_DIR) await act.screenshot({ path: `${process.env.SHOT_DIR}/desk-lead-cue-folded-${scheme}.png` });
    await lines.nth(2).locator(".dk-cue-t").click();
    await expect(lines.nth(2).locator(".mg-full")).toHaveCount(0);
    await expect(act.locator(".dk-cue").filter({ hasText: YOURS })).toHaveCount(0);
    // Cue's line uses the theme's colours: its words aren't the bubble's, and nothing has hover text.
    const [ink, want] = await page.evaluate(() => {
      const w = document.querySelectorAll(".dk-moves .dk-cue .dk-cue-w")[1], d = document.createElement("i");
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

// Back from Away (#612): Cue's words to the lead show as one tidy line, how long and what moved, with the way to the
// Away tab; no dump of Cue's words. Light and dark, no hover text.
const BACK = "Away mode is off: the owner is back (2h 10m, 4 items moved, 3 calls made). Cue shows them what happened while they were away; don't welcome them back or recap it. Ask and decide as usual.";
for (const scheme of ["light", "dark"]) {
  test(`coming back from Away shows one tidy line with the way to the Away tab (${scheme}) (#612)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1500, height: 950 });
    await cue.open("desk");
    await openDesk(page);
    await cue.change((st, { msg }) => { st.sessions.find((x) => x.session_id === "lead").thread.push(msg); }, { msg: { role: "you", text: BLOCK + BACK, at_ms: NOW - 5000 } });
    // The Desk's own (#688): a count line in the chat, the line in Activity.
    await expect(page.locator(".dk-chat .dk-cnt")).toHaveText([/^1 Desk update · .*Activity ›$/]);
    await expect(page.locator(".dk-chat")).not.toContainText("recap");
    await page.evaluate(() => deskOpenActivity());
    const line = page.locator(".dk-moves .dk-cue");
    await expect(line).toHaveCount(1);
    await expect(line).toContainText("Back");
    await expect(line.locator(".dk-cue-w")).toHaveText("You're back after 2h 10m · 4 items moved · 3 calls made");
    await expect(line).not.toContainText("recap");
    const h = await page.evaluate(() => document.querySelector(".dk-moves .dk-cue").getBoundingClientRect().height);
    expect(h).toBeLessThan(80);
    if (process.env.SHOT_DIR) await page.locator(".dk-moves").screenshot({ path: `${process.env.SHOT_DIR}/desk-away-back-${scheme}.png` });
    await expect(page.locator("[title]")).toHaveCount(0);
    await line.locator(".dk-cue-go").click();
    await expect(page.locator(".dk-tab.on")).toHaveText("Away");
    await expect(page.locator(".dk-away-log")).toBeVisible();
  });
}
