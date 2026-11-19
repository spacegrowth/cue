# Plan: finished work waits in a "Waiting to install" lane

Item #329. Plan and mocks only; nothing is built until the owner picks.
Mocks: the ideas canvas, page "Waiting to install" (Today, A, B, C, light and dark).

## What's wrong today

When a session says DONE, its item stays in **In progress** while Cue checks it, lands it on main and waits
for the install. #339 made each row say which step it's on, but the group still counts it. So "In progress 7"
can mean four sessions working and three items that are finished and only waiting on Cue. The next install's
time and Install now sit in the Items head, away from the items they bring in.

## The options

- **A · One lane, "Waiting to install" (suggested).** In progress keeps only items a session is working on.
  Under it, a new lane holds every item from DONE until the install: being checked, waiting its turn to land,
  on main. Rows are in the order they go live. The lane's head says when the install is ("9:05pm · in 12m")
  and has Install now; both leave the Items head. Cost: small.
- **B · Two lanes, "Landing" then "Waiting to install".** Landing is the landing line itself, numbered, with
  "Land this first" on any but the first. Waiting to install holds only what's on main. Cost: small to medium.
  More exact, but two lanes for what is often one or two items, and rows hop between them as they land.
- **C · No lane, one folded line.** Under In progress: "3 finished · 1 checking · 1 to land · 1 on main ·
  install 9:05pm" with Install now; it opens to the rows in place. Cost: small. Quietest, but you can't see
  what's in the install without a click.

## How it fits How work lands

- **The four steps (Said done, Checked, Landed, Live).** The lane holds an item from Said done until Live.
  At Live it leaves the lane for Ready to review (or Waiting on you when it has a check for you), as today.
- **The landing line.** The lane's order is the landing line's order, then the items already on main. B shows
  the line's numbers; A and C keep the order without them. Reordering the line (the lead, or "Land this
  first") reorders the lane.
- **The waits list (#250).** Waiting in the landing line and waiting on the check are entries in the one waits
  list. A row's second line is its entry there, in the list's own words ("Lands after #301 · 4m"), so the lane
  and the list never disagree. Only waits on a step Cue is taking stay in the lane.
- **When something goes wrong it leaves the lane.** A failed check goes back to its holder: the row returns to
  In progress, saying "Check failed". A circle, a clash with main or a question for the owner is a wait on a
  person: the row moves to Waiting on you with the button that clears it, as on the How work lands page row 3.
- **Projects with no install.** The lane is named after the project's after-landing step from its "how work
  lands" setting (#252): "Waiting to deploy", "Waiting to serve". With no after-landing step it holds only
  checking and landing and is named "Landing"; an item leaves it as soon as it's on main.
- **Owner commits.** An item "checked, waiting for your commit" is waiting on you, not on Cue, so it goes to
  Waiting on you, not this lane.

## Picked: B (18 Nov 2026)

Two lanes under In progress: **Landing**, the landing line itself, numbered in the order items land, and
**Waiting to install**, what is already on main. (A, one lane, was the suggestion; the owner picked B.)

## Building B

- `ui/desk.js` `deskGroups`: finishing items leave `working`. Steps commit, line, checking and landing go to a
  new `landing` group, in landing line order (the one checking first). Steps queued, building, installing and
  starting go to `install`, in the order they reached main.
- `deskList`: "Landing N" with "one at a time" in its head; rows numbered 1, 2…; "Land this first" on every
  row but the first (it arrives with the waits list's offers, #250; until then the number and the "Waits on
  #…" line only).
- "Waiting to install N" carries the install time and Install now in its head (from `deskNextInstall`, which
  leaves the Items head). Its name follows the project's after-landing step ("Waiting to deploy"); a
  project with none has no such lane, and Landing is the last before Ready to review.
- Rows use the finish step line from #339, shortened next to the holder ("Checking · step 2 of 4",
  "Waits on #301", "On main · c71e0b4"), and the waits list's words once #250 lands.
- A new row icon: the ring with an arrow down into it ("done, on its way in").
- A failed check moves the row back to In progress with its holder; a wait on a person moves it to
  Waiting on you.
- Window tests: both groups, their order, the numbers, the install head, and a failed check moving back.
