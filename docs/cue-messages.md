# What Cue sends a session or a Desk lead, and how it shows

Every message Cue itself writes into a session's chat or a Desk lead's chat (#478). The session or lead always gets
the full text. In the chat you see one compact card (#501, one card family, `ui/styles/messages.css`): a head row with
Cue's mark, who it went to, a tag saying what kind it is, the item's chip and the age; then what it asks or tells in two
lines, More opening the full text under them; then the foot, how it went at the left (#422: sending, waiting, delivered
to whom and when) and its actions at the right. A click on the item's chip opens the item.

    ◔ Cue → cue-383   NOT DONE   #461                                                   2m
    Asked: not done: “Are we done here completely?”
    ✓ Delivered to cue-383 · 14:09                                                    More

One family for every message (#501): whatever is said is one body size (13.5px), everything around it (head, foot,
status line) one smaller size (12px). An ask is the same card outlined all round, plain again once answered with the
answer in its foot; a message still going is dashed. The agent's reply stays plain text with the same head row, and
your own words stay your bubble at the right. Colours come from the theme's tokens, so light and dark follow.

- **Asked**: Cue wants the session to do something or answer. **Told**: it informs.
- A message for a teammate ("For your teammate ui-tests: …") names the teammate, not its lead.
- Cue's standing words to the session ("Its fixes stay in #…", "Say DONE: #… when it is"), a log, the choices are
  only in the fold. When there's more, the line ends with what: "· its log", "· 2 choices", "· 1 more line".
- Left-aligned, the item number the same chip everywhere; up to two lines in a narrow chat.
  No stripe, no hover text. Light and dark.

The patterns live in `ui/desk/cue_said.js` (`CUE_MSGS`); `tests/ui/specs/desk-cue-said.spec.mjs` renders each one in
light and dark; `tests/ui/specs/messages-one-family.spec.mjs` checks every kind below uses the family's classes.

## Where they show

| Place | How |
| --- | --- |
| Session's chat (its Desk page, the main chat) | the card, full text folded |
| Item's page (its holder's chat about it) | the same card |
| Lead's chat (the Desk) | the same card; the lead's own Desk lines (ROUTE, ITEM, DONE…) are plain rows with the item's chip |
| Inbox (a session's "Cue asked", its run lines) | the card's words as a plain line (`.mg-plain`) |
| Session card on the board, "Working on", the notch | the same plain line, at the row's size |

Before #478 every kind below showed as a raw bubble on the right, as if you had typed Cue's whole instructions. One,
"Desk item #…: the Desk lead took back its copy", was worse: it showed as a hand-off card with that as its title.

## To a session (on an item, or a teammate through its lead)

| Kind | Sent when | Hub | The line |
| --- | --- | --- | --- |
| Isn't done (sent back) | You send an item back with a note | desk_act.rs | Asked X · #id not done: “your note” |
| Isn't done (finish failed) | Build, window, install, check or landing failed; a clash; nothing new landed; its PR closed | desk_finish.rs `finish_back`, desk_install.rs | Asked X · #id not done: “The build failed” · its log |

Not a kind of its own: when the holder is a teammate, any of these starts with "For your teammate X: " (desk_act.rs,
desk_asked.rs, desk_install.rs, desk_main_line.rs). The line drops the prefix and names the teammate (`cueMate`), so a
teammate's "not done" after a failed finish is the same line as above.
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
| Teammate quiet (automatic) | A teammate at work has been quiet a while with nothing running | team.rs quiet tick: “(Cue) Your teammate Y has been quiet for N min…” | Asked X · check on teammate Y (quiet N min) |
| Teammate quiet (your Resume) | You press Resume on a quiet teammate's row | team.rs `resume_words`: “Your teammate Y has gone quiet. Ask it where it is…” | Asked X · check on teammate Y |
| Hand off | You asked a session to hand off | hub/hand_off_note.rs `HANDOFF_ASK` | Asked X · hand off: write a note for a fresh session |
| Carry on | The next session after a hand-off | hub/hand_off_note.rs `resume_ask` | Asked X · carry on from the hand-off note |
| No longer the lead | The lead was taken off its Desk, or another took it | desk_members.rs, desk/hub/lead/mod.rs | Told X · no longer runs cue's Desk |
| Team carried over | A hand-off carried the crew over | team_out.rs `carried_note` | Told X · the team from the session before · 2 teammates |

## To a Desk lead

| Kind | Sent when | Hub | The line |
| --- | --- | --- | --- |
| Landing clean-up question | A session asks how to clear the way for its landing | desk_cleanup.rs `lead_words` | Asked lead · #id X's landing question: “…” · 2 choices |
| Wait circle | Items wait on each other in a circle | desk_waits.rs `circle_note` | Asked lead · cut a wait circle: #a waits on #b waits on #a |
| Desk paused, resumed | You pause or resume the Desk | desk_pause.rs | Told lead · the Desk is paused (by …), or the Desk is resumed |
| Route: you said yes | Its route to a session off the Desk waited on your yes, and you said yes | desk_members.rs | Told lead · #id routed to Y (a yes) |
| Route: you said no | The same, and you said no: “Cue asked … before routing #id to Y … The answer: …” | desk_members.rs | Told lead · #id not routed to Y: answer “No” |
| Route: yes, but it failed | You said yes, but Cue couldn't route it: “… said yes, but Cue couldn't route #id to Y. …” | desk_members.rs | Told lead · #id not routed to Y: “why” |
| Lands change answered | You answered its LANDS: change, or it was refused | desk_lands.rs | Told lead · its first line |
| Decide the open ASKs | Away came on with its ASKs still open | desk_away.rs `sent_back_words` | Asked lead · decide the open ASKs yourself (away) · 2 ASKs |

Decide the open ASKs is not always a message of its own: desk_said.rs adds it to the replies to the lead's commands,
and `route_answers` sends them as one message. With any reply before it, the line is that reply's (“Told lead · its
first sentence · N more lines”), and the open ASKs are only in the fold.
| Away sweep | You're about to go away | desk_away.rs `sweep_words` | Asked lead · settle what you can before Away |
| Away on, off, paused | Away turned on, off, changed or ran out | desk_away.rs `tell_lead_away` | Told lead · Away is on, until 18:00 (or is off, or paused: …) |
| Replies to its commands | Refused:, Noted:, "#id not cut / split / closed / paused / …", "note not sent", "Couldn't read this line as a route", "No question from #id…", FEATURE, "Cue couldn't route / queue…", "Cue didn't reopen…" | desk_said.rs, desk_route.rs, desk_unstick.rs, desk_features.rs | Told lead · its first sentence · N more lines |

## Kinds that keep their own design

These already showed as short cards or lines before #478 and are unchanged:

| Kind | How it shows |
| --- | --- |
| Hand-off of an item ("Desk item #id (kind): title …") | the family's card tagged HAND-OFF (#174), at the right of the session's chat |
| New items from you | a card tagged NEW ITEMS from you, at the right of the lead's chat, one row per item |
| A session stopped without DONE ("#id (title) stopped…") | a card from Cue tagged STOPPED, with Open #id |
| A session's TO LEAD: line | a card from the session tagged TO LEAD |
| Items done | a card tagged DONE, one row per item |
| Cue couldn't start a new session for #id | a card from Cue tagged COULDN'T START |
| A new lead's hello ("You run cue's Desk now…") and hand-off ("X now runs cue's Desk. Please route …") | already one short line: its bubble, the `<desk …>` block dropped |
| Your answer to its ASK ("Your ASK about #id: …") | hidden: the ASK's card shows the answer |
| From another Desk's session | a card with that Desk's chip in its head (#472's band is now a card) |
| What Cue sent a crew lead for its crew | a card tagged CREW with "+ What Cue sent" (team.js `CUE_SAID`, below) |

## Not a short line yet

The hub (or the window) sends these too, but no pattern in `CUE_MSGS` knows them, so today they show as a raw bubble
on the right with Cue's full text, as if you had typed it:

| Kind | Sent when | Hub | How it shows today |
| --- | --- | --- | --- |
| Question from the item's page | You ask about an item that's to check or done | desk_asked.rs | your bubble: “A question about Desk item #id (title), which is …: your words”, Cue's “Answer it…” words under it |
| Carry on after the usage limit | A turn stopped on the usage limit, and it reset (Carry on is on) | sessions/send_at.rs `CARRY_ON` | your bubble: “Carry on where you stopped: your usage limit reset.” |
| Resume with the hand-off skill | The next session after a hand-off, when the skill takes `resume` | hub/hand_off_note.rs | your bubble: “/handoff resume” |
| Background agent quiet, or stop it | You wake or stop a session's background agent | ui/agents.js (the window) | your bubble: “Your background agent “X” has gone quiet…”, or “Stop your background agent “X” now…” |
| Cut refused | The lead's CUT: names a wait that isn't there | desk_said.rs | the lead's chat, your bubble: “Not cut: #a doesn't wait on #b” |
| CLOSE commit not on main | The lead's CLOSE: names a commit that isn't on main | desk_unstick.rs | the lead's chat, your bubble: “Desk item #id not closed: … isn't on main…” |

A Cue message waiting to go (queued or held) shows as the card in the chat, dashed; the session's card on the board
(“Queued: …”) and its Inbox row (“Queued for X: …”) give its plain line, not Cue's raw text.

## What Cue sends a crew lead for its crew

A second pattern table, `CUE_SAID` in `ui/team.js`, draws these. It runs before `CUE_MSGS`, so in a chat they are not
#478's line: each is the family's card from Cue tagged CREW, with “+ What Cue sent” folding the full
text. In the Inbox and on a session's card only the automatic quiet one has a `CUE_MSGS` line; the rest quote Cue's raw
text there.

| Kind | Sent when | Hub | In the chat |
| --- | --- | --- | --- |
| Review | A teammate finished: Cue asks its lead for a review by a fresh agent | team_review.rs `review_words` | Asked for docs's review, the review folded under it |
| Send fixes | You press Send fixes on a review | team.rs `fixes_words` | Sent docs's findings back to it |
| Fixes for a carried-over teammate | Send fixes for a teammate the session before started | team.rs | Asked to start docs again with its fixes |
| Stop a teammate | You press Stop on its row | team.rs | Asked to stop docs |
| Commit a teammate's work | You press Commit on its row (several are joined in one message) | team_tidy.rs | Asked to commit docs's work |
| Wake a teammate | Resume on a quiet teammate's row, or the automatic “(Cue) … quiet for N min” | team.rs | Asked docs where it is |
| Hand out a task | You hand a task to a new teammate | team_out.rs | Handed out a task · its first line |

## Slash commands Cue types

| Kind | Sent when | Hub | How it shows today |
| --- | --- | --- | --- |
| /compact | Compact on a nudge or the ⋯ menu, while it's at its prompt | send.rs `session_command` | a “/compact” bubble of yours when Claude Code logs it as typed, then the Compacting line under the chat |
| /rename | You rename a session (typed once its turn ends, if it's working) | sessions.rs | not in the chat: Claude Code logs it as a command record, which the chat leaves out; only the rename notice if it failed |
| /exit | Cue moves a terminal session that won't quit | ipc/sessions.rs `ask_to_exit` | not in the chat, for the same reason |

## Sends the window makes for you

Your own words or your click, so they show as your bubble, but Cue wrote the text:

| Kind | Sent when | Window | How it shows today |
| --- | --- | --- | --- |
| Quick phrase | You press one of your quick phrases (chat, item page, crew page, mini); on an item to check it's a note to its holder (#488) | chat/compose.js `sendPhrase`, desk/act.js | your bubble: “Go ahead” |
| Commit, continue | Commit or continue on a finished turn's card | app/clicks.js | your bubble “Commit” or “continue”, and a toast |
| A nudge's command | A nudge's button (e.g. Hand off) | insights.js `insFill` | your bubble: “/handoff”, toast “/handoff sent” |
| /compact while it works | Compact while a turn runs | insights.js `insCompact` | your bubble “/compact”, queued; toast “/compact queued…” |
| A command's output | Send to… on a “!” command's card | chat/send.js `ranAsMessage` | your bubble: “I ran `npm test` in ~/work/cue (exit 1):” and the output |
| Run in its terminal | Run in terminal on a “!” card | app/clicks.js | your bubble: “! npm test”, toast “Typed into its terminal” |
| Forward a reply | ↗ Send to… on a reply | sheets.js `sendForward` | in the receiving chat: “↘ From website · forwarded by you”, your note above the quote |
| Note about a teammate | You write to a teammate from the crew box | desk/act.js | your bubble: “About docs: …” |
| Resend after the usage limit | Resend once the limit is lifted | board/usage.js `resend` | a second copy of the last prompt, as your bubble |

## Messages between sessions

| Kind | Sent when | Hub | How it shows today |
| --- | --- | --- | --- |
| From another session | Another agent session sends this one a message (not another Desk's, which is #472's card) | send.rs `prompt_from_peer` | the family's card with the session's chip and FROM A SESSION (FORWARDED for a forward), its words under it; was “↘ From notes-1 · 2m ago” and its words, on the left |

## Hub notices and Desk steps

| Kind | When | Hub | How it shows today |
| --- | --- | --- | --- |
| Asked for a hand-off note | Cue asked a session for a hand-off note by itself (cache ending) | desk.rs `desk_cue_asked` | a step by Cue on the item's Record: “Asked host-lead for a hand-off note” · why |
| How a Resume went | You pressed Resume on an item | desk.rs `desk_restart_went` | on the item's Needs resume line: “Resume went to X”, or “Resume couldn't reach X: why” |
| The lead didn't come up | You write to a Desk whose lead couldn't start | desk/hub/lead/send.rs `desk_send_with` | your message red in the lead's chat: “! Couldn't send · X's lead didn't come up (…), so this wasn't sent: … · Retry · Edit” |

## Status lines and bands in a chat

One line under a chat, saying where the session is now (`whereNow`, `chat/render.js`), or a line in it. Each is the
family's status line (`.mg-status`), centred at the meta size:

| Kind | When | The line |
| --- | --- | --- |
| Trust its folder | Claude Code asks whether you trust the folder | **Claude Code is asking whether you trust ~/work/cue.** It won't start until you answer. · Trust folder |
| Waiting for Enter | A screen before it starts waits for Enter | **Claude Code is waiting for Enter in its terminal before it starts.** Nothing runs until then. |
| API error, retrying | The API errs or can't be reached, and it tries again | **The API returned an error.** (or **Can't reach the API.**) 529 Overloaded. Claude Code is trying again (3 of 10)… |
| Stuck | No new output for a while | **No new output for 9m.** Running npm test. If it's waiting… |
| Working | A turn runs | Reading header.js… · 20s, or Working on: “its prompt” |
| Compacting, compacted | /compact runs, and after | Compacting: summarizing the conversation to free up context, then Compacted: the conversation was summarized… |
| Waiting on you | It waits on your reply, answer or OK | Waiting on you for your reply (your answer: …, your OK to run: …); on an item's page the same band; on a crew lead, “Waiting on you: docs is ready for review” |
| Usage limit | Its turn stopped on the usage limit | **Out of usage** · resets 4:10pm. “…” didn't run. (or **Opus used up**…), then **Claude Code is back.** “…” didn't run. · Resend |
| Ended by a restart, stopped, ended | Cue restarted mid-turn; you stopped it; the session is gone | Cue restarted mid-turn, so that turn ended…; Stopped by you · it's at its prompt; This session has ended. |
| Restarted | A restart (after an MCP change) finished or didn't | ✓ Restarted with 2 MCP servers · 4s, or ✕ The restart didn't finish: why |
| Rename failed | Claude Code didn't take a new name | **Couldn't give Claude Code the new name: …** Cue keeps “name”. · Try again |

## One-line places

| Place | The lines |
| --- | --- |
| Inbox, a session's run row | Compacting…; Not started: asking whether you trust its folder; No new output for 9m; Queued for X: “…”; Not sent yet: “…”; Sends later: “…”; what it's doing, or Thinking; Delivered to X 14:12, or Sent |
| Session card on the board | Compacting…; Not started: …; what it's doing; Queued: “…”; › its prompt |
| Mini and notch, a Desk's tab | Waiting on you (N), Ready to review, In progress, “+2 working · 2 queued”; “Nothing waits on you here.” when nothing does |

## Your own words, kept as your bubble

- What you type to a session or the lead (a lead's copy has its `<desk …>` block dropped, as before).
- Your note from an item's page ("About #id (title): …", #410's style).
- The "(Crew: on / off …)" line Cue adds to what you type to a crew lead.
