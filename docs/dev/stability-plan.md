# Cue stability plan

Status: PROPOSAL, nothing implemented. Written 2026-11-06 against main @ 5e5b9f8 (v0.1.86) plus the
uncommitted `hub/mod.rs` patch (settings/connections/usage_file read before the store lock).
Every number below was measured on that tree; "unverified" marks what wasn't.

## 0. What the tree looks like (measured)

| What | Number | How measured |
|---|---|---|
| Rust lines, `src-tauri/src` | 34,441 (46 files) | `wc -l` |
| UI lines, `ui/*.js,css,html` | 12,803 | `wc -l` |
| `#[tauri::command]` | 135: 53 sync, ~82 async | grep; the 53 sync ones are listed in §2.1 |
| `.st()` (hub store lock) call sites | 208 | grep |
| `locking::lock(` sites / `.lock()` sites | 186 / 135 | grep |
| Lock declarations (static + field) | ~62 across 25 files | grep `Mutex<`; full list §1.3 |
| Spawn sites (`thread::spawn`, `async_runtime::spawn`, `spawn_blocking`, `tokio::spawn`) | 150 | grep |
| `Command::new` (child processes) | 96 (23× `/bin/sh`, 11× tmux, 8× `/bin/ps`, 5× ssh, 5× git, 2× osascript, …) | grep |
| `std::fs::*` calls | 354 | grep |
| `crate::db::` calls | 143 | grep |
| Rust `#[test]` | 408 (402 pass, 6 ignored, **92 s**, `cargo test -q`) | ran it |
| Python protocol tests (`tests/test_protocol.py`) | 38, against a private running Cue | counted |
| Node tests (`tests/*.test.js`) | 4 files; `tests/run.sh` runs only `status.test.js` | read `run.sh` |
| `tests/preview.html` fake-bridge coverage | 69 of 135 commands have a fake | grep `cmd ===` |
| CI | none for tests (`.github/workflows/pages.yml` only) | ls |
| Functions > 60 lines (Rust, tests included) | 34 (28 outside tests); biggest: `lib.rs:180 run` 237, `hub/activity.rs:670 refresh_activity` 235, `hub/activity.rs:109 on_event` 227, `ipc/insights_tab.rs:256 summary` 188, `talk.rs:293 run_loop` 161, `logs/steps.rs:1257 observe` 150, `logs/steps.rs:1409 take` 109, `sessions.rs:216 mark` 106, `hub/mod.rs:282 build_snapshot` 99 | `cargo run -p xtask -- sizes` (from the syn parse); full list in `docs/dev/inventory.md` (Phase 1) |
| Functions > 60 lines (JS) | 10; `ui/app.js:233 bindMain` **683** (its click handler at `:257` alone 439), `ui/app.js:29 renderMain` 158, `ui/app.js:759` (anonymous) 156, `ui/board.js:712 boardView` 106, `ui/insights_tab.js:274 insightsTabView` 105, `ui/insights.js:536 insAct` 91, `ui/sessions.js:485 sessionsView` 90 | `xtask sizes` (brace-aware scan) |
| Files > 1,000 lines | `logs/steps.rs` 3,230; `leads.rs` 2,147; `sessions.rs` 1,769; `terminal/focus.rs` 1,464; `db.rs` 1,450; `logs/transcript.rs` 1,342; `hub/activity.rs` 1,241; `machines/hooks.rs` 1,019; `ui/chat.js` 2,665; `ui/style.css` 2,412; `ui/board.js` 1,081; `ui/core.js` 1,065; `ui/app.js` 1,026 | `xtask sizes` (Phase 1) |
| 30-day churn | 338 commits, Rust +32,745 / −8,502 | `git log --since` |
| Release symbols | `[profile.release] strip = true` (`Cargo.toml:33`); no dSYM kept; `scripts/release.sh:79` remaps paths | read |

Existing safety net worth keeping: `locking.rs` (debug 5 s try-lock panic + a source lint for lock-in-block-head
and two-locks-per-line), `tests/run.sh` (private Cue under `CUE_HOME`/`CUE_QUIET`), `tests/preview.html`
(the real UI on a fake `window.__TAURI__`), `tests/exec_attach.sh` (headless Chrome `--dump-dom` over
preview.html), `hooks/cue-ctl` (socket client), `live::set_claude` (`logs/live.rs:101`, the one real test seam
for an external fact).

## 1. Inventory (what exists, what to generate, how to keep it current)

### 1.1 Threads and where code runs (the three worlds)

1. **Main (AppKit) thread.** Every *sync* `#[tauri::command]` runs here (Tauri 2 runs sync commands on the
   main thread; async ones on its tokio runtime). 53 of them, 67 call sites into `hub.*` from sync commands.
   Hot sync ones: `ipc/state.rs:10 get_state` → `Hub::snapshot` (file reads of every transcript tail and
   git HEAD, `hub/mod.rs:255-273`, plus a `config::set` **file write** at `hub/mod.rs:345`), `ipc/state.rs:15
   respond`, `ipc/terminal.rs:122 term_write` → `term::write` (`terminal/term.rs:353`: holds `open_terms`
   across a **blocking PTY write**), `term_resize`, `ipc/chat.rs search/session_log/session_log_page`
   (transcript file reads), `ipc/settings.rs set_setting`, `ipc/sessions.rs closed_sessions` (db read).
   Also: `Hub::redraw`/`changed` call `snapshot()` from whichever thread changed state, and `notify_full`
   (`hub/mod.rs:415`) does `UNUserNotificationCenter` XPC from that thread.
