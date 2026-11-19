# #294 split: paused state (2026-11-18)

Paused on request (owner's tokens low until 1:30; #361 Away mode goes first). Not done; nothing committed.

Rule from the Desk: land AFTER #361. Merge main in once #361 lands, then redo the split on top of it.

Three teammates made splits against main 388e0582 (each stopped mid-check; uncommitted, in worktrees):
- hub/desk.rs → desk_act.rs, desk_route.rs, desk_said.rs, desk_view.rs (+ hub/mod.rs, docs/dev/desk-files.md)
  .claude/worktrees/agent-a033b10a43d47620e — stopped at a failed build, not yet looked at.
- ui/desk.js → desk_act.js, desk_list.js, desk_page.js, desk_sessions.js, desk_thread.js, desk_view.js
  (+ index.html, mini.html, docs/dev/inventory.md, desk-files.md)
  .claude/worktrees/agent-ad1c588a7e394c243 — node checks done; desk UI specs not run yet.
- testing/tests/desk.rs → desk_holders.rs, desk_items.rs, desk_lead.rs, desk_queue.rs, plus tests added to desk_routes.rs
  .claude/worktrees/agent-ac96e10d902b8fde2 — test names checked against the old file (only 2 helpers made pub(super)); cargo test not run.

Next:
1. Wait for #361 on main.
2. Use these worktrees as the map (which function goes in which file) and redo each cut on the new main, so #361's edits are kept.
3. Merge the three desk-files.md sections into one.
4. Build, `cargo test --lib`, UI desk specs; land on main; check on a clean copy; install.sh.
Not in scope: desk_finish.rs (994) and desk_theirs.rs (928) are also over 500 lines.
