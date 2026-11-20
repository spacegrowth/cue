# One chat per item (#560)

A plan only. Nothing here is built. It follows on from `docs/desk-item-page-design.md` (the item keeps its own
thread, picked 18 Nov) and `docs/desk-item-chat-live.md` (why that thread doesn't feel live).

## Today: two chats for one piece of work

An item's page shows its own **thread**. It is not the session's chat. Cue builds it from the session's turns that
it thinks belong to the item, and it decides that turn by turn: the turn counts only when the message that started it
says "#560".

- **Type on the item page**: Cue adds "#560" and sends it to the session. Its reply shows in both places.
- **Type in the session on the left**: the session works and answers, but without "#560" the item page never shows
  that turn. You see a reply on the left and nothing on the item.
- **A session holds two or three items**: its chat mixes them, so the item page has to pick turns out. When it
  guesses wrong, words land on the wrong item or on none.
- **The thread redraws itself** every half second, cuts replies down at the end of a turn and lags behind the
  session's chat (#386).

So you have two places that look alike and show different things. You learn to open the session to be sure, and
then the item page is just in the way. Both options below fix this by making the item page **be** a chat, not a
copy of one.

## A. A session holds one item at a time; the item's page is that session's chat

**What you see.** Open #560 and you see the chat of the session doing it, starting at the moment it picked the item
up. It is the same chat as the session on the left, drawn by the same code: the same messages, live, in the same
order. The left row and the item page are two doors into one room.

**Where your message goes.**

| You type… | It goes to | You see it on the item page | You see it in the session on the left |
|---|---|---|---|
| on the item page | the session holding #560 | yes, at once | yes, at once |
| in the session on the left | the same session | yes, at once | yes |
| on the item page, nobody holds it yet | the Desk lead | yes, as a line "to the lead" | (no session yet) |

No "#560" tagging and no guessing: while the session holds #560, everything in its chat is #560's, because it holds
nothing else.

**A session keeps its item until you press Looks right.** It says DONE, the item goes to "ready to check", and the
session waits on it. Your questions, Send back and follow-ups all stay in the same chat, with a session that still
has the work in mind. Only Looks right lets it go.

**After Looks right.** The session is free and the Desk gives it its next item. Its chat carries on under that item.
The #560 page keeps its piece of the chat, from pick-up to Looks right, as a record, with one line at the bottom:
"Looks right, 11:52 · cue-268 moved on to #561". The box turns into **Reopen**.

- Reopen sends #560 back to cue-268 if it is free. Its page grows a second piece below a "Reopened, 15:10" line.
- If cue-268 is busy or gone (closed, or too full), #560 goes to a fresh session with a short hand-off note: what
  the item was, what was done, the files, and the last few messages. That session's chat is added under
  "Reopened with cue-301", so the page still reads top to bottom.

**The cost of this:** a session sits idle while its item waits for you to check it. With five items waiting on you,
five sessions sit idle. That's cheap in money (an idle session costs nothing until it's woken, and waking a warm one
is cheap), but it means more sessions on the Desk, and new work starts in a fresh session more often. A limit keeps
it in hand: a session whose item has waited for checking over a day lets go of it (the page keeps the record, and
Reopen works as above).

### The queue under A

Today an item gets a holder as soon as it's routed. #528 and #535 both sat on cue-496 before it began either, and
the session held both in its chat. Under A, **waiting items have no session**. They sit in the Desk's queue.

**What you see.** At the top of the Desk's list, a **Queue** group, in order. Each row says what it waits for, in
words:
- "#271 Copy code blocks · next for cue-268, which knows the chat page"
- "#272 Copy as Markdown · after #271" (waits for another item)
- "#273 Font sizes · any free session"

Nothing in the queue has a session yet, so nothing in it can mix into a chat.

**Who picks the next session.** The Desk lead, as today, by the routing rules: the session that last finished work
in that area goes first. The difference is *when*: the lead picks when an item can start, not when it's added. It
writes the pick on the row ("next for cue-268") so you can see it and change it, but the item doesn't move into the
session until that session is free. When a session is freed (Looks right), Cue starts the first queued item marked
for it, at once, with no need to wake the lead.

**Items that wait on each other.** "After #271" stays exactly as it is today (`after:`), but it now means an
item, not a place in a session's chat. #272 starts when #271 is checked (Looks right), not just DONE, because #271's
session keeps it until then. If both are marked for the same session, they run there one after the other, in order.
If #271 is reopened, #272 waits again only if it hasn't started yet.

**When the best session is busy.** A simple rule: **wait up to 20 minutes for the session that knows the area, then
go to a fresh session with a short hand-off.**
- The row shows the wait: "next for cue-268 · goes to a fresh session at 14:20".
- The hand-off is what the lead already writes for a moved item, plus the files the best session touched for that
  area, so the fresh session starts by reading those, not the whole repo.
- A free session with a record in that area beats a fresh one (rules 1–2 of the routing rules). A free session with
  no record never takes it: a fresh one starts cleaner.
- An item the best session is waiting to have checked does not count as "busy for a while": the clock still runs, so
  your slow check doesn't hold up the queue past 20 minutes.
- You can always say "wait for it" or "start now" on the row.

20 minutes because most items take 10 to 40 minutes, and a fresh session costs a few minutes of reading; the
figure is a setting, not a rule baked in.

