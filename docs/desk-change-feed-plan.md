# A Desk change feed that wakes the lead on every change

Plan for Desk item #379, 2026-11-19. Nothing built. Revised the same day after a review by a second model.

## In short

Today the lead hears about Desk changes through about eight separate messages, each sent from its own place in the
code. Some are lost, and some changes never reach the lead at all.

Suggested fix (option B below): one numbered list of Desk changes, a small queue saved in Cue's database. Each Desk
has two marks: "sent up to here" and "heard up to here". One sender gives the lead everything past "heard", in order,
when the lead is free. "Heard" moves only once the lead has finished a turn on it, so nothing is skipped. About
4–5 days, in three steps.

## What the lead hears today, and what it misses

| Change | Today |
|---|---|
| New item to route | Sent, grouped over 1 s. The list is emptied before the send, so a send that fails isn't tried again; the item only shows again in the next Desk summary. |
| Stopped (holder quiet 10 min, or stopped on a question) | Sent once per item. A busy lead gets it when its turn ends. Lost when the lead is asking you something, or when the send fails: the stop is marked as told before the send. It is also sent to a lead that hit its usage limit, which can cancel Cue's carry-on (a bug today). |
| Away mode notes | Sent if the lead is live and not limited; otherwise only in the next summary. |
| Needs you (a holder asks you something) | Shown to you; the lead isn't told (only a question left with uncommitted work goes to it). |
| Clash with main on landing | Goes back to the holder (and #371 makes that the rule). The lead isn't told, so a holder that's gone or full leaves it stuck. |
| Stuck landing (a finishing step that doesn't move) | Nobody is told. You notice it on the Desk. |
| Finished (DONE, landed, installed) | The lead isn't told. |
| Permission stop (a session waiting on a permission it can't grant) | Only in the Away list (#368). |
| You picked an option on a mocks item | The pick goes only to the mocks holder, as the reason on Send back or the check, and the item closes. The lead never hears it, so the "Build: <topic> (option X)" item isn't filed (#348's pick "lets go with C" became #383 only when you asked). |
| A route or new-session start that failed | Only in the next Desk summary. |
| A holder that ended or hit its limit with an item at work | Only seen as a stop later. |

What causes the misses:
- Each place decides by itself whether and how to tell the lead. There's no shared list.
- A send that fails isn't tried again.
- "Sent" isn't "heard". Cue counts a message as sent when it only kept it to send at the end of the lead's turn, and
  that kept copy lives in memory, so a restart loses it.

## Which changes wake the lead

Wake at once:
- **Stuck landing**: a finishing step hasn't moved for 15 minutes, or a check failed twice. Normal waits in the
  landing line (waiting its turn) don't count.
- **Stopped**: as today (10 quiet minutes, or a stop on a question).
- **Permission stop**: a session waits on a permission. The lead never grants it; it moves other work along and
  makes sure you see it.
- **New item**: as today.
- **You picked an option on a mocks item**: the row carries the item, the option and your words. The lead
  files the "Build: <topic> (option X)" item and routes it at once.
- **A route or new-session start that failed.**
- **A holder that ended or hit its limit** with an item at work.

Wake, but grouped (they go out with the next wake, or on their own after 5 minutes if nothing else comes):
- **Needs you**: a holder's question is yours by design (#249); the lead only needs to know. While Away is on, it
  wakes at once, since the lead is then the one moving things along.
- **Clash**: the holder already has it (#371). If it doesn't move, a stuck landing follows, and that wakes at once.
- **Finished**: DONE said, landed, installed.
- **Away on or off**: moved into the list so it stays in order with the rest.

Never wake, but kept as silent rows so the lead's picture is right at its next wake:
- Your own clicks on the Desk (Send back, Restart, Drop, Land first).

Never in the list: a holder's progress steps and its plain words.

Each wake costs the lead a turn. A row only wakes a lead that's parked (resumes it) if it's a wake-at-once kind.

## How the changes reach the lead in order, without missing any

1. **One list, saved in the same write as the change.** Today Cue changes an item in memory and saves it later,
   one item at a time, from about seven places, and ignores save errors. So first: one save helper that every item
   save goes through, which saves the item and its new rows in one database transaction. Each row gets its running
   number inside the same in-memory change, so the order is the order things happened.
2. **Two marks per Desk: "sent" and "heard".** Both saved. "Sent" moves when the message is handed to the lead.
   "Heard" moves only when the lead's next turn ends cleanly. On a restart, or a new lead, sending starts again from
   "heard". Each mark also records which lead it belongs to.
3. **Numbered rows, so a repeat is harmless.** A crash between send and save, or a resend from "heard", can send a
   change twice. Each message says its numbers ("Desk changes 41–44"), and the lead's skill gets one line: skip
   numbers you've already seen.
4. **One sender, off the turn-end thread.** It runs on Cue's own thread (as Away's notes do today), never while the
   lead is mid-turn or asking you something, and never to a lead that hit its usage limit.
5. **Rows only, no summary.** Today every message to the lead carries the whole Desk summary, which costs tokens on
   every wake. A feed message carries only its rows. The summary goes only to a new or resumed lead, followed by the
   rows that still apply.
6. **Checked against the item before sending.** The item stays the one record of truth (as #293 asked). The sender
   reads each item again and leaves out rows that no longer apply (a stop already settled, an item dropped); the
   marks still move past them. Two rows for the same item and kind fold into the latest one.
7. **No repeats from timers.** A row that comes from waiting (stuck 15 minutes) is keyed by item, kind and the item's
   last change, so the 10 s tick doesn't add it again each time.
8. **A backstop.** The existing finishing tick (about every 10 s, and at each turn end) also checks for rows past
   "sent", so a missed wake-up costs at most 10 s.

The 1 s grouping for new items becomes part of this and goes away. The per-item "told" mark for stops stays: the Desk
uses it to show "stopped again after the lead had it once".

## How it fits #293 (the libraries review)

- #293's first suggestion was one wake-up (a `tokio::sync::Notify`) that runs the finishing work as soon as
  something changes. It isn't built yet. Once it is, the same wake-up runs the finishing work and the sender.
- #293 said no to a job-queue library (apalis) because it would keep a second copy of each item's state. The feed
  doesn't: its rows are notices, not state, and they're checked against the item before they're sent (step 6).
- #293's second suggestion, item states and finishing steps as Rust enums, gives the feed a fixed list of kinds of
  change. Not needed first, but makes the "which change wakes" lists checkable at build time.
- #293's third suggestion, file watches, is option C below.

## How it fits #371 (a clash goes back to the session)

- #371 decides who gets a clash: the holder, with what clashed. The feed doesn't change that.
- The feed adds a "clash" row so the lead knows as well (grouped). If the holder can't take it (gone, limited, too
  full) or hasn't moved in 15 minutes, a "stuck landing" row follows and the lead steps in: hand it to a new session
  (#364), or bring it to you.
- "Send back always works" (#371) is a button for you; it isn't affected.
- Build this after #371 lands, so the clash row is added where #371 hands the clash back.

## Options

**A. Fill the gaps where they are.** Add a direct message to the lead at each missing place, and retry a failed
send a minute later.
- Cost: about 1 day, ~150 lines.
- Still no order across changes, still lost on a restart, and more separate paths to keep in step. Not worth doing.

**B. One numbered list with "sent" and "heard" marks (suggested).** As described above.
- Cost: about 4–5 days, 600–800 lines with tests. Done in three steps:
  1. The one save helper (item and rows in one transaction), the list, the two marks and the sender, with stops
     and new items moved over. Fix today's bug of sending a stop to a limited lead. Tests: order kept, lead busy,
     lead asking you something, lead limited, lead ended and replaced, Cue restarted between send and heard, a
     repeat skipped by number.
  2. The missing kinds: stuck landing, permission stop, your pick on a mocks item, failed route or start, holder
     ended or limited, and the grouped ones (needs you, clash, finished, Away on/off), plus your clicks as silent
     rows. One test each. The pick's test: pick C on a mocks item, its holder still gets the pick, the item closes,
     and the lead's next message names the item and option C.
  3. Once #293's wake-up is built, it drives both the finishing work and the sender; the tick stays as backstop.
- Gains: nothing is missed, the lead hears things in the order they happened, fewer tokens per wake, and one place
  to change what wakes it.

**C. B plus file watches.** Watch the install status files and main's branch, so an install finishing or a commit
on main adds its row at once instead of at the next tick. This is #293's third step.
- Cost: 1–2 more days on top of B.
- Gain: seconds, not correctness. Worth it only if the 10 s delay shows up as a problem.

## Suggested

**B, in its three steps, after #371 lands.** C later, only if needed.

Watch for after it ships: how many extra lead turns the new wakes cost per day. If they're too many, widen the
grouping window before adding anything else.
