# Desk tests: what they check, and what's left (#498)

These tests are written from what the Desk is meant to do, not from what the code does now. The sources are
docs/desk-routing-rules.md, docs/desk-routing-merged.md, docs/cue-messages.md, the other Desk docs, and the "done
when" of closed Desk items. Each test names its source in a one-line comment.

How to run them:
- The hub: `cd src-tauri && cargo test --lib desk`
- The window: `npx playwright test -c tests/ui/playwright.config.mjs tests/ui/specs/desk-*.spec.mjs`

Results: **passes** · **caught a bug, fixed** (where) · **skipped** (why, and the item it waits on).

## Routing (`desk_spec_routing.rs`)

| Test | What it checks | Why (source) | Result |
|---|---|---|---|
| a_session_with_two_open_items_is_refused_a_third… | A third open item for one session is refused, and the lead is told to route it again | Merged rule 6, routing rules "Refuse a third open item" | skipped: both routing docs are proposals not picked yet; today one session may hold a chain of any length (see "Not tested") |
| an_item_waiting_for_your_check_doesnt_count_toward_the_two | One to check plus one at work still takes one more behind it | Routing rules, "Open" | passes |
| a_send_back_of_an_item_the_session_holds_isnt_a_third | A send-back counts as the same item, not a new one | Merged rule 6; routing rules, "Open" | passes |
| the_leads_instructions_say_a_session_holds_at_most_two… | The lead's instructions state the cap | Merged build order 1 | skipped, with the cap |

## Queued items and waits (`desk_spec_queue.rs`)

| Test | What it checks | Why (source) | Result |
|---|---|---|---|
| nothing_queues_behind_an_item_waiting_for_your_check | Queued behind an item to check: it starts now, and the lead hears why | Routing rules, "Open" | passes |
| a_queued_item_starts_by_itself_when_the_item_ahead_is_dropped | Dropping the item ahead starts what waited on it | #340 | passes |
| dropping_the_middle_of_a_chain_leaves_the_last_waiting_on_the_first | Dropping the middle of a→b→c leaves c waiting on a, not started on the same files | Routing rules 8; lead guidance "chain each one after the previous item" | caught a bug, fixed in desk/hub/turn/act.rs and desk/hub/turn/route.rs (`hand_on`) |
| parking_the_middle_of_a_chain_leaves_the_last_waiting_on_the_first | Same for Park: nothing is left waiting on a parked item | #385 | caught a bug, fixed in desk/hub/stall/unstick.rs |
| parking_the_item_ahead_starts_what_waited_on_it | Park with nothing ahead starts what waited on it | #385 | passes |
| start_now_on_an_item_queued_after_another_hands_it_to_its_holder_now | Start now on a queued item hands it to its session as new work, not "pick up where you left off" | #401, #476 | caught a bug, fixed in desk/hub/turn/act.rs and ui/desk_page.js |
| put_before_an_item_already_at_work_is_refused_and_nothing_moves | Put before an item at work is refused, with a reason | desk_queue.rs, #387 | passes |
| a_queued_item_stays_queued_across_a_relaunch… | Queued survives a relaunch and still starts by itself | #256 | passes |
| the_waits_list_names_what_each_queued_item_waits_on_and_since_when | The Waits list names what waits on what, and since when | #250 | passes |
| you_cut_a_queued_items_wait_and_it_starts_now | Cutting a link starts the item; a missing link is refused | #250 | passes |
| a_chain_moves_one_step_at_a_time | When the first is done the second starts; the third still waits | Routing rules 8 | passes |
| the_lead_sees_how_long_the_item_ahead_has_been_at_work | The lead's Desk block shows how long the item ahead has been at work | Routing rules 9, merged rule 11 | skipped: to file, the lead's block gives no age |

## Copies and landing (`desk_spec_copies.rs`, `desk_spec_landing.rs`)

| Test | What it checks | Why (source) | Result |
|---|---|---|---|
| two_items_at_once_work_in_copies_of_their_own… | Two items at once each get a copy and each lands only its own work | docs/desk-landing-design.md | passes |
| a_copy_whose_check_failed_keeps_its_work… | A failed check keeps the copy's work; it lands only once the fix does | docs/desk-landing-design.md, #246 | passes |
| another_sessions_new_file_and_edit_in_the_shared_folder_stay_out… | Another session's files in the shared folder stay out of the item's commit | docs/desk-finish-breaks-main-2026-11-18.md | passes |
| the_second_item_in_the_landing_line_is_checked_with_the_first_already_on_main | The landing line checks each item on top of the one before | docs/desk-landing-design.md | passes |
| an_item_whose_check_failed_is_never_to_check… | A failed check never shows as to check; Looks right is refused | docs/desk-landing-design.md | passes |
| an_item_that_clashed_with_main_is_never_to_check… | Same for a clash with main | docs/desk-landing-design.md | passes |
| a_later_commit_on_main_that_takes_a_copied_items_work_out_opens_it_again | Work taken out by a later commit reopens the item | docs/desk-gaps-2026-11-19.md | passes |
| a_landing_with_nothing_set_never_pushes_main | Landing never pushes unless that's set | docs/desk-landing-design.md | passes |
| an_item_dropped_while_it_waits_in_the_landing_line_never_lands | Dropped in the line: never lands | docs/desk-landing-design.md | passes |
| an_item_sent_back_while_it_waits_in_the_landing_line… | Sent back in the line: lands only at its next DONE | docs/desk-landing-design.md | passes |

