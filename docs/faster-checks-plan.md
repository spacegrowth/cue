# Faster checks: a plan to test and finish cards sooner (#678)

A plan only. Nothing here is built yet.

## The short version

Today every card's session decides for itself what to test, and most run everything, often several
times. The full runs take turns across all cards, so cards wait on each other.

The recommended setup:

1. **Each card runs only the tests for what it changed.** One command picks them. Two minutes or less.
2. **The full run happens once, after landing, on the second Mac.** One run covers every card that
   landed since the last one.
3. **The half-hourly install waits for a green full run.** Main can be wrong for a few minutes. The app
   you use never is.
4. **If the full run fails, Cue finds the card that broke it and reopens it**, with the output.

Expected gain: about 20 to 30 minutes off a typical card (median today: 109 minutes from picked up to
"to check"), more on busy days, and this Mac stops spending about a day of processor time per day on
repeated full runs.

## What happens today

Measured from the last four days of session logs and the Desk's own step times.

- 243 cards landed.
- Their sessions spent about 163 hours inside test and build commands. About 100 of those hours were in
  runs of four minutes or longer: roughly 690 long runs, close to three per card.
- A full Rust run takes about 7 minutes alone on this Mac, 8 with the build (docs/dev/test-speed.md).
  Only one full Rust run goes at a time across all copies of the project, so with five cards at work a
  session can wait half an hour for its turn.
- The window tests are 786 tests in 131 files, run two browsers at a time. Nobody has recorded how long
  a full run takes. Measuring it is the first step of the build.
- The check Cue runs before a card lands only loads the window from a clean copy. It runs no tests.
  So "did the whole suite pass on main with this card in" is never asked after landing. Only a release
  asks it.
- Step times for a typical card: 136 minutes from picked up to the first build, 13 minutes from tests
  to commit, then 26 minutes (mean 82) between landing and "to check". That last wait is the install,
  not the tests. See "Not in this plan".

So two things are wrong at once. Cards pay for the full suite again and again, and the one moment where
the full suite matters, main after a landing, has no run at all.

## Part A. Card tests: one command, only what the card touched

A new mode of the test script: `tests/run.sh card`. It looks at the files the card changed against main
and runs the matching tests.

Which tests each kind of card runs:

- **Docs or notes only:** nothing.
- **Window files for the Desk** (`ui/desk/`): the Desk window tests (`desk-*.spec.mjs`), the
  cut-off-text test, and the window load check.
- **Other window files** (`ui/chat/`, `ui/app/`, and so on): that area's window tests, the cut-off-text
  test, the node tests, and the window load check.
- **Desk hub code** (`src-tauri/src/desk/`): the build, the code check (`xtask check`), and the Rust
  tests for that part of the Desk. Landing code runs the landing and finish tests. Routing code runs the
  routing and queue tests. Not all 77 Desk test files.
- **Other Rust code** (hosted sessions, voice, install, the hook): the build, the code check, and that
  module's tests.
