# Should Cue drop its terminal chat? (#843)

Written 2026-11-24. Nothing in the app changed for this; it is research only.

## The short answer

1. **Remove Cue's terminal chat: yes, but in two moves.** First make hosted the default for everyone
   and give the few things that still only start in the terminal a hosted way. Then, once a release
   has gone by with nobody needing the terminal, delete it. You already made this call on 7 Nov (step
   H6 of `docs/dev/cue-hosted-sessions.md`). Steps H1 to H5 are built. What's left is the gaps listed
   below and the deletion itself.
2. **Keep supporting sessions in Terminal and iTerm: yes.** Cue keeps watching them and typing into
   them, and Move to Cue should always bring them in hosted. The Desk works with them while they're
   open, but it can't bring one back once it ends. The fix is to move them in, not to improve the
   typing.

## Terms

- **Cue terminal chat**: Claude Code running in tmux. Cue shows the live terminal above the reply box,
  types your messages into it, and reads the screen and hooks to tell what happened.
- **Hosted chat**: Cue runs Claude Code (or Codex, or Pi) itself, with no terminal, under `cue host`.
  Every message, step and question arrives as data.
- **Terminal and iTerm sessions**: ones you start yourself in those apps. Cue sees them through hooks and
  types into them.

## How you actually use them

From Cue's session records (counted per week, by when each session started):

| Week of | Hosted | Cue terminal | Other (Terminal, iTerm, helpers) |
|---|---|---|---|
| 5 Oct to 26 Oct | 1 | 334 | 76 |
| 2 Nov | 13 | 26 | 44 |
| 9 Nov | 11 | 32 | 26 |
| 16 Nov | 350 | 2 | 29 |
| 23 Nov (so far) | 114 | 0 | 9 |

Since 16 Nov, when you wrote "ok moved to hosted" and the Desk started, you have effectively stopped
using the terminal chat: 464 hosted sessions against 2 terminal ones, and 3 Terminal.app sessions in two
weeks. Every Desk lead and every session the Desk starts is hosted, whatever the setting says.

**But for anyone else installing Cue, the terminal is still the default.** `hosted.on` is "off until you
turn it on" (`src-tauri/src/config.rs:216`), and "New sessions run in Cue" (tmux) is on by default
(`config.rs:221`).

## How often each has broken

These are counted by hand from the fixes in the git log. Features, refactors and release-note commits are
left out.

