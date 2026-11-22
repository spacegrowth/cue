# Faster checks: test and finish cards sooner (#678)

A plan only. Nothing here is built yet.

## The problem, simply

Cue has about 2,500 tests. Running all of them takes around ten minutes and uses most of the Mac.

Today every card runs all of them, about three times, before it says done. And only one card can run
them at a time, so the others wait in a queue.

It is like every cook in a kitchen re-tasting the whole menu after changing one dish, one cook at a
time.

In the last four days, 243 cards landed. Together they spent about 100 hours on these long runs.

There is a second problem. Once a card has landed, nobody runs all the tests on the result. So the one
place where "everything together still works" matters is never checked.

## The fix, simply

Three changes.

**1. A card tests only what it touched.**
A card that changed the Desk window runs the Desk window tests. A card that changed how cards land
runs the landing tests. A card that changed only a document runs nothing. This takes two minutes or
less, and no card waits for another.

**2. All the tests run once, after landing, on the second Mac.**
Each time cards land, the second Mac runs everything on the result. If three cards landed together,
one run covers all three. This Mac stays free for the work.

**3. The app only updates to a version that passed.**
The half-hourly install takes the newest version that passed all the tests. So the app you use never
gets a broken version.

**If the full run fails:** Cue finds which card broke it, reopens that card, and sends its session the
failure. The other cards stay landed.

## Which Mac runs what

No Mac runs all the tests for every bug fix any more. That is the whole point.

- **The Mac where a card is worked on** (this Mac, or any other Mac a card is sent to) runs only that
  card's small set. Two minutes or less. It never runs everything.
- **The second Mac** runs everything, but not once per card. It runs once for each group of cards that
  landed since its last run. Five bug fixes landing close together get one run, not five.

So on a day with sixty cards: today that is about 180 full runs, all on this Mac. With this plan it is
sixty short runs spread over wherever the cards are worked on, plus perhaps twenty to thirty full runs
on the second Mac, where nobody is waiting on them.

The only time this Mac runs everything is when the second Mac has been out of reach for 30 minutes.
Then it does one run, at a moment when no card is building.

## What you would notice

- Cards reach "to check" about 20 to 30 minutes sooner. A typical card takes 109 minutes today.
- This Mac is less busy, so everything else on it runs faster too.
- A new line on the Desk after cards land: "Full run passed" or "Full run failed".
- Now and then a card you saw land is reopened a few minutes later, because the full run caught
  something its own tests could not see.

The 20 to 30 minutes is an estimate from the session logs, not a measurement.

## The trade-off

A card's own tests can miss a break in another part of Cue. Today the card would catch that itself
before saying done. With this plan the full run catches it ten to fifteen minutes after landing,
before the app updates.

So the main line can be wrong for a few minutes. The app you use cannot.

The other way would be to run everything before each card lands. That is safest, but cards land one at
a time, so each would wait ten minutes behind the one ahead. With sixty cards a day that is slower
than today.

## If the second Mac is off

After 30 minutes with no answer, the full run happens on this Mac instead, at a moment when no card is
building. The Desk says where it ran.

## Build order

1. **Card tests.** Gives most of the speed. A few days of work. (built)
2. **The full run on the second Mac**, with the Desk line and reopening the card that broke it. (built)
3. **The install waits for a passed run.** (built)

Step 1 can ship first. But once cards stop running everything, something else must, so step 2 should
follow within days.

## Not in this plan

After a card lands, it waits a median of 26 minutes before it shows as "to check". That wait is the
install, not the tests. docs/desk-install-lane-plan.md covers it.

## Details for whoever builds it

- Card tests: a new mode, `tests/run.sh card`. It compares the card's files against main and picks
  tests from a small map file in the repository (folder → tests). A session that finds a gap adds a
  line.
- Shared ground (the hub's core, the data model, the database, the test harness) has no safe narrow
  set. Those cards run the full suite for the side they touched. They are few.
- The hand-off message for each card gains one line: run `tests/run.sh card`; do not run the full
  suite yourself.
- Guard: a full Rust run started inside a card's copy stops and points to `tests/run.sh card`. A
  session with a real reason sets one named switch to go ahead.
- Full run: this Mac sends each new main commit straight to the second Mac over the connection Cue
  already has. Nothing goes through GitHub. A plain script there (no AI) keeps one clean copy with a
  warm build folder and runs the build, the code check, the Rust tests, the node tests and the window
  tests. One run at a time.
- Finding the card on a failure: first run what failed once more. What passes that time fails only
  sometimes: it is listed apart and no card is blamed for it. For the rest, search the batch's commits
  by halving, running only the failing tests. A test file that is not in a commit does not fail there,
  so a new failing test is blamed on the commit that added it.
- A card is blamed only when the commit found is that card's own landed commit on this Desk. It is
  reopened once per commit, and only when it has landed (to check, done, or waiting for its install).
  A card already at work is not reopened: its session gets a note.
- A run is never made twice. Asked again for the same two commits, the script waits for the run still
  going, or reads the result already there, and answers with it. So after Cue restarts it asks for the
  same run again, not for a new one. `--again` forces a new run.
- Window-only changes served in place keep being served right after landing. If the full run fails on
  one, it is reopened and the served window goes back one step.
- The 2-hour rule: if main has moved and no full run has finished on it (passed or failed) for 2 hours,
  or nothing is kept of full runs and none is going, the install takes the newest version as it did
  before, and one Desk line says the app was installed without a full run.
- A failing run never blocks the app for good:
  - The run failed and no card of the batch caused it (it failed before the batch, it fails only
    sometimes, it was the first run, or the search could not finish). The app still updates to that
    version. The Desk line names the failing tests and says so. Cue keeps a separate note that its
    tests failed.
  - The run failed and a card caused it. The app stays on the last version that passed, for at most
    4 hours from that failure. After that it takes the newest version, and one Desk line says the app
    was installed although the full run still fails, naming the card. A later passing run clears this.
  - If the last version that passed is no longer on main, nothing counts as passed and the same rules
    apply.
- Install now is the owner's word: it takes the newest version without waiting for a run.
- Not built: putting the served window back one step when the full run fails on a window-only change.
  The card is reopened; the window keeps what was served until its fix lands.
- First step of the build: time a full window-test run on both Macs. Nobody has recorded it.
- Watch: if more than one card in ten is reopened by the full run, the map is too narrow.
- Today's numbers: a full Rust run is about 7 minutes alone (docs/dev/test-speed.md); 786 window tests
  in 131 files; about 690 runs of four minutes or longer in four days.
