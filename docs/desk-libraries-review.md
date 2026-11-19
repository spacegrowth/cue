# Could proven Rust libraries replace Cue's own wait, queue and step code?

Review for Desk item #293, 2026-11-18. No code was changed.

## In short

| Library | Would replace | Recommendation |
|---|---|---|
| petgraph | the circle check in `hub/desk_waits.rs` | No, not now |
| apalis | the landing line and the install queue | No |
| statig | the item states and finishing steps | No, but a plain Rust enum: yes |
| notify + tokio channels | the 3 s and ~10 s timers | Yes, in part, keeping a slow timer as a backstop |

The one change worth making soon is waking the finishing work with an event instead of waiting for the next tick. The second is typing the item steps as enums.

## petgraph: waits and the circle check

**What it would replace.** In `hub/desk_waits.rs` (about 60 lines), `waits()` builds the list of who waits on whom. `circles()` and `walk()` then search that list for circles.

**Gain.** petgraph is well tested and ships `tarjan_scc` and `is_cyclic_directed`, so the hand-written search would go.

**Cost.**
- Today the only wait is "queued after", and an item has at most one `after`. Every item points at no more than one other, so finding a circle just means following the pointers. The current code already does that, and is small and tested.
- petgraph's main cycle tools (`tarjan_scc`, `is_cyclic_directed`) find groups of items that are tangled together, or just say whether a circle exists, not the circles one by one. Cue tells the lead about each circle as its own list of links (`circle_key`, `circle_words`), so we would still write code to turn each group back into a circle.
- It would also add a new dependency.

**Risk.** Low, but nothing would be gained.

**Recommendation: no.** Look again only if waits gain more kinds and an item can wait on several things at once (files, sessions, items). With many links per item, finding every circle is hard to get right by hand, and then petgraph pays off.

## apalis: the landing line and the install queue

**What it would replace.**
- The landing line (`desk_land_line` and `land` in `hub/desk_trees.rs`): one item at a time, oldest DONE first. LAND FIRST moves an item up. An item that has to wait steps aside, and a check cut short is put back in its place.
- The install queue (`finish_tick` in `hub/desk_finish.rs`): one install at a time, everything waiting goes in one install, about half an hour after the first item. It holds off while a session is mid-turn unless you say Install now, and it follows `status.json` and `running.json`.

**Gain.** Workers, retries and job storage come built in, and jobs survive restarts.

**Cost.**
- Both queues are not separate jobs. They are views over the items' own fields (`finish.step`, `first`, `at`, `tree`), saved with each item. apalis would keep a second copy of that in its own job tables, and the two would have to be kept in step. That is the kind of drift that strands an item.
- Cue's rules (coalescing, the half-hour wait, holding off for a mid-turn session, LAND FIRST) don't fit a standard job queue's model. We would end up writing them around apalis rather than with it.
- Cue's SQLite library and apalis's SQLite backend each link their own SQLite. Cargo allows only one, so their versions would have to match exactly. That can block upgrades of either.

**Risk.** High for the gain: two sources of truth for an item's finishing step, plus a build-level conflict.

**Recommendation: no.** The queues are short (a few items), single-machine, and already survive restarts, because the item is the record.

## statig: item states and finishing steps

**What it would replace.**
- Item states, kept as text: "new", "queued", "working", "finishing", "to_check", "done", "dropped".
- The finishing steps, also text: "commit", "line", "checking", "landing", "queued", then the install steps.
- These are set in about 24 places for states and 19 for steps, across `hub/desk*.rs` and `desk/`.

**Gain.** Allowed moves would be written in one place and checked when the code is built.

**Cost.**
- statig drives a state machine by sending it events through handlers. Cue's code reads and writes the item in place, inside a store write. Fitting that model means rewriting most of the Desk's finishing code.
- The states are also saved and sent to the window and tests as text, so statig's types would need saving and loading glue.

**Risk.** Medium to high. A wide rewrite of code that lands and installs your work, for a check we can get more cheaply.

**Recommendation: no to statig. Yes to plain Rust enums** for the item state and the finishing step:
- Saved and sent with the same text, so the window, the store and the tests don't change.
- Plus one `fn can_move(from, to)` that every change goes through, with a test covering each allowed move.

That gives most of the safety (no misspelled step, no unplanned move) for a small change that can be done one file at a time.

## notify plus tokio channels: react to changes instead of timers

**What it would replace.**
- Polling every 3 s: live sessions and folders (`local_reads`), and the usage files (`usage_file`).
- The finishing tick, about every 10 s (`desk_finish_tick`). It also checks `install/status.json` and the main line for commits that take work back out.

**Gain.**
- Faster reactions: an install's progress, a commit on main and a finished check would show at once instead of up to 10 s later.
- Less disk and CPU work while nothing changes.

**Cost.**
- Watches have to be set up for each folder and file.
- On macOS, file events can arrive grouped or late, and are sometimes dropped.
- Your other machines are read over SSH and can't be watched, so their schedule stays as it is.

**Risk.** Low to medium if the timers stay as a slower backstop, say every 30–60 s. High if they are removed: a missed event would leave an item stuck.

**Recommendation: yes, in two steps.**
1. Inside Cue, no new library: a `tokio::sync::Notify` (or a channel) that kicks the finishing work as soon as something it waits on happens. That covers a DONE, a commit made, an install step written, or a check ended. Several places already call `desk_finish_tick()` directly for this; one wake-up would replace them and fold a burst of changes into one run.
2. Then `notify` (FSEvents on macOS) for the local files only: `install/status.json`, `install/running.json`, and the main checkout's branch file under `.git/refs` (for the taken-out check). Keep the timer as a slower backstop.

## Other fits looked at

- **`tokio_util::time::DelayQueue`**, for the half-hour install wait and Cue's own "later" sends (`hub/send_at.rs`). It's neat but not needed: those are a few timestamps checked on a tick. No.
- **`gix`, a git library in place of running `git`**, for the landing line's git work (`land_keep.rs`, `safe_git.rs`). Bigger gain in speed and error handling, but a large change, and git's own merge rules are what we rely on. Out of scope here; worth its own review if git calls ever show up as slow.

## What to do next, if you want it

1. A wake-up for the finishing work, keeping the 10 s timer as a backstop. Small change, quick win.
2. Item state and finishing step as enums, with `can_move` and a test for each allowed move. Medium change.
3. File watches for the install files and the main branch, after step 1. Small to medium change.