**Context.** The session keeps everything it learned, item to item. That is what the routing rules want ("the session
that last finished work in this area goes first"). Nothing is lost by moving on. The cost is that each session's chat
grows over many items, and it hands off or compacts sooner; Cue's hand-off note already handles that.

**Crew members under A.** They do not become useless. They stop being item holders and become what they are best
at: helpers inside one item. A session on #560 can start two teammates (one on the parser, one on its tests); they
show as cards inside #560's chat, exactly as on the lead's crew page today. What goes away is the lead keeping an item
and handing it to one of its crew ("ROUTE: #id -> me | crew: …"). The Desk lead only routes anyway, so that path is
rarely used.

**What breaks today.**
- The routing cap goes from "one at work, one waiting behind" to "one held, at work or to check". Routing no longer
  gives an item a holder; it marks the session it should go to, and the item stays in the Desk's queue until then.
- Cue starts the next queued item when a session is freed, and moves it to a fresh session after the wait. Today
  the lead starts things.
- Looks right frees the session. Today it only ends the item.
- The item thread, its "#560" rule and the end-of-turn copy of replies are replaced by a slice of the session's chat.
  The record lines (hand-offs, Done, send-backs) are drawn into that slice.
- Items on the Desk lead itself (it holds none today) and add-on loops are not affected.

**What it costs.** Medium to large, about 4 to 5 days. Cue already records when a session took an item and when its
next item started (`held_from`, `held_before`, `held_window`). The work: draw the session chat inside the item page
for those windows (this is the C of #386, which we wanted anyway), about 2 days; the queue (items with a marked
session and no holder, the Queue group, start on Looks right, the 20-minute move with hand-off), about 2 days;
Reopen and joining pieces when an item comes back or changes hands, and the lead's new instructions, about a day.

## B. A crew member is bound to the item; the item's page is its chat

**What you see.** Each item gets its own crew member, started for it by a lead session. The item page is that crew
member's chat: what it was asked, its steps, its replies, live. It never does another item, so the page is all #560,
always.

**Where your message goes.** A crew member has no box of its own in Claude Code: only its lead can talk to it.

| You type… | It goes to | You see it on the item page | You see it on the left |
|---|---|---|---|
| on the item page | the lead, which passes it on to the crew member | yes, once passed on (seconds to a minute; longer if the lead is mid-turn) | in the **lead's** chat, as "to button-builder: …" |
| in the lead's chat on the left | the lead; it may or may not pass it on | only if passed on | yes |
| (the crew member itself is not a row on the left today) | | | |

So B fixes "two chats" only if the crew member gets its own row on the left, under its lead, and that row opens the
same chat as the item page. Without that row, the session on the left (the lead) still shows a different chat from
the item page: the same problem we have now, moved one level down.

**When the work moves on.** It doesn't: a crew member does one item and stops. The #560 page stays its chat for good.
Writing there later wakes it through its lead, with its whole context, as long as its lead session is still alive.
If the lead is gone, the crew member cannot be reached any more: a new holder starts from a hand-off note, as in A.

**Context.** The crew member starts nearly empty, with only its brief. It knows nothing of the area from earlier items,
so it re-reads files every time. The lead keeps the area knowledge but doesn't do the work. Each of your messages also
costs a lead turn to pass it on.

**Crew members under B.** They are the whole point: every item is one crew member. Leads become routers that hold
several crew, which is roughly what the Desk lead plus a session does today, with an extra hop.

**What breaks today.**
- Cue cannot write to a crew member directly; everything goes through the lead. A direct way in is new.
- Crew members' chats are not kept as chats today: Cue shows their last words and steps, not their replies over time.
- The left list has no rows for crew members; the "one chat" promise needs them.
- A crew member's question to you surfaces through its lead (the "stop with an open ask" case).
- Ready-to-check, Send back, commit and landing are all built around a session holding the item; they move to the
  member, whose work lives in its lead's worktree.

**What it costs.** Large, about 1 to 2 weeks: a direct way in to a crew member, keeping its chat, crew rows on the
left, reworking hand-offs and landing for members, and a rule for what happens to its items when a lead ends.

## The plain answer: does A make crew members useless?

No. Under A crew members stop **holding** items, but they keep doing what they do now inside an item: splitting one
piece of work across files in parallel, and fresh-eyed reviews. They show inside the item's chat. Only the "lead keeps
the item, a crew member does it" route goes, and the Desk barely uses it.

## Side by side

| | A: item = session | B: item = crew member |
|---|---|---|
| Item page and left row show the same chat | yes | only with new crew rows on the left |
| Your message from the item page reaches the worker | directly | through the lead |
| After the item is done | session keeps it until Looks right; then the page is a record, Reopen goes back | page stays; member stops |
| Waiting items | in the Desk's queue, no session; marked for the best one | each waits for its lead to start a member |
| Area knowledge carried over | yes, in the session | no, the member starts fresh |
| Parallel items | one per session; more sessions | many members per lead |
| Idle cost | a session per item waiting for your check | a lead stays alive while its members' items wait |
| Cost | medium to large, 4–5 days | large, 1–2 weeks |

## Recommendation

**A, with the session keeping its item until Looks right.** It answers the question directly: the item page and
the session on the left are the same chat, so wherever you type, you see it in both places, live. Checking and
follow-ups happen in that same chat. After Looks right the page is a record, and Reopen goes back to the same
session if it's free, else to a fresh one with a hand-off. Waiting items sit in the Desk's queue with no session;
the lead marks each for the session that knows its area, and it goes to a fresh session after 20 minutes if that one
is still busy. It keeps what sessions know from one item to the next and reuses the session chat we already draw
well. The price is idle sessions while items wait for your check.

B gives a cleaner one-item life, but only after building a way to talk to crew members directly and giving them
rows of their own; until then it repeats today's split one level down. Crew members stay under A, as helpers inside
an item.

Build order: the item page as the session's chat first (it fixes the two-chats problem on its own), then the queue.
