# What Cue sends a session or a Desk lead, and how it shows

Every message Cue itself writes into a session's chat or a Desk lead's chat (#478). The session or lead always gets
the full text. In the chat you see one short plain line saying who it went to and what it asks or tells, with the full
text folded under it. A click on the line opens the full text; a click on the item's number opens the item. Under it, the one line every
message has (#422): sending, waiting, delivered to whom and when.

    Cue · Asked cue-383 · #461 not done: “Are we done here completely?”                2m
    Cue · Told web-lead · refused: #d5 is done · 1 more line                            just now

- **Asked**: Cue wants the session to do something or answer. **Told**: it informs.
- A message for a teammate ("For your teammate ui-tests: …") names the teammate, not its lead.
- Cue's standing words to the session ("Its fixes stay in #…", "Say DONE: #… when it is"), a log, the choices are
  only in the fold. When there's more, the line ends with what: "· its log", "· 2 choices", "· 1 more line".
- Left-aligned and quiet like #433's lead lines, the item number the same chip; up to two lines in a narrow chat.
  No stripe, no hover text. Light and dark.

The patterns live in `ui/desk_cue_said.js` (`CUE_MSGS`); `tests/ui/specs/desk-cue-said.spec.mjs` renders each one in
light and dark.

## Where they show

| Place | How |
| --- | --- |
| Session's chat (its Desk page, the main chat) | the short line, full text folded |
| Item's page (its holder's chat about it) | the same line |
| Lead's chat (the Desk) | the same line, except the kinds that already had their own design (below) |
| Inbox (a session's "Cue asked", its run lines) | the line as plain text |
| Session card on the board, and "Working on" | the line as plain text |

Before #478 every kind below showed as a raw bubble on the right, as if you had typed Cue's whole instructions. One,
"Desk item #…: the Desk lead took back its copy", was worse: it showed as a hand-off card with that as its title.

## To a session (on an item, or a teammate through its lead)

| Kind | Sent when | Hub | The line |
| --- | --- | --- | --- |
| Isn't done (sent back) | You send an item back with a note | desk_act.rs | Asked X · #id not done: “your note” |
| Isn't done (finish failed) | Build, window, install, check or landing failed; a clash; nothing new landed; its PR closed | desk_finish.rs `finish_back` | Asked X · #id not done: “The build failed” · its log |
| Open again | A later commit on main took its work back out | desk_finish.rs | Asked X · #id open again: “Commit … took its work back out” |
| Pick up where you left off | Start on an item that stopped before done | desk_act.rs | Asked X · #id pick up where it stopped |
| Lead's note | The lead's NOTE:, or a ROUTE back with a why while finishing | desk_route.rs | Told X · #id the lead's note: “…” |
| Moved | The item went to another session | desk_route.rs `moved_words` | Told X · #id moved to Y: stop work on it |
| Parked | PARK on a queued item | desk_unstick.rs | Told X · #id parked: off its list |
| Paused | The lead's PAUSE | desk_unstick.rs `pause_words` | Asked X · #id paused (why): stop at a clean point |
| Resumed | The lead's RESUME | desk_unstick.rs | Told X · #id resumed: carry on |
| Copy taken back | The lead's TAKE COPY | desk_unstick.rs | Told X · #id copy taken back: stop work on it |
| Closed | CLOSE: its work is already on a branch | desk_unstick.rs | Told X · #id closed: already on main |
| Dropped | You drop the item | desk_act.rs | Told X · #id dropped: stop work on it |
| Option picked | You pick a mock | desk_act.rs | Told X · #id option B picked: done, #n builds it |
| Folded into | Another item was merged into one it has | desk_said.rs | Told X · #a folded into #b: “…” |
| Done? | A turn on its item ended with no DONE | desk_stall.rs | Asked X · #id done? or what's left · 1 more item |
| Desk paused | The Desk is paused | desk_pause.rs `PAUSE_WORDS` | Asked X · the Desk is paused: stop at a safe point |
| Desk resumed | The Desk resumes | desk_pause.rs | Told X · the Desk is resumed: carry on with #a, #b |
| TO LEAD failed | Its TO LEAD: line didn't reach the lead | desk.rs | Told X · TO LEAD didn't reach the lead: “…” |
| TO LEAD not sent | Its TO LEAD: named no item of its Desk | heard lines | Told X · TO LEAD not sent: “…” |
| Not a new item | Its ITEM: line was a fix of the item it's on | heard lines | Told X · #id not a new item: “title” stays here |
| Several Desks | Its ITEM: could go to more than one Desk | heard lines | Told X · item not added: several Desks could take it |
| Held steps answered | You answered its Ask-me-before steps held while away | desk_held.rs | Told X · held steps answered: 1 allowed, 1 refused |
| Ask me before changed | You changed Ask me before | guard.rs, one_way.rs | Told X · Ask me before changed: before … |
| Go back on your call | You pressed Undo on a decision | desk/mod.rs `undo_words_with` | Asked X · #id go back on: “what it decided” |
| Undo a crew call | You undid a crew decision (bring back, revert, undo) | crew_log.rs `go_back_words` | Asked X · bring Y back, or revert: “…” |
| Teammate quiet | A teammate went quiet | team.rs | Asked X · check on teammate Y |
| Hand off | You asked a session to hand off | ipc/insights.rs `HANDOFF_ASK` | Asked X · hand off: write a note for a fresh session |
| Carry on | The next session after a hand-off | ipc/insights.rs `resume_ask` | Asked X · carry on from the hand-off note |
| No longer the lead | The lead was taken off its Desk, or another took it | desk_members.rs, ipc/desk.rs | Told X · no longer runs cue's Desk |
| Team carried over | A hand-off carried the crew over | team_out.rs `carried_note` | Told X · the team from the session before · 2 teammates |

## To a Desk lead

| Kind | Sent when | Hub | The line |
| --- | --- | --- | --- |
| Landing clean-up question | A session asks how to clear the way for its landing | desk_cleanup.rs `lead_words` | Asked lead · #id X's landing question: “…” · 2 choices |
| Wait circle | Items wait on each other in a circle | desk_waits.rs `circle_note` | Asked lead · cut a wait circle: #a waits on #b waits on #a |
| Desk paused, resumed | You pause or resume the Desk | desk_pause.rs | Told lead · the Desk is paused (by …), or the Desk is resumed |
| Route asked of you | Its route to a session off the Desk waited on your yes or no | desk_members.rs | Told lead · #id routed to Y (a yes), or not routed to Y: answer “No” |
| Lands change answered | You answered its LANDS: change, or it was refused | desk_lands.rs | Told lead · its first line |
| Decide the open ASKs | Away came on with its ASKs still open | desk_away.rs `sent_back_words` | Asked lead · decide the open ASKs yourself (away) · 2 ASKs |
| Away sweep | You're about to go away | desk_away.rs `sweep_words` | Asked lead · settle what you can before Away |
| Away on, off, paused | Away turned on, off, changed or ran out | desk_away.rs `tell_lead_away` | Told lead · Away is on, until 18:00 (or is off, or paused: …) |
| Replies to its commands | Refused:, Noted:, "#id not cut / split / closed / paused / …", "note not sent", "Couldn't read this line as a route", "No question from #id…", FEATURE, "Cue couldn't route / queue…", "Cue didn't reopen…" | desk_said.rs, desk_route.rs, desk_unstick.rs, desk_features.rs | Told lead · its first sentence · N more lines |

## Kinds that keep their own design

These already showed as short cards or lines before #478 and are unchanged:

| Kind | How it shows |
| --- | --- |
| Hand-off of an item ("Desk item #id (kind): title …") | the hand-off card (#174), in the session's chat |
| New items from you | the new-items note in the lead's chat |
| A session stopped without DONE ("#id (title) stopped…") | the stop note in the lead's chat |
| A session's TO LEAD: line | the "From X" card in the lead's chat |
| Items done | the done note in the lead's chat |
| Cue couldn't start a new session for #id | the start-failed note in the lead's chat |
| A new lead's hello ("You run cue's Desk now…") and hand-off ("X now runs cue's Desk. Please route …") | already one short line: its bubble, the `<desk …>` block dropped |
| Your answer to its ASK ("Your ASK about #id: …") | hidden: the ASK's card shows the answer |
| From another Desk's session | #472's band across the lead's chat |
| What Cue sent a crew lead for its crew (reviews, "(Cue) Your teammate … quiet") | Cue's crew line with "+ What Cue sent" (team.js) |

## Your own words, kept as your bubble

- What you type to a session or the lead (a lead's copy has its `<desk …>` block dropped, as before).
- Your note from an item's page ("About #id (title): …", #410's style).
- The "(Crew: on / off …)" line Cue adds to what you type to a crew lead.