## To check, done, send-back (`desk_spec_check.rs`)

| Test | What it checks | Why (source) | Result |
|---|---|---|---|
| looks_right_on_a_landed_item_closes_it_and_leaves_main_as_it_is | Looks right closes it; main is untouched | docs/desk-item-page-design.md | passes |
| send_back_after_landing_keeps_its_commit… | Send back keeps the first commit, sends your words, and the fix lands as a second commit | docs/desk-landing-design.md | passes |
| reopen_on_a_done_item_puts_it_back_to_check_with_its_commit… | Reopen puts a done item back to check with its commit | docs/desk-item-page-design.md | passes |
| a_dropped_item_at_work_tells_its_holder_to_stop_and_a_late_done_lands_nothing | Drop tells the holder to stop; a late DONE lands nothing | docs/cue-messages.md, "Dropped" | passes |
| a_dropped_item_to_check_leaves_the_desk_and_its_landed_commit_stays | Dropping an item to check leaves its commit on main | docs/desk-landing-design.md | passes |

## Notes and quick phrases (`desk_spec_notes.rs`, `desk-spec-quick.spec.mjs`)

| Test | What it checks | Why (source) | Result |
|---|---|---|---|
| a_note_held_while_its_holder_is_busy_goes_in_when_its_turn_ends | A note to a busy holder is kept, then goes in; never lost | #437, #419 | passes |
| a_typed_note_on_an_item_to_check_while_its_holder_is_busy… | That note reopens the item; the busy turn about something else isn't taken as the item stopping | #437, #484 | caught a bug, fixed in desk/hub/stall/stall.rs |
| every_quick_phrase_on_an_item_to_check_reaches_its_holder… | Each phrase on an item to check is a note; the item stays to check | #488, #477 | passes |
| a_quick_phrase_while_its_holder_is_busy_waits… | A phrase for a busy holder waits; the item stays to check | #488, #419 | passes |
| a_quick_phrase_on_a_working_item_goes_to_its_holder_at_once | A phrase on a working item is sent at once | #477 | passes |
| two_notes_while_its_holder_is_busy_both_reach_it | A second note doesn't replace the first | #437 | passes |
| (window) the lead's chat shows a session chat's quick phrases | The same phrases over the lead's box | #128 | passes |
| (window) a phrase goes at once on its own; the draft stays; a double click sends once | In the lead's chat, a session's page and an item's page | #477 | passes |
| (window) an item to check: a phrase with a draft goes as a note; the item stays to check | | #488 | passes |

## Waiting on you and asks (`desk_spec_asks.rs`, `desk-spec-waiting.spec.mjs`)

| Test | What it checks | Why (source) | Result |
|---|---|---|---|
| the_leads_ask_is_answered_once_and_the_answer_reaches_it_once | An answer reaches the lead once; a second click sends nothing | desk.rs `desk_answered` | passes |
| the_answer_to_the_leads_ask_reaches_it_after_a_busy_turn | Answering a busy lead goes in after its turn | #419 | passes |
| a_note_answering_a_holders_question… | A note that answers a question takes the item off Waiting on you | #419, #475 | passes |
| a_holder_that_asks_you_is_not_asked_done… | "Done?" is asked once; a question for you never gets "Done?"; the lead isn't told it stopped | #182, desk_stall.rs | passes |
| (window) the lead's panel lists what waits on you, the same as the list's group | Each row opens its item | #200, #392 | passes |
| (window) answering an item's question on its page takes it off Waiting on you | Group, count and panel | #475 | passes |
| (window) a tool approval on an item's session waits on you as that item | It leaves once allowed | #299, #431 | passes |
| (window) an item whose session stopped without asking anything isn't waiting on you | | #182 | passes |

## The lead's lines and refusals (`desk_spec_lead_lines.rs`, `desk_spec_messages.rs`)

