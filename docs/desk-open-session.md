# What the Desk shows when you open something

Suggestions only. Nothing here is built.

## The model: Inbox

In Inbox, opening a session never leaves the page. The list stays on the left and the open row stays lit in its
place. Beside it is the session's **brief**, which is never empty:

- **Main column**, what you act on: its own words before the question, the question with each choice, what it plans
  next, and a box to answer in. After Send it stays open.
- **Side column**, what you read: what you asked it, what it did this turn (repeats folded), what this turn changed
  (a peek of the biggest change), and its context.
- **Full chat** is one link away ("Open chat ›"). It is never the default.

## Today on the Desk

The Desk's right card already opens a page in place for a session (`deskHolderPane`), a crew member
(`deskCrewPane`) and an item (`deskItemPane`). Each has a head, a NOW line, a panel and a box. Two things break the
Inbox model:

- **A crew member's "Open its session"** swaps the board and the chat for the crew page, which is a full view.
- **A session's "Open its chat"** replaces the page with the bare chat. You then lose its crew, its items and what
  it decided.

## The rule

Everything opens in the Desk's right card, as a brief, and the left list stays as it is. No full-view page: "Open
chat ›" opens the chat inside the same card, with "‹ Back" returning to the brief.

| You open | Its row on the left (stays lit in place) | Main column: what you act on | Side column: what you read | The box sends to | Chat, one link away |
|---|---|---|---|---|---|
| **A session** (a lead or a session on its own) | Its name, its dot, "N items, N waiting on you" | Its last words before the question (1–2 paragraphs); the question with each choice; else "Next for it: <item>". No question and nothing next: its latest reply, short. | What you asked it last; what it did this turn (folded, e.g. "Read 6 files"); what it changed (biggest change peeked); its crew, one line each, with a red dot when one asks; its items; its decisions, the latest 3 then "N more ›" | That session | "Open its chat ›" shows its chat in the card |
| **A crew member** | Its row under its lead, lit; the lead's row stays where it is | What it stopped to ask, with Allow / Deny when it asks; else its step now, and Stop it | Its item (a link to it); files it changed (names, "N changed"); its latest summary; its lead | Its lead, saying "To <lead>, who passes it on" (it has no box of its own) | "Open its session ›" shows the member's chat in the card. It no longer swaps the board for the crew page; the crew page stays reachable from the lead's crew. |
| **An item** (bug or feature) | Its row, lit; it never moves | The NOW line: what it waits on you for and what that unblocks; its acts (Looks right, Send back, Commit to main); the thread, newest by the box | Done when, with ticks; who holds it and its lead; its files; its tabs (Decisions, Steps, Files) | Whoever holds it, else the lead ("To cue-268, who has this item") | "Watch <holder> ›" shows the holder's chat in the card |
| **A task** (an item whose kind is task) | Same as an item | Same as an item, but no "try it" step: it is done when its result is there (a doc, a list, an answer) | Done when; who holds it; **what it made** (the file or answer, opened in place) in place of Files | Same as an item | Same as an item |

## How it moves

- **Esc or ‹ Back** goes one step up: from a chat to its brief, from a crew member to its lead, from a lead to the
  Desk's own chat. Esc does this already for crew members.
- **After Send**, the brief stays open and the reply lands on it, as in Inbox.
- **Nothing open** shows the Desk lead's chat, as today. This is the Desk's "never empty".
- **Something that needs you** opens first in Inbox. On the Desk, nothing opens by itself: you came to look at the
  board, so a jump would move what you were reading.

## What it changes

- Crew member: "Open its session" stops swapping the board. This is the one real change; the rest uses what is there.
- Session: its chat opens inside the card, with ‹ Back, so its crew and items are one step away.
- Each brief's main column leads with the question, not the head. The head shrinks to one line.

Cost: small to medium. The panes and the box routing exist. The work is the crew member's in-card chat and the
order of the blocks in each pane.