2. **tokio workers** (`tauri::async_runtime`). `server.rs:72` spawns a task per socket connection; `handle`
   (`server.rs:143`) calls `hub.event/add_ask/finish/respond/subscribe/set_cost` **directly in the async
   task** (lines ~160-230), i.e. a std `Mutex` + file I/O + `snapshot()` inside a tokio worker, not under
   `spawn_blocking` (only `reply`, `send_to_session`, `interrupt`, `set_rates`, `agent_behind` are). Tickers in
   `lib.rs:294-361` (3 s leads/live refresh + `sweep` + `sync_live`; remote poll; usage file 3 s; updater 6 h)
   and `server::watch` (`lib.rs:328`), `talk::start`, `actions::start`, `archive::start`, `turns::start`.
3. **Plain threads.** PTY reader/exit threads (`term.rs:424-479`), one SSH watcher per machine
   (`machines/hooks.rs:225`), remote crew polls (`lib.rs:337`), `machines/logs.rs` copies, `talk.rs` voice
   loops, `tasks.rs`, `notify.rs:98/110` callbacks, `safe_git.rs:68`.
4. **Separate process:** `cue hook <event>` (`hook.rs`) — reads `config.json` via `config::load`
   (`hook.rs:38`, `statuslist.rs:64,175`) and talks to the socket; never touches `cue.db`.

### 1.2 Locks and the lock graph (today)

Coarse: `Hub.store: Mutex<Store>` (`hub/mod.rs:131`, 208 sites), `Hub.typing` + per-session typing gates
(`hub/mod.rs:135,156`; held for seconds by design, `send.rs:13 wait_turn` 45 s), `term::open_terms`
(`term.rs:68`), `db::DB` (`db.rs:17`, one connection, lazily opened, **no `busy_timeout`**), `leads::INDEX`
plus 11 other leads statics (`leads/`), `logs/steps.rs` FEEDS/HISTORY/WINDOWS/SEEN, `logs/live.rs`
CLAUDE/ABOUT/BRANCHES/REPOS/TOPS, `machines/mod.rs:358 caches`, `machines/hooks.rs` watchers/SEEN/WATCHERS,
`actions/mod.rs` STORE/ONE/BUSY/WATCH, `tasks.rs` TRACKS/TOTAL, `talk.rs` STATUS/RETELL/OWN, `notify::QUIET`,
`sounds::PLAYING`, `commands.rs` LISTS/READING, `statuslist::SEEN`, `model::DECLARED`, `extensions::ONE`,
`usage_file::LAST`, `turns::CATCHING_UP`, `archive::INDEX`, `shells::KEPT`, `ipc/restore.rs BUSY`,
`ipc/insights.rs CONTINUING`, `ipc/chat.rs FOUND`×2, `remote_poll` schedule (local Arc).

Known edges while holding `store`:
- `store → leads::INDEX` (`hub/mod.rs:289 reviews_view` → `hub/reviews.rs:143-148 is_reviewed/is_done/missed/needing_you`).
- `store → live::CLAUDE` (`snapshot_sources`, `hub/mod.rs:380 live::claude()`).
- `store → sessions::*` methods (`sessions.rs:216 mark`, 106 lines, pure).
- `store → db::DB` in `changed()` is correctly **dropped first** (`hub/mod.rs:387`), but 143 `db::` sites
  need the lint in §2.3 to prove none is under `store`.
- `open_terms → PTY write` (`term.rs:353`) and `open_terms → child.kill/wait` via `Open::end` is *outside* the
  lock (`term.rs:181-189`, good).
