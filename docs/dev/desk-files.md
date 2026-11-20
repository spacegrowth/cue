# Where the Desk's code lives

New Desk work goes in the file for its topic. Keep each file under about 500 lines; when one grows past that,
split it by topic the same way.

## Where things live

Start here to find a Desk feature, then read its file's row below. Hub files are in `src-tauri/src/desk/hub/<area>`, UI files
in `ui/`.

### Items and their states

- Records and the lead's lines: `desk/model.rs`, `desk/parse.rs`; saved by `db/desk.rs`.
- Shown to the window: `core/view.rs`. Steps for Progress: `core/steps.rs`.
- What you do to an item: `turn/act.rs`; a question on a done item: `turn/asked.rs`.
- Features and their pieces: `turn/features.rs`. Who waits on whom: `core/waits.rs`.
- A turn with no DONE: `stall/stall.rs`. A holder silent mid-turn: `stall/stuck.rs`.

### Routing

- ROUTE, give, note, reopen, start fresh: `turn/route.rs`.
- Lines read at a turn's end: the lead's in `turn/said.rs`, everyone else's in `turn/heard.rs`.
- Who is on a Desk, and asking before routing outside it: `core/members.rs`.
- A Desk's identity and workspace: `core/spaces.rs`.

### The lead

- Which session, what it's told, how messages reach it: `lead/run.rs`, `core/desk.rs`.
- Picking, starting and taking on a lead: `ipc/desk.rs` (for now, plan section 3).
- The lead unsticking things: `stall/unstick.rs`. Stops while landing go to it: `lead/stop_lead.rs`.
- Questions about clearing the way: `lead/cleanup.rs`. A session grown too big: `lead/handoff.rs`.
- Pausing the whole Desk: `away/pause.rs`.

### Landing and copies

- After DONE: `landing/finish.rs` (commit, merge, install), with `landing/main_line.rs` (the main branch and merges)
  and `landing/install.rs` (the installer).
- The item's commit: `landing/commit.rs`, with `landing/git.rs` (git and the word check).
- Other sessions' edits kept out: `theirs/theirs.rs`, read from logs by `theirs/theirs_log.rs` and
  `theirs/theirs_codex.rs`.
- Each item's copy: `landing/trees.rs`, on disk `landing/tree_files.rs`; kept inside it: `landing/copy_guard.rs`;
  cleared away: `landing/copies.rs`.
