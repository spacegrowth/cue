# The chat: older build vs today

Older version compared: 7b31df4a9 (installed 20 Nov 2026, 21:19). The Desk lead assumed this one while you were away; say if it was a different one.
Today: main at 968e8ef9c. Since then only small chat follow-ups landed (more of #604's one-line steps, the "hasn't
started yet" card #626, plain green/red diff colours in both themes #647); none changes an entry below.

Both builds ran in the screen-test harness with the same made-up chat, same window (1400×900), text size Compact,
steps "one line". Screenshots are NN-topic-old.png / NN-topic-new.png. Nothing in the chat was changed.

Two parts: how much Desk activity fills the chat, and how messages are delivered and shown.

## Desk activity in the chat

The same run of Desk activity in both builds: the lead starting, moving and reopening items; you sending items back
with and without a note; two checks that looked right; one item Cue sent back itself; a Ready to check card; four
"Desk changes" messages; a "Done?"; an answered ASK. Screens 17, 18, 19.

**Total room it takes in the lead's chat: Then 1597 px, Now 1133 px.** All 464 px saved comes from #616 folding
Cue's messages. Everything else is unchanged since the older build, so today's "too much" isn't new: it was there
before too. Biggest blocks today: the Ready to check card (~85 px), the "sent back" note with paths (~75 px), each
folded Cue message (~50 px, two lines).

A1. **The lead's own acts** ("Started #d3 with web-lead", "Reopened #d7" + why): the same in both. One line each,
    two with a why. From #360, #433, #452.
A2. **Your checks** ("You · sent #d5 back · no note", "You · looked at #d8 and said it looks right · Committed … ·
    Undo · back to check"): the same in both. One or two centred lines each. From ba6321004, #410.
A3. **"You sent back" on items Cue sent back itself, with file paths** (was #644)
    - Both builds: "You · sent #d3 back: “Its work isn't on main yet … 2 commits in /work/cue/.claude/worktrees/desk-3 …”",
      about 3 lines, though you never sent it.
    - Cue sends an item back itself when its work isn't on main (landing/finish.rs, #448, #481), when an install
      stops (landing/install.rs), or when a later commit took its work out (landing/main_line.rs). The check line
      (ui/desk/commit.js) always says "You" and quotes the note whole; the record doesn't say who sent it back.
    - Fix: small to medium. Record who sent it back, say "Cue sent #d3 back: its work isn't on main", keep paths on
      the item page.
A4. **Ready to check card**: the same in both, ~85 px, the tallest single block. From #501.
A5. **Cue's "Desk changes" messages**: Then a full bubble, 4–7 lines plus "Delivered" (~105–175 px). Now two
    lines, "+ Cue · Desk changes [Bug #d5] 40s" / "#d9 finished · #d8 changed" (~50 px); + opens the full text.
    A "landed and installed" row reads only "changed". Changed by 968e8ef9c (#616). Back: small.
A6. **Cue's "Done?"**: Then a card with Delivered and More (~85 px). Now two lines (~50 px). #616.
A7. **An answered ASK sent back to the lead**: Then a full bubble. Now two lines. #616.
A8. **On the item page**: the same 136 px in both (your send-backs only).

## Messages

1. **Your message on an item page shows only your words, under a small item header**
   - Then: your words wrapped in Cue's instructions ("About #d5 … Work in its own copy …"), a long bubble.
   - Now: a header line "Bug #d5 · Hosted sessions can't open the canvas", then only "make the canvas open at once".
     The session still gets the full text.
   - Changed by: f83fcca3e (#605). Back: small.
   - Screens: 02, 14.
2. **Your bubble is wider**: it starts ~50 px further left, so lines run longer. Commit not traced. Back: small.
   Screens: 01.
3. **A message from another Desk's lead shows as a card** ("FROM ANOTHER DESK"): Then a plain message.
   Changed by: cb8f211dc (#540). Back: small. Not in the screenshots.

Voice messages to the lead (#617) look the same in both builds on screen: #617 changed where they're delivered,
not how they show.

## Steps

4. **Every turn's steps fold into one chip**
   - Then: the newest turn listed its steps open, one per line, with "Show all"; older turns a summary line.
   - Now: every turn is one chip, "Working · 3 steps · 2 commands, 1 edit ▸"; each reply ends in "· N steps".
   - Changed by: 441ba3b75 (#542). Back: small for "newest turn open" (Settings → Steps "Open" opens every turn);
     medium for the exact old mix.
   - Screens: 01, 09, 13.
5. **The chip's wording changed**: Then "▸ 4 steps · read 2 files · 1 search · 1 command · 1 failed" (triangle left).
   Now "4 steps · 1 command, 2 reads, 1 search, 1 failed ▸" (triangle right, different order). #542. Back: small.
   Screens: 02.
6. **"Show all" is gone from an opened turn**: the step rows look the same; only the step you click opens.
   Changed by: 0f378520a (#604). Back: small to medium. Screens: 03.

## Tool output

7. **Command output runs on instead of sitting in a box**
   - Then: the command cut to one line with "…"; output in a box capped at 360 px that scrolls sideways (a long
     panic line runs off the right edge).
   - Now: the command wraps onto several lines; output wraps (one long line becomes 3) with no height cap, so all
     39 rows run on down the chat.
   - Changed by: 441ba3b75 (#542). Back: small (styles only). Screens: 05.
7b. **Diffs are brighter**: Then muted grey-green and pale pink rows on a pale panel, thin −/+. Now brighter green
    and pink, a bordered panel, bold +/−. Changed by: 441ba3b75 (#542); colours 219b27407 (#527). Back: small.
    Screens: 04.
8. **A command the agent asks to run shows whole** — Then: 4 faded lines and "Show all 9 lines" next to
   "✓ Allowed". Now: all 9 lines, no fold. Changed by: 441ba3b75 (#542). Back: small. Screens: 20.

## Folding

9. **The item page shows only this item's part of the session's chat**
   - Then: the session's whole chat, other items' turns folded.
   - Now: only the times the session held this item, plus your turns naming it; "moved on to #X" and "Before this,
     <session> had it" lines; earlier messages load until the item's start is in view.
   - Changed by: 9c41e7368 (#575). Back: medium. Screens: 13, 14.

## Sending

10. **No "Waiting on you" line under a message still on its way to the lead**: Then "Sending…" and under it
    "Waiting on you for your answer: …". Now "Sending…" and nothing under it. Changed by: a1c2f423d (#587).
    Back: small. Screens: 16.

10b. **A message waiting to send has a dashed outline; you asked for dotted** (was #623)
    - Both builds: your bubble while sending, queued or kept in Cue has a 1.5px dashed outline (messages.css,
      `.msg-sending/.msg-held/.msg-queued`); a waiting card has a 1px dashed one. It came from #422 and #501, both already in the older build.
    - Fix: small, two style lines (dashed → dotted). Screens: 11, 16.

Typing, Sending… and Delivered look the same in both (screens 10–12), and both keep you at the bottom or where you
scrolled when a reply comes in (08, 09). Both show "via via tmux pane" on Delivered: a bug in both, not a difference.

## Status

11. **The status line under the chat is gone**
    - Then: "Running npm test -- canvas · 31s · ⌘D tells you when it's done" under the chat, in session chats and on
      item pages.
    - Now: no line. While steps run, the newest step sits at the foot of the chip with a dot and its time. While the
      session works without steps, nothing shows that it's working (screen 08).
    - Today's foot step says "Running /work/cue/ui/canvas.js" while the board card says "Running npm test -- canvas":
      they disagree.
    - Changed by: 441ba3b75 (#542), 0f378520a (#604). Back: small.
    - Screens: 01, 08, 11, 13.

## Text and layout

12. **Text size changes only the words**: Then it zoomed the whole window. Now three sizes; the window doesn't zoom.
    An old zoom of 0.9 reads as Compact, 1.0 and 1.15 as Default, 1.3 as Large. At Compact both builds draw 13px text,
    so the screenshots look the same size. Changed by: 368ae4271 (#594). Back: medium.
13. **The item page header doesn't fit**: today the Chat / Record / Decisions / Files tabs sit ~28 px higher and cut
    the "Open host-lead" and "Drop this item" buttons in half. Likely the header-buttons height change in 219b27407
    (#527); not confirmed. Fix: small. Screens: 13, 14, 19.