- **Both window and Rust:** both sets.
- **Shared ground** (the hub's core, the data model, the database, the test harness, the fake bridge):
  no narrow set is safe. The command says "wide card" and runs the full suite for the side it touched.
  These are few.

The map from files to tests lives in one small file in the repository, so a session that finds a gap
adds a line.

The hand-off message for each card gains one line: run `tests/run.sh card` before you say done; the
full run happens after landing; do not run the full suite yourself.

**Guard.** A full Rust run started inside a card's copy stops with a line pointing to `tests/run.sh
card`. A session with a real reason sets one named switch to go ahead. Without the guard, habit wins
and nothing gets faster.

## Part B. The full run: once, after landing, on the second Mac

Each time main moves, this Mac sends the new commit straight to the second Mac over the connection Cue
already has to it. Nothing goes through GitHub and main is not pushed anywhere public.

On the second Mac, a plain script (no AI) keeps one clean copy of the project with a warm build folder
and runs, in order: the build, the code check, the full Rust tests, the node tests, the full window
tests. One run at a time. If three cards land while a run is going, the next run covers all three.

The result comes back to the Desk as one line: "Full run passed on main (cards #a, #b, #c)" or "Full
run failed".

**When it fails.** The script runs the failing tests alone on each card's commit in the batch, oldest
first, to find which card broke them. That takes a minute or two, not a full run each. That card is
reopened and goes back to its holder with the output, the same way a failed landing check does today.
The other cards in the batch stay landed.

**When the second Mac is off or out of reach.** After 30 minutes without an answer the run happens on
this Mac instead, at a quiet moment (no card building). The Desk says where it ran.

**Nightly.** The long tests that drive the real app (e2e) and the soak already have a nightly script.
They stay nightly and move to the second Mac too.

## Part C. The install waits for a green full run

The half-hourly install takes the newest commit on main that a full run has passed, not the tip of
main. A card is "to check" once it is in the installed app, as today.

Window-only changes that are served in place today keep being served right after landing. They have
the window load check and their own window tests behind them. If the full run then fails on one, it is
reopened and the served window goes back one step.

## Why the full run goes after landing, not before

The other honest choice is to run the full suite inside the landing line, before each card reaches
main. Main would never be wrong. But the line is one card at a time, and each time main moves the check
starts again. At ten minutes a run and sixty cards a day, the line alone would need ten hours, and
every card would wait behind the ones ahead. That is slower than today.

After landing, with the install held back, gives the same protection where it counts (the app you run)
at the cost of main being wrong for a few minutes now and then. Cards cut from main in those minutes
could inherit the break. The reopened card's fix usually lands within the hour, and their own card
tests would show a break in their area.

## How much faster

Estimates, from the numbers above. The first week of the build should replace them with measured ones.

- **Per card:** about three long runs today, around nine minutes each, plus waiting for a turn. With
  card tests at two minutes or less and no turns to wait for, a typical card saves 20 to 30 minutes of
  its 109. A card that had to wait behind other full runs saves more.
- **This Mac:** about 100 hours of long runs in four days go away, replaced by about 60 full runs a day
  at most on the second Mac (fewer in practice, since a run covers a batch). Builds and the app itself
  get that processor time back.
- **Tokens:** a small saving. Fewer turns spent waiting on, reading and re-running long test output.
- **What gets slower:** nothing a card waits on. A break in another area shows up 10 to 15 minutes
  after landing instead of before the session says done.

## Risks, and how we would know

- **Card tests miss a break in another area.** The full run catches it before the install. The cost is
  a reopened card. Count the reopens that come from the full run. If more than one card in ten is
  reopened this way, the file-to-tests map is too narrow and gets widened.
- **Tests that pass on one Mac and fail on the other.** The first week will show them. Each is a real
  bug in a test (timing, paths) and gets fixed, not skipped.
- **The second Mac becomes one more thing to keep running.** The 30-minute fallback covers a day when
  it is off. If it is off for long stretches, the full run simply lives on this Mac at quiet moments,
  and Parts A and C still hold.

## Build order

1. Measure a full window-test run on both Macs. Half an hour of work.
2. Part A with the guard. Gives most of the per-card gain by itself, and is safe to try alone because
   sessions can still be told to run everything.
3. Part B. The full run on the second Mac, the result line on the Desk, finding the card on a failure.
4. Part C. The install takes the last green commit.

Part A alone is a fair place to stop and look. But without B, nothing runs the full suite at all once
sessions stop doing it, so A should not stay alone for more than a few days.

## Not in this plan

- **The wait between landing and "to check"** (median 26 minutes, mean 82). It is the largest wait after
  the work itself, and it is the install lane, not testing. docs/desk-install-lane-plan.md covers it.
  Part C must not make it longer: a full run is shorter than the half hour between installs.
- **Making the full run itself faster** (more browsers at once on the second Mac, where no window
  server is in use by a person; Rust tests that no longer take one shared turn). Worth doing after B,
  when the full run has one home and its time is measured daily.
