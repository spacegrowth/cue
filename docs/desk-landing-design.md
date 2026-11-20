# How a Desk item lands and gets checked

A design, for any project a Desk runs. Status: step 6 (parts 1 to 3, for Cue's fixed setup) is built and not yet
merged, in #251. Step 7, the per-project setting with its defaults, is built and not yet merged, in #252: the Desk's
header shows it and Change opens it; the engine honours the shared folder or an own copy, items at once, one commit
per item or you commit, and no check or Cue's own. Step 8, PR landing and the push choices, is built and not yet
merged, in #253. Your own steps for the check and after landing, one settings screen per project (how work lands,
Ask me before, Away, old working folders) and the lead proposing a change you say yes to, are built and not yet
merged, in #381. A branch and merge are still shown as "not yet".

It follows from the review of 17 November 2026, `docs/desk-fable-review-2026-11-17.md`. Where a choice
here rests on that day, I point to the section there as "review 1" to "review 8".

## The idea in one paragraph

Each project gets one setting: **how work lands**. It answers six questions. Where does an item work?
How does its work reach main? What must pass first? What happens after it lands? Who may push? How
many items work at once? Today Cue
answers these the same way for every project, and mostly by accident: every item works in the shared
folder, the Desk commits whole files, and nothing checks main before the app gets the change. That is
what made 17 November painful (review 1, 2, 3 and 7).

## The setting

One block per project. Six parts, each with a small set of choices.

| Part | Choices | Default |
| --- | --- | --- |
| Where an item works | shared folder, own worktree, branch | own worktree |
| How it lands | Desk squash, merge, PR, owner commits | Desk squash |
| Check before landing | none, or one or more steps in order, each marked "each landing" or "before going live" | none, and the item says so |
| After landing | nothing, a command (install, deploy, serve) | nothing |
| Push | never, ask each time, after each landing | ask each time |
| Items at once | a number | three |

**Where it is set.** In the Desk's own settings for that project, kept in Cue's data, next to the
Desk's name and its "Ask me before" choices. Not in the project's files. A project stays free of Cue's
bookkeeping, and a public repo never gains a Cue file.

The check is set there too, and only there. Cue reads no check file from the project (decision 2 in
part 8).

---

## 1. Where an item works