| Source | Cue terminal chat | Hosted chat |
|---|---|---|
| Commits touching it | 206 | 71 |
| Real breakage fixes | 33 (22 in Oct, 11 in 1–7 Nov, none since) | 22 (all in Nov) |
| Desk bugs (Desk began about 16 Nov) | 2 open, both small (#842, #852) | about 9 (#33, #258, #290, #291, #473, #573, #761, #773, #787) |
| Hand-off notes mentioning it | almost none | 15 |
| Your messages since 3 Nov | 3 mention it, no complaints | 19 mention it, about 5 complaints |

**Per day of real use they're about the same.** The terminal chat had about 1 fix a day from 6 Oct to 7 Nov.
Hosted has had about 1.3 a day from 7 Nov to 24 Nov. Hosted fixes per week went 4, 6, 7, 5, so they aren't
falling yet. The terminal going quiet means it stopped being used, not that it got better.

**What kept breaking in the terminal chat:**
- Messages lost or wrongly marked failed: paste left in the box, extra Enter, "didn't go through" when it
  did, typed into a dialog or a shell. Examples: ff15c41f93, 00f275f56b, 8a0dead021, b733169892, 7ddf5be12c.
- tmux versions and environment: 3.0a, 3.7, locale, an inherited `TMUX` (d1a9a0830e). On 6 Nov a tmux
  kill-server and a deleted socket took every terminal session down at once.
- Hangs and lockups while opening or writing to the terminal (7291aacbf4, aeda863cf2).
- Its permission prompts, sandbox questions and MCP forms often couldn't be answered from a card ("Answer
  in terminal").

That whole kind of failure can't happen in hosted: nothing is typed and nothing is read off a screen.

**What kept breaking in hosted:**
- Surviving restarts and installs: turns cut off, working sessions shown as stopped, Codex not picked back up
  (70304b0d1c, 887fe2c193, 2c38f9b2f7 #291, 94754b9363).
- Stopping and sending mid-turn: a send stopped a teammate's background work (#473), and it came back as
  #787 three days later. Escape loses the text you sent in hosted but gives it back in the terminal (#832,
  still open).
- Things only a terminal could do: `/login`, the canvas tools (1c1ba3e77f), Answer-in-terminal cards.
- The usage limit and installs (#33, #258, #290).

These are fixable bugs in Cue's own code, with tests around them. The terminal's failures came from
guessing what a screen meant, which is why they never fully stopped.

Your own complaints about hosted, from 8 Nov: "much slower than the TMUX way" (the wait for a message to
be picked up), "escape in hosted session is not bringing back edit", and sessions looking stopped after a
restart. On 16 Nov: hosted sessions not getting the canvas. The slowness, the restart one and the canvas
one were fixed. The Escape one is #832.

The terminal-as-chat spike (`spike/chat-html`, b4bd5eb, 11 Oct) was never merged, and no item refers to it.
It answered a different question: showing the terminal as the chat. Hosting replaced that.

## Where the Cue terminal chat is used or offered

Paths are under `src-tauri/src` unless they start with `ui/`.

**What decides terminal or hosted**
- `config.rs:211` `tmux_sessions()`: Settings `sessions.tmux`, which defaults to on. `config.rs:216`
  `hosted_on()`: defaults to off.
- `terminal/launch.rs:24-31` `Place::chosen`: Tmux, a tmux tab, or a plain terminal tab.
- `hub/starting.rs:44-81`, + New: hosted only when picked and `hosted.on` is on. Otherwise tmux (`:76`).
- **Always terminal, with no hosted path today:**
  - Fork (`ipc/sessions.rs:276`, and `:280-285` on a machine).
  - Resume older (`ipc/sessions.rs:38`).
  - Resume a closed or parked session that wasn't hosted (`hub/resuming.rs:49,64`).
  - Rebuilding tmux panes at launch (`ipc/restore.rs:29,103,127`, from `startup/watchers.rs:15`).
- Hand-off fresh keeps whatever the old session was (`hub/hand_off_note.rs:162`).
- Move to Cue: hosted if `hosted.on`, else tmux (`ipc/sessions.rs:441-498`, tmux at `:455-456`), falling back
  to a terminal tab (`:466-473`).
- A machine without hosted: tmux there (`hub/starting.rs:86-100`, `machines/mod.rs`).
- **Desk: always hosted.** The lead (`desk/hub/lead/mod.rs:157-164`) and new crew (`desk/hub/lead/run.rs:174-176`).
  Loops start a lead, so they're hosted too (`desk/hub/turn/said.rs:260`). A Desk can still take an existing
  tmux session as lead and type to it (`desk/hub/lead/mod.rs:98,131`).

**Where the UI offers it**
- `ui/chat/terminal.js` (264 lines):
  - the "Runs in Cue" icon (`:21-26`)
  - the place tab (`:33-45`)
  - **Move to Cue** (`:48-55`)
  - **Move to hosted** (`:59-63`)
  - the "Waiting in its terminal" strip (`:66-72`)
  - opening the terminal (`:74-92`)
  - It uses xterm from `ui/vendor/xterm` (`ui/index.html:7,37-38`).
- `ui/chat/pane.js:130`: the strip in the chat pane.
- `ui/app.js:5-17`: polls the screen every 2 s. `ui/app.js:126`: terminal events.
- `ui/sheets.js:15-45`: the Move to Cue dialog. `ui/sheets.js:299`: Settings "New sessions run in".
  `ui/sheets.js:300`: "Cue-hosted sessions (trial)".
- `ui/sessions.js:133-159,297,392,425`: + New's "Runs in" picker.
- `ui/chat/render.js:693-700`: ⋯ → Move to terminal. `ui/chat/render.js:776`: Move to Cue on quiet rows. This
  is how "Move to Cue terminal" shows up during a lead hand-off (#842).
- `ui/app/clicks.js:21-23` and `ui/app/clicks_chat.js:14-17,123`: the move, terminal and reader-terminal clicks.
- `ui/chat/reader.js:101`. `ui/chat/compose.js:127-128,331`: "Move to terminal to use it" for terminal-only
  `/` commands.
- `ui/board/board.js:251-254,414` and `ui/app/clicks_board.js:125-127,150`: Answer in terminal.
- `ui/shells.js` (385 lines): the floating shells. They use tmux too but aren't the chat. They can stay.

**What only the terminal path does**
- Typed delivery and the check that it landed (`hub/send.rs:49-218,502-530,688`).
- Esc to stop (`send.rs:375,617`).
- `/compact` and `/rename` typed in (`send.rs:769-806`, `hub/sessions.rs:524`).
- Permission cards answered with number keys (`hub/asks.rs:94-185`, `terminal/dialog.rs`).
- The trust-folder and press-Enter-to-start screens (`ipc/sessions.rs:346-392`, `hub/activity.rs:285-318`).
- The hand-off check that the input box is empty (`hub/hand_off.rs:25-33`).
- Raw screen and keys (`ipc/terminal.rs:43-61`).

Hosted has its own answer for each of these, built in H5: cards, the `interrupt` request, Cue's own rename,
and terminal-only commands opened in a terminal tab (`hub/terminal_only.rs`).

**Tests that depend on tmux**
- `terminal/fake_tmux.rs`, `terminal/test_tmux.rs`
- `testing/terminal.rs`, `testing/launcher.rs`
- `testing/tests/{typing,typing_edges,typing_failures,typing_support,launching,relaunch,terminal_screens}.rs`
- `terminal/focus/typing_tests.rs`, `terminal/term/edges.rs`, `terminal/input/edges.rs`
- The tmux guard: `terminal/tmux_guard.rs` and `.sh`, installed by `hook/mod.rs:75,634`.

**Size.** `terminal/` is about 5,200 lines of Rust. About 3,900 of them serve the terminal chat: term 1,292,
input 338, most of focus, dialog, keys, launch and the guard. Add `ipc/restore.rs` (241),
`ipc/move_out.rs` (105), `ipc/terminal.rs` (about 60), about 300 lines of `hub/send.rs`, about 400 lines of JS,
and about 2,000 lines of tests.

**Shared with Terminal and iTerm, so it stays:**
- `terminal/focus/apps.rs` and `terminal/focus/typing.rs`: typing into those apps by AppleScript.
- The `Keys` port.
- The landed check in `send.rs`.

## Terminal and iTerm sessions on the Desk today

| Can the Desk… | Today | Where |
|---|---|---|
| route an item to one | **Yes**, like any live session of the project, once it's on the Desk | `desk/hub/core/desk.rs:113-117,148`, `desk/hub/core/members.rs:30` |
| send it notes and item messages | **Yes**, by typing via AppleScript. Held until its turn ends; refused while it's showing its own prompt | `desk/hub/core/desk.rs:711-735`, `hub/send.rs:509-515,631`, `terminal/focus/typing.rs:39-95` |
| read its DONE | **Yes**. The Stop hook reads the transcript, so it doesn't matter where the session runs | `hook/mod.rs:259-269`, `hub/events.rs:27-31`, `desk/hub/turn/said.rs:749-777` |
| wake it when "cold" | **Yes, while it's open**. Cold only means the cache ran out; waking is just sending | `desk/hub/lead/run.rs:396-416` |
| bring it back once it has ended | **No.** It drops out of Cue's live list, and sends fail with "that session is gone". Resume only sends. Items waiting for a check go to a new hosted session instead | `hub/events.rs:136-141`, `hub/send.rs:524`, `desk/hub/turn/act.rs:478-496`, `desk/hub/lead/route.rs:267-306` |

**What breaks or is missing:**
1. An ended or closed Terminal or iTerm session can't be resumed by the Desk.
2. Terminal.app can't be sent Esc (`typing.rs:126`), so Pause, "send now" and stop-to-send fail there.
3. Terminal.app joins a multi-line message into one line (`typing.rs:57`). Desk notes and hand-offs arrive
   flattened.
4. A Terminal.app tab that moved isn't found again. iTerm falls back to the session's current terminal
   (`focus/apps.rs:238-256`).
5. If you're typing in that window when Cue sends, the two mix. The first paste isn't checked against the
   screen.
6. Typing needs macOS Automation permission and the app running. It times out otherwise
   (`exec.rs:114`).
7. On the Desk's session list, a closed Terminal or iTerm session just disappears. Only hosted ones show as
   closed (`ui/desk/sessions.js:238-241`).
8. Move to Cue is the only way to make one dependable, and the Desk can't do it. It also stops the original
   process (`hub/resuming.rs:89-107`) and, today, still moves to tmux unless `hosted.on` is on.

**Recommendation:**
- Keep watching and typing into Terminal and iTerm sessions. It costs little and it's how people arrive with
  sessions they already have.
- Don't invest in making Terminal.app typing better; points 2 to 5 are the same screen-guessing that hosted
  removed.
- Make Move to Cue always hosted.
- Let the Desk move a session in hosted when it routes to one, or when it needs to bring one back. It should
  ask you first, since it stops the original window. That fixes 1, 2, 3, 4 and 7 in one step.

## The removal plan

Sizes: S is under a day, M is one to three days, L is more than three. Each step can ship on its own.

| # | Step | Size | Risk |
|---|---|---|---|
| 1 | **Hosted becomes the default.** `hosted.on` defaults to on. + New's "Runs in" defaults to hosted. Move to Cue always goes hosted. This removes the "Move to Cue terminal" offer (#842 goes away). The tmux choice stays for now as a way back | S | New users meet hosted bugs (about 1.3 fixes a day lately) without a terminal to fall back to by default. Claude Code CLI changes to `stream-json` hit everyone; the compatibility suite pins this |
| 2 | **Hosted ways for what still only starts in tmux:** Fork, Resume older, Resume a closed non-hosted session, hand-off of a terminal session, + New on a machine | M | Fork needs `--fork-session` through the hosted launcher; a machine needs cue-host there (built in H4) |
| 3 | **Fix the open hosted gaps** before taking the terminal away: Escape gives back what you sent (#832); stopping never loses a message (#473 came back as #787) | M | These are what you'd reach for the terminal to avoid |
| 4 | **Desk: Move to Cue from the Desk** for a Terminal or iTerm session it routes to or needs back. It asks you first. Closed ones show as closed on the Desk's session list | M | It ends the original window. Hence the ask |
| 5 | **One release with the terminal still there but not offered.** Then check that nobody, you included, started one | wait | — |
| 6 | **Bring the last terminal sessions over.** At launch, live tmux sessions keep running until they end. Resuming one starts it hosted. Remove the launch rebuild (`ipc/restore.rs`) | S | A tmux session mid-turn at upgrade time: leave it alone until it ends |
| 7 | **Remove the UI.** `ui/chat/terminal.js`, `ui/vendor/xterm`, the strip and the 2 s screen poll, the "Runs in" picker, both Settings rows, Move to hosted, Answer in terminal | M | Small. The chat already renders everything from the transcript and events |
| 8 | **Remove the Rust.** `terminal/term`, `terminal/input`, `terminal/dialog.rs`, tmux places in `terminal/launch.rs`, the tmux guard and its hook install, tmux panes in `machines/listing.rs`, `ipc/move_out.rs`'s tmux path, `ipc/terminal.rs`. Keep `focus/apps.rs`, `focus/typing.rs` and `Keys` for Terminal and iTerm. Remove the add-on's screen and key calls too, so the add-on build still compiles | L | About 4,000 lines. Shared code is easy to over-cut, so build with the add-on before removing anything public. The floating shells (`ui/shells.js`) also use tmux: keep them, or decide separately |
| 9 | **Tests.** Drop the tmux-only typing tests (about 2,000 lines). Rework `FakeLauncher`'s tmux paths and the World cases that start terminal sessions. Keep the Terminal and iTerm typing tests | M | Coverage of the AppleScript typing must stay |

Move to terminal stays throughout. It's the way out: it resumes a hosted session in a terminal app, from the
same transcript.

## Caveats

- The commit counts were sorted by subject and by the files each commit touched. A few could go either way.
  Recent Desk send bugs (#525, #531, #587, #439) were in the Desk's own delivery and are counted in neither
  column.
- There's little session history before 3 Nov, so complaints from the terminal-only weeks are undercounted.
