# Special lines in Cue's chats (Desk item #676)

Agents on a Desk talk to Cue in lines that start with a word in capitals: `ROUTE:`, `DONE:`, `DECIDED:` and so on.
Cue reads them and acts. This page lists every kind that really shows up, how often, and how each one looks in
the chat today, so the mocks for #676 can give each kind its own look instead of capitals.

## Where the numbers come from

One read-only pass over the last eight days of Claude Code transcripts for this repo and its worktrees (about 640
transcripts, mid-November 2026). A line counts when it starts with a capitalised word and a colon, after an
optional list mark or bold. "Lead" means a transcript that routes work (it has `ROUTE:` lines); "session" is every
other one. Counts are rough: a transcript that was compacted and resumed repeats some lines, and a few lines are
quoted inside explanations rather than said for real. Cue's own messages to the lead were counted from the user
side of the lead transcripts.

## What the lead writes

The lead is the one chat where nearly every kind shows up. Most of a lead's reply is these lines.

| Kind | Rough count | Real example (shortened) | Looks like today (lead chat) |
|---|---|---|---|
| ROUTE | 1,260 | `ROUTE: #43 -> cue-new-idea \| why: stopped at the usage limit, pick up the mocks where you left off` | One grey sentence: "Gave Task #43 to cue-new-idea", the why folded under it |
| ITEM | 490 | `ITEM: bug \| Chat replies show as plain text, not formatted \| done when: replies show bold, lists and links` | "Added Bug #12: title", what you'll see folded under it |
| NOTE | 380 | `NOTE: #466 \| Don't start. It stays queued behind #519, which is paused.` | A small card with a NOTE tag in capitals: "lead → cue-387 NOTE Feature #418", the words under it |
| DECIDED | 375 | `DECIDED: the mocks go on a new canvas, not the crowded one` | A block headed "Decided on its own · N" in small caps, each call with its reason in grey |
| ASK | 170 | `ASK: Remove Actions? \| choices: Remove it fully / Keep it hidden / Leave it for now` | Taken out of the text; Cue shows it once as an "Asked: …" card, your answer beside it |
| SEE | 140 | `SEE: #388 \| Click a Cue notification: it opens the item it's about` | "Said what you'll see for Feature #388", the words folded under it |
| START | 105 | `START: #350` | Hidden from the chat |
| TITLE | 90 | `TITLE: #113 \| The bug icon sits on its own line above the title` | "Renamed Bug #113", the new title under it |
| DONE | 60 | `DONE: #638` | "Bug #638 is done", its title under it |
| CLOSE | 50 | `CLOSE: #321 \| commit: 5fdfd2e1` | Hidden from the chat |
| TAKE COPY | 20 | `TAKE COPY: #466 \| why: it waits on a layout pick` | Hidden |
| MERGE | 15 | `MERGE: #467 into #465 \| why: same build of option E` | "Folded #467 into Feature #465" |
| DESIGNS | 13 | `DESIGNS: #676 <canvas link>` | "Open the mocks ↗", a link in the accent colour |
| LAND FIRST | 9 | `LAND FIRST: #331` | Hidden |
| ANSWER | 9 | `ANSWER: #604 \| The owner is away; leave it as it is` | Hidden |
| PAUSE / RESUME | 9 | `PAUSE: #519 \| why: the redesign it served was stopped` | Hidden |
| COPY / COPY FIRST | 10 | `COPY FIRST: #583 \| why: the fix is written; only the check is left` | Hidden |
| DROP, HOLD, SPLIT, PASS | 2–4 each | `SPLIT: #125 \| task: new mocks \| bug: what you send back doesn't stay` | Hidden |
| TELL DESK / TELL DESKS | 2 | `TELL DESK: cue \| Got it, your test message showed up` | A band naming the other Desk |
| CONTEXT | under 5 | `CONTEXT: me -> keep \| why: still mid-review` | "Kept my context" |
| Lines naming a card | ~760 lines start with `#123` | `#638 is ready to land. The one open thing is my scratch test.` | Plain text; the number is a clickable chip only inside the kinds above |

## What sessions write

A session (one working an item) writes far fewer kinds, but writes them a lot. In its own chat these mostly show
as plain text, capitals and all, because the special drawing above is the lead chat's.

