# Desk away mode (#330): plan

A plan only. Nothing here is built. Mocks: the ideas canvas, page "Away mode".

## The problem

When you step away, the Desk stops at the first question. The lead asks you (ASK), the item shows as waiting on
you, and its session sits idle until you come back. Most of those questions are small and reversible ("keep the old
group name?", "route it to cue-125 or start a new session?"). Meanwhile, the questions that do need you are spread
over the Inbox, the item pages and the lead's chat.

## What away mode is

A switch on the Desk, **Away**, that you turn on when you leave. While it's on:

- **The lead decides** whatever is on the "may decide" list below, writes each one as a `DECIDED:` line, and keeps
  the work moving.
- **Everything else waits in one list**, "For you when you're back", instead of asking you then and there. The item
  that asked moves on to other work if it can. Its session takes the next item.
- **You come back to one screen**: what needs you, what was decided for you (each with Undo), and what's done and
  ready to check.

You turn it on and off yourself. Cue never guesses whether you're there: no idle timer, no location. You can also
give it an end time ("until 18:00"). When that time comes, away mode ends and the list waits for you.

## What the lead may decide alone

1. **Routing**, as today: route, re-route, split, merge, wake a cold session, start a new session (up to 3 new
   while away).
2. **"How" questions inside an item**, when either answer is easy to change later: a name, a layout detail, which of
   two equal approaches. It answers the session and writes `DECIDED: #id <what> — <why>`.
3. **A stopped item**: send it back with what to do next, or route it on.
4. **Order**: which item goes first when two would change the same files.
5. **A session's own question in its chat** (the "idle holder" case): answer it if it's on this list, else lift it to
   the list.

## What always waits for you

1. **Ask me before steps**: a push to main, a release, deleting files, as your Desk's setting says. Cue's guard stops
   them as today; while you're away the question goes to the list, not a pop-up.
2. **Picking a mock.** A plan with mocks waits for your pick. Nothing is built from a mock you didn't pick.
3. **Looks right / Send back** on a finished item. The check is yours.
4. **"What" questions**: scope, what you want, taste, anything where the answer changes what gets built.
5. **Pulling in a session that isn't on the Desk.**
6. **Spending past the away budget** (see Limits).

## Limits while away

- **A budget** for the time away (default $10). Past it, the lead starts nothing new; work under way finishes.
- **An end time**, optional.
- **No pings for each decision.** One notification only when nothing on the Desk can move without you.

## Coming back

The list opens on the Desk when you turn Away off or open the Desk after its end time:

- **Needs you (n)**: each with the question and its buttons, as an ask today.
- **Decided for you (n)**: each `DECIDED:` line with Undo. Undo sends the item back with the other choice.
- **Done, to check (n)**: as today.
- A one-line summary: "2h 10m away · 6 items moved · $3.40".

## Three ways to show it (mocks)

- **Today**: the lead's ask sits in its chat and the item waits; nothing else moves.
- **A · An Away switch and a "While you were away" card.** The switch sits in the Desk's head strip, with until and
  budget. The card tops the Items column on return, in three groups.
  Cost: M.
- **B · A rules card.** Turning Away on opens a card listing what the lead may decide and what waits, each a switch
  you can change. The list is its own tab, "Away", beside Items. Cost: M to L.
- **C · Held in the lead's chat.** No new list: the lead's chat gets a pinned "Held for you · 4" strip, its decisions
  show inline with Undo, and you answer the held asks there. Cost: S.

**Suggested: A.** One switch, one card, in the place you already look on return. B makes the rules visible, but most
of them shouldn't change. C is the cheapest, but a long chat buries the held asks.

## What Cue would need to change

- **The lead's instructions**: an AWAY block (on, until, budget left) in the `<desk>` block, with the two lists above.
- **ASK while away**: Cue holds it on the list instead of showing it as asked now; the item's session is told it
  waits and is offered the next item.
- **DECIDED with Undo on an item**: decisions already carry `undo`; show them on the card and make Undo send the item
  back with the other choice.
- **The guard's question** (hub/guard.rs) goes to the list while away.
- **The switch, budget and end time**: per Desk, stored with the Desk.
- **Spend while away**: sum the Desk sessions' spend since the switch went on.

## Open questions for you

1. Should the lead be allowed to **build the suggested mock** on its own copy while you're away, without landing it?
   (Today's rule: nothing is built before your pick.)
2. Should a session that's waiting on you **take other work**, or stay on its item?
3. Is $10 the right default budget, or no budget at all?