| Test | What it checks | Why (source) | Result |
|---|---|---|---|
| every_line_of_one_turn_cue_cant_make_comes_back_in_one_message | Refused ROUTE, SPLIT, MERGE and CUT in one turn come back as one message, none dropped | #248, docs/cue-messages.md | caught a bug, fixed in desk/hub/turn/said.rs (SPLIT, CUT and MERGE refusals were sent on their own) |
| a_route_to_a_parked_item_is_refused_and_it_stays_parked | A route to a parked item is refused, with why | LEAD_SKILL, PARK | passes |
| route_new_names_the_item_just_above_it_in_the_reply | Two ITEM+ROUTE pairs route each item | LEAD_SKILL | passes |
| dropping_an_item_at_work_tells_its_holder_to_stop_work_on_it | | cue-messages "Dropped" | passes |
| a_moved_item_tells_its_old_holder_where_it_went_and_to_stop | | cue-messages "Moved" | passes |
| parking_a_queued_item_tells_its_session_its_off_its_list | | cue-messages "Parked" | passes |
| an_item_line_from_a_holder_without_why_is_not_a_new_item… | | cue-messages "Not a new item" | passes |
| an_item_line_several_desks_could_take_is_added_to_none… | | cue-messages "Several Desks" | passes |
| to_lead_from_a_session_on_several_desks_with_no_item_comes_back_unsent | | cue-messages "TO LEAD not sent" | passes |
| to_lead_that_cannot_reach_the_lead_tells_the_session_with_its_words | The session gets back what it wrote, not only that it failed | cue-messages "TO LEAD failed", #474 | caught a bug, fixed in desk/hub/core/desk.rs and desk/hub/turn/heard.rs |

## Hand-offs (`desk_spec_handoff.rs`)

| Test | What it checks | Why (source) | Result |
|---|---|---|---|
| every_item_a_handed_off_session_holds_moves_to_the_session_that_took_over | Every item it holds that isn't done (at work, queued, to check) moves, and the new session is on the Desk | Routing rules, "Fit with the context bands"; merged rule 17 | passes |

## Esc, send and pull-back in Desk chats (`desk-spec-esc-send.spec.mjs`)

| Test | What it checks | Why (source) | Result |
|---|---|---|---|
| the lead's chat: Enter sends; kept while it works; Esc brings back just your words | Not the Desk summary Cue put before them, and ahead of what you typed since | #494 | caught a bug, fixed in ui/chat/compose.js |
| the lead's chat: Edit on a kept message puts your words back in the lead's box | The box it went from | #422 | caught a bug, fixed in ui/app/clicks_chat.js, ui/desk_act.js |
| a session's page: kept while it works; Edit brings it back into that page's box | The draft is kept after it | #494, #422 | passes |
| an item's page: Esc takes back the note kept for its busy holder | Your words alone, the draft after them | #494, #422 | passes |
| Esc with words in a page's box keeps the page and the words; empty, it closes the page | Nothing typed is lost | #494 | passes |
| an item's page: one Esc doesn't stop its working holder; the second does | | #494 | passes |

## Existing window tests fixed on the way

- desk-redraw-keep: the test sometimes read the line the redraw had just replaced. It now waits for the redraw to
  finish. The app was right.
- desk-old-copies: since #447 made the screen shorter, the label is its section's heading.
- The Give to… menu lost its scroll on a redraw (#308). Fixed in ui/desk_lead.js: it's placed again from where you'd
  scrolled it, once the list's scroll is back.

## Already covered (not written again)

- Routes made and refused in general: desk_routing.rs, desk_routes.rs. Routes off the Desk: desk_members.rs.
- Put before, and moving the queue: desk_queue.rs. Copies waiting their turn: desk_copy_wait.rs. Pause and resume:
  desk_pause.rs.
- Install after landing, and its failures: desk_install.rs, desk_lands.rs.
- Messages between Desks: #472's band, in flight.

## Not tested, and why

- **Cue refusing a third open item** (merged rule 6). Both routing docs say they're proposals, and Cue lets one
  session hold a chain of any length; older tests (desk_queue, desk_copy_wait, desk_pause) rely on that. The lead
  follows the cap from its own guidance. Whether Cue should enforce it is still to pick; the two tests stay skipped
  until then.
- **The lead seeing how long the item ahead has been at work** (routing rules 9). Skipped: to file.
- **In-flight items:** a failed landing going to the lead (#483), closing without installing (#490), unpark (#499),
  remote items (#500), a stuck session (#502), the change feed (#384) and the look of Cue's messages (#489, #501).
  They're being changed now; their tests belong with them.
- **Sizes and cold wakes** (merged rule 17: 110k, 140k, the hand-off at cache expiry). They aren't Cue settings yet.
