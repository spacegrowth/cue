# Refactor plan: making Cue's code easy to understand and extend

Status: a proposal for you to approve. No code has moved yet.
Written 2026-11-20 from reading main at db0f9dee. Line counts are `wc -l`.

## The short version

Cue isn't one big tangle. Most files are under 1,000 lines, and the build already enforces limits:
`xtask check` fails on a function over 60 lines or a file over 1,000. The problems are a level up from
single files:

1. **The Desk is spread out flat.** Its code sits in 38 `hub/desk_*.rs` files (11,405 lines) in one folder
   next to 37 non-Desk hub files, plus `desk/`, `ipc/desk.rs`, `db/desk.rs`, `lands.rs` and `one_way.rs`.
   The UI side is the same: 16 `ui/desk_*.js` files sitting next to everything else.
2. **The message box exists in nine copies.** Sending, putting a draft back, Enter, Esc, focus and
   scroll-to-end are each written several times: once for the Board chat, again for the Desk, the Inbox,
   the mini panel and the sheets. That is why "Esc does the wrong thing on the Desk" and "the draft
   vanished from the item box" kept coming back as separate bugs.
3. **Some names say the wrong thing.** Two `held` concepts, three unrelated "lands" files, and a
   `shell_writes.rs` under `sessions/` that has nothing to do with sending.
4. **There is a little dead code and some stray files.** About 8 MB of screenshots sit in a folder named
   `undefined/`, plus test output that is tracked in git.

The plan is 16 small steps in four phases. Each step lands on its own and changes no behaviour, except
step 0b, which fixes a failing test. The order keeps steps away from files that Desk items have open
right now.

---

## 1. What's there today

| Area | Lines | Files |
|---|---|---|
| Rust app (`src-tauri/src`) | 93,871 | 398 |
| of which `hub/` | 22,833 | 75 |
| of which `hub/desk*.rs` | 11,405 | 38 |
| of which scenario tests (`testing/tests/`) | 17,483 | 85 |
| UI (`ui/`, JS + CSS + HTML) | 25,968 | 103 |
| of which `ui/desk*.js` | 4,954 | 16 |
| Tests outside Rust (`tests/`) | 21,265 | 154 |
| Swift helpers (`voice/`, `speak/`, `listen/`) | 3,133 | 21 |
| Scripts | 1,431 | 17 |

### Largest files

These are the files the size check flags, or that come close.

