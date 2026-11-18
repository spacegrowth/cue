# Why closing Desk items was so painful on 17 November 2026

A second review, by a fresh session that did none of the day's work.

## How I checked

I read the day's commits (104 across all branches). I read the Desk tables in Cue's database, read-only.
I read the session logs of the Desk lead, the sessions that held the items, and the Codex sessions. I
read the code that finishes an item. I changed nothing but this file.

Times are local. "Lead log" means the Desk lead's session log for the afternoon (id starts `96872caa`),
with its line number. Other logs are named by session.

Two limits. My scan covered the 25 session logs changed that day for this project, plus three Codex
logs. And where I could not prove something, I say so.

## The short answer

Three things caused nearly all of it.

1. **Many sessions worked in one folder.** Three Desks pointed at the same checkout. No item had its
   own copy. Every other problem grows from this.
2. **The Desk's own commit step could not tell whose edits were whose, and nothing checked the result.**
   It took other sessions' half-done edits into an item's commit. No step built a clean copy of main, or
   opened the window from it, before the change reached the running app.
3. **A stuck finish was a dead end.** The item showed one button, "Try again", which repeated the same
   wait. The lead's routes for it were dropped without a word. Two separate waiting systems could wait
   on each other.

The numbers for scale: 57 items on the main Desk were touched that day. The Desk recorded 363 decisions.

---

## 1. The Desk would not open, three times

**Cause.** Commits reached main that called functions main did not have. The window script stopped at
the first missing name, so a click on Desk did nothing. Each time, tests had passed, because tests ran
in the shared folder, where the missing files sat uncommitted.

**Proof.**

- First time. `905cc0e6` at 17:42, a hand commit by cue-200. Its own message says it pushed the whole
  shared `ui/desk.js` into main. That file called `deskApprovals`, `deskSessionRequestFoot`,
  `mateShownFiles` and `deskDeleteSessions`. None were on main. It was a window-only change, so Cue
  served it to the running app at once (`ui_in_place`, `src-tauri/src/hub/desk_finish.rs` about lines
  292 to 325). The owner at 17:45, lead log line 237: "can yuo check why I am not able lick Desk ?".
  Fixed by `115c83d6` at 17:56.
- Second time. `e6869d92` at 21:03. This was **the Desk's own commit** for item #182. It has the item
  title as its message and no author trailer, and the item's commit field lists it. It put the calls to
  `deskApprovals()` and `deskSessionRequestFoot()` back, without `ui/desk_approvals.js`. The owner at
  21:08, lead log line 795: "Oh god, now I again cannot click the desk". Fixed by `c370c65c` at 21:15.
- Third time. `c370c65c` was not enough. `ui/desk.js` still called `deskDeleteConfirmation`,
  `deskDeleteSessions` and `mateShownFiles`. Fixed by `57d72d4e` at 21:29, then `72e759eb` at 21:34.

Main was broken three more times that day, without the Desk going dead:

- `493c6da7` at 14:37, the Desk's commit for #191, took other sessions' half-done edits. Main stopped
  building. Undone by `61040c8f` at 14:39.
- `7948f376` at 14:54 added a file without the line that includes it. Then `8ea72076` at 15:06, the
  Desk's commit for #182, carried others' edits. The build failed at 15:37. `1f2d7a35` at 15:38 undid
  it, and also took #182's window part back out.
- Those two left `deskSaid` declared twice. The web view skips a script that does that, with no error.
  Fixed by `2453ee79` at 16:21.

So main broke six times. Three of the six were the Desk's own commits.

**Why the Desk's commit leaked.** A fix shipped at 14:54 to keep other sessions' edits out of an item's
commit (`src-tauri/src/hub/desk_theirs.rs`). It reads other sessions' logs to learn what they changed.
It has two holes:

- It only reads logs in one format: Edit, Write and shell blocks (`read_edits`, about lines 109 to
  143). It has no code for Codex logs. The missing functions were written by Codex sessions on another
  Desk. I checked: `ui/desk_approvals.js` was created by a Codex session started on 16 November.
- It only reads sessions that are still listed (`theirs`, about lines 57 to 76). A session that ended
  leaves edits nobody owns.

When it finds no one else's edit in a file, it treats the whole file as the item's (`own_part` returns
`Own::Whole`, about line 152). So unknown edits are committed under the item.

**Missing evidence.** I looked for a step that refuses a commit calling a name main lacks. I found none
in `desk_finish.rs`. Sessions ran a duplicate-name check by hand. I did not find it wired into the
finish step.

