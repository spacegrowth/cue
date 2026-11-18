# Desk routing: one merged set of rules (#188)

This is a review of the cost-aware strategy from #187 (docs/desk-token-strategy.md) by the
session that wrote the routing rules for #180 (docs/desk-routing-rules.md). It says what holds
up, what to change, where the two disagree, and then gives one set of rules. Neither of those
two files has been changed. Nothing in Cue has been changed. You pick.

## Do its numbers hold up?

Mostly yes. I counted again from the session logs for the same 28 hours, with Cue's own prices.

- Total: I get 453 dollars before the Fable sessions, which Cue has no price for. It says 464
  with them. That fits.
- Prices: a cold wake at 7 dollars 80 per million and a warm read at 20 cents per million match
  Cue's price list.
- Cold wakes: I find 23, costing 16 dollars 70. It says 25 and 16 dollars 50. Same picture.
- The second Desk lead: I get 12 dollars 85. It says 13 dollars 10.
- Sessions compact on their own at about 166 thousand: confirmed. The middle value in the logs
  is 168 thousand. So Cue's hand-off at 180 thousand never happens. My rules said it would. That
  was wrong, and the strategy is right.
- Compactions: I count 103 marks in the logs, it says 77. Mine include crew and helper logs, so
  its cost for compaction is, if anything, low.

Not checked: the split by kind of work, the "130 lead turns that decided nothing", and the
cost of an item in a fresh session against one in a session that worked nearby (1 dollar 74
against 64 cents). Those depend on how it sorted messages, which I did not redo.

## What I agree with

- Compaction in the middle of an item is the real waste, not cold wakes. The size limit should
  come from the 166 thousand mark, so a new item must fit under it.
- The usage limit caused nearly every cold wake. My review missed that. Routing can't fix it;
  slowing down before the limit can.
- A quieter Desk lead. It is woken for changes it does nothing about.
- Keep-warm pings only for a session with work promised to it, and built last.
- "Done?" sent once, one review and one re-check per crew, no second crew for the same work.
- The record of who built what should follow a hand-off to the session that took over.

## What I would change, and why

1. **The 110 thousand limit is right for new work, but not for a send-back or a small
   follow-on.** By its own numbers, knowing the area saves about a dollar ten per item. A cold
   wake costs that much only at about 140 thousand. So a session should keep its own send-backs
   and direct follow-ons up to 140 thousand, and take no new work over 110.

2. **The biggest saving rests on something not yet seen working.** 18 dollars a day assumes a
   fresh session with a note does as well as the session that was there. The strategy itself
   says neither of Cue's two hand-off notes was written. Treat this as a trial: watch the cost
   of the first ten items after a hand-off before counting the saving.

3. **"No item waits longer" is not quite true.** Starting nothing new at 90 percent of the usage
   limit makes items wait. That is a fair trade against a two-hour stop, but it should be said.
   The lead also can't see how much of the limit is used today; Cue would have to show it.

4. **The lead handing itself off at 80 thousand must come after the record is shown.** Today the
   lead picks from memory, and this morning its memory was wrong. A fresh lead has no memory at
   all. That is fine once its list shows who built what, and harmful before.

5. **Drop the "50 thousand smaller wins" step in the tie-break.** It saves under a dollar a day
   and makes the pick harder to predict. Keep one fixed order.

6. **Its build order starts with the limits.** I would start with what the lead sees: open
   items per session, idle time, and each session's finished items. Every rule in both files
   leans on that.

## Where it conflicts with the #180 rules

- **Size limit:** 110 thousand against 150. The strategy wins, with the exception in change 1.
- **Hand-off mark:** the #180 rules relied on the hand-off at 180 thousand. It never acts. Move
  it to about 140.
- **Tie-break order:** it puts size before recency. I keep recency first.
- **More fresh sessions:** the #180 rules start a new session only when nobody fits. The
  strategy starts one after every hand-off. These agree only if the note carries the record.
- **The lead's memory:** the #180 rules say "read the record, not memory". The strategy shrinks
  the lead's memory further. Same direction, but the order of building matters (change 4).

## The merged rules

"Open" means at work or waiting in line. An item waiting for your check does not count.
The lead picks among sessions on the Desk. For any other session, Cue asks you first.

**Who takes it**

1. The session that last finished work in this area goes first. Read it from the record, not
   from memory. If that session was handed off, the session that took over has its record.
2. When two sessions both have a record: the one with nothing open; then the one that finished
   there most recently; then a warm one over a cold one; then the smaller one.
3. A session takes new work only under 110 thousand. Cold or idle doesn't matter under that.
4. A send-back, or a direct follow-on to an item it just finished, stays with the session up to
   140 thousand.
5. A session over 110 thousand finishes its item, writes a note, and a fresh session with that
   note takes the next item in that area.
6. A session holds at most two open items: one at work, one waiting. If the best fit has two,
   take the next session with a record.
7. Start a new session when no session passes rules 1 to 6, when you ask, or when the work
   needs fresh eyes. Never wake a cold session for work it has no record in.
8. When the match is unsure, the lead says so in its why line.

**Now, or after another item**

9. If the session picked has an item at work, the new one waits behind it.
10. If the item would change the same files as an open item in another session, it waits behind
    that item, and still goes to the session picked. If both waits apply, it waits behind the
    one that will finish later. If the clash is with an item waiting for your check, the lead
    names it in the why line, or asks you.
11. If the item ahead has been open over 30 minutes, the lead asks you: wait, or start a smaller
    piece on other files. With no answer, it waits.
12. The why line names the record and the size. For example: "cue-125: built 160 and 165 on the
    item page, nothing open, 95 thousand."

**Running the Desk for less**

13. Near the usage limit, at about 90 percent, the lead starts no new item and no crew. Running
    items finish. After a stop, it wakes only sessions holding an open item, nearest to done
    first. This one does make items wait.
14. Cue wakes the lead only for what it acts on: a new item, your message, an ask, a DONE, a
    send-back, a crew report. The lead hands itself off at 80 thousand, once its list shows the
    record.
15. One review and one re-check per crew, then it comes to you. "Done?" is sent once per item.
    No second crew for work a crew already holds.
16. Cue pings a session to keep it warm only when work is promised to it: an item queued behind
    another session's item, or an item waiting for your check. Up to four times. Never during a
    limit stop.

## Build order

1. Rewrite the lead's instructions to these rules, and show it each session's open items, idle
   time and finished items with their files.
2. The 110 thousand limit, the hand-off between items, and Cue's hand-off mark moved to 140.
   Watch the first ten items after a hand-off.
3. The quieter lead, then its own hand-off at 80 thousand.
4. Show the lead how much of the usage limit is used, and the 90 percent rule.
5. "Done?" once, review rounds, no second crew.
6. Keep-warm pings.
