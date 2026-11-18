# Why finished items keep breaking main, and the one fix

A fresh session's look, 18 November 2026. It builds on `docs/desk-fable-review-2026-11-17.md`. I read the
commits, the Desk's items (read-only), the session logs and the Codex logs. I changed nothing but this file.

## The short answer

When an item finishes, the Desk commits the item's files **from the shared folder**, where other sessions
also have unfinished work. It tries to subtract the other sessions' edits, but it can only subtract the
edits it can find in a log it knows how to read. **Anything it can't trace, it treats as the item's own.**
So each new kind of writer leaks straight onto main.

And because the item's session tested in that same shared folder, its tests passed using code that main
doesn't have. Main only learns that after the commit is already there.

## What happened today

#192 was the item that built the subtraction (`desk_theirs.rs`, commit `7948f376`, yesterday 14:54).
Today it leaked four times in 76 minutes:

| Time | Desk commit | Item | What it took that wasn't the item's |
|---|---|---|---|
| 11:11 | `911bda3a` | #247 | Calls to `actionableRequest` in `ui/desk.js` |
| 11:45 | `04157d5f` | #274 | The same calls in `ui/desk.js`, plus `desk.spec.mjs` |
| 12:05 | `89466073` | #247 | The same edits again, 23 minutes after they were taken back |
| 12:27–28 | `25adf46c`, `3528755a` | #274 | The same 9-line removal of `deskPhraseIn`, committed twice |

The `actionableRequest` code came from a **Codex session** on another Desk. It patched `ui/core.js` and
`ui/desk.js` between 08:35 and 08:49 and didn't commit. I confirmed that in its log. `desk_theirs.rs`
only reads Claude logs, so to the Desk, nobody else had touched `ui/desk.js`. It committed the whole file,
but not `core.js`, which wasn't the item's. Main's window then called a name it didn't declare.

Eight repair commits followed (`20d425ce`, `87bb031d`, `32912edd`, `1d983f23`, `62208c4d`, `0e96bc3d`,
`cd0a0eef`, `10c8d20c`). One of them, `32912edd`, was built from a stale copy and took out three other
sessions' commits, so it needed its own repair.

## Why patching the guard won't end it

Yesterday's review asked for the guard to read Codex logs too. That would have stopped today's case, but
not the next one. The guard is a list of writers it knows. Every writer it doesn't know leaks the same way:

- **Codex** sessions (today's case).
- **Ended sessions**, whose logs are dropped.
- **A teammate of the holder's own lead.** The code says so itself (the SHORTCUT note in `theirs`).
- **Writes the logs don't show as edits**: `git checkout`, `git apply`, a Python script, a formatter,
  or you in an editor.
- **Same file, other session, timing**: once an edit is taken back, it's still in the folder, so the
  item's next finish takes it again (the 12:05 repeat).

Even a perfect guard leaves the second half of the problem. The item **depends on** others' unfinished
code (`#279`: `desk.js` was right, but needed `core.js` lines that only existed in the folder). Taking the
other session's lines out gives you a file that doesn't work. Leaving them in gives you a commit that
isn't the item's. Inside one shared folder there's no right answer.

Two guards added since yesterday help, but only after the damage is done. The window check (`#246`) and
the declared-names test (`#286`) run once the commit is **already on main**. They stop the install, but
every other session's next build, test and finish then starts from a broken main.

## The fix: each item works in its own copy, and lands only if the merged result passes

**What changes**

1. When the Desk starts a session for an item (`desk_start_new`), it first makes a worktree from main:
   `.claude/worktrees/desk-<id>` on a branch `desk/<id>`. The session starts there, not in the shared
   folder. An item handed to an existing session comes with that path, and the session is told to work
   there (Claude's `EnterWorktree`).
2. Finishing commits everything in that worktree. There is no subtraction and no "who else changed it":
   nobody else works there.
3. Landing is `merge_to_main`, which already exists for crew worktrees: a dry merge (`git merge-tree`),
   the word check, then the merge. Add one step **before main moves**: check out the merged tree in a
   temporary folder and run the window check and `cargo check` on it. If it fails, main is untouched and
   the item goes back to its holder with the failure.
4. Once the item lands, remove its worktree.

The shared folder stays for you and for sessions not on an item. `desk_theirs.rs` stays only for items
whose holder still works in the shared folder, until none do.

**What it removes.** Taking others' work (all five leak routes above), passing tests on code main lacks,
the "Try again" wait on someone else's file, the commit-wait circles, and the wrong blame. It also covers
most of yesterday's list (1, 2, 3, 4, 7 and 8).

**What it costs**

- **Code: medium, about a day.** Most of it exists: `merge_to_main`, the conflict and word checks, the
  on-main check, and `live_ui::check`. New code: making the worktree at start, the path in the hand-off,
  the check of the merged tree before the merge, and removing the worktree after. Plus Desk tests for a
  leak (another session edits the same file in the shared folder; the item's commit must not change).
- **Build time and disk.** Each worktree needs its own Rust build. A cold `cargo check` takes minutes and
  a `target/` folder takes GBs. Point all Desk worktrees at one shared `CARGO_TARGET_DIR` for checks.
  `.cargo/` already wraps test runs, so that's the place to add it. There are already 74 worktrees on
  disk, so add removal on landing in the same change.
- **Conflicts surface at landing instead of being hidden.** Two items in `ui/desk.js` now meet in
  `merge-tree`, which says "conflicts with main in ui/desk.js" and goes back to the holder. That's
  honest, and it's what should happen, but expect it on busy files at first.
- **The window shown before landing.** A session can't show you its change in the running app before it
  lands. Today it can't do that safely either (the shared folder isn't what gets installed). Screenshots
  from the worktree's own UI harness cover the "show me first" items.

## If you want a stop-gap first

Small, about two hours: flip the default in `own_part`. A changed file the holder's own log doesn't
account for line by line is **not** the item's, so the commit waits and names "someone else". Today's
four leaks would have waited instead of landing. It still leaves the dependency problem, and it turns
leaks into waits, so treat it as a bridge to the fix above, not a replacement.