**Fix.**

- Give each item its own working copy. Then a commit can only hold that item's work.
- Before any change is installed or served to the running app: build a clean copy of the new main and
  load the window from it. Fail on a missing name or a doubled name. Send the item back if it fails.
- Until both exist: read Codex logs too, and treat an edit nobody owns as "not mine", not as the item's.

## 2. Items stuck on "Try again", no "Looks right"

**Cause.** "Looks right" shows only when an item's work is committed and installed. An item whose commit
waits past 15 minutes gets a question, and an item with a question shows only "Try again". That button
runs the same commit step again. It hits the same wait.

**Proof.**

- `ui/desk.js` line 871: an item with a finish question gets the "Try again" button. "Looks right"
  needs the state `to_check` (line 302, and lines 879 to 882).
- `src-tauri/src/hub/desk_finish.rs` line 18: the wait is 15 minutes. Line 235 writes the question
  "Its commit has waited 15 minutes". Lines 601 to 622: Try again just reruns the commit.
- Item #202 started waiting at 18:25 and was not ready to check until 21:03. That is 2 hours 38
  minutes. #203 started waiting in the same minute. #184 started waiting at 20:42.
- The lead had told the owner the button would be there. At 20:59, lead log line 645: "You're right, I
  was wrong. 'Try again' means each item's commit step stopped partway".

**Fix.**

- Say on the item what the commit waits on and who holds it. The wait text already exists.
- Don't offer "Try again" while the block is still there. Offer what would clear it: "Ask them to
  commit", or "Commit without that file".
- With one working copy per item, this wait goes away.

## 3. Commits waiting on each other in a circle

**Cause.** Cue has two kinds of waiting. An item can be queued "after" another item. And a commit can
wait for another session to commit a shared file. Nothing looks at both together. So A can be queued
behind B while B's commit waits on A's session.

**Proof.**

- Decision at 14:39 and again at 20:41: "Queued #183 after #182 for cue-180".
- Lead log line 735, at 21:02: #182 could not commit until cue-180 committed its change to
  `src-tauri/src/desk/mod.rs`. But cue-180's item, #183, was queued to start after #182. And #184, #200
  and #202 each waited on #182's uncommitted changes in `ui/desk.js`.
- Decision at 21:02: the lead "broke the loop by starting #183 now instead of after #182".
- A second circle. Decision at 21:38: "#213 commits before #183", "they were each waiting on the other".
- Item #182 also went through four review rounds (decision at 20:55), with the others held behind it.

The first circle stood from 20:41 to 21:02 before anyone saw it. The commit wait only speaks up after
15 minutes, and then only as "Try again".

**Fix.**

- Keep one list of who waits on whom, covering both kinds of wait. Check it for a circle every time a
  wait is added. On a circle, tell the lead at once and name the items.
- With one working copy per item, the commit wait goes away, and only the queue is left.

## 4. A wait that blamed the wrong session

**Cause.** The wait said "cue-180 also changed src-tauri/src/desk/mod.rs". cue-180 had not. Codex
sessions on another Desk made that edit.

**Proof.**

- Decision at 21:03: "Cue's commit wait blames you for the desk/mod.rs lead-instructions edit, but
  Codex sessions #193/#194 made it".
- cue-180's own log has no edit of that file in the main folder. Its only shell commands naming it
  before the wait are reads (grep, sed without in-place, git diff).
- A Codex log from that day (started 11:40) has two patches to `src-tauri/src/desk/mod.rs`.
- cue-180 ended it by committing the nine changed lines itself, as `4250b016` at 21:03.

**Why Cue named cue-180: not proven.** My best lead: one of cue-180's helpers worked in a separate copy
of the repo and did change its own copy of that file there. `desk_theirs.rs` reads helpers' logs but
resolves their relative paths against the session's main folder (about lines 70 to 72 and 113). A
shell write in the copy would then look like a write in the shared folder. I could not find the exact
line that triggered it. The Codex sessions could never be named, because their logs are not read.

**Fix.**

- Resolve a helper's paths against the folder that helper ran in.
- Read Codex logs.
- When Cue cannot tell who changed a file, say "someone else changed it", not a name.

## 5. The lead's routes were ignored

**Cause.** Two causes, both silent.

- Cue drops a route for an item that is done or finishing. `src-tauri/src/hub/desk.rs` lines 412 to
  414: the code returns with no message.
- The lead once wrote its routes in the wrong form, with arrows and dashes Cue does not read.