**Status:** built for "own worktree, a pool of three" (#251). An item past three waits for a free copy and
says so. A dropped item's copy with work in it is kept and listed under "Kept copies", with a two-click Remove;
the seven-day ask is not built. The other choices wait for step 7.

Three choices.

**Shared folder.** Every session edits the one checkout. This is today. It is fine for one session, or
for a project where nothing is built. With more than one writer it is the root of review 1, 2, 3, 4
and 7. With one writer, the Desk can still commit for the item, since every edit in the folder is
that item's. With more than one writer it cannot know whose edits are whose (review 1 and 4), so the
Desk commits nothing and the owner commits (see part 2).

**Own worktree.** Each item gets its own copy of the repo, on its own branch, cut from main when the
item starts. Only that item's holder and its helpers write there. This is the default.

**Branch.** The item gets a branch in the shared folder. This only works when one session works at a
time, since a folder holds one branch. It suits a single-session project that wants history per item.

**Who creates and removes them.** Cue does, not the session.

- Created when the item is routed and its holder starts. The holder is told the folder and works there.
- Kept while the item is open, including while it waits to be checked.
- Removed when the item is closed and its commit is on main. The branch goes with it.
- Kept, and listed, when an item is dropped with work in it. After seven days Cue asks the owner once:
  remove it, or keep it. Cue never deletes work that is not on main without that yes.
- One worktree per item, shared by the holder and its helpers. Helpers do not get their own.
- An item that touches two repos gets a worktree in each.
- A count of live worktrees shows on the Desk, with their size on disk.

**What it costs with a slow build.** A fresh copy has no build output. A first build in it can take
many minutes and several gigabytes. Three ways to cut that, in order of how much they save:

1. **Don't build in the item's copy at all.** Most items only need the fast tests. The full build runs
   once, in the check before landing (part 3), in one place that stays warm.
2. **Share the build cache.** Point every worktree at one shared build folder or cache where the
   toolchain allows it. Builds then reuse each other's output. The cost is that two builds at once
   wait on each other.
3. **Reuse worktrees.** Keep a small pool, say three. An item takes a free one and resets it to main.
   The build output inside stays warm. Past the pool, items queue.

The setting has a number for this: how many items may work at once. With a slow build, a small number
is right. 17 November ran far more at once than the build could serve (57 items touched in a day).

**Evidence that it works.** At 14:32 that day the Desk moved one session to its own copy "because the
shared checkout was full of other sessions' half-done edits". Two other sessions tested in clean
copies by hand. Those are the ones that caught the breaks (review, last section).

## 2. How it lands

**Status:** Desk squash and the landing line are built (#251): one item at a time, oldest DONE first, a
fast-forward of main. The item shows "in the landing line" and what it waits on. The commit wait is gone. Merge
and PR landing are not built.

Four choices.

**Desk squash.** The Desk takes the item's branch and puts it on main as one commit. The message is
the item's title, plus the holder's own summary line. One item, one commit. This is the default. It
keeps undo simple, which the 164-file commit lost (review 8).

In the shared folder there is no branch. There, with a single writer, the Desk commits all the
folder's changes as one commit, with the same message rule.

**Merge.** The Desk merges the item's branch, keeping its commits. For projects that want the steps.

**PR.** The Desk pushes the branch and opens a pull request. The item waits, shown as "in review",
until it is merged. For projects with reviewers or a remote check.

**Owner commits.** The Desk commits nothing. The item shows its changed files and waits for the owner.
This is the only choice allowed with the shared folder and more than one writer. With the shared
folder and a single writer, Desk squash is still allowed.

**When main has moved.** The Desk brings the item's branch up to main first. If that is clean, it goes
on to the check. If the two clash, the item goes back to its holder with the clashing files named. The
Desk never resolves a clash itself.

**One at a time.** Landings go through a single line, one after another. Each is checked against main
as it will be after it lands. Two items can never land on top of each other unchecked.

**Who may push.**

- Sessions never push main. That does not change.
- The Desk may push an item's branch for a PR, since that is not main.
- A push to main asks the owner first. This is the existing rule and it stays the default. The ask
  names the commits. The owner can choose "after each landing" for a project to stop being asked.
- "Never" is there for projects with no remote, or where the owner pushes by hand.

## 3. The check before landing

**Status:** built for Cue (#251): the check runs before landing, shown as "checking…". A failure shows "check
failed" and goes back to its holder with the output. A repository with no check lands as "landed, not checked".
The split into "each landing" and "before going live" is not built yet.

**What it is.** One command per project, run by the Desk, not by the item's session. It runs in a
clean copy holding main plus the item's change. Exit code zero passes. Anything else fails.

A project can list more than one step: build, tests, open the window. They run in order and stop at
the first failure. Each has a time limit.

Each step is marked "each landing" or "before going live". The first kind runs for every item, before
it lands. The second kind runs once on main before an install, deploy or in-place serve, covering
every item landed since the last one. It is for steps too slow to run per item. If a "before going live" step fails,
nothing goes live, the items stay at "On main", and the lead is told which landings came since the
last good run. A project with no Live step can only use "each landing".

**More than one repo.** The check runs in each repo the item touched. All must pass before any of them
lands. Then they land one after another. If a later one cannot land, the Desk undoes the earlier ones,
so an item is on main everywhere or nowhere.

**Why the Desk runs it, in a clean copy.** On 17 November every break had passed its tests. The tests
ran in the shared folder, where missing files sat uncommitted (review 1). A check only means something
when it sees exactly what main will hold.

**Where it is set.** In the project's landing setting (see "The setting"). Cue can suggest one when it
recognises the project: the test script of a package file, the test command of a Rust or Go project.
It suggests. The owner confirms. Cue never guesses and runs.

**When no check exists.** The item still lands. But it says "landed, not checked" in plain words, on
the item and on its commit card. It never says "checked". The Desk also shows one standing line for the
project: "No check is set. Items land unchecked." An owner who wants silence can set the check to
"none on purpose", and the line goes away.

**How a failure is shown.**

- The item goes back to its holder, state "check failed". Nothing reaches main.
- The item shows the step that failed and the last lines of its output. The full output is one click
  away.
- The holder gets the same text and is asked to fix it.
- The lead gets one line: which item, which step.
- A second failure of the same step on the same item goes to the lead as a decision, not a third try.

**A check that fails on main already.** Cue runs the check on plain main now and then. If main itself
fails, landings pause and the Desk says so, with the first commit that failed. Otherwise every item
gets blamed for someone else's break. On 17 November ten window tests failed on clean main before and
after the big commit (review 8), and no one could tell new failures from old.

## 4. What "done" means

Four steps, in order. An item shows exactly one of them.

| Step | What happened | What the item shows | Buttons |
| --- | --- | --- | --- |
| Said done | The holder wrote its DONE line | "Done, being checked" | none |
| Checked | The check passed on main plus this change | "Checked, landing" | none |
| Landed | Its commit is on main | "On main" with the commit | Looks right, Send back |
| Live | Installed, deployed or served | "On main and live" | Looks right, Send back |

How the other landing choices fit these steps:

- **Merge.** The same four steps. Landed means the merge is on main.
- **PR.** The check runs before the PR is opened. After Checked the item shows "In review" with a
  link to the PR, and no buttons. It becomes Landed when the PR is merged and Cue sees its commit on
  main. A PR closed without merging sends the item back to its holder.
- **Owner commits.** The Desk still runs the check, on the item's changed files as they stand. After
  Checked the item shows "Checked, waiting for your commit" with its files. It becomes Landed when
  Cue sees a commit on main that holds those changes. Under the shared folder with several writers
  the check sees everyone's edits, so the item says "checked with others' changes in the folder".

Rules that hold this together:

- **"Looks right" appears only from Landed on.** Never before. On 17 November items showed as ready
  while their work was only in the folder, and vanished from the app at the next install (review 7).
- **Cue decides Landed by looking at main**, not at the folder and not at what a session said. It asks
  the repo whether the commit is in main.
- **Live is its own step.** For a project with no install or deploy command, there is no Live step,
  and Landed is the last.
- **A project that has a Live step says which one the item is on.** "On main, in the next install at
  about 21:30" is honest. "Ready" was not.
- **Nothing is served before Checked.** Today a window-only change goes to the running app at once
  with no check (review 1). Serving in place becomes a kind of Live step, so it comes after the check.
- **Send back after landing** asks one thing: "Undo it" or "Fix it". "Undo it" makes a new commit
  that undoes the item's commit, and reopens the item. "Fix it" leaves the commit on main and reopens
  the item, and the fix lands as a second commit. Undo is for a broken app. Fix is for the rest.
  With one commit per item this is one clean undo.

Only the owner closes an item. That does not change.

## 5. The waits

**One list.** Cue keeps one list of who waits on whom. Every kind of wait goes in it:

- an item queued after another item,
- an item waiting for a free worktree,
- an item waiting its turn in the landing line,
- an item waiting on its check,
- an item waiting on a person: the owner, a reviewer, a push answer.

Each entry says who waits, on what, since when.

**The circle check.** Each time an entry is added, Cue walks the list. If the new entry closes a
circle, Cue does not add it silently. It tells the lead at once, names every item in the circle, and
offers which link to cut. On 17 November the first circle stood 21 minutes before anyone saw it, and
there was a second one that night (review 3).

With own worktrees, the worst wait of that day is gone: a commit waiting for another session to commit
a shared file. It remains only under the shared folder choice.

**The wrong name.** A wait names a session only when Cue knows. When it does not know, it says
"someone else". On 17 November a wait named a session that had not made the edit (review 4).

**What "Try again" offers.** The button goes. It reran the same wait (review 2). In its place the item
says what it waits on, and offers what would clear that:

| The item waits on | It offers |
| --- | --- |
| Another item ahead in the landing line | "Land this first" |
| A check that failed | "Send back to fix", "Show the output" |
| A clash with main | "Send back to bring up to date" |
| A session editing a shared file (shared folder only) | "Ask them to commit", "Land without that file" |
| The owner's answer | the question itself, with its answers |
| A PR review | "Open the PR" |
| Nothing Cue can name | "Run the landing again", and it says this is a plain retry |

A plain retry is still there for a real hiccup. It is just never the only thing on offer while a known
block stands.

**Waiting is not stopping.** A session that ends its turn saying what it waits on is shown as waiting.
The lead gets no notice for it. One notice per item, not one per turn (review 6).

## 6. The lead's part

**What it may do to a finishing item.** Today the lead can do nothing: its routes for a finishing item
are dropped (review 5). The lead gets a short list instead:

- Move it up or down the landing line, without asking. Each move is recorded as a decision with its
  reason, so the owner can see it.
- Send it back to its holder, with a reason.
- Cut a link in a circle, when Cue shows one.
- Hand a failed check to another session, when the holder is gone or cold.
- Ask the owner, when the choice is the owner's.

**What it may not do.** Skip the check. Push. Land an item by hand. Close an item. Those stay with the
Desk's rules and the owner.

**Every route gets an answer.** One line back to the lead, every time:

- "Taken: #12 to session A."
- "Refused: #12 is landing. You can move it, send it back or ask." With the reason, always.
- "Couldn't read this line as a route." For a line that looks like a route and does not parse. On 17
  November the lead wrote routes in the wrong form and learned of it only when nothing happened
  (review 5).

The lead's summary of the Desk shows the waits list, so it sees a block before the owner does.

## 7. Defaults, and Cue's own setup

**A new project with no settings.**

- Where: own worktree, one per item.
- Items at once: three.
- How it lands: Desk squash.
- Check: none set. Items say "landed, not checked". Cue suggests a check if it recognises the project.
- After landing: nothing.
- Push: ask each time.
- If the folder is not a git repo: shared folder, owner commits, no check. The Desk says why.
- If the owner runs a single writer and wants the old way: shared folder, with Desk squash or owner
  commits, as the owner picks.

These defaults are safe before they are fast. Nothing reaches main mixed. Nothing is called checked
that was not.

**Cue itself.**

- Where: own worktree, a pool of three, with a shared build cache so a copy starts warm.
- How it lands: Desk squash, one landing at a time.
- Check, each landing, in order: build; the Rust tests; the window loaded from the clean copy,
  failing on any name that is missing or declared twice. These three would have caught all six breaks
  of 17 November (review 1).
- Check, before going live: the window tests. They run once before each install, and also before
  each in-place serve, since serving in place is going live (part 4). Window-only landings that are
  waiting together are tested and served as one.
- After landing: window-only changes are served in place, after the check. Others join the half-hourly
  install, as now.
- Push: ask each time.
- The release rule stays as it is: a release is built without add-ons and refuses to ship otherwise.

**The path from today, ranked by impact and size.**

1. **An interim check before anything is installed or served.** Build a clean copy of the new main
   and load the window from it. Fail on a missing or doubled name. Needs no worktrees. Stops the Desk
   going dead (review 1). Medium. It is interim because it runs after the commit is already on main:
   it can stop a bad install, not a bad commit. At step 6 it moves to where part 3 puts it, before
   landing.
2. **Show an item as ready only when its commit is on main.** A small change to what the item shows.
   Ends "the old look came back" (review 7). Small.
3. **Answer every route, and let the lead act on a finishing item.** At this step the lead can send an
   item back, hand it on, or ask the owner. Moving an item in the landing line arrives with step 6,
   and cutting a circle with step 5. Small.
4. **Waiting is not stopping.** One notice per item, none when the turn says what it waits on. Small.
5. **One waits list with the circle check, and "Try again" replaced.** The item says what it waits on
   and offers what clears it. The two belong together, since the offers read the list. At this step
   the list holds today's waits: the queue and the commit wait. Three offers arrive later, with
   step 6: "Land this first", the wait for a free worktree, and "Send back to bring up to date".
   Medium.
6. **Own worktree per item, created and removed by Cue, with Desk squash and the landing line.** The
   big one. It removes the cause under review 1, 2, 3, 4, 7 and 8. Large. The check from step 1 moves
   before landing. The waits list gains the landing line, the worktree pool and the check, and loses
   the commit wait. Steps 1 to 5 make the days until then bearable, and all are still needed after.
   Built to the answers in part 8: three items at once, one worktree per item shared with helpers,
   a worktree in each repo for an item that spans two, landed in all or none, the check split into
   "each landing" and "before going live", "Undo it" or "Fix it" on Send back, and the seven-day ask
   for a dropped item's worktree.
   **Status:** built in #251, not yet merged: the pool of three with the wait for a free copy, Desk squash, the
   landing line, the check before landing, "landed, not checked", and kept copies with Remove. The commit wait
   is gone. Still to come: the seven-day ask for a kept copy, the check split, an item that spans two
   repositories landed in all or none, "Undo it" or "Fix it" on Send back, "Land this first", and "Send back
   to bring up to date".
7. **The per-project setting itself, with its defaults.** Until here, Cue runs on its own fixed
   setup. This step opens it to other projects. The check is set in the Desk's
   settings only, and a project with none lands as "landed, not checked". Medium.
8. **PR landing and the push choices.** Only when a project asks for them. Medium.
   **Status:** built in #253, not yet merged. A pull request: the check runs in the landing line, then the item's
   one commit is pushed as its own branch (never main) and a pull request opened with GitHub's gh; the item shows
   "In review" with the link and "Open the PR". Cue reads the pull request once a minute: merged and its commit seen
   on main (the remote's, fetched; the main checkout brought up to it as a landing moves it, edits kept, only while
   it's on the pull request's base branch and no other item is landing; until then the item says what it waits on),
   it's landed; merged but never seen on main for half an hour, it stops and asks for the landing to be run again;
   closed unmerged, it goes back to its holder with its copy kept. A push or a pull request that fails says why and
   waits, with "Run the landing again". It needs an own copy: an item without one (its holder on another machine, or
   in a worktree of its own) lands as before. Push: "ask each time" is the rule as it was (a session's push to main
   asks you; the Desk pushes nothing); "never" has the Desk push and ask nothing; "after each landing" has the Desk
   push main as it is (all of it, never forced) after each landing, saying on the item whether it did. Every push,
   fetch and gh call is cut off after a minute and never prompts. An open pull request for the item's branch is used
   again; dropping or sending back an item in review leaves its pull request open on GitHub. Sessions never push main. A session's push is
   still guarded by Ask me before, whatever the push choice.

Until step 6 ships, one stopgap from the review still earns its place: read Codex logs and ended
sessions, and treat an edit nobody owns as "not mine" (review, fix 3).

## 8. The open questions, settled

Ten questions were open. Nine are decided here, each with its reason. One is the owner's alone and
comes last.

1. **Pool size: three items at once on Cue, and three as the default.** A fresh copy needs a build,
   and 17 November ran far more at once than the build could serve.
2. **The check lives only in the Desk's settings.** Nothing of Cue's goes into a project's files, and
   a public repo never gains a Cue file.
3. **With no check set, the item lands and says "landed, not checked".** A new project should work on
   day one, and the harm on 17 November came from calling things ready that were not, not from
   landing them.
4. **A long check is split.** Fast steps run on each landing. Slow steps run once before going live.
   For Cue: build, Rust tests and the window load on each landing, the window tests before each
   install or in-place serve. The first three catch every break of that day, and the install is
   already batched.
5. **One worktree per item, shared by the holder and its helpers.** The holder already splits its
   helpers' work by file, and one copy means one build and one landing.
6. **An item in two repos gets a worktree in each, and lands in all or none.** Every repo is checked
   first, and a landing that fails undoes the ones before it. Half an item on main is the mixed state
   this design exists to end.
7. **A dropped item's worktree is kept seven days, then Cue asks once.** Long enough to change one's
   mind, short enough that the disk does not fill. Cue never removes unlanded work without a yes.
8. **Send back after landing offers "Undo it" or "Fix it" each time.** Undo suits a broken app and
   fix suits the rest, and only the person looking at it knows which. One commit per item keeps the
   undo clean (review 8).
9. **Push: see the owner's question below.** Until answered, Cue stays on "ask each time", which is
   the existing rule.
10. **The lead may reorder the landing line without asking, and each move is recorded.** On 17
    November the lead ended the first circle by reordering, and waiting for the owner would have kept
    it standing (review 3). The rest of the list in part 6 stands as written.

### For the owner to decide

**When may Cue push Cue's own main?**

- **Ask each time, as now (recommended).** The repo is public, and the check is new and unproven. The
  ask names the commits.
- **Ask once per install.** One ask covers every item in that install. Fewer asks, same control.
- **After each landing, without asking.** Fastest. Only worth it once the check has held for
  some weeks.

"Ask once per install" is not one of the setting's push choices today. If the owner picks it, it is
added as a fourth. Either way this is one value in the setting, and no build step waits on it.
