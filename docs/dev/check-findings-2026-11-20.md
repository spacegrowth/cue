# Code check findings, 2026-11-20

`cargo run -p xtask -- check` on main failed with 21 findings: lock-order cycles and files, the
database or other processes reached while the hub's store was locked. Each is fixed at its cause;
none is allow-listed. The check now passes.

## A closure kept for later was read as run in line (the checker)

Findings: the typing gate waiting on itself, and typing gate → FOUND, ended sessions, typing gates.

A hosted session's report callback (`hub/hosted.rs` `hosted_report`) is boxed and called later on the
session's reader thread. The checker treated `Box::new(move |…| …)` like any outside call and walked
the closure in line, under the typing gate held where the session was woken. So every path from a
reply's handling back into a send looked like it ran under the gate that started it.

Fix: `Box::new`, `Arc::new` and `Rc::new` now keep what they're handed (`walk/classify.rs` `keeps`).
A closure given to them is a `Frame::Kept`, read on its own with no locks held. Four `[[lock_order]]`
entries that only this false path produced no longer happen and were removed: typing gate →
approval trace, desk saves, live_ui.served, logs read for their edits.

## Files read under the store's lock

**When a holder's log was last written** (`written_ms` from `desk/hub/core/view.rs`
`wrote_just_now`): the Desk view statted each open item's transcript while building under the store.
Now the hub looks at those times with the store let go (`hub/activity.rs` `look_at_holder_logs`):
it reads the list under the lock, stats the files off it, and saves the times in
`Store::holder_logs`. It looks as any session's turn ends (`desk_turn_ended`, where a holder may
have just stopped), before an item is handed back (`Hub::item_shown`), and once a second in
`refresh_activity`, which asks for a redraw when the set of holders that wrote lately changes. The
view reads only that map, and building the window's state reads no file.

**Whether a folder is Cue's own source** (`builds_this_cue` from `desk/lands.rs` `place_of`, and the
`lands cue folders` cache lock under the store and the typing gate): that question is now part of
the folder lookup in `logs/repos.rs` (`Facts::cue`), which already runs on its own thread.
`place_of` reads only memory. Code off the lock (`lands_of`, `check_of`) first waits for the
folder's lookup once (`first_look`). Code under the store uses the new `lands_known`, memory only.
`desk/hub/landing/trees.rs` (`desk_trees_wanted`, `desk_trees_free`) now works out how each project
lands before taking the store, not inside it.

## The database written under the store's lock

`desk_delete_selected` (`desk/hub/core/spaces.rs`) deleted a Desk's rows inside `self.write`. It now
checks and picks the sessions under the lock, deletes the rows off it, then removes the Desk from
memory. The Desk's saves lock is held throughout, so no other save lands in between.

## Lock naming

- `desk/hub/landing/tree_files.rs`: `l.try_lock()` is a file lock on cargo's lock file, not a mutex.
  It is now written `std::fs::File::try_lock(l)`, which says so.
- "logs read for their edits" was taken in both `theirs.rs` and `theirs_log.rs`. The lock now has
  one owner, `theirs_log::so_far_kept`, which both call.
