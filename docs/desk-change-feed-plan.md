# A Desk change feed that wakes the lead on every change

Plan for Desk item #379, 2026-11-19. Nothing built.

## In short

Today the lead hears about Desk changes through about eight separate messages, each sent from its own place in the
code. A message that can't go out is written to the log and lost. Some changes never reach the lead at all.

Suggested fix (option B below): one ordered list of Desk changes, saved with the change itself. Each lead has a
"read up to here" mark. One sender hands the lead everything past its mark, in order, whenever the lead is free. The
mark only moves once the message has gone out, so nothing is skipped. About 2–3 days, in three steps.

## What the lead hears today, and what it misses

| Change | Today |
|---|---|
| New item to route | Sent, grouped over 1 s. Lost if the lead has ended. |
| Stopped (holder quiet 10 min, or stopped on a question) | Sent once per item. Lost if the lead is busy at the wrong moment or limited. |
| Away mode notes | Sent if the lead is live and not limited; otherwise only in the next summary. |
| Needs you (a holder asks you something) | Shown to you; the lead isn't told (only a question left with uncommitted work goes to it). |
| Clash with main on landing | Goes back to the holder (and #371 makes that the rule). The lead isn't told, so a holder that's gone or full leaves it stuck. |
| Stuck landing (a finishing step that doesn't move) | Nobody is told. You notice it on the Desk. |
| Finished (DONE, landed, installed) | The lead isn't told. |
| Permission stop (a session waiting on a permission it can't grant) | Only in the Away list (#368). |

Three things cause the misses:
- Each place decides by itself whether and how to tell the lead. There's no shared list.
- A send that fails (lead ended, limited, mid-turn) isn't tried again.
- Cue keeps no record of what the lead has already been told, except a per-item "told" mark for stops.

## Which changes wake the lead

Wake at once:
- **Needs you**: a holder asks you something. The lead can answer it from what it knows, or bring it to you.
- **Clash**: landing hit a clash with main. After #371 the holder gets it first. The lead is woken too, so it can
  step in if the holder doesn't move.
- **Stuck landing**: a finishing step hasn't moved for 15 minutes, or a check failed twice.
- **Stopped**: as today (10 quiet minutes, or a stop on a question).
- **Permission stop**: a session waits on a permission. The lead never grants it; it moves other work along and
  makes sure you see it.
- **New item**: as today.

Wake, but grouped:
- **Finished**: DONE said, landed, installed. These go out together with the next wake, or on their own after
  5 minutes if nothing else comes. Each wake costs the lead a turn, and finished work rarely needs it to act.

Never wake:
- A holder's progress steps, its plain words, and your own clicks on the Desk (you already know).

## How the changes reach the lead in order, without missing any

1. **One list, saved with the change.** Every change above adds a row to a `desk_feed` list: a running number, the
   project, the item, what happened, in plain words. The row is saved in the same database write as the change
   itself, so the change can't happen without its row.
2. **A "read up to here" mark per Desk.** Saved too, so it survives a restart.
3. **One sender.** When the lead's turn is over (or a new lead starts), the sender takes every row past the mark,
   oldest first, and sends them as one message. The mark moves only once the send went through. If the lead is
   mid-turn, the rows wait for its turn to end; nothing is typed into a busy session.
4. **Checked against the item before sending.** The item stays the one record of truth (as #293 asked). Before
   sending, the sender reads each item again and leaves out rows that no longer apply (a stop already settled, an
   item dropped). Two rows for the same item and the same kind fold into the latest one.
5. **A lead that's gone, limited or replaced** keeps its mark. The next lead, or the same one once it's back, gets
   the backlog in its next message, next to the Desk summary Cue already puts before each one.
6. **A backstop.** The existing 10 s finishing tick also checks for rows past the mark, so a missed wake-up costs at
   most 10 s.

The per-item "told" mark for stops, and the 1 s grouping for new items, both become part of this and go away.

## How it fits #293 (the libraries review)

- #293's first suggestion was one wake-up (a `tokio::sync::Notify`) that runs the finishing work as soon as
  something changes. The feed uses the same wake-up: "something changed on the Desk" runs the finishing work and
  the sender. One mechanism, not two.
- #293 said no to a job-queue library (apalis) because it would keep a second copy of each item's state. The feed
  doesn't: its rows are notices, not state, and they're checked against the item before they're sent (step 4).
- #293's second suggestion, item states and finishing steps as Rust enums, gives the feed a fixed list of kinds of
  change. That isn't needed first, but makes the "which change wakes" table checkable at build time.
- #293's third suggestion, file watches, is option C below.

## How it fits #371 (a clash goes back to the session)

- #371 decides who gets a clash: the holder, with what clashed. The feed doesn't change that.
- The feed adds a "clash" row so the lead knows as well. If the holder can't take it (gone, limited, too full) or
  hasn't moved in 15 minutes, a "stuck landing" row follows and the lead steps in: hand it to a new session
  (#364), or bring it to you.
- "Send back always works" (#371) is a button for you; it isn't affected.
- Build this after #371 lands, so the clash row is added where #371 hands the clash back.

## Options

**A. Fill the gaps where they are.** Add a direct message to the lead at each missing place (clash, stuck
landing, finished, permission stop), and retry a failed send a minute later.
- Cost: about 1 day, ~150 lines.
- Still no order across changes, still lost if the lead ends or is limited, and a ninth and tenth separate path to
  keep in step.

**B. One ordered feed with a "read up to here" mark (suggested).** As described above.
- Cost: about 2–3 days, ~400 lines with tests. Done in three steps:
  1. The list, the mark and the sender, with stops and new items moved over. Tests: order kept, lead busy, lead
     ended and replaced, Cue restarted mid-way.
  2. The missing kinds: needs you, clash, stuck landing, finished (grouped), permission stop. One test each.
  3. The #293 wake-up drives both the finishing work and the sender; the 10 s tick stays as backstop.
- Gains: nothing is missed, the lead hears things in the order they happened, and one place to change what wakes it.

**C. B plus file watches.** Watch the install status files and main's branch, so an install finishing or a commit
on main adds its row at once instead of at the next tick. This is #293's third step.
- Cost: 1–2 more days on top of B.
- Gain: seconds, not correctness. Worth it only if the 10 s delay shows up as a problem.

## Suggested

**B, in its three steps, after #371 lands.** C later, only if needed.

Watch for after it ships: how many extra lead turns the new wakes cost per day. If finished items or new items wake
it too often, widen their grouping window before adding anything else.
