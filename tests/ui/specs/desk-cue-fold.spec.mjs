// Opening a folded Cue message shows only what matters to you (#625): whose words, which item, and what happens next
// in one plain line. Cue's working rules for the session (how to write DONE and CHECK, "don't add new items", "don't
// change any files") aren't shown; the session still gets them in full. The card's tag reads in its own case, not
// capitals. Light and dark, no hover text.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";

// What a session gets when you send its item back while every copy is in use (act.rs send_back, trees.rs).
const SENT_BACK = "Desk item #d5 (Hosted sessions can't open the canvas) isn't done: looks good but no underline\n"
  + "Its fixes stay in #d5; don't add new items for them. Say DONE: #d5 when it is.\n"
  + "With your DONE, on a line of its own, write the one line shown to whoever checks it, built from its you'll-see (if it has one) and what you changed: CHECK: #d5 | Try: <one concrete thing to do in the app> when there's something to see, or CHECK: #d5 | Nothing to see: <why, e.g. tests passed, or a change nobody can see>, close it.\n"
  + "It waits for a free copy of /w/cue: every copy is in use. Don't change any files until Cue sends you where its copy is; reading, planning and answering now are fine.";
const NOTE = "Note from the Desk lead on Desk item #d5 (Hosted sessions can't open the canvas): Check the dark theme too.";
const RULES = ["Its fixes stay", "don't add new items", "Say DONE", "With your DONE", "CHECK: #d5", "on a line of its own", "Don't change any files"];

for (const scheme of ["light", "dark"]) {
  test(`An opened Cue message shows whose words, the item and what's next, not the session's rules (${scheme}) (#625)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1500, height: 950 });
    await cue.open("desk");
    await openDesk(page);
    await cue.change((st, { msgs }) => {
      st.sessions.find((x) => x.session_id === "host").thread.push(...msgs);
    }, { msgs: [{ role: "you", text: SENT_BACK, at_ms: NOW - 60000 }, { role: "you", text: NOTE, at_ms: NOW - 30000 }] });
    await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
    const chat = page.locator('.dk-right [data-desk-chat="host"]');
    // Your send-back is your own bubble now (#688), as the board draws it: the item's head and your words, none of
    // the session's rules. The lead's note is the Desk's own: a count line, its card in Activity.
    await expect(chat.locator(".mg.cue-msg")).toHaveCount(0);
    await expect(chat.locator(".dk-cnt")).toHaveText([/^1 Desk update · .*Activity ›$/]);
    const back = chat.locator(".cv-you.mg-you", { hasText: "looks good but no underline" });
    await expect(back.locator(".dk-about")).toContainText("#d5");
    await expect(back.locator(".cv-you-text")).toHaveText(/looks good but no underline$/);
    for (const r of RULES) await expect(back).not.toContainText(r);
    // The lead's note, in Activity: its words, the item, what's next.
    await page.evaluate(() => deskOpenActivity());
    const cards = page.locator(".dk-moves .mg.cue-msg");
    await expect(cards).toHaveCount(1);
    await expect(cards.first().locator(".mg-tag")).toHaveCSS("text-transform", "none");
    await cards.first().locator(".mg-lk", { hasText: "More" }).click();
    await expect(cards.first().locator(".cue-fold-k")).toHaveText(["The lead said", "Item", "Next"]);
    await expect(cards.first().locator(".cue-fold-v").first()).toHaveText("“Check the dark theme too”");
    await expect(cards.first().locator(".cue-fold-v").last()).toHaveText("host-lead reads it and carries on");
    // Its rows' names are muted, the words in ink: the theme's own colours.
    const [k, v, muted, ink] = await page.evaluate(() => {
      const f = document.querySelector(".dk-moves .cue-fold"), d = document.createElement("i"), e = document.createElement("i");
      d.style.color = "var(--muted)"; e.style.color = "var(--ink)"; document.body.append(d, e);
      const r = [getComputedStyle(f.querySelector(".cue-fold-k")).color, getComputedStyle(f.querySelector(".cue-fold-v")).color, getComputedStyle(d).color, getComputedStyle(e).color];
      d.remove(); e.remove(); return r;
    });
    expect([k, v]).toEqual([muted, ink]);
    await expect(page.locator("[title]")).toHaveCount(0);
    if (process.env.SHOT_DIR) await chat.screenshot({ path: `${process.env.SHOT_DIR}/desk-cue-fold-${scheme}.png` });
    // The session still got every word.
    const sent = await page.evaluate(() => state.sessions.find((x) => x.session_id === "host").thread.map((e) => e.text));
    expect(sent).toContain(SENT_BACK);
  });
}