| Kind | Rough count | Real example (shortened) | Looks like today (session chat) |
|---|---|---|---|
| DONE | 1,720 | `DONE: #638` | Raw line in capitals at the end of the reply |
| DECIDED | 1,070 | `DECIDED: Fix the cause in the row code, not the shared helper` | The "Decided on its own" block |
| DECIDED (minor) | 310 | `DECIDED (minor): copied a helper binary into this worktree so the tests would build` | Raw line: the block only knows `DECIDED:` without "(minor)" |
| CHECK | 315 | `CHECK: #615 \| Try: open the page, answer a question in A, press Turn on Away` | Raw line in capitals |
| TO LEAD | 220 | `TO LEAD: my scratch test must not land; removing it waits on the owner's OK` | Raw line; in the lead chat it arrives as a card tagged TO LEAD |
| DESIGNS | 190 | `DESIGNS: #676 <canvas link>` | Raw line with a bare link |
| IF PICKED | 75 | `IF PICKED: #593 \| 2 \| task \| Keep opened steps expanded \| you'll see: no change` | Raw line, pipes and all, also inside the item's card |
| ITEM | 70 | `ITEM: task \| Mocks for the item page` | Raw line |
| USER VERIFY | 20 | `USER VERIFY: open an item a session holds and check its chat opens` | Raw line |
| SEE, NOT DONE, ASK | under 10 each | `NOT DONE: #594 \| zoom back to whole-window; chat text size moves to A−/A+` | Raw line |

## What Cue writes to the lead

Cue sends the lead its own messages. Since #616 the lead chat folds each into one short line, "+ Cue · Desk
changes", that opens in place. Item and question messages get a card with a capitalised tag instead.

| Kind | Rough count | Real example (shortened) | Looks like today |
|---|---|---|---|
| Desk changes | 160 | `#558 is stuck finishing: its landing step hasn't moved for 187 minutes …` | Folded line "+ Cue · Desk changes", the gist under it |
| Your answer to an ASK | 140 | `Your ASK: Put the selected rows back to the dark highlight? The answer: Yes` | Not drawn; the ASK card already shows your answer |
| New items | 135 | `New item #43 task: mocks for how the Desk shows "waiting on you"` | A card tagged NEW ITEMS, a row per item |
| Stopped without DONE | 120 | `#191 stopped: cue-191 ended its turn with no DONE and no question` | A card from Cue with a capitalised tag |
| Back from Away / Away | 130 | `You're back after 6h · 3 done, 1 sent back` | Folded line, with "See what happened ›" |
| Ready to check | (per DONE) | `Task #125 is ready to check` | A card tagged READY TO CHECK or WAITING FOR INSTALL |
| Not closed | rare | `#320 wasn't closed: its work isn't on main yet` | A card tagged NOT CLOSED |
| Hand-off | rare | `cue-3 hands #12 to cue-7` | A card tagged HAND-OFF |
| Picked, Checked, Not done, Paused, Resumed, Moved, Parked, Dropped… | rare each | `#650: option B was picked, so it's done. #678 builds it.` | Folded "+ Cue · Picked" line in the lead chat; in a session's chat the same one-line Cue fold |

## What this means for the mocks

- Five kinds carry most of the weight: ROUTE and NOTE in the lead chat, DONE, DECIDED and CHECK in session chats.
  Each of these needs a look you can tell apart at a glance.
- Today the lead chat already turns its lines into sentences, but they all look alike: a grey line each. The
  capitals that remain are card tags (NOTE, TO LEAD, NEW ITEMS, READY TO CHECK, NOT CLOSED, HAND-OFF) and the
  "Decided on its own" head.
- Session chats show most of their lines raw. `DONE:`, `CHECK:`, `IF PICKED:`, `TO LEAD:`, `USER VERIFY:` and
  `DECIDED (minor):` read as shouted code at the end of a reply.
- Lines that only steer Cue (START, CLOSE, TAKE COPY, LAND FIRST, PAUSE, COPY…) are hidden in the lead chat today.
  They are a fifth of the lead's lines, so a quiet trace of them ("Started #350", "Closed #321") may be worth
  showing.
- About 760 plain lines start with a card number ("#638 is ready to land…"). A card chip with the item's title
  would make these clickable without changing the words.
