# Desk routing rules (proposed)

How the Desk's lead should pick a session for each item. These are proposals from a review of one
day of Desk records. Nothing here is built yet. The 150k and 30-minute figures are new; neither
is a Cue setting today.

**The sizes here are superseded.** The 150k limit and the 180k hand-off in rules 3 and 4 and in
"Fit with the context bands" are replaced by the sizes in docs/desk-routing-merged.md (its
rule 17). Since 2026-11-21 sessions compact near 500k and the hand-off mark is 400k (#687).

**Open** means at work or waiting in line. An item waiting for your check does not count toward
the cap of two below: Cue won't queue anything behind it either. If you send it back, it is at
work again and counts. This is narrower than Cue's hand-off note from 100k, which counts every
item a session holds that isn't done, items waiting for your check included. That stays as it is.

**On the Desk.** The lead picks among sessions on the Desk. A session that isn't on the Desk
still means Cue asks you first, as today. A closed session can't take work, so its record
doesn't count. (The idle sessions in the review were all on the Desk.)

## Step 1: who takes it

1. **The session that last finished work in this area goes first.**
   A new item has no files yet: Cue learns them only once a session works on it. So the lead
   matches the item's title and screen against finished items and their files. Read that from the
   record, never from memory and never from "warm".
   Why: it knows where things are, and the record can't misremember.

2. **When two sessions both have a record:** the one with nothing open wins; then the one that
   finished work there most recently; then a warm one over a cold one; then the smaller one.
   Why: a free session starts now, recent work is the freshest, and warm is cheaper to wake.

3. **Cold or idle is not a reason to skip it.** Wake it if it holds under 150k. Warm counts only
   in rule 2's order, between two sessions that both have a record.
   Why: a cold session re-reads its context once. That costs far less than an item waiting an hour.

4. **One item per session, at work or waiting for your check.** A route to a busy session queues the
   item for it: it starts there once that session's item gets Looks right, or in a fresh session
   with a hand-off after 20 minutes. If the best fit holds 150k or more, take the next session with a record.
   Why: one chat about one thing. A second item in the same chat mixes their replies.

5. **Start a new session when** no session passes rules 1 to 4, when you ask, or when the work
   needs fresh eyes (a review). Never wake a cold session for work it has no record in.
   Why: a new session starts near empty. An unrelated cold one re-reads its old work first.

6. **When the match is unsure, say so** in the why line ("picked by area, not sure").
   Why: a guess is fine when it is marked as one.

## Step 2: now, or after another item

7. **If the session picked has an item at work, the new one waits behind it** (`after:` that item).
   That is its one waiting place.

8. **If the item would change the same files as an open item in another session, it waits behind
   that item** (`after:`), and still goes to the session step 1 picked. Otherwise it starts now.
   "Same files" is the lead's judgment: the new item's area against the files it can see on open
   items. It is not a lookup.
   Why: the queue stops two sessions editing one file. It is not a way to keep work in a busy session.

   An item waiting for your check can still have uncommitted edits in those files, and Cue
   won't queue behind it. Then the lead names the clash in the why line and starts the item; if
   the two would plainly undo each other, it asks you first with an ASK.

   When rules 7 and 8 both apply, `after:` takes only one item: name the one expected to finish
   later, and say the other in the why line.

9. **If the item ahead has been open over 30 minutes, the lead asks you first:** wait, or start a
   smaller piece on other files now. It asks with an ASK and those two choices. With no answer,
   the item waits, and the lead's line says how long the item ahead has been open.
   Why: a few waits ran past an hour and nobody was told.

10. **The why line names the record.** Example: "cue-125: built #160 and #165 on the item page,
    nothing open, idle 5 h."

## Send back and Reopen

Cue routes these itself; the lead doesn't pick a session for them (docs/item-chat-plan.md option A,
step 3).