- Work left off main: `landing/unlanded.rs` (the item's branch), `landing/own_work.rs` (folders it made itself).
- Pull requests and pushes: `landing/pr.rs`. Files changed: `landing/diff.rs`. Check cards: `lead/checks.rs`.
- How work lands, per project: `desk/lands.rs` (setting), `lead/lands.rs` (the lead asking to change it).

### Away and Ask me before

- Away itself: `away/away.rs`; its past stretches: `away/away_log.rs`.
- Ask me before steps held while away: `away/away_holds.rs`.
- UI: `desk_away.js` (the switch, its sheet, held steps with Allow and No, the Away log). A session's other
  pending decisions: `desk_approvals.js`.

### Messages and delivery

- Where a sent message stands: `sessions/delivery.rs`, `sessions/kept_message.rs`; UI `desk_delivery.js`.
- Sending to the lead and the wait-or-claim logic: `desk/hub/lead/send.rs` (`desk_send_with`, `deliver`); its command `desk_send` stays in `ipc/desk.rs`. Starting, resuming and taking on a lead: `desk/hub/lead/mod.rs`.
- What Cue itself sends, drawn as one line: `desk_cue_said.js`.
- The Desk's asks in the Inbox: `inbox_desk.js`.

### UI pages

- State and item basics: `desk.js`. The Items list: `desk_list.js`. Progress: `desk_progress.js`.
- Sessions on the left: `desk_sessions.js`. Desk home: `desk_home.js`.
- An item's page and thread: `desk_page.js`, `desk_thread.js`, `desk_view.js`, `desk_files.js`.
- Acting, clicks and keys: `desk_act.js`. Commit lines and check cards: `desk_commit.js`.
- The lead's chat and its lines: `desk_lead.js`.

### Naming traps

- `desk/hub/away/away_holds.rs` is an Ask me before step held during Away. A message kept until a busy session's turn ends is
  `sessions/kept_message.rs` (it was `sessions/held.rs`).
- Three "lands": `desk/lands.rs` is the per-project landing setting, `desk/hub/lead/lands.rs` the lead's LANDS: line asking to
  change it, `desk/hub/landing/land_keep.rs` the fast-forward of main that keeps other sessions' edits.
- `desk/model.rs` holds the Desk's records; `desk/state.rs` (`DeskRun`, reached as `desk.run`) holds what it keeps only while
  Cue runs and never saves (who it nudged, what it sent, claims, stalls told); top-level `model.rs` holds wire and card types.
- `landing/trees.rs` and `landing/tree_files.rs` are both about an item's copy: the first decides, the second touches disk.
  `landing/copies.rs` clears copies away after landing.

## Hub (src-tauri/src/desk/hub)

| File | What lives there |
|---|---|
| `core/desk.rs` | `Effects` and `NewStart`, shared helpers (names, holders, routable sessions, asks), restoring the Desk, setting its lead, Cue's asks to the lead, your answers and picks, `desk_done` (carrying out a change's Effects) |
| `core/view.rs` | How an item is shown: its holder as it is now, files, sources, held window; the done-items page; `state.desk` |
| `turn/route.rs` | Routing: route, note, give, start fresh, reopen, release what waits on an item, decisions |
| `turn/said.rs` | The lead's lines at a turn's end (ITEM, ROUTE, DONE, TITLE, SPLIT, CUT, MERGE, KEEP…), and `desk_turn_ended`, which reads every turn |
| `turn/heard.rs` | Other sessions' lines at a turn's end: any session's ITEM lines, designs, replies on an item, a holder's DONE and DECIDED; making a new item |
| `turn/act.rs` | What you do from the window: add, give, act on an item (looks right, reopen, send back, start now, put before, pick an option, drop), undo |
| `lead/run.rs` | The lead: which session it is, how a message reaches it, what it's told before each message, new items to route |
| `away/away.rs` | Away mode: the lead decides by your rules while you're away and asks you nothing; it ends by itself |
| `away/away_log.rs` | What happened in each past Away stretch (items moved, the lead's calls, held steps), for the Away tab |
| `away/away_holds.rs` | Ask me before steps held while you're away: noted, the session carries on, sent once you Allow or say No |
| `core/members.rs` | A Desk's own list of sessions, and asking you before routing outside it |
| `core/spaces.rs` | A Desk's identity and workspace |
| `stall/stall.rs` | A turn that ended on an item with no DONE: it waits on you, and the holder is asked once |
| `stall/stuck.rs` | A holder stuck mid-turn (silent, or on an unanswered prompt): its lead is told once per spell |
| `stall/unstick.rs` | The lead unsticking its Desk: CLOSE, TAKE COPY, LAND FIRST, START, and PAUSE / RESUME of one item |
| `away/pause.rs` | Pausing a whole Desk: every session stops at a safe point, queued items wait, Resume picks it all up |
| `lead/stop_lead.rs` | A landing that stops goes to the lead in one plain line, not to you; edits in the way named with who made them |
| `core/waits.rs` | The list of who waits on whom, and circles of items waiting on each other |
| `lead/cleanup.rs` | A session's question about clearing the way for a landing, asked of the lead first |
| `lead/checks.rs` | The check card in the lead's chat for each DONE |
| `landing/finish.rs` | Done means it's in your app: commit, merge and install after DONE |
| `landing/main_line.rs` | The main line for a finishing item: which branch is main, merging a worktree or branch into it, later commits that take work back out |
| `landing/install.rs` | A finishing item's install: the installer, the tick that starts the next, Install now, Run again |
| `landing/unlanded.rs` | An item is done only once every commit on its own branch (`desk/<item>`) is on main |
| `landing/own_work.rs` | Work its holder did outside its copy (its own worktrees and branches), found in its log and landed |
| `landing/pr.rs` | Landing by pull request (push the item's branch, open and watch the PR), and the push-to-main choices |
| `lead/lands.rs` | The lead's `LANDS:` line: a change to how work lands, asked of you; only your yes saves it |
| `landing/trees.rs` | Each item in its own copy (a git worktree), landed as one squash |
| `landing/tree_files.rs` | An item's copy on disk: made from its branch, linked to the shared build folder, checked, removed |
| `landing/copy_guard.rs` | Sessions stay in their own copy: the edit and git guard stops one that leaves it |
| `landing/copies.rs` | Copies cleared away once their work is in the app, and old copies found once a launch and offered to remove |
| `landing/commit.rs` | Looks right with a commit: only the item's files, the word check, never pushed |
| `landing/git.rs` | git for the Desk: run with the repository's filters off, git's own words when it refuses, the word check |
| `theirs/theirs.rs` | Other sessions' edits in an item's files, kept out of its commit |
| `theirs/theirs_log.rs` | Those edits read from Claude Code's and Pi's logs, each log read once and then only what it adds |
| `theirs/theirs_codex.rs` | Those edits read from a Codex rollout: its patches and what its shell commands write |
| `landing/diff.rs` | An item's Files changed |
| `core/steps.rs` | When each open item reached each of its six steps, for Progress |
| `turn/asked.rs` | Your question on an item that's to check or done: its holder answers without reopening it |
| `turn/features.rs` | Big features: the lead's FEATURE line puts items under one, and a feature nobody holds is done once all its pieces are |
| `lead/handoff.rs` | Handing a session grown too big to a fresh one |

Outside the hub, one line each:

- `src-tauri/src/desk/`: the Desk's records (`model.rs`), its unsaved runtime state (`state.rs`), the lines Cue reads from the lead (`parse.rs`), and its words
  (`mod.rs`, with `summary`).
- `src-tauri/src/ipc/desk.rs`: the Desk's commands from the window. It also starts, resumes and takes on a lead, and
  holds the wait-or-claim delivery to it (plan section 3 moves these out).
- `src-tauri/src/db/desk.rs`: saving and loading Desk records.

## UI (ui/)

The Desk view is plain scripts loaded by `ui/index.html` and `ui/mini.html`, in this order (`inbox_desk.js` loads
earlier, with the Inbox). A top-level name is
declared in one file only (WebKit skips a whole script that declares a name twice; `cargo test --lib --
the_windows_scripts` catches it). A file may call functions from a later one, but top-level code that runs at load
uses only what is declared before it. The document listeners keep their old order: Away's (`desk_away.js`), then the
key time (`desk_view.js`), then the Desk's clicks and keys (`desk_act.js`).

| File | What lives there |
| --- | --- |
| `inbox_desk.js` | The Inbox's part of the Desk: a lead's or a holder's ask, Open in Desk, Add to Desk on a session's ask. |
| `desk_lead.js` | Who runs a Desk (its lead), Give to…, the quick-add line, the lead's own lines in its chat (ITEM, ROUTE, DONE…), messages from another Desk as a band, hand-offs, stop notes. |
| `desk_cue_said.js` | What Cue itself sends a session or a lead, drawn as one short line with the full text folded under it. |
| `desk.js` | The Desk's state (selected Desk, recent Desks and their kept rows, what's open, boxes, sent words) and item basics: lookups (`deskOf`, `deskItem`), kind icons and numbers, finish and install steps, what waits on you, seen/landed, made links, Files changed, stalled items. |
| `desk_list.js` | The Items list: holders (a lead and its crew, a session, an add-on), unblocks, the one-line ask (`deskVerb`), file chips and the small markdown (`deskMd`), a tool's allow line, groups by state, an item's row, finished items loaded as you scroll. |
| `desk_progress.js` | Progress: the popover with one row per open item and its six steps. |
| `desk_away.js` | Away (#361): its rules, the Away switch and card, what still needs you, paused sessions, the Away sheet and its save, and its own click, input and key listeners. |
| `desk_sessions.js` | The sessions on the left: session state, size and cache, rows, the lead's bar, the head strip and Delete, the Desk switcher, Add to / Remove from Desk. |
| `desk_page.js` | The chat card and its pages: Decisions and steps tables, the lead's chat, a session's chat, the reader, What the lead moved (the Dispatcher Actions tab, loaded as you scroll), the one page pattern (`deskPage`) and its panel, and an item page's parts (done when, mocks and picks, checks, its ask and your answer, buttons, build log, decisions, what it waits on). |
| `desk_thread.js` | An item's Thread (`deskThread`, live turn, taken, turn links, the hook fold), its page (`deskItemPane`, meta, NOW), a holder's page, a crew member's page. |
| `desk_view.js` | The boxes that send (`deskBoxKey`, `deskBox`), an item's foot, the right card, the lead's NOW and ask, `deskView`, following the thread, focus and scroll after a draw (`deskSettle`). |
| `desk_act.js` | Acting: open (`deskShow`, `deskOpenItem`), find (`deskFindHere`), send (`deskSendBox`), send back, item actions (`deskAct`), undo, the click table (`DESK_CLICKS`), keys, and the document listeners. |
| `desk_files.js` | The Desk panel's files (#313): what an item, a session or a crew member changed, the first three with +/−. |
| `desk_delivery.js` | Where a sent-back message stands (held, queued, pending), as its session has it. |
| `desk_approvals.js` | Pending decisions of a session (and its crew), with or without an item. |
| `desk_home.js` | Desk home (All projects): every Desk as a row of one table, + New Desk. |
| `desk_commit.js` | Looks right's commit, the commit line and its Undo, a DONE's check line in the lead's chat. |

## Tests (src-tauri/src/testing/tests)

| File | What lives there |
| --- | --- |
| `core/desk.rs` | Shared helpers (`holder`, `new_lead`, `second`, `desk`, `item`, `ends`, `sent_to`, `summary`) and how the Desk's lines are read (ITEM, ROUTE, TITLE, SPLIT, MERGE, NOTE, the summary block). |
| `desk_items.rs` | Items: added by you or an ITEM: line, kept across a relaunch, said again, retitle, split, fold, DESIGNS: links, pictures, Give, undo, what you do to an item. |
| `lead/run.rs` | The lead: starting or picking it, changing it, parking and resuming it, fast mode, its asks, messages sent while it starts or changes. |
| `desk_routing.rs` | ROUTE lines that are made: to a holder, to new or the lead, to nobody, to another holder, said again, reopened; Start now; an item's files. |
| `desk_routes.rs` | ROUTE lines Cue can't make, and the answer the lead gets (#248). |
| `desk_queue.rs` | Queued items: waits for the work before it, Put before, idle routes first. |
| `desk_holders.rs` | A holder at work: its DONE and what it made, its asks, its crew, replies and steps kept on the item. |
| `desk_stops.rs` | A turn that ends without DONE: asked once then the Desk lead, stalls, Restart, a stop-hook follow-up. |
| `away/away.rs`, `landing/finish.rs`, `desk_land*.rs`, `landing/commit.rs`, `core/waits.rs`, `desk_wait_list.rs`, `desk_pick.rs`, `core/members.rs`, `theirs/theirs.rs`, `stall/unstick.rs`, … | One topic each, named after the hub file they test. |
