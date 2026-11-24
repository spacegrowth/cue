# One rule for every chat

Item #837. Cue has five chats: the lead's chat on a Desk, a session's chat on a Desk, an item's page, the Board's open
card, and a crew member's chat. All five follow this rule.

## The rule

**In a chat you see what you typed and what the AI wrote to you. Everything else is Cue's, and looks like Cue's.**

Every message in a chat is from one of three:

| From | What it is | How it shows |
| --- | --- | --- |
| You | What you typed or said, a quick phrase you pressed, your note or question from an item's page, your send-back | your bubble at the right |
| The AI | The session's or lead's reply | its reply at the left |
| Cue | Anything Cue wrote and sent: item briefs, routes, notes, nudges, follow-ups, Desk updates, questions passed on, answers to the lead, crew messages | a quiet Cue line, Cue's card, or in Activity; never your bubble |

Who a message is from is set where it is sent, never guessed from its words. The same words typed by you are yours.

## How it's kept

- **Where it's sent.** Every way text goes into a session names its sender. Your words go through one path
  (`Hub::send_to_session`); Cue's go through `Hub::tell`, `tell_now`, `cue_says` or the Desk's sends, and each must
  name its kind. The compiler refuses one that doesn't, and a test fails if any code hands text into a session
  another way.
- **What the chat gets.** The session's copy of a message of Cue's is marked `cue`, with its `kind`, and saved so.
  An older page of the chat has it too.
- **One filter.** All five chats ask the same filter (`ui/chat/who.js`) who a message is from and where it goes in
  that chat. Words may still choose how Cue's message is drawn (which card, which short line), never whether it's yours.
  A message of Cue's that nothing knows how to draw is one quiet Cue line.
- **Older history.** Messages saved before the marks existed keep how they showed before.

## Cue's kinds and where each shows

"Activity" means the chat's Activity tab, under Cue, by its item. The lead's, a Desk session's and an item's chat move
Cue's messages there; the Board and a crew member's chat show the quiet line in place.

| Kind | What Cue sent | Lead | Desk session, item | Board | Crew member |
| --- | --- | --- | --- | --- | --- |
| brief | an item handed to a session | n/a | the hand-off card | the hand-off card | n/a |
| route | where an item went, or its new state (moved, parked, paused, resumed, closed, copy given or taken back) | Activity | Activity | quiet line | quiet line |
| note | the lead's NOTE, an option you picked, one item folded into another | Activity | Activity | quiet line | quiet line |
| nudge | done?, stuck, a quiet teammate, carry on, Resume or Stop of a background agent | Activity | Activity | quiet line | quiet line |
| followup | DONE and CHECK follow-ups: not done, write the check line again, not a new item | Activity | Activity | quiet line | quiet line |
| desk | Desk changes, the Desk paused or resumed, Away | Activity | Activity | quiet line | quiet line |
| question | a session's TO LEAD, a landing question, a wait circle, your answer for several leads | Activity, with the lead's answer under it | Activity | quiet line | quiet line |
| answer | Cue's answers to the lead's own lines: SHOW ("#852 in full, as you asked:"), refusals, Queued, route results, your Yes or No on its ask | Activity | n/a | quiet line | n/a |
| crew | review, Send fixes, Stop, Commit, hand out a task, crew carried over | Activity | Activity | the CREW card | quiet line |
| handoff | write a hand-off note, a new lead's hello | Activity | Activity | quiet line | quiet line |
| peer | another Desk's lead, through Cue | the band from that Desk | n/a | quiet line | n/a |
| notice | anything else, and any kind added later | Activity | Activity | quiet line | quiet line |

A message from another session (not a Desk's) shows as Cue's FROM A SESSION card in every chat.

## What stays as it is

- In the lead's chat: the hand-off band, the bands from other Desks, and the "+ Desk update" fold under your words.
- Your words that Cue wraps (your note or question from an item's page, your send-back) stay yours: your bubble, the
  item's head on it, Cue's words around them left out.
- A message kept while the session works that holds your words and Cue's: your bubble shows only yours; yours go
  first, Cue's at its next turn.

## Adding a new kind of message

Send it with `tell`/`tell_now` (or the Desk's sends) and its kind, nothing else is needed: it shows as Cue's quiet
line in every chat. Give it a card of its own only if a short line isn't enough (`ui/desk/cue_said.js`), and add its
row to `docs/cue-messages.md`.
