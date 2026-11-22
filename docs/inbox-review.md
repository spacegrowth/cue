# The Inbox next to the Desk: a review (#713)

22 November 2026. Nothing in the app changed for this review.

## The recommendation

Remove the Inbox. Keep one small thing from it: the count in the header, as "Needs you", which takes you to the
first thing waiting, on its Desk or on the Board.

The reason in one line: almost everything the Inbox lists today is Desk work that the Desk already shows, and the
parts that were the Inbox's own are not being used.

## How the Inbox gets used today

I read the Inbox's code and its history, and counted what the running app held at the time of writing (read only).

- **What it would list.** There were 306 sessions. 303 of them sat on a finished reply, so the Inbox's "Finished
  replies" section held about 295 rows. Only 8 sessions needed an answer. 6 of those 8 were Desk sessions, whose
  question also shows on their Desk item. That leaves 2 asks the Desk did not have.
- **What its count is made of.** The number on the Inbox button adds those 8 to the Desk items waiting to be
  checked, up to 17 more. Those rows are links: a click leaves the Inbox and opens the item on its Desk. So about
  two thirds of the count is the Desk's count, shown a second time.
- **Its own tools.** Pin, Snooze and Later: none of the 306 sessions had any of them set. Not one.
- **Where the work went.** The Inbox was built from 14 to 16 November, about twenty changes in three days. Since
  17 November, three changes were for the Inbox itself (grouping by Desk, the Open rows, the count). Every other
  change that touched it was an app-wide one (the refactor steps, text sizes, the one look for messages) that had
  to carry the Inbox along.
- **Where the sessions are.** 208 of the 306 sessions belong to a Desk. The Inbox was designed the week before the
  Desk had a lead, when sessions were loose and you answered each one yourself. That is no longer how the work runs.

## Where the Desk does the same job better

- **A question from a session.** The Inbox shows it as a row for a session. The Desk shows it on the item it is
  about, with what the item is for, what was asked before, and who else is waiting on it. The same question, with
  its context.
- **Something to check.** The Inbox can only point at the Desk for this. Looks right, Send back and the check
  steps live on the Desk.
- **Housekeeping.** The Inbox raises a session whose context is half full, for you to compact or hand off. On a
  Desk the lead does this without you.
- **Loose ends.** The Inbox side lists a crew's loose ends: finished but not committed, a stopped merge, a review
  not sent. For Desk work, landing does this: an item is not done until its changes are in.
- **Running and finished.** The Desk shows what is at work per item, with its steps. The Inbox shows it per
  session, and with 300 finished sessions that list no longer says anything.

## What the Inbox does that the Desk does not

Three things, said plainly, because removing it loses them unless they are kept.

1. **One number across everything.** The Desk home page shows how many wait on each Desk, but nothing in the
   header says "17 things need you" when you are somewhere else. This is worth keeping, and it is cheap.
2. **Sessions that are on no Desk.** 98 sessions, 2 asks at the time of writing. The Board already shows these in
   its Waiting column, with the same question card. Nothing is lost.
3. **Answering like mail.** The reply box under the message, and the next ask opening by itself after Send. It
   is pleasant, but it is a second way to do what the item page and the Board card already do, and it is the part
   that costs the most to keep working.

Loose ends for sessions on no Desk (the Open rows) would go too. They are shown nowhere else. With two thirds of
sessions on a Desk and landing in place, I would let them go rather than move them.

## What each choice costs

**Remove it (recommended).**
- You lose: the Inbox button, Pin and Snooze, the Open rows, the mail-style reply.
- You keep: every ask (on its Desk item or on the Board), Later and Parked (they are the Board's), and a
  "Needs you" count in the header that opens the first thing waiting.
- Size: about 1,300 lines of screen code, 380 lines of styling, 59 tests, and 350 lines behind the Open rows come
  out. A few pieces other screens borrow from the Inbox (the Open in Desk button on the lead's band, the list of
  changed files) stay and move to the Desk's side. One item, about a day, and it has to be built once with the
  add-on to see nothing there leans on it.
- Why now: the Desk comes first until it is reliable. Every app-wide change this week had to be made and tested
  in the Inbox as well. Removing it makes each of the next ones smaller.

**Keep it, with a few changes.** If you would rather have one page across all Desks, the Inbox would need to
become only that: asks and items to check, nothing else.
- Change: drop Finished replies, Running, Housekeeping, Pin and Snooze; answer an item's question and press
  Looks right in place, without jumping to the Desk; group by Desk.
- Size: this is a new page in the Inbox's place, not a few fixes. A mock first, then a build of several days, and
  the Inbox keeps costing on every app-wide change after that.
- Why not now: it is a feature build, and feature builds wait for the Desk.

**Leave it as it is.** No work now. It keeps showing the Desk's count twice and 295 finished rows, and keeps
adding to every app-wide change.

## If you turn the removal down

The cheapest useful step is still the header count: make "Needs you" in the header work from any screen. That can
be done with the Inbox in place, and it is the part of the Inbox that earns its keep.
