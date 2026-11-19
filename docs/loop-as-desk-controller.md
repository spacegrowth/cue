# Can a loop run a Desk? (#334)

**Short answer: not today, and it shouldn't.** What you'd want from it (work keeps moving, and a report on a
schedule) is better done by a *Desk sweep*: Cue wakes the Desk's own lead on a schedule with a short "what's stuck"
message. The loop's scheduler is the part worth borrowing, not the loop.

## Why a loop can't drive a Desk today

- **Only the Desk lead's lines count.** Cue reads `ROUTE:`, `ITEM:`, `ASK:`, `DONE:` only from the session that is the
  project's Desk lead (`hub/desk_lead.rs`, `desk/parse.rs`). A loop's lead is a different session. Its route lines
  would just be text.
- **A loop is built to change files.** Each round, Cue runs the loop's check in its folder, a reviewer reads the
  diff, and Cue stops the loop when nothing moves. Routing changes no files. There is no check that proves "the Desk
  is moving", so every round would go to the reviewer for a verdict, which costs money and means little.
- **Each scheduled run is a fresh session.** A daily or weekly loop starts a new hosted session every time. Before it
  can act, it has to read the Desk again: the items, who holds them, and each holder's record.
- **Loops don't know about the Desk.** No code in the add-on reads or writes Desk items. A loop can't hold an item,
  and Cue can't route an item to a loop.

## How it would fit with the Desk lead

If a loop *were* the controller, there would be two dispatchers on one Desk. Cue keeps one lead per Desk on purpose:
the lead answers you at once, never works in the background, and owns every route (see the Desk lead's rules in
`desk/mod.rs`). A loop beside it would either:

- **Replace the lead.** That fails the "answer at once" rule: a loop run is busy for minutes and then ends.
- **Sit beside the lead.** Then two sessions route the same items, and their routes and asks for you collide.

The only fit that keeps one dispatcher is the loop *asking the lead*. If the loop only passes a message to the lead,
it adds nothing that a scheduled message from Cue wouldn't.

## What Cue already does to keep work moving

These already run without the lead being asked:

- **An item that stopped:** its holder is asked "Done?" once (`desk_stall.rs`).
- **A circle of waits:** found and shown, and the lead is told once (`desk_waits.rs`).
- **A session that grew too big:** handed off (`desk_handoff.rs`).
- **A finished item:** committed and installed (`desk_finish.rs`).

What nobody does yet:

- Look across the whole Desk at a set time.
- Notice an item that's been waiting on you or on another item for too long.
- Catch a holder that went idle with its question left in its own chat.
- Write you one short report.

## What it can't do, either way

- **Answer for you.** It can't approve a one-way step, answer an `ASK`, or pick between your mocks. It can only list
  them for you.
- **Do item work.** Under the lead's rules it only dispatches.
- **Run while Cue is closed or the Mac sleeps.** A missed time runs once at the next launch, as loops do.
- **Wake sessions during a usage-limit stop.** Nothing can be sent then.
- **Prove its own quality.** No command can check that a route was a good one.

## Cost

Figures are from docs/desk-token-strategy.md.

- **A sweep, lead warm:** about 5 to 7 cents per wake. A lead turn cost about 7 cents at its median 107k, and about 5
  at the 80k hand-off mark.
- **A sweep, lead cold:** about 60 to 80 cents, because after an hour the cache has gone and the context is read
  again. So sweeping every hour would cost about $15 a day, mostly for nothing.
- **A loop run as controller:** about 75 cents to $1.25 just to start a fresh session and find its way, plus a
  reviewer pass each round. Three runs a day comes to about $3 to $5, for less than the lead can already see.

## Recommendation

**Build a Desk sweep, not a controller loop.**

- **When it runs.** A per-Desk setting, off by default: run at given times (say 09:00 and 14:00), or when something
  has been stuck past a limit.
- **What it sends.** Cue sends the Desk lead one message: items waiting on you over 30 minutes, items waiting behind
  another over 30 minutes, holders idle with an open item, and items still not routed.
- **What the lead does.** It acts with its usual lines and writes a short report. Cue shows the report on the Desk
  and notifies you.
- **What it reuses.** Two parts of the loop's scheduler: one run per due time, and a missed time runs once.
- **When it doesn't run.** It skips a time when nothing is stuck, so a quiet Desk costs nothing. It never sends
  during a usage-limit stop.
- **Cost.** About $0.20 to $2 a day.

Letting an item go to a loop as its *worker* (`ROUTE: #id -> loop`, with the item's "done when" as the loop's check)
is a different and useful idea. It is about how work gets done, not about who controls the Desk.
