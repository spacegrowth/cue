// What Cue itself sends a session or a Desk lead shows as one short plain line (#478): "Asked host-lead · #d5 not
// done: “…”" where a line is all there's room for, and in a chat as the family's card (#501): its head
// "Cue → host-lead · NOT DONE · #d5 · 3m", the line's words under it, More opening its full text. In the lead's own
// chat on the Desk, one quiet line instead (#616, desk-lead-cue-folded.spec).
// The session still gets the full text. Every kind is listed in docs/cue-messages.md.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";

// The card as a chat draws it, from the plain line: who it went to and its item in the head; its words the line's
// own, capitalized, then what's folded besides (" · its log"), if anything.
const card = (rows) => rows.map(([, w]) => { const [, who, id, what] = /^(?:Asked|Told) (\S+) · (?:(#\S+) )?(.*)$/.exec(w); return { who, id, what: what[0].toUpperCase() + what.slice(1) }; });
const words = (rows) => card(rows).map(({ what }) => new RegExp(`^${what.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}( · .+)?$`));
async function heads(lines, rows) {
  await expect(lines.locator(".mg-h .mg-who")).toHaveText(rows.map(() => "Cue"));
  await expect(lines.locator(".mg-h .mg-to")).toHaveText(card(rows).map(({ who }) => `→ ${who}`));
  await expect(lines.locator(".mg-h .mg-tag")).toHaveCount(rows.length);
  for (const [n, { id }] of card(rows).entries()) await expect(lines.nth(n).locator(".mg-h .mg-id")).toHaveText(id ? [id] : []);
}


// [the hub's words, the line] for each kind a session gets (docs/cue-messages.md, "To a session").
const TO_SESSION = [
  ["Desk item #d5 (Fix (the) header) isn't done: Are we done here completely?\nIts fixes stay in #d5; don't add new items for them. Say DONE: #d5 when it is.", "Asked host-lead · #d5 not done: “Are we done here completely?”"],
  ["For your teammate ui-tests: Desk item #d5 (Fix) isn't done: The build failed.\nThe end of its log:\nerror[E0425]\nIts fixes stay in #d5; don't add new items for them. Say DONE: #d5 when it is.", "Asked ui-tests · #d5 not done: “The build failed”"],
  ["Desk item #d5 (Header) is open again: Commit abc1234 (Revert x) took its work back out. Put it back, then say DONE: #d5.\nIts fixes stay in #d5; don't add new items for them.", "Asked host-lead · #d5 open again: “Commit abc1234 (Revert x) took its work back out”"],
  ["Desk item #d5 (Header): you stopped before it was done. Pick up where you left off.\nSay DONE: #d5 on a line of its own when it's done.", "Asked host-lead · #d5 pick up where it stopped"],
  ["Note from the Desk lead on Desk item #d5 (Header, finishing): Check the dark theme too.", "Told host-lead · #d5 the lead's note: “Check the dark theme too”"],
  ["Desk item #d5 (Header) has moved to site-lead: stop work on it.", "Told host-lead · #d5 moved to site-lead: stop work on it"],
  ["Desk item #d5 (Header) is parked: it's off your list. Don't start it; Cue tells you if it comes back.", "Told host-lead · #d5 parked: off its list"],
  ["Desk item #d5 is paused by the Desk lead (clash with #d3). Stop at the next clean point: finish or undo the edit you're in so the files make sense, don't start anything new on it, write in a few lines where you are and what's next, and end your turn. Keep it: Cue tells you when to resume.", "Asked host-lead · #d5 paused (clash with #d3): stop at a clean point"],
  ["Desk item #d5 is resumed: carry on from where you stopped. Say DONE: #d5 on a line of its own when it's done.", "Told host-lead · #d5 resumed: carry on"],
  ["Desk item #d5: the Desk lead took back its copy (/w/desk-5). Its work, if any, is kept on branch desk/d5. Stop work on it until it's routed to you again.", "Told host-lead · #d5 copy taken back: stop work on it"],
  ["Desk item #d5 is closed: its work is already on main (abc1234). Stop work on it.", "Told host-lead · #d5 closed: already on main"],
  ["Desk item #d5 was dropped: stop work on it.", "Told host-lead · #d5 dropped: stop work on it"],
  ["Desk item #d5 (Mocks): option B was picked, so it's done. #d9 builds it; the Desk lead routes that. Mark \"Picked: B\" on its row.", "Told host-lead · #d5 You picked B; #d9 builds it"],
  ["Desk item #d3 was folded into #d5, which you have. It said: Make it blue", "Told host-lead · #d3 folded into #d5: “Make it blue”"],
  ["Done? Write DONE: #d5, or say what's left\nDone? Write DONE: #d3, or say what's left", "Asked host-lead · #d5 done? or what's left"],
  ["The Desk is paused. Stop at the next safe point: finish the step you're on, leave your work as it is in your copy (don't commit, push or undo anything, and don't say DONE), say in one line where you stopped, and end your turn. Cue tells you when to go on.", "Asked host-lead · the Desk is paused: stop at a safe point"],
  ["The Desk is resumed: carry on with #d5, #d3 from where you stopped.", "Told host-lead · the Desk is resumed: carry on with #d5, #d3"],
  ["Your TO LEAD: line didn't reach the Desk's lead: it isn't running", "Told host-lead · TO LEAD didn't reach the lead: “it isn't running”"],
  ["TO LEAD: not sent: #x isn't an item on your Desk. Name one you're on, or leave the # out.", "Told host-lead · TO LEAD not sent: “#x isn't an item on your Desk. Name one you're on, or leave the # out”"],
  ["Not added as a new item: Dark theme pass. Its fix or next step stays in #d5, the item you're on: do it there, or say it in your reply.\nOnly something truly separate from it is a new item.", "Told host-lead · #d5 not a new item: “Dark theme pass” stays here"],
  ["Several Desks could receive this item. Add it through the intended Desk so it reaches the right lead.", "Told host-lead · item not added: several Desks could take it"],
  ["Robin answered the steps held while they were away (Ask me before), in the order you asked:\n1. ALLOWED: run it now, exactly as written:\nrm x\n2. NO: don't run it:\ngit push\n", "Told host-lead · held steps answered: 1 allowed, 1 refused"],
  ["Ask me before, changed by Robin just now: ask Robin first, and end your turn, before a release, or deleting files. This replaces any earlier Ask me before or One-way rule you were given. No reply needed.", "Told host-lead · Ask me before changed: before a release, or deleting files"],
  ["Go back on your call on #d5: you decided to use red (your reason: brand). Undo it and take the other way, then carry on; say what that changes.", "Asked host-lead · #d5 go back on: “to use red”"],
  ["Bring parser-tests back: it was set aside (too slow). Pick its part up again: start a teammate for it.", "Asked host-lead · bring parser-tests back"],
  ["(Cue) Your teammate parser-tests has been quiet for 12 min with nothing running. Ask it where it is.", "Asked host-lead · check on teammate parser-tests (quiet 12 min)"],
  ["Hand off: write a short note so a fresh session can carry on from here.\n\nPut it in HANDOFF.md at the project root.", "Asked host-lead · hand off: write a note for a fresh session"],
  ["Carry on from the previous session's hand-off.\n\nRead its hand-off note at HANDOFF.md.", "Asked host-lead · carry on from the hand-off note"],
  ["You no longer run cue's Desk: web-lead does now. Keep on with the items you hold.", "Told host-lead · no longer runs cue's Desk"],
  ["The team from the session before you. Their processes ended with it.\n- a: x (done).\n- b: y (working).", "Told host-lead · the team from the session before"],
];

// To a Desk lead (docs/cue-messages.md, "To a Desk lead").
const TO_LEAD = [
  ["host-lead, on #d5, asks about clearing the way for its landing (it's waiting on the answer):\nThere are leftover edits in main. Commit them?\n1. Yes: commit\n2. No\nAnswer it yourself: ANSWER: #d5 | <a choice's number, or your own words>. Only if the user must decide: PASS: #d5 | <why>. Without either in 10 minutes, it goes to the user.", "Asked web-lead · #d5 host-lead's landing question: “There are leftover edits in main. Commit them?”"],
  ["Circle on the Desk: #d5 waits on #d3 waits on #d5. None of them can move until one link is cut.\n- #d5 waits on #d3: queued after it", "Asked web-lead · cut a wait circle: #d5 waits on #d3 waits on #d5"],
  ["Cue couldn't start a new session for #d5 (Header): no agent. It's still new; ROUTE it to a session, or to new to try again.", "Told web-lead · #d5 no new session: “no agent”"],
  ["The Desk is paused (by Robin, 3m ago). Its sessions stopped at a safe point.", "Told web-lead · the Desk is paused (by Robin)"],
  ["The Desk is resumed: its sessions carry on and what was held is sent. Route as usual.", "Told web-lead · the Desk is resumed"],
  ["Your ASK about #d5: Blue or red?\nThe answer: Blue", "Told web-lead · #d5 its ASK answered: “Blue”"],
  ["Cue asked Robin before routing #d5 to site-lead, which isn't on the Desk. The answer: No. #d5 stays as it was.", "Told web-lead · #d5 not routed to site-lead: answer “No”"],
  ["Robin said yes: site-lead is on the Desk now, and Cue routed #d5 to it as you asked.", "Told web-lead · #d5 routed to site-lead (a yes)"],
  ["Robin said no to changing how work lands on cue (check → none). Nothing changed.", "Told web-lead · Robin said no to changing how work lands on cue (check → none). Nothing changed."],
  ["Robin is away: Cue didn't show them these ASKs. Decide each one, in this order, yourself.\n- #d5: x\n- y", "Asked web-lead · decide the open ASKs yourself (away)"],
  ["Robin is about to go away. Go through every open item (working, waiting, to check).", "Asked web-lead · settle what you can before Away"],
  ["Robin is about to go away. Since your last pass, only these changed:\n- #d5 (Header): new, not routed yet\n- a new question from host-lead: \"Blue?\"\nSettle just these, nothing else.", "Asked web-lead · settle 2 changes before Away"],
  ["Away mode is off: the owner is back. Ask and decide as usual.", "Told web-lead · you're back"],
  ["Away mode is off: the owner is back (2h 10m, 4 items moved, no calls made). Cue shows them what happened while they were away; don't welcome them back or recap it. Ask and decide as usual.", "Told web-lead · you're back after 2h 10m"],
  ["Away mode:\nRobin is away until 18:00, with no budget set.\n", "Told web-lead · Away is on, until 18:00"],
  ["#d5 not cut: this Desk has no item #d5\n#d3 not split: it's done already", "Told web-lead · #d5 not cut: this Desk has no item #d5"],
  ["Refused: #d5 is done. You can ask the user to reopen it (ASK: #d5 | …), or add a new ITEM for what's left.", "Told web-lead · refused: #d5 is done"],
];

test("each kind of message Cue sends reads as one short line; your own words and notes don't (#478)", async ({ page, cue }) => {
  await cue.open("desk");
  const got = await page.evaluate(([s, l]) => [s.map(([t]) => cueMsgPlain(t, "host-lead")), l.map(([t]) => cueMsgPlain(t, "web-lead"))], [TO_SESSION, TO_LEAD]);
  expect(got[0]).toEqual(TO_SESSION.map(([, w]) => w));
  expect(got[1]).toEqual(TO_LEAD.map(([, w]) => w));
  // Not Cue's: your words, your note from an item's page, and the hand-off (its own card).
  expect(await page.evaluate(() => [
    cueMsgPlain("Can you make the header blue?"),
    cueMsgPlain("About #d5 (Header): make it blue"),
    // A new lead's hello and hand-off note are short already: they keep their bubble (desk-quick-add.spec's Desk handoff).
    cueMsgPlain("You run cue's Desk now. Say so in one line, then hand out anything not given out yet."),
    cueMsgPlain("Done? Write DONE: #d5, or say what's left\n\nDesk item #d5 (bug): Hosted sessions can't open the canvas\nSay DONE: #d5 on a line of its own when it's done."),
  ])).toEqual([null, null, null, null]);
  // A new lead's first message is Cue's, never your words (#816): one line in Activity.
  expect(await page.evaluate(() => cueMsgPlain("web-lead now runs cue's Desk. Please route Header, Footer."))).toBe("Told · web-lead now runs cue's Desk");
});

// A crew lead's "(Cue) Your teammate … has been quiet" keeps the crew's own Cue line (team.js), as before.
// In the lead's chat, two already have their own drawing (desk/lead.js deskCueNote): a start failure, and your answer
// to its ASK (shown with the ASK).
const IN_LEAD = TO_LEAD.filter(([t]) => !/^(Cue couldn't start|Your ASK)/.test(t));
// Your own send-back (no log: you sent it, not Cue's landing check) is your bubble with the item's head, as the board
// draws it (#688), not one of Cue's lines.
const YOURS = TO_SESSION[0][0];
const IN_CHAT = TO_SESSION.filter(([t]) => !t.startsWith("(Cue) ") && t !== YOURS);

/** Cue's messages to the host session and to the lead, as each test starts. */
async function cueSaidSetup(page, cue, scheme) {
  await page.emulateMedia({ colorScheme: scheme });
  await page.setViewportSize({ width: 1500, height: 950 });
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st, { s, l, at }) => {
    const host = st.sessions.find((x) => x.session_id === "host"), lead = st.sessions.find((x) => x.session_id === "lead");
    s.filter(([t]) => !t.startsWith("(Cue) ")).forEach(([text], n) => host.thread.push({ role: "you", text, at_ms: at + n * 1000 }));
    host.thread.push({ role: "you", text: "Can you make the header blue?", at_ms: at + 99_000 });
    l.forEach(([text], n) => lead.thread.push({ role: "you", text, at_ms: at + n * 1000 }));
  }, { s: TO_SESSION, l: TO_LEAD, at: NOW - 3 * 60 * 1000 });
}

for (const scheme of ["light", "dark"]) {
  test(`Cue's messages to a session leave its chat; in Activity each is a short line, the full text folded under it (${scheme}) (#478, #789)`, async ({ page, cue }) => {
    await cueSaidSetup(page, cue, scheme);
    await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
    const chat = page.locator('.dk-right [data-desk-chat="host"]');
    // Your own words keep their bubble, your send-back too: its item's head and your words, no rules (#688).
    await expect(chat).toContainText("Can you make the header blue?");
    const yours = chat.locator(".cv-you.mg-you", { hasText: "Are we done here completely?" });
    await expect(yours).toHaveCount(1);
    await expect(yours.locator(".dk-about")).toContainText("#d5");
    await expect(yours).not.toContainText("Its fixes stay");
    // Cue's messages to a session aren't in its chat (#789): they're cards in Activity, newest first, each saying who
    // it went to.
    await expect(chat.locator(".dk-cnt, .mg.cue-msg")).toHaveCount(0);
    if (process.env.SHOT_DIR) await chat.screenshot({ path: `${process.env.SHOT_DIR}/desk-cue-said-session-${scheme}.png` });
    // All of them at once (the tab shows 30 till you scroll).
    await page.evaluate(() => { deskMovesShown.set("cue", 200); deskOpenActivity(); });
    const act = page.locator(".dk-moves"), R = [...IN_CHAT].reverse();
    const to = await page.evaluate(() => `Cue to ${deskHolderName("host")}`);
    const lines = act.locator(".dk-mv-ln", { hasText: to }).locator(".mg.cue-msg");
    await expect(lines).toHaveCount(IN_CHAT.length);
    await expect(lines.locator(".mg-t")).toHaveText(words(R));
    await heads(lines, R);
    const first = lines.last();
    await expect(first.locator(".mg-tag")).toHaveText("Not done");
    await expect(first.locator(".mg-tag")).toHaveCSS("text-transform", "none");
    await expect(act.locator(".mg-hand")).toHaveCount(0);
    // Each has the one line every message has (#422): delivered, to whom.
    await expect(lines.locator(".msg-line.ml-delivered")).toHaveCount(IN_CHAT.length);
    await expect(first.locator(".cue-more")).toHaveText("· its log");
    await expect(lines.nth(IN_CHAT.length - 1 - 13).locator(".cue-more")).toHaveText("· 1 more item");
    // Folded: the standing words aren't in the line; a click opens Cue's full text under it, again closes it.
    await expect(first).not.toContainText("Its fixes stay");
    await expect(first.locator(".mg-full")).toHaveCount(0);
    await first.locator(".mg-lk", { hasText: "More" }).click();
    // Opened, it shows what matters, not the session's rules (#625, desk-cue-fold.spec): your words, the item, what's next.
    await expect(first.locator(".mg-full .cue-fold-v.said")).toHaveText("“The build failed”");
    await expect(first.locator(".mg-full")).toContainText("ui-tests fixes it and says it's done");
    await expect(first.locator(".mg-full")).not.toContainText("Its fixes stay");
    await expect(first.locator(".mg-lk")).toHaveAttribute("aria-expanded", "true");
    // No left stripe, no hover text.
    // Found and measured in one go: a redraw replaces the chat, and a locator finds it in one call and measures it in the
    // next, which a redraw can land between (then it measures the old line, gone: no box, no style).
    const border = await page.evaluate(() => getComputedStyle(document.querySelector(".dk-moves .mg.cue-msg")).borderLeftWidth);
    expect(border).toBe("0px");
    await expect(page.locator("[title]")).toHaveCount(0);
    await first.locator(".mg-lk", { hasText: "Less" }).click();
    await expect(first.locator(".mg-full")).toHaveCount(0);
    // Its item's number opens the item.
    await first.locator("button.mg-id").click();
    await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "d5"]);
  });

  test(`Cue's messages to the lead leave its chat; in Activity each is one quiet line, a click opening its full text (${scheme}) (#478, #616, #789)`, async ({ page, cue }) => {
    await cueSaidSetup(page, cue, scheme);
    // Cue's messages to the lead: out of its chat too (#688); in Activity each one quiet line (#616), "+ Cue · its
    // tag", its item's chip and age, its words under them; a click opens Cue's full text.
    await page.evaluate(() => deskShow({ kind: "chat", sid: "lead" }));
    const lchat = page.locator(".dk2b-chat .dk-chat");   // the lead's chat is the left card (#666)
    await expect(lchat.locator(".dk-cue, .mg.cue-msg")).toHaveCount(0);
    // Its start failure is Cue's note too (#789): in Activity, not the chat; your answer to its ASK stays with the ASK's card.
    await expect(lchat).not.toContainText("no agent");
    await expect(lchat).not.toContainText("The answer: Blue");
    if (process.env.SHOT_DIR) await lchat.screenshot({ path: `${process.env.SHOT_DIR}/desk-cue-said-lead-${scheme}.png` });
    // All of them at once (the tab shows 30 till you scroll; the session's lines are in it too).
    await page.evaluate(() => { deskMovesShown.set("cue", 200); deskOpenActivity(); });
    const act = page.locator(".dk-moves");
    const RL = [...IN_LEAD].reverse(), llines = act.locator(".dk-cue");
    await expect(llines).toHaveCount(IN_LEAD.length);
    await expect(llines.locator(".dk-cue-w")).toHaveText(words(RL));
    await expect(llines.locator(".dk-cue-t .mg-who")).toHaveText(RL.map(() => "Cue"));
    for (const [n, { id }] of card(RL).entries()) {
      if (id) await expect(llines.nth(n).locator(".mg-id")).toContainText(id);
      else await expect(llines.nth(n).locator(".mg-id")).toHaveCount(0);
    }
    const top = llines.last();
    await expect(top.locator(".dk-cue-w .mg-v")).toHaveText("· 2 choices");
    await expect(llines.nth(1).locator(".dk-cue-w .mg-v")).toHaveText("· 1 more line");
    await top.locator(".dk-cue-w").click();
    await expect(top.locator(".mg-full .cue-fold-v.said")).toHaveText("“There are leftover edits in main. Commit them?\n1. Yes: commit\n2. No”");
    await expect(top.locator(".mg-full")).toContainText("The lead answers it");
    await expect(top.locator(".mg-full")).not.toContainText("ANSWER: #d5");
    await expect(act).not.toContainText("The answer: Blue");
    await expect(act).toContainText("no agent");
    await expect(page.locator("[title]")).toHaveCount(0);
  });
}