- Reverse edges (`INDEX → store`) none found; `leads::refresh` runs under `spawn_blocking` and the hub calls
  `redraw` after. **Unverified:** nothing in `leads/`/`steps.rs` reaches a `Hub` method while holding its own
  static (they don't have a `Hub` handle, which is the structural reason; keep it that way).

What `locking.rs`' lint does *not* catch (and is the best remaining explanation of the "waits on a mutex owned
by itself" report): a guard bound with `let st = self.st();` that is still alive when a *helper* is called that
locks again (`self.changed()`, `self.notify(..)` → `notify_full` → `self.st()` at `hub/mod.rs:426,443`, or any
`Self::x(&mut st)` helper that reaches `self.st()`). The lint only scans the same line and block heads. Release
builds have no 5 s panic, so that shape is a silent hang. `hub/activity.rs` is careful (`drop(st)` before every
`self.changed()` I read, e.g. `:118-120`, `:237-241`) but it's convention, not enforcement. Fix: §2.2 runtime
reentrancy guard + §2.3 scoped access that makes the shape impossible.

### 1.3 How to generate the inventory mechanically and keep it current

New dev-only crate `src-tauri/xtask/` (a `cargo xtask` binary, not shipped) using `syn` to walk `src/`:
- `xtask inventory` → `docs/dev/inventory.md` (checked in): every `#[tauri::command]` with sync/async, every
  `Mutex`/`RwLock`/`OnceLock` with its name, every spawn site, every `Command::new`/`fs::`/`db::`/`osascript`/
  `UNUserNotificationCenter`/`SMAppService`/`UnixStream`/`ssh` call site, and a lock graph built by walking
  call chains (callee reachable while a guard binding is live → edge). Output also as
  `docs/dev/lock-graph.dot`.
- `xtask check` runs the same analysis as assertions (replaces the hand-rolled line scanner in
  `locking.rs:50-104`): no sync command may be listed as sync unless in an allow-list; no I/O/process/XPC/other
  lock reachable under `store`; lock order `store → INDEX → CLAUDE → DB` only; no cycle. A `#[test]` in
  `locking.rs` shells out to `cargo run -p xtask -- check` so `cargo test` fails on drift, and `inventory.md`
  must match the regenerated one (`git diff --exit-code`), so the doc can't rot.
- Effort: 2–3 executor-days. Worth it: it is the only way "no I/O under the lock" survives 338 commits a month.
  The current awk/grep numbers above are the seed, not the tool.

## 2. Concurrency architecture

### 2.1 Rule: the main thread never takes an app lock

- Every sync command in `ipc/*.rs` and `extensions.rs`/`actions/*` becomes `async fn` whose body is
  `spawn_blocking(move || …).await` (the pattern already used at `ipc/terminal.rs:55-63 term_open`). 53 to
  convert. Window-only ones stay sync (`hide_window`, `set_fullscreen`, `is_fullscreen`, `dictate_*`,
  `speak_send`, `test_notification`) — they touch AppKit, which *must* be main-thread.
- `get_state` stops calling `snapshot()`: the hub keeps the **last snapshot** in an `arc_swap::ArcSwap<Value>`
  (or `RwLock<Arc<Value>>`) refreshed by `changed()`; `get_state` returns the Arc, never waits. `redraw`
  computes off-thread.
- `term::write` (`term.rs:353`): take the writer handle out under the lock (`Arc<Mutex<Box<dyn Write>>>` per
  terminal, or clone the pty writer via `try_clone_writer`), drop `open_terms`, then write with a bounded
  wait. Same for `resize`. A PTY write to a stalled client (tmux over a dead SSH) must fail in ≤ 2 s, not block.
- `server.rs:143 handle`: every `hub.*` call moves under `spawn_blocking` (today only 5 of ~14 are). A std
  mutex + disk reads in a tokio task stalls every other socket client on that worker.
- `notify_full` (`hub/mod.rs:415`): XPC call goes to a dedicated `notify` thread with a channel; hub never
  blocks on Notification Center.
- `snapshot` (`hub/mod.rs:250`) after the uncommitted patch still runs `live::about` (transcript tail reads),
  `live::repo_root/checkout_top/branch` (git/file reads), `leads::view`, `db::handoffs/ahead_watched`, and a
  `config::set` write — all on the calling thread. Fine off-main; wrong on main. §2.1 bullet 2 fixes the thread;
  §2.4 caches the slow facts.

### 2.2 Phase-0 guards (cheap, ship first)

- **Reentrancy guard, all builds:** `locking::lock` keeps a `thread_local! { HELD: RefCell<Vec<&'static str>> }`;
  taking a lock whose name is already in HELD on this thread logs `cue: LOCK REENTRY <name> at <caller>` with
  a `std::backtrace::Backtrace::force_capture()` to `~/Library/Logs/Cue/locks.log` and, in debug, panics. That
  turns the unexplained self-wait into a named line the next time it happens. Cost: one Vec push/pop per lock.
- **Lock-wait watchdog, release builds:** `locking::lock` becomes try-lock with a 100 ms spin then
  `lock()`; if acquisition took > 2 s it logs name, thread name, wait ms, and the backtrace of the *holder*
  (record holder's `thread::current().name()` + `Instant` in a side table keyed by lock address when a guard
  is created; wrap the guard in `locking::Guard<T>` with `Deref/DerefMut` and `Drop` that clears it). Release
  keeps going; the log names the pair. Requires every `.lock()` site (135) to go through `locking::lock` — the
  xtask lint enforces it.
- **Main-thread probe:** a 1 s `dispatch_async` ping on the main queue from a watcher thread (objc2
  `dispatch_get_main_queue`); if a ping isn't answered in 3 s, log "main thread stalled" + every lock holder
  from the side table. Also the signal used by the soak harness (§5). Dev builds additionally
  `SIGQUIT` themselves after 10 s so `sample` output is attached.
- **dSYMs:** `[profile.release] strip = "none"`, `debug = 1` (line tables), `split-debuginfo = "packed"`;
  `release.sh` runs `dsymutil` per arch, zips `Cue.dSYM` into `dist/v<ver>/` and uploads it as a release asset
  (private repo asset or a local `dist/` only — the owner's call; the dSYM is ~30 MB and leaks nothing the binary
  doesn't). Strip the shipped binary with `strip -S` **after** dsymutil. Keep the UUID (`dwarfdump --uuid`) in
  `.release-notes`. A hang report then symbolicates with `atos -o Cue.dSYM/... -l <load addr>`.
  The `--remap-path-prefix` flags stay.

### 2.3 Scoped access replacing `st()` guards

Replace `fn st(&self) -> MutexGuard<Store>` with two closures and no public guard:

```rust
fn read<R>(&self, f: impl FnOnce(&Store) -> R) -> R
fn write<R>(&self, f: impl FnOnce(&mut Store) -> R) -> R
```

A closure can't outlive the lock, can't be held across `self.changed()`, and the xtask lint can check that
no call inside a `read`/`write` closure body reaches I/O, `Command`, XPC, `db::`, or another `locking::lock`
(walk the closure's callees). Migration is mechanical: 208 sites, mostly `let mut st = self.st(); …; drop(st)`
→ `self.write(|st| …)`. Where a function reads, drops, does I/O, then writes again (`send.rs:215 reply`), it
stays two closures — that's the point. Everything that needs the store **and** effects is restructured as:
compute a plain `Decision` value under `write`, then perform effects outside (this is also §6's "pure
transitions vs effects" split).

### 2.4 Cache slow external facts

Slow things `snapshot` or hub methods read today, each to become a cache with its own refresher thread and a
`Cached<T> { value, at }` read without blocking: `config::login::enabled()` (SMAppService XPC, `login.rs:24`;
refresh on Settings open and every 60 s), `config::connections()` (settings-file parses), `usage_file::read`,
`live::branch/repo_root/checkout_top` (already memoized in `live.rs` statics by mtime — verify they never
`git` on a miss from the main path), `live::about` (transcript tail), `leads::view` (already behind INDEX
refreshed every 3 s), `machines::caches`. Rule, enforced by the lint: a function reachable from `snapshot` may
read memory and nothing else.

### 2.5 Lock order and the actor alternative

Lock order to document and lint: `store → typing-gate` **never** (gate is taken first in `reply`, store inside,
`send.rs:219-227`; keep that), `store → INDEX → CLAUDE`, `store → DB` **forbidden**, `open_terms` alone,
every other static alone (leaf). Any new edge needs the lint's allow-list changed in the same commit.

**Actor (single owner thread) alternative.** One `StateThread` owns `Store` outright; callers send
`Box<dyn FnOnce(&mut Store) -> R>` over an mpsc and await a oneshot (`hub.write(..)` keeps the same signature,
so §2.3 is the enabler). Pros: deadlock on `store` becomes impossible by construction; every transition is
serialized and loggable (a free event log for the soak harness); `snapshot` is computed on that thread and
published as an `Arc`. Cons: a slow closure stalls every transition (same as today's lock, but visible: the
thread's queue depth is a metric); callers in sync contexts (`hook.rs`? no — separate process; plain threads in
`term.rs`/`machines`) need a blocking `call` variant; ~2–3 days extra after §2.3.
**Recommendation:** do §2.3 (scoped closures) in Phase 1 — it removes the guard-held-across-helper class and is
lint-checkable — and decide on the actor at the end of Phase 2 using soak data (lock-wait histogram). If the
watchdog shows waits > 200 ms after I/O is out from under the lock, the actor isn't the fix; the closure is.
If reentrancy hits keep appearing, switch — the swap is one file (`hub/store_access.rs`) once §2.3 is in.

## 3. Autonomous UI testing (feasibility, verified 2026-11-06)

Facts (from v2.tauri.app/develop/tests/webdriver and webdriver.io/docs/desktop-testing/tauri):
- Raw `tauri-driver` is Windows/Linux only ("macOS has no WKWebView driver tool available").
- **WebdriverIO's Tauri service does support macOS** via an *embedded* WebDriver server: Rust crate
  `tauri-plugin-wdio-webdriver` added under `[target.'cfg(debug_assertions)'.dependencies]` and
  `#[cfg(debug_assertions)] builder.plugin(tauri_plugin_wdio_webdriver::init())`; npm `@wdio/tauri-service`
  spawns the debug app with `TAURI_WEBDRIVER_PORT`, drives it over W3C WebDriver, kills it after. Disabled in
  production builds. Needs `withGlobalTauri` (already on, `tauri.conf.json:10`). Not stated by the docs:
  screenshot/executeScript support of the embedded server — **to verify in the spike**; attaching to an
  already-running app — not offered.
- Tauri documents `mockIPC`/`mockWindows` in `@tauri-apps/api/mocks` for Vitest. Cue doesn't use
  `@tauri-apps/api` (plain scripts, `ui/core.js:2-3` reads `window.__TAURI__`), so `tests/preview.html`'s
  hand-made bridge is the equivalent and already exists.

Layered strategy (cheapest, highest coverage first):

**Layer A — Rust headless harness (no window).** `Hub::new(None)` already runs without an app
(`hub/mod.rs:140`). Build `src-tauri/src/testing/` (cfg(test)): a `World` that owns a `Hub`, a fake tmux
(§6 port `Terminal` trait), fake transcripts on a temp `CUE_HOME`, a fake clock, and drives hook events by
calling the same functions `server::handle` calls (factor `handle`'s match arms into `hub::inbound::apply(hub,
ClientMsg)` so the socket layer is 20 lines). Assertions on `snapshot()` JSON. This is where the "every nook"
matrix (§4) mostly lives. Fast (ms per case), fully autonomous.

**Layer B — real UI in Chrome against a scripted fake bridge.** Promote `tests/preview.html`'s bridge into
`tests/ui/bridge.js` (one object: `invoke` table covering **all 135 commands**, an `emit(name, payload)` to
push `state`/`term`/`speak`/`upload` events, a recorder of every invoke with args). Drive it with Playwright
(`npm i -D playwright`, Chromium; or the chrome-devtools MCP Claude already has for ad-hoc runs). Each test:
load `tests/ui/app.html?scenario=<name>` (scenarios are JSON snapshots recorded from Layer A's `World`, so UI
fixtures never drift from the real snapshot shape — add a Rust test that writes `tests/ui/scenarios/*.json`),
click/type, assert DOM + which invokes were called with what, and `page.screenshot()` into
`tests/ui/shots/<name>.png` compared pixel-wise (`pixelmatch`) against committed baselines; `?still=1` already
kills animations. Claude reads the PNGs directly (Read tool renders images) — this is the "screenshot-based
visual check". `tests/exec_attach.sh` is the prototype of this layer; fold it in.
Gap it can't cover: WKWebView-specific rendering/IPC and xterm WebGL; accepted.

**Layer C — real app, WebdriverIO embedded driver (debug build).** Spike first (1 day): add the plugin behind
`cfg(debug_assertions)`, write one wdio test that launches the debug app with `CUE_HOME=tmp CUE_QUIET=1`, feeds a
hook event over the socket (reuse `test_protocol.py`'s payloads via `cue-ctl`), finds the card, clicks Allow,
asserts the hook process got `allow`. Confirm screenshots and `executeScript` work; if not, we still get real
IPC + real WKWebView + real main-thread behaviour, and screenshots come from `screencapture -l <windowid>`
(CGWindowID via `osascript`/`GetWindowID`). Keep Layer C small (10–20 journeys: launch, permission, finished
turn + reply, terminal open/type, settings toggle, park/restart, update offer) — it's the one that would have
caught today's freeze, because it is the only layer with a real main thread and real PTY.

**Layer D — macOS Accessibility automation of the installed app.** Not recommended: brittle, needs TCC
grants per build, and Layer C covers the real-app need. Debug-only automation endpoint in the app: not needed
beyond what the wdio plugin gives, except one thing worth adding anyway — a `cue-ctl probe` socket message that
returns the watchdog's lock table + main-thread latency (§2.2), used by the soak harness.

How Claude runs it alone: `tests/run.sh` grows into `tests/run.sh [unit|ui|e2e|soak|all]`; `ui` needs only
Node + Chromium (no window focus stolen); `e2e` launches a *debug* Cue with a private `CUE_HOME` — never the
installed one — the way `run.sh` already does. All headless-safe; `CUE_QUIET` keeps notifications off.

## 4. Test matrix ("every nook and corner")

Layer key: U = Rust unit, I = Layer A integration (fake tmux/transcripts/hook events), UI = Layer B, E = Layer C,
F = failure injection (listed per row; all run in Layer A unless marked E). Numbers are new cases to write.

| Area | Code | Cases (U/I/UI/E) and failure injections |
|---|---|---|
| Sessions list / board | `hub/sessions.rs`, `sessions.rs`, `ui/board.js`, `ui/sessions.js` | I: start/stop/compact/end events ordering, driven_by hides from Waiting, missing-from-registry sweep (`GONE_AFTER_MS`), ended list, project moves. UI: row states, 0/1/50 sessions, long names. F: registry file half-written, session with empty cwd, 500 sessions. (~40) |
| Typing: send / reply / queue / hold / ⌘↵ | `hub/send.rs`, `terminal/focus.rs:1059 type_into` | U: `tmux_word`, `enter_gap`, `at_shell_prompt`. I: reply→typed→settle (`settle_typed`), held message released on turn end, queued review drain, concurrent sends to one session serialize on gate, 45 s gate timeout error, send to ended session. F: tmux `display-message` hangs (fake returns after 30 s), pane shows shell prompt, paste accepted but Enter not, transcript never confirms. E: type 200 chars fast into the built-in terminal with a stalled client. (~45) |
| Permission asks / questions | `hub/asks.rs`, `server.rs`, `hook.rs`, `ui/board.js` | I (extend `test_protocol.py` into Rust): allow/deny/always/answer, answered in terminal clears card, hook reconnect after Cue restart (`RECONNECT_FOR`), duplicate ask ids, ask after session end. UI: A/D/1-4/R keys. F: hook disconnects mid-wait, 100 asks in 1 s. (~30) |
| Terminal attach / write / resize / close | `terminal/term.rs`, `shells.rs`, `ui/shells.js` | U: `attach_args`, `pane_chain`. I (fake PTY): open joins in-flight open, replace old client, close during opening, write to closed, resize bounds. E: real tmux pane open/type/close, SSH-less. F: PTY writer blocks (pipe full), tmux binary missing, ssh exits 255. (~25) |
| Relay leads / executors / review cards | `leads/`, `hub/reviews.rs`, `ui/reviewcard.js`, `ui/agents.js` | U already heavy (`leads/tests/`); add I: review queue when lead busy, set-aside, handoff chain, remote executor attach. F: relay state folder half-written, 50 executors. (~20) |
| Status list / background agents | `statuslist.rs`, `logs/steps.rs`, `ui/status*.js`, `ui/background.js` | U exists (node + Rust); add I: status follows into hand-off, background agent reports after finish. F: 5 MB status payload, malformed markdown. (~15) |
| Tasks / About | `tasks.rs`, `ui/about.js` | I: track/turn_ended/stop; F: slow summarizer. (~8) |
| Voice / speak | `talk.rs`, `terminal/speak.rs`, `dictation.rs` | U: `hook_context`, status parsing. I: fake `cue-voice` binary that prints scripted lines. Real mic/speaker: **not automatable, low value** — a 3-step manual checklist in the release gate instead. (~10) |
| Notifications | `notify.rs` | U: `quiet`/`remove` bookkeeping. E: one real notification in Layer C (asserts no crash; content not asserted). F: XPC 5 s stall via a fake `post_sound` → hub never waits. (~6) |
| Machines / remote over SSH | `machines/*`, `remote_poll.rs` | U: `remote_poll::Schedule`. I: fake `ssh` shim on PATH (script that replays canned output / sleeps / exits 255). F: drop mid-stream, watcher restart, unreachable backoff, `/home` automounter stall (stat never returns). (~20) |
| Hooks CLI | `hook.rs` | I: run the real `cue hook` binary against Layer A's socket for every event; exit 0 silently with no Cue; 15 min reconnect loop with a fake clock. (~12) |
| db / history | `db.rs` | U: migrations from each `user_version`, `history_keep` trimming, `rewrite_paths`. F: `SQLITE_BUSY` from an external writer holding the file (today no `busy_timeout`, `db.rs:218` — set one, 2 s), corrupt db file → fresh db + backup, disk full. (~15) |
| Insights | `ipc/insights*.rs`, `logs/usage.rs`, `logs/turns.rs` | U: cost math from fixture transcripts; I: cache_tick warnings once per expiry. F: 200 MB transcript, transcript with invalid JSON lines. (~15) |
| Settings | `config.rs`, `ipc/settings.rs`, `ui/sheets.js` | U: `every_setting_the_window_saves_is_allowed` exists; add roundtrip per key. F: corrupt `config.json` (truncated, wrong types) → defaults + warning, not a crash; settings file read-only. UI: each tab renders. (~15) |
| Hand-offs | `hub/activity.rs hand_off_ahead`, `ipc/insights.rs handoff_*` | I: sent/continue/fresh/keep; ahead watch tick. (~10) |
| Parking / restarting | `ipc/sessions.rs`, `ipc/restore.rs`, `hub/sessions.rs` | I: park→resume→restart (mcp and plain), restart step timeouts, restore after reboot (`restore_sessions`). F: tmux session vanished mid-restart. (~15) |
| Uploads / images | `uploads.rs`, `hub/send.rs images_there` | U: paths; I: remote copy progress events; F: 50 MB image, unreadable file. (~8) |
| Project moves | `hub/mod.rs put_projects/move_projects` | U exists; add UI case. (~3) |
| Extensions / actions | `extensions.rs`, `actions/*` | U: allow/run/stop; F: action prints 10 MB, never exits. (~12) |
| Updater | `lib.rs find_update` | I: fake `latest.json` server; F: 404, bad signature. (~4) |

~330 new cases; the existing 408 Rust tests stay. Priority order: Typing, Terminal, Asks, Sessions (the
real-use bugs), then the rest.

## 5. Stress / soak / chaos harness

`src-tauri/src/testing/soak.rs` (cfg(test), `#[ignore]` by default, run by `tests/run.sh soak`) and an E
variant against the debug app:
- **Generator:** N fake sessions (default 40) each with a scripted agent loop: prompt → working (fake transcript
  grows at 50 KB/s) → stop → ask → answered → repeat; hook events through the real socket at up to 200/s bursts;
  typing bursts (10 concurrent `reply`/`send_to_session` per second); terminal opens/closes; a machine with a
  fake `ssh` that drops every 30 s; `cue-ctl list` polls at 5/s (the window's `get_state`).
- **Detectors:** main-thread probe (§2.2) latency > 500 ms = fail; lock watchdog any wait > 1 s = fail; lock
  reentry = fail; memory growth > 2× after warm-up = fail; snapshot size growth unbounded = fail; a hook
  that waits > 10 s for a decision that was given = fail; any `panic` in the log = fail.
- **Chaos knobs** (env): `CUE_CHAOS_TMUX_DELAY_MS`, `CUE_CHAOS_SSH_DROP_EVERY`, `CUE_CHAOS_XPC_DELAY_MS`,
  `CUE_CHAOS_DB_LOCK_SECS`, `CUE_CHAOS_FS_DELAY_MS` — honoured by the fake adapters (§6), and by the real
  adapters only in debug builds.
- Run length: 10 min in the pre-release gate, 60 min nightly (`launchd` job on the Mac, log to
  `~/Library/Logs/Cue/soak/`). Output: a one-page text report (p50/p99 main-thread latency, lock-wait
  histogram per lock name, events/s) that Claude reads.

## 6. Refactor / modularization map

Target structure (ports/adapters so every external thing can be faked; pure state separate from effects):

```
src-tauri/src/
  core/            pure: model.rs, sessions.rs (state machine only), asks.rs, reviews state, store.rs
  hub/             Hub = Store access (store_access.rs: read/write/actor) + transitions (per area, pure fns
                   `fn on_stop(st: &mut Store, ev) -> Vec<Effect>`) + effects.rs (runs Vec<Effect> outside the lock)
  ports/           traits: Terminal (tmux/pty), Remote (ssh), Db, Clock, Notifier, LoginItems (XPC),
                   Transcripts (fs), Processes (ps/kill), Voice
  adapters/        the real ones: tmux.rs, ssh.rs, sqlite.rs, un_notify.rs, sm_appservice.rs, fs_transcripts.rs
  testing/         fakes for every port + World + soak generator (cfg(test))
  ipc/             commands only: parse args → hub call under spawn_blocking → serialize
  server/          socket framing + inbound.rs (ClientMsg → hub) — handle() shrinks to a loop
  xtask/           (separate crate) inventory + lints
```

Ordered, safe steps (tests land first in each):
1. `server::handle` (147 lines) → `inbound::apply` + 10 arm functions; Layer A harness built on it. (2 d)
2. `locking.rs` reentry guard + watchdog + `Guard<T>` wrapper; all `.lock()` → `locking::lock`. (1 d)
3. Sync→async commands (53) + snapshot cache; `term::write` off-lock. (2 d)
4. `Hub::st()` → `read`/`write` closures, file by file (`hub/activity.rs` 1,241 lines first, split into
   `events.rs`/`cache_tick.rs`/`usage.rs`/`hand_off.rs` while touching it; `on_event` 227 and
   `refresh_activity` 235 lines each become a dispatcher + ≤60-line handlers). (4 d)
5. Ports: `Terminal` trait over `focus.rs` tmux calls + `term.rs` PTY (fake = in-memory pane that records
   keystrokes and can be told to stall); `Remote` over `machines/*` ssh; `Clock` (so soak/tests use a fake
   clock instead of `sleep`); `Db` last (db.rs is already one module). Each trait lands with its fake and the
   matrix rows that need it. (5 d)
6. Split the big files: `logs/steps.rs` 3,230 → `steps/{feed,history,windows,codex,claude,detail}.rs`;
   `leads.rs` 2,141 → `leads/{index,relay,pilead,remote,ledger,verify}.rs`; `terminal/focus.rs` 1,464 →
   `{tmux,iterm,dialog_probe,type_into}.rs`; `sessions.rs` 1,769 → state vs view; `lib.rs run` 234 lines →
   `startup/{tickers,watchers,window}.rs`. Mechanical moves, each its own commit, no behaviour change,
   `cargo test` green between every one. (4 d)
7. UI: `ui/app.js bindMain` 683 lines → per-view binders (`board_bind.js`, `chat_bind.js`, …) with one
   `delegate(root, selector, handler)` helper; `ui/chat.js` 2,646 → `chat/{render,compose,thread,tools}.js`.
   `index.html` script order is load-bearing (`lib.rs:393` test) — keep the test, extend it to the split. (3 d)

## 7. Release process

- **Gates, in `scripts/release.sh` before `tauri build`:** `tests/run.sh all` (unit 92 s + ui + e2e + 10-min
  soak) must be green; `cargo run -p xtask -- check` green; `git diff --exit-code docs/dev/inventory.md`;
  the Swift helpers built from a clean scratch dir. The script refuses otherwise (`--skip-gates` exists but is
  logged into `.release-notes`).
- **dSYMs** as §2.2; `dist/v<ver>/Cue.dSYM.zip` kept forever (small), plus `Cargo.lock` hash and rustc version
  recorded so a build is at least *identifiable*; true reproducibility isn't worth chasing (SwiftPM, Xcode).
- **One ship per day:** `release.sh` refuses a second tag with today's date unless `--hotfix "<why>"`. 7 ships
  on Nov 1 is the symptom this plan treats; the rule is cheap and forces batching.
- **Beta channel:** `tauri-plugin-updater` supports multiple endpoints — publish `latest-beta.json` from
  `release.sh --beta`; Settings → "Get beta builds" switches the endpoint. the owner dogfoods beta ≥ 24 h (the
  soak harness on his own sessions) before `--publish` promotes the same artifact (no rebuild) to `latest.json`.
- **Build-cache robustness (`build.rs:62`):** scratch dir moves from `$TMPDIR/cue-<name>-build` (macOS purges
  and half-purges `$TMPDIR`) to `src-tauri/target/swift/<name>/<hash of Package.resolved + swift --version>`;
  if `swift build` fails, wipe the scratch dir once and retry before failing the Cargo build; `cargo clean`
  then clears it like everything else. Also `install.sh --clean-swift`.

## 8. Phasing, effort, parallelism, acceptance

Effort in executor-days (Sonnet executors in worktrees; Fable lead reviews). Phases 1–3 have independent
tracks that can run in parallel; the file-split (6.6) is sequenced after §2.3 to avoid rebase pain.

| Phase | Content | Effort | Parallel tracks | Acceptance |
|---|---|---|---|---|
| **0 – guards** | §2.2 reentry guard + watchdog + main-thread probe; dSYM retention in `release.sh`; `term::write` off-lock; `get_state`/`respond`/`term_write`/`term_resize`/`search`/`session_log*`/`set_setting`/`closed_sessions` to async+`spawn_blocking`; `server::handle` hub calls under `spawn_blocking`; `busy_timeout`; commit the hub/mod.rs patch. | 2 d | 2 (locks+dSYM / commands+term) | `cargo test` green; a deliberate reentrant lock in a test logs the named pair; 1 release built with a dSYM and `atos` resolves a frame; typing 1,000 chars into a terminal whose client is `kill -STOP`ped never stalls the window (E spike). |
| **1 – harness + inventory** | Layer A `World` + `inbound::apply`; xtask inventory + lints wired into `cargo test`; Layer B bridge covering 135 commands + Playwright + screenshot baselines for the 12 main screens; Layer C spike (wdio embedded). | 5 d | 3 (A / xtask / B+C) | `tests/run.sh all` runs green headless from a fresh clone with only Rust+Node+Xcode; inventory doc regenerates identically; drift test proven by adding a fake `.lock()` and watching it fail. |
| **2 – scoped access + caches** | §2.3 closures across 208 sites; §2.4 caches; lock order lint on; all 53 sync commands async except the AppKit ones; actor decision taken from soak data. | 5 d | 2 (hub files / ipc+caches) | zero `MutexGuard` escapes the hub (lint); soak 10 min at 40 sessions: main-thread p99 < 50 ms, max lock wait < 100 ms. |
| **3 – test matrix** | §4 rows in priority order; failure injection via ports as they land (§6.5). | 8 d | 4 (typing+terminal / asks+sessions / remote+hooks / rest) | ~330 cases green; every matrix row has ≥1 F case; mutation spot-check: 10 random reverted bug-fix commits from the last 30 days each fail ≥1 test. |
| **4 – ports + splits** | §6.5–6.7. | 10 d | 3 (Rust ports / Rust splits / UI splits) | no function > 60 lines outside tests (xtask lint), no file > 1,000 lines; behaviour-neutral: Layer B screenshots identical. |
| **5 – soak + release gates** | §5 full harness, nightly launchd run, §7 gates, beta channel, build cache fix. | 3 d | 2 | one beta shipped through the gates; nightly report readable; a second same-day release refused. |

Total ≈ 33 executor-days; with 3 parallel executors ≈ 3 weeks wall-clock, Phase 0 in the first two days.

## 9. Things I'd push back on / couldn't verify

- "Every nook" for voice/speak and real notifications: the Swift helpers and Notification Center can't be
  asserted on headless; a 3-item manual checklist per release is the honest answer.
- macOS Accessibility automation (§3 Layer D): low value once Layer C works; skip.
- The self-owned-mutex hang remains a hypothesis (§1.2). Phase 0's reentry guard is designed to name it, not
  claim it's fixed. The strongest *concrete* main-thread freeze path found is `term::write` under
  `open_terms` on the main thread with a PTY that can block (`term.rs:353-357` via `ipc/terminal.rs:122`).
- Not verified: whether `live::branch/repo_root` shell out to `git` on a cache miss from `snapshot`
  (`logs/live.rs:181-222` memoize by mtime; a miss on the main thread would still be a stall) — the xtask
  lint answers this mechanically in Phase 1.
- Not verified: wdio embedded server's screenshot/`executeScript` support on macOS (docs silent) — the Phase 1
  spike decides; fallback is `screencapture -l`.
- Function and file lengths in §0 were re-measured with `xtask sizes` in Phase 1 (they were awk brace counts).
- `tests/run.sh` runs only `status.test.js`; `agents/review_card/status_panel.test.js` aren't in any runner
  today — Phase 1 adds them.

## Follow-ups queued during Phase 1 (lead)

- UI layout bugs seen in Layer B baselines (fix after review-click lands on main, it owns reviewcard.js):
  three review cards squeeze the chat to one line (relay-review); "needs you" overlaps the time on a
  lead's Waiting card; at 900 px the Later button clips and a mono command breaks mid-word; a stray "0" at
  the right end of the time bar on the waiting board; the open status-list drawer cuts off its cost line.
  After review-click's rows landed: the top review row is clipped under the active card's header, and the
  active card's header chips crowd together ("li… ne 7 ag") when a lead has a listen chip and agents.
- Terminal "stuck" detection: e2e spike saw a SIGSTOPped tmux client's pty accept ~8 MB before blocking,
  so 1,000 typed chars never show the bar — track D is investigating.
