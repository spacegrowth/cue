# Option C of the Desk-like-the-Board mocks: a review

22 November 2026, for #734. Written by Fable in a fresh session, from the canvas page of #726 ("Desk layout"), today's Desk as its real screenshots on that page, and the Board as it is in the app. No new mocks were made.

## The verdict

Don't build C as drawn. Keep everything in it above the bottom strip, and take D for the bottom strip. D is what you picked earlier today, and I think that pick was right: D is C with C's one real flaw fixed.

You have been through many rounds on this: B, C and D, moss against the old column, the panel shown and hidden, three kinds of machine pill. That page now holds 18 frames. Nothing I found in C is a reason for another round. The five suggestions below are all small, and all of them can go into the build of D without a new mock. I'd call the layout settled.

## What C is

Three columns on the moss green.

- Left: the sessions, sitting right on the green with light text, grouped by machine, with the lead as a soft block pinned on top.
- Middle: the chat as a white card, with its panel kept at its right and a Hide panel button.
- Right: Items. Waiting on you and Working share one white card that never folds. Under it, Up next and Done today sit side by side on the green, with no card, and each can fold to its heading.

## What I would keep

- **White for what is live, green for what can wait.** The one white card on the right holds only what needs you and what is running. Up next and Done today fall back onto the green. Your eye lands in the right place first. Today's Desk gives all four groups the same weight in one long white list.
- **The sessions column.** Today's Desk has no list of sessions at all; you learn who is working only from the panel. C gives the Desk the same thing the Board has: every session, its dot, and what it is on. This is the change that makes the Desk feel like the Board.
- **Machines as headings.** "This Mac, 3 of 4 busy" says once what a pill on every row said many times.
- **The lead pinned on top** of the sessions, lit when its chat is open. It is always one click away.
- **The panel kept as it is today**, with Hide panel. No new thing to learn.
- **Light text on the green reads fine.** I measured the faintest text on the moss at about 6 to 1 in light mode, well above the 4.5 it needs.

## What is wrong with C

The trouble is all in the bottom strip.

1. **Side by side, folding buys nothing.** Fold Done today and Up next gets no wider and no taller. The space just becomes empty green. On the Board, folding a drawer hands its room to Sessions. In C the fold is decoration.
2. **Side by side halves the titles.** The frames are drawn 2560 wide. There, each list gets about 480. On a laptop-width window, once the sessions column and the chat have taken their share, Items has roughly 470 in all, so each list gets about 220. A title like "Rail dot blinks for a reply you didn't send" shows three words.
3. **The canvas disagrees with itself about the fold.** The close-up shows a small button at the right of each heading and the word "folded". The full frames show the Board's arrow at the left of the heading. Only one of those is the Board's.

D fixes all three: the two lists are stacked under Working as drawers, the whole heading line is the fold, and opening one closes the other.

## Where C clashes with the Board

- **The fold.** The Board folds a drawer when you click anywhere on its heading line, the arrow is at the left, and only one drawer is open at a time. C's close-up has a separate button at the right and lets both stay open.
- **Drawers stack on the Board.** Starred, Recently answered and Parked sit one under the other at the foot of a column. C puts its two lists side by side.
- **Sessions are on the other side.** On the Board the sessions are the right-hand column. C puts them at the left. I would leave this alone: on the Desk the right-hand side is needed for Items, and the left is where you'd look for "who" before "what". But it is a difference, and you should know it is there.
- **Long lists.** The Board never shows half a card and says "3 more" at the bottom of a cut-off list. C's lists on the green show no such thing. They should behave the same.

## Five suggestions

Each one says what it changes and what it costs. None needs a mock.

1. **Stack Up next and Done today under Working, and fold them the Board's way.** This is D. It changes the bottom strip from two half-width lists to two full-width drawers, one open at a time. Cost: nothing extra, since D is already being built this way.

2. **Done today starts folded.** Today's Desk already keeps it to one line, and that is right: finished work is the thing you need least. Opened, it shows the latest few and then "N more", as the Board's lists do. This changes what you see first under Working to Up next. Cost: small, about two hours.

3. **Trim the lead's panel now that the sessions column exists.** With the lead's chat open, "2 wait on you" appears in five places in C: the header, the lead's block, the sessions column, the panel, and Items. "Working 4" appears in three. Keep Now and Activity in the panel, with its Check button; drop its Waits on you and Working lists, which the columns on either side now show better. This changes the panel from four sections to two. Cost: small, about half a day, mostly updating the tests that look for those lists.

4. **Decide what happens in a narrow window before building, by reusing today's rule.** Today the Desk never lets Items go under 520 wide. Keep that rule: when the window is too narrow for all three columns, the sessions column is the one that gives way, shrinking to a strip of dots or hiding behind a button. This changes nothing on a big screen and keeps the laptop usable. Cost: half a day to a day. This is the one I would not skip, because every frame on the canvas is drawn at 2560 and none shows a laptop.

5. **Say out loud what C removes from today's Desk.** Today's Items has tabs (All, Waits on you, Up next, Done today), a List and Cards switch, and a place for calls that came while you were away. C shows none of them and no note says so. My recommendation: drop the tabs, since all four groups are now on screen at once; keep the calls-while-away list as a third drawer when there are any; keep the Cards switch only if you still use Cards. This changes Items' header to a single line. Cost: small, about half a day, mostly removing things and their tests.

## Smaller things to watch for

- **The counts in the mock disagree.** The header says "1 asking you" while everything below says two wait on you. In the build they must come from the same count.
- **The step track is wide.** C names all six steps across the top of Items, with a time under each. Today's Desk uses a short six-dash bar. The wide track is fine at 1000 and will not fit at 520. Fall back to today's bar when Items is narrow.
- **Dragging on the green.** The row you are about to drop onto is marked with a very faint lighter band. Make sure it can be seen in dark mode, where it is fainter still.
- **Two looks to keep alive.** D is behind a "Try the new Desk layout" switch. That is right for trying it, but every day both looks exist is a day of fixing things twice. Set a date to remove the old one.

## In one line

C's bones are right and its bottom strip is not; D already fixed the bottom strip; build D with the five small changes above and stop mocking.