| File | Lines | What it mixes |
|---|---|---|
| `tests/ui/specs/desk.spec.mjs` | 4,571 | about 164 Desk tests in one file (tests aren't size-checked) |
| `ui/styles/desk.css` | 1,324 | over the 1,000-line limit; some selectors are declared twice in the file |
| `ui/inbox.js` | 1,029 | over the limit: rows, list, brief, box, side panel, navigation, pin/snooze, keys, clicks |
| `ui/board.js` | 1,020 | over the limit: actions, question card, recently answered, the Board, usage limits, column drag, sheets |
| `src-tauri/src/talk.rs` | 953 | voice conversation, sitting at the top level; its tests are in `voice_tests.rs` |
| `src-tauri/src/logs/mcp.rs` | 939 | |
| `src-tauri/src/hub/sessions.rs` | 927 | |
| `src-tauri/src/desk/model.rs` | 914 | about 15 saved Desk fields next to about 20 runtime-only scratch maps and test seams |
| `src-tauri/src/config.rs` | 907 | settings, with the login item and Mac facts in `config/` |
| `src-tauri/src/hub/team.rs` | 904 | |
| `src-tauri/src/hub/send.rs` | 900 | the send path, plus a second copy of it in `type_reply` |
| `ui/core.js` | 845 | state, the add-on contract, helpers, project colours and picker, markdown, bars, scroll |
| `src-tauri/voice/.../Pill.swift` | 833 | |
| `ui/insights.js`, `sessions.js`, `mini.js`, `team.js`, `sheets.js`, `desk_list.js` | 600–714 each | `mini.js` overrides `render`/`renderMain` and has its own draw loop |

### Functions over 60 lines

From `docs/dev/inventory.md`.

- **Rust:** `desk_done`, `acted_on`, `desk_away_set`, `finish_after`, `desk_summary`, `item_lines`,
  `route`, `lead_said`, `stalls_asked`, `desk_trees`, `land`, `desk_close_on_main`, `shown`.
  All 13 are in `hub/desk_*`.
- **JS:** `chatHtml`, `deskLineHtml`, `deskQPress`, `deskSessions`, `deskThread`.
- **Missed by the JS scan:** `DESK_CLICKS` (`desk_act.js:370`, 128 lines) and `addOn` (`core.js:10`,
  80 lines) are object literals, which the brace scan doesn't count.

---

## 2. Duplicated logic

### 2a. The message box (UI)

This is the biggest source of repeated bugs.

| Concern | Where it is written | How the copies differ |
|---|---|---|
| **Box markup** (textarea, mic, attach) | 9 places, each built by hand: `chat/compose.js:484` `box()`, `desk_view.js:53` `deskBox`, `desk_view.js:66` `deskItemFoot`, `desk_act.js:316` `deskBackHtml`, `desk_page.js:54` `deskGoBackHtml`, `inbox.js:399-402`, `mini.js:234`, `sheets.js:265`, `chat/send.js:165` `btwPanel` | different classes (`.composer`, `.dk-box`, `.ib-box`, `.m-dk-box`), different buttons |
| **Sending** | `chat/send.js:58` `deliver`, `chat/compose.js:96` `sendTo`. The Desk adds `desk_act.js` `deskSendBox`:17, `deskSendItem`:43, `deskSendLead`:78 and `deskSendBack`:115, with retries at :148, :200 and :213. Raw `send_to_session` calls are also at `board.js:539`, `sheets.js:287`, `agents.js:102`, `insights.js:576/585` and `app/clicks.js:30-35` | each copy builds its own images and `now` arguments. The "It's restarting / asking you something first" guard is copied 4 times (`compose.js:96`, `desk_act.js:27, 57, 187`). Stripping images down to name/mime/data is copied 4 times |
| **Putting a draft back** | `send.js:88` (failed send), `compose.js:101` `pullHeld`, `compose.js:525` `sendAtEdit`, `desk_act.js:157` `deskBackInBox`, `desk_act.js:246` `deskHeldEdit` (a copy of `pullHeld`) | the same "text + blank line + what's there" merge, written 5 times |
| **Enter to send** | `app/keys.js:178` `keyTyping` → `enterInBox`, `desk_act.js:555` (capture phase, so it runs first), `mini.js:557` `keyDesk` (copies the Desk's Enter and Esc) | three key paths for one key |
| **Esc** | the main chain is `KEY_STEPS` (`app/keys.js:225`). There are about 12 more listeners: capture-phase ones in `team.js:587`, `team_bar.js:193`, `team_page.js:399`, `desk_act.js:578`, `inbox.js:1020`, `extensions.js:244`, `desk_away.js:358`, `insights.js:638` and `app/bind.js:97`, and window-level ones in `file_view.js:124`, `diff_overlay.js:68` and `desk_home.js:228`. `mini.js:485` `escKey` re-implements part of `keyEscape` | which listener wins depends on registration order. The "is something open over this" guard is copied 3 times |
| **Focus** | `chat/patch.js:137` `focusComposer` (it only knows `.active-pane`), `desk_view.js:243` `deskFocusOpened`, `desk_act.js:318` `deskBackFocus`, `inbox.js:1029` `inboxRefocus` | putting the caret back after a redraw is written twice (`mini.js:273`, `app/render.js:317`) |
| **Scroll to end** | `chat/helpers.js:54/66` `drawAtChatEnd`/`pinChatEnd`, `patch.js:175` `scrollChat`, the Desk's own `deskChatEnd` flag (`desk_view.js:191, 252`, `desk_act.js:597`), `mini.js:430` `toLatest`, `team_page.js:231`, `inbox.js:683` | the "within 24px of the bottom" test is written about 10 times |
| **Box height** | one `grow()` (`patch.js:187`, max 180px), but CSS caps it separately: desk 160px, inbox 260px, mini 72px | `grow` keeps the chat at its end only for `.active-pane` and `.crew-page`, not the Desk's `.dk-chat` |
| **Chat rendering** | `chat/render.js:135` `chatHtml` is shared by the pane, history and the Desk. Separate renderers: `desk_thread.js:23` `deskThread`, `desk_thread.js:287` `deskItemChat`, `inbox.js:592/794`, `sheets.js:247`, `chat/parked.js:104`, `desk_lead.js:36`. `mini.js:286` `drawPane` copies `app/render.js:82` `renderMain` | |

**Coupling in both directions:**
- Shared chat code calls into the Desk: `compose.js:460` calls `deskSendBox`, `compose.js:229` calls
  `deskRecall`, `chat/reader.js:83` calls `deskSettle`, `chat/render.js` calls `deskHandHtml`, and
  `app/render.js` reaches `deskView`, `deskThreadLift` and `inboxKeepEnd`.
- About 40 `typeof X === "function"` guards hide these links.
- Three overrides:
  - `mini.js:424` replaces the global `render` and `renderMain`.
  - `mini.js:545` temporarily swaps `deskSelected` around a send.
  - `team_page.js:44` writes into the `addOn` registry.

### 2b. Sending (Rust)

The main path is `ipc/chat.rs` `send_to_session` → `Hub::send_yours` (`hub/team_out.rs:180`) →
`hub/send.rs:413` `send_to_session` → `send_saved` (takes the typing gate) → `send_in_turn` →
`send_as_seen`. That last function goes one of three ways:
- a hosted session gets the message over its connection;
- a busy session keeps it until its turn ends (`sessions/held.rs`);
- otherwise Cue types it into the terminal or sends it straight to Pi.

The duplicates:
- `hub/send.rs:289` `type_reply` (replying to a card) repeats that three-way branch, including the
  retried Enter.
- `ipc/hosted.rs:63` and `type_reply` call `send_to_hosted` directly and skip the typing gate.
- Slash commands typed by Cue each go to the terminal on their own: `send.rs:707`, `/rename` in
  `hub/sessions.rs:531/588`, `/exit` in `ipc/sessions.rs:728`, and `talk.rs:616`.
- About 30 internal callers use `send_to_session(…, &[], false)` to mean "tell this session X", with no
  name that says so.

### 2c. Swift helpers

- The `emit()` JSON-line writer is written 3 times (`listen/main.swift:12`, `cue-speak/main.swift:26`,
  `cue-voice/main.swift:9`).
- The microphone and speech setup is written twice (`listen/main.swift:22-46`,
  `cue-voice/Listen.swift:35-66`).
- `cue-voice listen` (`Dictate.swift`) does everything `cue-listen` does, so `cue-listen` is a subset of it.
- On the Rust side, `terminal/speak.rs` (47 lines) and `terminal/dictation.rs` (50) are near copies:
  start the helper, read its lines, send "done" at the end.
- `talk.rs:168` has a third copy of the "find the helper next to the app" lookup.

### 2d. Tests

- 27 UI spec files each define their own `pick`/`openDesk` helpers. None of the shared helper files
  exports them.

---

## 3. Unclear boundaries

| Today | What it really is | Problem |
|---|---|---|
| `hub/desk_*.rs` (38 files, flat) | 7 sub-areas (see below) | you can't tell where a Desk feature lives. `docs/dev/desk-files.md` lists 22 of the 38 and is out of date |
| `desk/model.rs` `Desk` (about 35 fields) | about 15 saved records plus about 20 runtime scratch maps (`nudged`, `left_said`, `sent_at`, `done_told`, `routes_held`, `pr_looked`, `finish_claims`, `circles`…) plus test seams (`installer`, `gh`, `net_cap_ms`) | adding a feature means adding a field to one big struct that every Desk file reaches into directly (`st.desk.` is used 49 times in `desk_act` alone) |
| `ipc/desk.rs` | holds starting, resuming and taking on a lead, and the wait-or-claim delivery logic | business logic sits in the layer that should only turn button presses into calls |
| `hub/` → `ipc` | the hub calls into ipc 26 times (`hand_off.rs`, `cache_tick.rs`, `desk_handoff.rs`: `desk_send_with`, `start_hosted`…) | the layering runs backwards |
| `sessions/held.rs` vs `hub/desk_held.rs` | the first is a message kept until a busy session's turn ends. The second is an "Ask me before" step held during Away | one word for two different things |
| `lands.rs` / `hub/desk_lands.rs` / `hub/land_keep.rs` | the per-project landing setting / the lead's LANDS: line / the fast-forward that keeps other sessions' edits | three unrelated things with the same name |
| `sessions/shell_writes.rs` | works out which files a shell command writes (used by crews and by "theirs") | the name and folder suggest sending |
| `model.rs` vs `desk/model.rs` | the first holds wire and card types (`Item`, `Exchange`, `Origin`) | two `model`s |
| `hosted/old_hosts.rs` | hosts started by older builds; still in use | the name reads as dead code |
| `machines/logs.rs` vs `logs/` | the first copies remote logs to this Mac | it is a mirror, not a log reader |
| `config/login.rs`, `config/mac.rs` | the login item and Mac facts | these aren't settings |
| `hub/team*.rs` (4 files, 2,195 lines) | crew actions | flat, the same way the Desk files are |
| Top-level `talk.rs`, `hook.rs`, `notify.rs`+`sounds.rs`, `self_install.rs`+`live_ui.rs`, `one_way.rs`, `commands.rs` | voice, the `cue hook` command, notifications, install, a Desk setting, the slash-command list | no folder says what they belong to |
| Rust tests | four layouts: inline `mod tests` (107 files), sibling `*_tests.rs` (17), per-module `tests/` folders (3), `testing/tests/` scenarios (85) | no rule for which to use. Desk logic is tested in both places |
| UI | everything lives in `ui/` with no folders except `app/` and `chat/`. Load order is fixed by 76 `<script>` tags in `index.html` and 70 in `mini.html` | `desk_lead.js` must load before `desk_act.js` (it is spread at load time). `mini.js` must load after `app/clicks.js` |

---

## 4. Dead code and stray files

I checked each of these with a grep across `ui/`, `tests/` and `src-tauri/src`.

- **JS with no callers:** `deskTakeBack` (`desk_act.js:138`), `changesTip` (`chat/render.js:313`),
  `shortModel` (`core.js:110`), `starredIds` (`board.js:206`).
- **CSS classes nothing produces:**
  - `desk.css`: `dk-aw-decs`, `dk-ilinks`, `dk-none`, `dk-say-h`, `dk-say-to`, `dk-shtop`, `dk-wo-done`,
    `dk-wo-f`, `dk-words`
  - `insights.css`: `ins-check`, `ins-why`, `ins-head`, `ins-note`, `ins-tag`, `linkish`
  - `inbox.css`: `ib-opt`, `ib-opts`, `ib-qa`, `ib-rec`, `ib-od`
  - others: `legend`, `mach-line`, `cp-proj`, `cp-to`, `set-line`, `pm-none`, `sr-full`
  - Not dead, although a scan flags them: `dk-done`, `dk-stop`, `h-pi`, `s-quiet`. They are built from
    variables.
- **Rust that only looks dead:** `terminal/shells.rs::open_plain` and `ipc/sessions.rs::start_session`
  have no caller in the public code. The optional add-on may use them, so check against an add-on build
  before removing them.
- **Duplicate CSS rules:**
  - inside `desk.css`: `.dk-sfoot` (:71, :1309), `.dk-st`, `.dk-ptabs`, `.dk-next` (:198, :630), `.dk-pnr`
  - across files: `.field`, `.active-pane`, `.ap-chat`, `.sec`/`.sec-drawer`, `.nf-where`
- **Stray tracked files:**
  - `undefined/`: 42 screenshots, about 8 MB. A screenshot spec ran without its output folder set, and
    they came in with commit 8089ad86.
  - `test-results/`: Playwright output, and missing from `.gitignore`.
  - `work/`: three old scratch files.
- **A test that fails today:** `tests/ui/commands.test.js`. These Rust commands have no UI fake:
  `desk_away_set`, `desk_away_sweep`, `host_restart`, `host_status`, `quit_check`, `quit_now`, `send_at`,
  `send_at_drop`, `send_at_now`.
- **An unwired test:** `tests/soak_interrupt.test.sh` isn't run by `tests/run.sh`.

---

## 5. Target layout

Moves only. No file is renamed unless its name is wrong today.

### Rust (`src-tauri/src`)

```
desk/                      everything Desk, one folder
  mod.rs  model.rs  parse.rs           (as today)
  state.rs                             runtime-only scratch maps split out of model.rs
  settings/  lands.rs  one_way.rs      (from the top level)
  hub/                                 the 38 hub/desk_*.rs, grouped:
    core/     desk, view, waits, members, steps, spaces              ~1,640
    turn/     act, route, said, heard, asked, features                ~2,230
    lead/     lead, handoff, cleanup, checks, lands                     ~960
    away/     away, away_log, pause, held → away_holds                ~1,030
    stall/    stall, unstick                                            ~850
    landing/  finish, commit, trees, copies, install, pr, tree_files,
              main_line, unlanded, own_work, diff, git, land_keep     ~3,880
    theirs/   theirs, theirs_log, theirs_codex                          ~960
  ipc.rs  db.rs                        (from ipc/desk.rs, db/desk.rs: they stay thin)
hub/
  team/      team, team_out, team_review, team_tidy
  send/      send.rs split: path.rs (one send path), reply.rs, slash.rs (Cue's own /commands)
  …          the rest as today
voice/       talk.rs, terminal/speak.rs, terminal/dictation.rs, one helper.rs (start, read, done)
hook/        hook.rs
notify/      notify.rs, sounds.rs
install/     self_install.rs, live_ui.rs
wire.rs      today's model.rs
sessions/    held.rs → kept_message.rs; shell_writes.rs → crews/ or logs/
```

The Desk files stay `impl Hub` blocks, as now. Moving them into `desk/hub/` changes only their `mod`
paths, not how they reach the store.

### UI (`ui/`)

```
ui/
  core/      core.js split: state.js, addon.js, helpers.js, md.js, colors.js
  box/       ONE message box: markup, send, put-back, Enter, Esc, focus, grow, stick-to-end
  chat/      as today, minus the Desk reach-ins
  desk/      the 16 desk_*.js + inbox_desk.js
  board/     board.js split: board, question card, column drag, usage limits
  inbox/     inbox.js split: list, box, side, keys
  …
  styles/    desk.css split by area: desk-list, desk-page, desk-lead, desk-landing
```

Every box (Board, Desk lead, Desk item, send-back, Inbox, mini, sheets, btw) uses `box/`. Each one
passes a small description of itself: where it sends, what Esc does, what "back in the box" means.

### Swift

- `cue-listen` is folded into `cue-voice listen`, which already does the same job.
- One shared `emit()`.
- The FluidAudio version is pinned in one place.

---

## 6. The steps, in order

Every step:
- changes no behaviour (except 0b, which fixes a test);
- leaves `tests/run.sh` (unit + ui) green;
- regenerates `docs/dev/inventory.md` with `cargo run -p xtask -- inventory` whenever Rust moves.
  `cargo test` fails otherwise. It also updates the file paths in `docs/dev/lint-allow.toml` when a file
  named there moves.

Rust moves are checked with `cargo build --features ext` as well as a plain build, so the add-on's
`use crate::…` paths still resolve.

**In flight while this was written (avoid these files until they land):**
- `desk-1`: transcript and thread
- `desk-3`: `ui/app/keys.js`, `ui/chat/compose.js`, `ui/chat/patch.js`, `ui/desk_act.js`
- `desk-4`: `hub/desk_finish`, `desk_held`, `desk_install`, `desk_main_line`, `desk_theirs`,
  `desk_trees`, `desk_view`, `desk/model.rs`, `config.rs`, `hook.rs`

The phases are ordered so the first ones touch none of these.

### Phase 0: clean up (no code paths touched, can land today)

| # | Step | Files | Risk | Tested by | Must not break |
|---|---|---|---|---|---|
| 0a | Remove `undefined/`, the tracked `test-results/` files and the old `work/` scratch files. Add `test-results/` to `.gitignore` | stray files only | none | `git ls-files` | `site/`, `pi/`, `hooks/` (all in use) |
| 0b | Add the 9 missing UI fakes so `commands.test.js` passes | `tests/ui/fakes/*.js` | low | `node tests/ui/commands.test.js` | existing specs |
| 0c | Remove the 4 dead JS functions and the dead CSS classes, and merge the duplicate rules inside `desk.css` | `core.js`, `board.js`, `chat/render.js`, `desk_act.js` (only the one function), `ui/styles/*.css` | low. `desk_act.js` is touched by desk-3, so do this after desk-3 lands or leave `deskTakeBack` for later | the full Playwright suite + baseline screenshots | anything built from a class name (`dk-${state}`) |
| 0d | Bring `docs/dev/desk-files.md` up to date (16 files are missing) and add a "where things live" section | docs | none | read it | |
| 0e | Put a shared `pick`/`openDesk` in `tests/ui/specs/cue.mjs` and use it in the 27 specs | tests only | low | Playwright | |
| 0f | Wire `soak_interrupt.test.sh` into `run.sh`, or delete it (your call) | `tests/run.sh` | low | `run.sh unit` | |

### Phase 1: Rust moves (names and folders only)

| # | Step | Files | Risk | Tested by | Must not break |
|---|---|---|---|---|---|
| 1a | Move the top-level strays into folders: `voice/` (talk + its tests), `hook/`, `notify/`, `install/`. Move each `*_tests.rs` next to its module | `lib.rs`, `main.rs` (`cue hook`), those files | low: a pure move, so the compiler finds every path | `cargo test --workspace`, an add-on build, `tests/hook_*` | the `cue hook` command, the voice helper lookup, the add-on's `use` paths |
| 1b | Renames: `model.rs` → `wire.rs`, `sessions/held.rs` → `kept_message.rs`, `desk_held.rs` → `away_holds.rs`, `old_hosts.rs` → `older_builds.rs`, `machines/logs.rs` → `log_mirror.rs`, `shell_writes.rs` → `logs/shell_writes.rs` | about 60 `use` lines | low | the same | database and config keys: only Rust names change, never stored strings |
| 1c | Group `hub/team*.rs` into `hub/team/` | 4 files + `hub/mod.rs` | low | `testing/tests/team*.rs` | |
| 1d | **After desk-4 lands:** move `hub/desk_*.rs` into `desk/hub/<area>/`, plus `lands.rs`, `one_way.rs` and `land_keep.rs` | 41 files, `hub/mod.rs`, `desk/mod.rs`, `lint-allow.toml` (many `lock_order` entries name these files) | medium: it touches every open Desk branch, so do it in one quiet window, announced to the Desk lead first | all `testing/tests/desk_*` + Playwright Desk specs | lock order (`xtask check`), the debug double-lock panic |

### Phase 2: one send path in Rust

| # | Step | Files | Risk | Tested by | Must not break |
|---|---|---|---|---|---|
| 2a | Make `type_reply` use the same three-way branch as `send_as_seen` (one function both call) | `hub/send.rs` | medium: this is the typing path | `testing/tests/typing_edges.rs`, `send*`, hosted tests, the protocol suite | the retried Enter, typing a reply to a card, a hosted reply |
| 2b | Route `ipc/hosted.rs:63` through the typing gate | `ipc/hosted.rs`, `hub/hosted.rs` | medium | hosted tests | a reply typed while a send is running must wait, not interleave |
| 2c | One `type_command(sid, "/x")` for Cue's own slash commands (rename, exit, voice, the one in send.rs) | `send.rs`, `hub/sessions.rs`, `ipc/sessions.rs`, `voice/talk.rs` | low | rename and restart tests | |
| 2d | Give internal "tell this session X" calls a name (`tell(sid, text)`) instead of `send_to_session(…, &[], false)` | about 30 callers | low: mechanical | cargo test | |
| 2e | Move starting, resuming and taking on a lead from `ipc/desk.rs` into `desk/hub/lead/`, and make the hub stop calling into `ipc` (26 calls) | `ipc/desk.rs`, `hand_off.rs`, `cache_tick.rs`, `desk_handoff.rs` | medium | `desk_lead`, `desk_handoff` and hand-off tests | who runs a Desk, resume after quit |
| 2f | Split runtime scratch maps out of `Desk` into `desk/state.rs` (not saved) | `desk/model.rs` + the readers | medium: touches most Desk files, so do it after 1d | Desk tests, plus a restart scenario | the saved file format: only fields that are already not saved move |

### Phase 3: one message box in the UI

This is the phase that removes the repeated "Desk box vs Board box" bugs. Each step moves one box onto
the shared code and leaves the others alone.

| # | Step | Files | Risk | Tested by | Must not break |
|---|---|---|---|---|---|
| 3a | **After desk-3 lands:** create `ui/box/`: one markup builder, one `send(target)` covering the four guards, image stripping and put-back on failure, one `putBack(text)`, one `atEnd(el)` (the 24px test) and one `stickToEnd`. No callers yet | new files, `index.html`, `mini.html` | none | node unit tests for the pure parts | load order |
| 3b | Move the Board chat box onto it (`compose.js`, `send.js`, `patch.js` focus/grow) | chat/* | medium | `typing-edges`, `draft-undo`, `board` specs + baselines | drafts kept per session, undo, "Send now", Send at |
| 3c | Desk lead box + item box + send-back (`desk_act.js`, `desk_view.js`, `desk_page.js`) | desk_* | medium | `desk*.spec.mjs` | the item box draft, sending to the item vs the lead, holder retries |
| 3d | Inbox box, mini panel box, sheets, btw | inbox.js, mini.js, sheets.js, send.js | medium | the inbox and mini specs | mini's small height (72px) |
| 3e | One Esc: every view registers what Esc does in `KEY_STEPS`. Remove the capture-phase listeners one view at a time; `mini.js` uses the same chain | `app/keys.js` + the 12 listeners | medium-high: Esc order is subtle | a new `esc-order.spec.mjs` that writes down today's behaviour first (overlay → sheet → crew → Desk → board), then each removal | every Esc step that works today, in the same order |
| 3f | One Enter and one focus: `enterInBox` and `focusComposer` learn every box, then `desk_act.js:555`, `mini.js:557` and `deskFocusOpened`/`deskBackFocus`/`inboxRefocus` go | keys.js, desk_*, mini.js, inbox.js | medium | Enter/Shift-Enter specs, focus specs | IME composition (Enter while composing must not send) |
| 3g | Remove the shared code's calls into the Desk (`compose.js` → `deskSendBox`/`deskRecall`, `reader.js` → `deskSettle`). The Desk registers hooks on the box instead | chat/*, desk_* | low once 3c is in | the same | |
| 3h | `mini.js` stops overriding `render`/`renderMain`: `app/render.js` takes a "mini" mode | mini.js, app/render.js, mini.html | medium | mini specs | the mini panel's own size and the Desk tab inside it |

### Phase 4: split the big files (after the box work, so they shrink first)

| # | Step | Files | Risk | Tested by |
|---|---|---|---|---|
| 4a | `board.js` → `board/` (board, question card, column drag, usage). `inbox.js` → `inbox/`. `core.js` → `core/`. Brings all three under 1,000 lines | those + the html files | low: moves only | Playwright + baselines |
| 4b | `desk.css` → 4 files by area | styles | low | baselines |
| 4c | The 16 `desk_*.js` → `ui/desk/`. Make the `DESK_LEAD_CLICKS` load-order dependency explicit (one registry function instead of a spread at load time) | desk_*, html | low | Desk specs |
| 4d | `desk.spec.mjs` → 6 files at its existing helper seams (lines 2072, 2186, 2214, 2734, 4066): lead, list, item page, quick-add, landing/approvals, look | tests | low | the same tests pass, and the test count is unchanged |
| 4e | Split the long functions: the 13 Rust and 5 JS functions over 60 lines, plus `DESK_CLICKS` and `addOn`. One file per step | each | low | its area's tests |
| 4f | Swift: fold `cue-listen` into `cue-voice listen`, one `emit()`, one FluidAudio pin. On the Rust side, one helper runner shared by speak, dictation and talk | `listen/`, `voice/`, `speak/`, `build.rs`, `voice/*.rs` | medium: build and bundle changes | the Swift tests, a dictation test by hand (`docs/dev/manual-checks.md`), a release dry run | Intel builds (`intel.swift` stubs), the bundled binaries list in `tauri.conf.json` |

### What makes each step safe to land alone

- A step moves or merges code but never changes what you see. When a step needs a behaviour written
  down first (Esc order, Enter during composition), it adds that test before moving anything.
- Rust moves can't silently break: the compiler, the add-on build and `xtask check` catch every stale
  path and lock-order name.
- UI moves are checked by the Playwright suite and its baseline screenshots, which catch a missing
  script tag or a lost style.
- Steps are small enough that one Desk session can do each, and only 1d and 2f touch most Desk files.
  Those two are scheduled when no Desk landing item is open, and the Desk lead is told first.

---

## 7. Two decisions for you

1. **Order.** Phase 3 (one message box) gives you the most: it ends the repeated Desk-vs-Board box
   bugs. Phase 1d (moving the Desk folder) touches the most files. My suggestion: 0 → 1a–1c → 3 → 1d → 2
   → 4.
2. **Reliability first.** The Desk is in "reliability first" mode. Phases 0 and 3 directly reduce bugs;
   phases 1, 2 and 4 are tidying. They could wait until the Desk is steady.