11. **Send back and Reopen go to the session that held the item.** Send back is on an item waiting
    for your check; Reopen is on a done item, with what isn't right typed in the box first. The
    words go to that session, the item is at work again with it, and on a done item its page shows
    the new part of the chat under "Reopened · <time>".
    Why: that session already knows the work.

12. **A fresh session takes it when that one can't.** That is when the session is closed, or, for a
    done item, when it is at work on another item (working, queued or finishing). The fresh
    session gets a hand-off: who did the item, its commits on main, its files, and your words.
    An item that still has its own copy stays with its session (rule 7 queues it there).
    The lead can do the same for an item to check (#784): a route to the session that did it finds
    it by the item's saved holder even when it's no longer on the Desk's list, and when that session
    is closed, the route (to its name, or to new) goes to a fresh session with the same hand-off and
    the lead's why, and its work on another machine carries over. With no why it's refused.
    Why: a done item's session has often moved on; waiting behind its new work would hide yours.

## What Cue would need to change

- **The lead's instructions.** At the time of the review they said "Route new work to a small or
  warm session over a big cold one". That is the opposite of rules 1 and 3, and the lead mostly
  can't see size anyway (see below). The instructions must give these rules instead.
- **Keep an item's files when it is finished.** Today finished items carry no files, and the lead's
  list leaves finished items out. Rule 1 has nothing to read until Cue stores the files (from the
  item's commit) and shows each session's last few finished items.
- **Show three things the lead can't see today:** each session's size, always; its open-item
  count; and its idle time. Today the lead sees a session's size and cache only while a hand-off
  is pending for it ("142k, cache 12m → Cue will hand off at expiry"; `context_note` in
  `src-tauri/src/desk/hub/lead/run.rs`). Rules 2, 3 and 4 all need the size. It already sees each
  item's holder, what it waits behind and up to 5 files, and when a session is working.
- **Refuse a third open item.** Cue tells the lead why and to route it again, as it does after a
  failed route, so the item doesn't sit as new.
- **Fit with the context bands.** 150k sits between Cue's existing marks: a hand-off note from
  100k for a session holding an item, the hand-off to a fresh session at 180k, and compacting from
  200k. A woken session that reaches 180k is handed off as today. To settle before building: an
  item routed to a session marked "will hand off at expiry" must move with the hand-off.

## Worked examples

Sizes are from the lead's own list that morning; waits are from the times of its decisions.

- **#176, an item's chat shows messages about other items.** Two sessions had a record: cue-125
  built the item's thread (#160, #165), cue-124 built the lead's chat (#147, #151). It went to
  cue-124, which had one item at work and one waiting. cue-125 had nothing open and was cold at
  about 145k. Rule 2 picks cue-125; rule 3 wakes it.
- **#178, an item's mocks are hard to find on its page.** It was queued on cue-124 as its third
  open item. cue-125 built that page's header (#134, #142, #163). Under the rules it goes to
  cue-125 and waits behind #176 there (rule 7).
- **#174, the hand-off shows as a wall of text in a session's chat.** It was third in line on
  cue-124 (behind #172 and #173) and waited about 25 minutes. cue-124 was the best fit but had two
  open, so rule 4 moves on: the next session with a record there, or a new one (rule 5).
- **#179, session rows don't look clickable.** New session because you asked; done in about
  9 minutes in a chat about one thing. Same under the rules (rule 5).

## Tests while you work

This one is built (#778): Cue puts it in the brief every session gets with its item, so it holds
without the lead repeating it.

13. **A session runs only the tests its change touches.** It runs the tests for the files it
    changed and for code that uses them, never the full suite: the full suite runs after landing.
    Why: a full run on every item made items slow; the run at landing still catches what the
    narrow runs miss.

    Each Desk picks this in its settings, under Tests (#780): All tests (a new Desk's), Only what
    changed (Cue's own Desk), or Changed, all before done (only what changed while it works,
    then the full suite once just before its DONE). The brief says whichever is picked.
