# Where the Desk's code lives

New Desk work goes in the file for its topic. Keep each file under about 500 lines; when one grows past that,
split it by topic the same way.

## Hub (src-tauri/src/hub)

| File | What lives there |
|---|---|
| `desk.rs` | `Effects` and `NewStart`, shared helpers (names, holders, routable sessions, asks), restoring the Desk, setting its lead, Cue's asks to the lead, your answers and picks, `desk_done` (carrying out a change's Effects) |
| `desk_view.rs` | How an item is shown: its holder as it is now, files, sources, held window; the done-items page; `state.desk` |
| `desk_route.rs` | Routing: route, note, give, start fresh, reopen, release what waits on an item, decisions |
| `desk_said.rs` | The lead's lines at a turn's end (ITEM, ROUTE, DONE, TITLE, SPLIT, CUT, MERGE, KEEP…), and `desk_turn_ended`, which reads every turn |
| `desk_heard.rs` | Other sessions' lines at a turn's end: any session's ITEM lines, designs, replies on an item, a holder's DONE and DECIDED; making a new item |
| `desk_act.rs` | What you do from the window: add, give, act on an item (looks right, reopen, send back, start now, put before, pick an option, drop), undo |
| `desk_lead.rs` | The lead: which session it is, how a message reaches it, what it's told before each message, new items to route |
| `desk_away.rs` | Away mode: the lead decides by your rules while you're away, and it ends by itself |
| `desk_members.rs` | A Desk's own list of sessions, and asking you before routing outside it |
| `desk_spaces.rs` | A Desk's identity and workspace |
| `desk_stall.rs` | A turn that ended on an item with no DONE: it waits on you, and the holder is asked once |
| `desk_unstick.rs` | The lead unsticking its Desk: CLOSE, TAKE COPY, LAND FIRST, START |
| `desk_waits.rs` | The list of who waits on whom, and circles of items waiting on each other |
| `desk_cleanup.rs` | A session's question about clearing the way for a landing, asked of the lead first |
| `desk_checks.rs` | The check card in the lead's chat for each DONE |
| `desk_finish.rs` | Done means it's in your app: commit, merge and install after DONE |
| `desk_trees.rs` | Each item in its own copy (a git worktree), landed as one squash |
| `desk_commit.rs` | Looks right with a commit: only the item's files, the word check, never pushed |
| `desk_theirs.rs` | Other sessions' edits in an item's files, kept out of its commit |
| `desk_diff.rs` | An item's Files changed |
| `desk_handoff.rs` | Handing a session grown too big to a fresh one |

Outside the hub, one line each:

- `src-tauri/src/desk/`: the Desk's records (`model.rs`), the lines Cue reads from the lead (`parse.rs`), and its words.
- `src-tauri/src/ipc/desk.rs`: the Desk's commands from the window.
- `src-tauri/src/db/desk.rs`: saving and loading Desk records.

## UI (ui/)

The Desk view is plain scripts loaded by `ui/index.html` and `ui/mini.html`, in this order. A top-level name is
declared in one file only (WebKit skips a whole script that declares a name twice; `cargo test --lib --
the_windows_scripts` catches it). A file may call functions from a later one, but top-level code that runs at load
uses only what is declared before it. The document listeners keep their old order: Away's (`desk_away.js`), then the
key time (`desk_view.js`), then the Desk's clicks and keys (`desk_act.js`).

| File | What lives there |
| --- | --- |
| `desk_lead.js` | Who runs a Desk (its lead), Give to…, the quick-add line, the lead's own lines in its chat (ITEM, ROUTE, DONE…), hand-offs, stop notes. |
| `desk.js` | The Desk's state (selected Desk, recent Desks and their kept rows, what's open, boxes, sent words) and item basics: lookups (`deskOf`, `deskItem`), kind icons and numbers, finish and install steps, what waits on you, seen/landed, made links, Files changed, stalled items. |
| `desk_list.js` | The Items list: holders (a lead and its crew, a session, an add-on), unblocks, the one-line ask (`deskVerb`), file chips and the small markdown (`deskMd`), a tool's allow line, groups by state, an item's row, finished items loaded as you scroll. |
| `desk_away.js` | Away (#361): its rules, the Away switch and card, what still needs you, paused sessions, the Away sheet and its save, and its own click, input and key listeners. |
| `desk_sessions.js` | The sessions on the left: session state, size and cache, rows, the lead's bar, the head strip and Delete, the Desk switcher, Add to / Remove from Desk. |
| `desk_page.js` | The chat card and its pages: Decisions and steps tables, the lead's chat, a session's chat, the reader, the one page pattern (`deskPage`) and its panel, and an item page's parts (done when, mocks and picks, checks, its ask and your answer, buttons, build log, decisions, what it waits on). |
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
| `desk.rs` | Shared helpers (`holder`, `new_lead`, `second`, `desk`, `item`, `ends`, `sent_to`, `summary`) and how the Desk's lines are read (ITEM, ROUTE, TITLE, SPLIT, MERGE, NOTE, the summary block). |
| `desk_items.rs` | Items: added by you or an ITEM: line, kept across a relaunch, said again, retitle, split, fold, DESIGNS: links, pictures, Give, undo, what you do to an item. |
| `desk_lead.rs` | The lead: starting or picking it, changing it, parking and resuming it, fast mode, its asks, messages sent while it starts or changes. |
| `desk_routing.rs` | ROUTE lines that are made: to a holder, to new or the lead, to nobody, to another holder, said again, reopened; Start now; an item's files. |
| `desk_routes.rs` | ROUTE lines Cue can't make, and the answer the lead gets (#248). |
| `desk_queue.rs` | Queued items: waits for the work before it, Put before, idle routes first. |
| `desk_holders.rs` | A holder at work: its DONE and what it made, its asks, its crew, replies and steps kept on the item. |
| `desk_stops.rs` | A turn that ends without DONE: asked once then the Desk lead, stalls, Restart, a stop-hook follow-up. |
| `desk_away.rs`, `desk_finish.rs`, `desk_land*.rs`, `desk_commit.rs`, `desk_waits.rs`, `desk_wait_list.rs`, `desk_pick.rs`, `desk_members.rs`, `desk_theirs.rs`, `desk_unstick.rs`, … | One topic each, named after the hub file they test. |