**Proof.**

- Lead log line 735, at 21:02: "Cue also ignores routes for an item that's mid-commit, which is why my
  last routes did nothing."
- Lead log line 666, at 21:00: "my last routes were written in the wrong format, so none of them
  reached a session".

The stuck items were all "finishing". So the lead could not act on exactly the items that needed help.

**Fix.**

- Every route gets an answer. Taken, or refused with the reason.
- A line that looks like a route but does not parse gets "I couldn't read this route".
- For a finishing item with a question, let the lead act: retry, change the order, or hand the blocked
  file to its owner.

## 6. Turn-ends with no DONE line: the count is not 53

**The count differs.** The lead got 53 by counting a phrase in its own log (lead log lines 968 and
1006). A log stores each message several times. The same count gives 60 now. That number is copies,
not notices.

What I counted, as separate messages:

- **12** "stopped with no DONE" notices reached the afternoon lead. #183 six times (16:02, 16:22,
  20:40, 21:02, 21:03, 21:38). #214 four times, all between 21:28 and 21:29. #188 once and #184 once,
  at 20:42.
- **1** more reached the morning lead (#191 at 14:32). So **13** for the day.
- Separately, Cue sent holders **35** "Done? Write DONE" nudges. 22 of those were before 09:15.

**Cause.** Most stops were not stops. The session was waiting on a build, a test run, another commit,
or a yes from the owner. Last lines of those turns: "I'm waiting on the test run", "still compiling",
"The build for 57d72d4e has started", "Recommendation: send fixes".

**Proof.**

- Decision at 21:29: the lead told cue-124 to keep its turn open through the build, because "every stop
  so far was just 'still building'".
- Decision at 20:36: a standing yes on "send fixes", to stop that kind of stop.
- `src-tauri/src/hub/desk_stall.rs` lines 86 to 89 already spot phrases like "waiting on" and "waiting
  for". But `desk_stall.rs` lines 213 to 218 still send the lead a notice in that case.

**Fix.**

- A turn that ends saying what it waits on is "waiting", shown as such, with no notice to the lead.
- Send one notice per item, not one per turn. #214 got four in two minutes.
- Let a session wait for its own build inside its turn.

## 7. The old look coming back

**Cause.** Work was shown as done while it was only in the shared folder, not on main. Early in the day
installs were built from that folder, so the work showed. Then installs changed to build from a clean
copy of main. Uncommitted work vanished from the app.

**Proof.**

- The switch: `ad75c2bd` and `21065643` at 09:12 to 09:15 (#181), and `1b4a438b` at 14:43 (#190).
- Lead log line 203, at 17:39: "The new screens aren't lost. They were never committed, so the 17:37
  install couldn't include them… Earlier installs were built straight from this folder, uncommitted
  edits and all".
- The undo `1f2d7a35` at 15:38 also removed #182's window part from main.
- Lead log line 514, at 20:49: #200's Stop button and #202's rows were never committed.

The switch to clean main was right. It exposed the gap. It did not cause it.

**Fix.**

- An item is never shown as ready or done until its commit is on main. Check the commit, not the folder.
- One working copy per item makes "in the folder but not on main" impossible to mistake for done.

## 8. The one commit that cleared it all

**Which.** `8089ad86` at 21:45, by cue-180: "Commit the Desk and hosted-session work left uncommitted
in main". 164 files. Then `394be0db` at 21:48 merged #183 (46 files).

**How it came about.** At 21:43 cue-180 offered two plans and advised the careful one. At 21:44 the
owner, worn out, told it to commit everything. Decision at 21:50: "committed every other leftover
change in main as-is, unreviewed | you said to commit everything."

**What is in it.**

- 70 files under `test-results/`: 69 deleted, 1 changed. Old test output that had been checked in.
- 42 screenshots in a stray top-level folder named `undefined/`. About 8 MB. Test data only.
- `src-tauri/src/testing/tests/desk.rs.orig`, 1173 lines. A merge leftover.
- 2 mock pages under `work/`, 3 docs, 4 tests.
- About 43 real source files: 33 in `src-tauri`, 10 in `ui`. 2209 lines added, 136 removed.

The source includes #213's work, unreviewed (`desk_finish.rs` alone grew by 206 lines). It also
includes the other Desk's unfinished Delete Desk and approvals code. At 21:28 cue-124 had judged the
Delete Desk server side as not finished.

**Was it right?** Yes, as a way out. It was the owner's clear choice. It ended both circles in one
move. It built and installed. The tests that ran afterwards passed, and ten window tests that already
failed on clean main still failed the same way. I checked the added text for private words and found
none. cue-180 rightly left out one file that held a private name.

**What it risked.**

- Unfinished features are now on main with nobody having read them.
- Per-item history is gone. 43 source files from at least three Desks sit under one message. Undoing
  one item, or finding which change broke something, is now hard.
- Light and dark were not checked by eye.
- It is public. The owner told the lead at 21:58 to push all 13 commits as they were (lead log line
  1022). The remote main is now `394be0db`. So `undefined/` and the `.orig` file are in the public repo.

**Fix.**

- One cleanup commit: remove `undefined/` and the `.orig` file, and stop tracking `test-results/`.
- Review #213's code and the Delete Desk and approvals code as their own items.
- Make "commit everything left over" a real Desk action with guard rails: list the files by owner
  first, leave out junk, and keep one commit per item where owners are known.

---

## Judging the lead's own account

The file is `docs/desk-day-review-2026-11-17.md`, written by the lead at 21:54.

**What it got right.**

- The root cause: many sessions in one folder.
- The three commits that fixed the Desk not opening.
- The two undos, `61040c8f` and `1f2d7a35`.
- Both circles, and who waited on whom.
- The wrong blame for `desk/mod.rs`. The logs support it, as far as they go.
- Routes for a finishing item being dropped. The code confirms it.
- "Try again" being the only button.
- `8089ad86` being 164 files and unreviewed.
- Its own three mistakes. It was honest about them.
- The owner's "Yes, all three" on #183 (lead log line 378, at 18:27).

**What it got wrong.**

- "53 stopped notices." It was 12 to that lead, 13 in the day. The count was of stored copies.
- That `desk_approvals.js` was #182's own file. A Codex session on another Desk made it. Item #214's
  title repeats the error.
- "Nothing is pushed." True at 21:54. Not true after 21:58. All 13 commits were pushed.
- "About 15 items." 57 items were touched that day, about 25 of them after 14:00.
- "About eight sessions." It undercounts. Three Desks shared the folder.

**What it missed.**

- The blind spot in the separation fix: Codex logs and ended sessions are not read. That is why a fix
  shipped at 14:54 still leaked at 21:03.
- That half the breaks came from the Desk's own commits, not from sessions committing by hand.
- That window-only changes go to the running app with no check at all.
- That main broke six times, not three.
- Why finished work vanished: the install source changed mid-day.
- That the queue and the commit wait are two systems that never compare notes.
- The junk inside `8089ad86`, now public.
- It said it had not checked whether #213 is inside `8089ad86`. It is.
- The four review rounds on #182, and the churn from hand-offs and one usage-limit stop.
- That the pattern is still live. As I write, item #215's edits sit uncommitted in the shared folder.

Its five proposed fixes are sound and match mine in direction. It leaves out the check on main before
a change ships, which I rank second.

---

## Ranked fix list

Highest impact first. "Removes" names the issue numbers above.

1. One working copy per item, landed on main as one commit by the Desk. Removes 1, 2, 3, 4, 7, 8. Large.
2. Check every landing: build a clean copy of the new main and load the window from it, before install
   or in-place serving. Fail on a missing or doubled name. Removes 1, 7. Medium.
3. Until 1 exists: read Codex logs and ended sessions, resolve helpers' paths correctly, and treat an
   edit nobody owns as "not mine". Removes 1, 4. Medium.
4. Let the lead act on a finishing item, and answer every route: taken, or refused with the reason,
   or "couldn't read it". Removes 2, 5. Small.
5. One list of who waits on whom, with a circle check that tells the lead at once. Removes 3. Medium.
6. "Try again" says what it waits on and offers what would clear it. Removes 2. Small.
7. Waiting is not stopping: no notice when the turn says what it waits on, and one notice per item.
   Removes 6. Small.
8. Show an item as ready only when its commit is on main. Removes 7. Small.
9. Clean up `8089ad86`: remove the stray folder and the `.orig` file, stop tracking test output, and
   review the unreviewed code as items. Removes the debt from 8. Small.
10. Cap review rounds at two per item, and don't hold other items behind a review. Eases 3. Small.

Proof that fix 1 works already exists. At 14:32 the Desk moved cue-191 to its own copy "because the
shared checkout was full of other sessions' half-done edits". Two other sessions tested in clean
copies by hand that day. Those are the ones that found the breaks.
