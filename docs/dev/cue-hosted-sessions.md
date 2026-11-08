# Cue-hosted sessions — design

Status (2026-11-07): **M1 and M2 built** on branch `hosted-m1`: the session client (§4.1) and its wiring into the
hub and the window (§4.2–4.4), behind a setting that is off by default (§3). Next: a few days of daily use with it
on (§8).

## 1. What and why

Today every Claude session runs in a terminal (iTerm, or Cue's own tmux), and Cue works *beside* it:
it hears about turns and permissions through hooks, reads the screen, and types your replies in as keystrokes.

A **Cue-hosted session** is one Cue starts itself with no terminal at all: Claude Code in headless mode
(`claude -p` with JSON in and out). Claude talks **directly to Cue**: every message, step, permission,
question and input request arrives as data, and your answers go straight back. It is the same Claude Code
with the same settings, skills, tools, hooks and session files. Only who Claude talks to changes.

What it fixes, as the owner put it:

- **"I say yes and the permission doesn't reach the terminal."** Prompts the permission hook never sees
  (sandbox network access, MCP input requests, dialogs) force an *answer-in-terminal* card today, answered by
  typing into the screen. In a hosted session there is no screen, and every one of them arrives as a request
  (verified, §2).
- **The lag between sending a message and Cue saying it landed.** Today: type it in, read the screen to check
  it landed, press Enter (again if needed), wait up to ~3 s for the hook. Hosted: one line written to Claude's
  input, echoed back by Claude when it takes the message.
- **Fragility from inferring state.** No tmux and no screen reading for hosted sessions, so the class of
  incident behind 2026-11-06's tmux kill-server and deleted-socket outages can't happen to them.

Terms used below: **terminal session** (what Cue has today, unchanged) and **hosted session** (new).

## 2. What was verified (2026-11-07, Claude Code 2.1.319, Haiku)

Every row below is now a test: the recorded lines are `src-tauri/src/hosted/fixtures/` (checked by
`hosted/translate_tests.rs`), and the real-Claude compatibility suite (`hosted/compat_tests.rs`, §5) runs each
case end to end. Launch line:

```
claude -p --input-format stream-json --output-format stream-json --verbose --replay-user-messages \
  --permission-prompts host --permission-prompt-tool stdio [--session-id <uuid> | --resume <uuid>]
```

| Case | Today | Hosted (observed) |
|---|---|---|
| Bash permission | hook card | `control_request` `can_use_tool` (tool, input, suggestions, `tool_use_id`) |
| Background agent's permission | hook card | same, with `agent_id` naming the agent |
| AskUserQuestion | question card | `can_use_tool` for `AskUserQuestion`; answers go back in `updatedInput.answers` |
| Sandbox network access | **answer in terminal** | `can_use_tool` for tool `SandboxNetworkAccess`, input `{host}` |
| MCP server asking for input | **answer in terminal** | `control_request` `elicitation` (server name, message, form schema); answer reached the server |
| Teammate's permission | **answer in terminal** | headless has no separate-terminal teammates (its agent tools are `Task`, `ListAgents`, `SendMessage`, `TaskStop`): a named agent runs in-process and its asks come like a background agent's, with its `agent_id` |
| Message echo | hook `prompt` | user message echoed with `isReplay: true` **when Claude takes it** (~0.8 s after send when idle) |
| Message sent mid-turn | typed into a busy TUI | queued by Claude, taken at the next tool boundary, **answered in the same turn**; echo arrives then |
| Interrupt | Esc keystroke | `control_request` `interrupt`: running tool stopped in ~10 ms, turn ended, session took the next message |
| Resume | `claude --resume` in a tab | `--resume <id>` headless keeps the conversation (recalled a word from before) |
| Preassigned id | n/a | `--session-id <uuid>`: Cue knows the id before Claude starts |
| Unprompted turn | Stop hook | a background task finishing **starts a new turn by itself** (init → assistant → result, no user message) |
| Always allow | hook card | the rule is kept where its suggestion says (`localSettings`: the folder's `.claude/settings.local.json`) and holds for the next call |
| Your hooks | run | still run, and see `CUE_HOSTED=1` (so Cue's own hook can stay out of the way) |
| Resume's transcript | n/a | `--resume` appends to the same `<session>.jsonl`; no second file |

Also seen: the `init` event lists tools, slash commands, `terminal_slash_commands` (the ones that only work in
a terminal), skills, MCP servers, model and permission mode. Claude still writes its transcript to
`~/.claude/projects/<slug>/<session>.jsonl`, so Cue's transcript readers keep working unchanged.

## 3. Scope

**v1 (this design):** Claude Code only, this Mac only, the **Cue app owns the Claude processes** (no
separate agent host yet; the owner's call, 2026-11-07). A hosted session survives a Cue restart as a conversation
(resumed with `--resume`), not as a running process: a turn in progress when Cue quits is lost.

**Not in v1:** the agent host process (§8 M4, now §9 H3), remote machines, Codex/Pi (§9), starting relay executors as hosted
sessions, removing the built-in terminal panel. Terminal sessions keep working exactly as today throughout.

**Behind a setting, off by default.** Hosted sessions ship as a trial you turn on: Settings → *Cue-hosted
sessions*, stored as `hosted.on` in config.json (missing means off). Turned off, Cue behaves exactly as it does
without this work: no `Cue-hosted session` under `+ New`, and Move to Cue resumes into Cue's tmux as today. Turned
on, `+ New` offers it and Move to Cue resumes as hosted. The switch only decides how *new* sessions start: turning
it off changes nothing about hosted sessions you already have: they keep working, paused ones resume hosted, and
Move to terminal is always there to turn one into a terminal session (the transcript is the same file either way,
so nothing is lost). Both paths
stay fully working for the whole trial; the setting becomes the default only after a few days of daily use hold
up, and only after that does M5 retire anything.

**The agent host comes back.** v1 leaves it out on purpose, not for good: M4 is the planned return, taken up as
soon as Cue restarts (updates, crashes) interrupting running turns are felt in daily use. The `Hosted` port
(§4.2) is the seam that move goes through, so the hub doesn't change when it happens.

## 4. Architecture

### 4.1 `src-tauri/src/hosted/` — the Claude session client (new, standalone)

Built (M1). No dependency on the hub, the store or the UI, and the code that later moves into the agent host
unchanged.

```
hosted/
  mod.rs        HostedEvent, Ask, Answer; HOSTED_ENV
  wire.rs       every line in and out: read leniently (unknown types kept, non-JSON skipped), written exactly
  translate.rs  pure: lines -> HostedEvent, with the little state a line can't carry (started once, in a turn,
                prompted or not). All the protocol knowledge
  launch.rs     Spec -> the command: headless flags, --session-id / --resume, permissions (the same table
                + New session uses: focus::permission_flags), model; through your login shell
  session.rs    one Claude: its process group, a reader, an exit waiter, pending asks (each answered once),
                interrupt, close (input closed -> grace -> SIGTERM -> SIGKILL, waited for each time)
```

`HostedEvent`: `Started` (once), `Taken`, `Said`, `ToolCall`, `ToolResult` (each with the `agent` that made it),
`Task`, `Ask`, `TurnEnded { text, failed, cost_usd, unprompted }`, `Refused` (a request of a kind Cue doesn't
know: refused at once so Claude never waits on Cue), `Exited { code, during_turn, last_words }`.

`Answer` reuses Cue's `model::Decision` through `hook::decision`, the one mapping the hook's PermissionRequest
output and a hosted session's answer share; a hosted allow always carries `updatedInput`. `Input` asks answer
with `{action: accept|decline|cancel, content}`.

Two process rules the tests pin: closing returns only once the process is gone (or killed), and when Claude
exits by itself while something it started still holds its output (a background command), that leftover is
ended, so a session is over when Claude is.

### 4.2 The seam: a `Hosted` port beside `Keys` and `Launcher`

Phase 4 put terminals behind ports (`terminal::keys::Keys`, `terminal::launch::Launcher`, in `hub/ports.rs`).
Hosted sessions get a third, `hosted::Hosted` (built in M2, `hosted/running.rs`):

```rust
pub trait Hosted: Send + Sync {
    fn start(&self, spec: &Spec, report: Report) -> Result<i32, String>; // new or resumed (spec.start); its pid
    fn send(&self, session_id: &str, text: &str) -> Result<(), String>;
    fn answer(&self, session_id: &str, request_id: &str, answer: &Answer) -> Result<(), String>;
    fn interrupt(&self, session_id: &str) -> Result<(), String>;
    fn close(&self, session_id: &str);        // returns once it's gone; its exit isn't reported
    fn running(&self, session_id: &str) -> bool;
    fn close_all(&self);                      // Cue quitting
}
```

`Running` is the real one (sessions by id; starting one already running closes it first, so two processes never
share a conversation). `testing::FakeHosted` stands in for it in the World: it records starts, sends, answers,
stops and closes, and a test hands the hub what a session reports with `World::hosted_says`. The report function
holds the hub weakly, so a session never keeps the hub alive.

### 4.3 Into the hub: the same doors hooks use

The hub should barely know a session is hosted. Each `HostedEvent` enters through the entry point the
equivalent hook message uses today, so turn handling, cards, Insights, voice and the island all work unchanged:

| HostedEvent | Today's hook path it reuses |
|---|---|
| `Ask` (Permission/Question/Network) | `ClientMsg::Ask` → ask item; the decision comes back through `Hub::respond` and the adapter writes the `control_response` instead of a socket reply |
| `Ask` (Input) | the same, with a new item kind `input` (a form card; the only new card type) |
| `Taken` | `prompt` event: the message bubble's "waiting for it to take this" clears immediately |
| `Step` | `doing` / activity (steps keep coming from the transcript too; the stream just makes them live) |
| `TurnEnded` | `stopped` event with the final text (`unprompted` turns treated like any finished turn) |
| `Exited` | `end` event; during a turn: a card saying it stopped and offering Resume |

As built (`hub/hosted.rs`): `Taken` while idle is an `active` event (the turn starts; the message was noted in
the chat when Cue sent it, so it isn't noted twice), mid-turn it settles the queued message. `Said`, tool calls
and tasks mark an idle session working (an unprompted turn). A failed turn goes through `failed`, as an API error
does. Each ask's answer waits on a thread of its own, so the session's next lines keep coming meanwhile; a card
that goes without an answer is a denial saying why (a form: dismissed), so Claude never waits on Cue.

`Origin` gets one explicit field, `hosted: bool`. It decides: messages, replies and Stop go through the port
instead of `Keys` (sent at once even mid-turn, since Claude reads them at its next step; nothing is held in Cue);
Go to tab, the terminal strip and Move to Cue aren't offered (the reply box's corner says Paused when it is, and
offers Move to terminal); Claude Code's own list of running sessions (which only has terminal ones) never ends
it. None of today's terminal code changes for terminal sessions.

**Cue's own hooks:** a hosted session still runs your hooks (relay's, the iTerm status hook, …), and they see
`CUE_HOSTED=1` (compat suite). Cue's `cue-hook` would report the same events twice, so when it's set the hook
reports nothing to Cue; what it adds to the session's own prompts (the status list, a voice message's context)
still runs. Built in M1 (`hook.rs`).

### 4.4 Lifecycle

- **Start:** + New → Runs in → **Hosted** (offered with the setting on, for Claude Code on this Mac), with the
  folder, MCP servers, model, permissions and flags + New already has. Cue picks the session id up front
  (`--session-id`). Flags are passed word by word (no shell reads them), so a quoted flag is refused.
- **Cue quits:** stdin closed, process group ended. A turn in progress is lost; the conversation isn't.
- **Paused:** a hosted session records a process only while one runs. After a Cue restart, or when Claude exits,
  it stays in the Board, paused. The first message you send resumes it (`--resume`, on the model it last used
  and its MCP selection), not before: resuming re-reads the conversation (cache cost), so nothing is resumed
  that you don't use. It resumes in Claude's own default permission mode (the one + New picked isn't kept yet).
- **Claude exits unexpectedly:** `Exited` → its asks finish as gone; mid-turn, the turn ends with a card saying
  why (its last words); it's paused. Garbage or unknown lines are logged, never fatal.
- **Close, Park, Resume:** as for any session; a parked or closed hosted session resumes hosted.
- **Unprompted turns:** a background task finishing can start a turn with no message from you; it shows up as
  any finished turn does.
- **Move to Cue** (a terminal session's button) keeps its rule: offered once the session isn't working, it
  ends the session in its tab, then resumes it as a hosted session with the setting on (in Cue's tmux off). The
  terminal copy must be gone first: two processes on one conversation would both write its transcript.
  Its history needs no conversion: a terminal session and a hosted one write the same transcript file in the
  same format, so the earlier turns render from the transcript as a terminal session's chat does today, and the
  new ones arrive live (and are appended to that file too). Compat suite: a resume appends to the same file.
- **Move to terminal** (the reverse, the escape hatch, in the reply box's corner): a hosted session that isn't
  working or asking is closed and resumed with `claude --resume <id>` in a terminal tab, where it becomes an
  ordinary terminal session (its name and chat stay).

### 4.5 Isolation (lessons from 2026-11-06)

- Processes run in their own process group, killed as a group; nothing touches tmux.
- Unit tests run against a **fake `claude`** (`testing/fake_claude.rs`, a scripted shell stand-in), never the
  real binary, inside the test binary's own home folder (`testing/sandbox.rs`).
- The real-Claude compatibility suite is opt-in (`CUE_CLAUDE_COMPAT=1`), runs each case in a temp folder with
  only its own settings (none of your hooks, plugins or MCP servers), and gives the real home folder only to
  the `claude` it starts (its login is there). It removes the session folder Claude writes under
  `~/.claude/projects` for it. It launches with `CUE_HOSTED=1`, so the live Cue never sees it.

## 5. Testing

1. **Fixtures** (built): recorded lines per §2 case (`hosted/fixtures/`, paths and the recording machine's own
   plugin and command lists taken out); `translate_tests.rs` checks each, plus unknown and non-JSON lines.
   `wire.rs` and `launch.rs` have their own unit tests (answers as Claude reads them, arguments, bad ids).
2. **Session lifecycle on the fake `claude`** (built, `session_tests.rs`): setup first, environment, an ask
   answered once, two asks answered out of order, messages and interrupts as written, an unknown request
   refused, non-JSON skipped, death mid-turn with its last words, closing (polite, stubborn, with a child left
   behind), and a leftover child holding the output open after Claude exits.
3. **Compatibility suite** (built, `compat_tests.rs`, 10 cases, ~20 s): permission, denial with its reason,
   question, sandbox network, MCP form, message mid-turn + interrupt, resume into the same transcript, hooks
   seeing `CUE_HOSTED`, always allow, a named agent's ask. Run on each Claude Code update and before a release
   that touches `hosted/`:
   `CUE_CLAUDE_COMPAT=1 cargo test --lib hosted::compat -- --ignored --test-threads=3`
4. **Hub wiring (M2, built, `testing/tests/hosted.rs`, 16 cases):** the setting off; + New hosted (and one that
   won't start); a message idle and mid-turn; a finished turn; a permission, a form and a card left unanswered;
   Stop; Claude exiting mid-turn, paused, resumed by the next message; a relaunch (paused, kept, resumed); Close;
   Park and Resume; Move to Cue into hosted; Move to terminal. The window: `tests/preview.html?hosted=1` (the form
   card, + New's Hosted) and `?hosted=paused`.

## 6. Plain-language summary of what changes for you

- A new kind of session you can start from Cue. It never sends you to a terminal: every permission,
  question and input request is a card, and your answers can't get lost in a screen.
- Messages show as taken the moment Claude takes them.
- Your terminal sessions keep working exactly as today.

## 7. Dependencies on the stability plan

`hosted/` (M1) has none. The port and hub wiring (M2) build on Phase 4 (merged 2026-11-07): its ports seam and
its split hub (`hub/events.rs`, `hub/asks.rs`, `hub/send.rs`).

## 8. Milestones

| | What | When | Done when |
|---|---|---|---|
| **M1** | `hosted/` client, fake `claude`, fixtures, compatibility suite, `cue-hook` skip; settle the teammate question | **done 2026-11-07** (branch `hosted-m1`) | fixture + lifecycle tests green; compat suite green on 2.1.319 |
| **M2** | `Hosted` port + World fake, hub wiring (§4.3), `input` card, `+ New → Cue-hosted session`, paused/resume, all behind the `hosted.on` setting (§3) | **built 2026-11-07**; the trial is next | World cases green, with the setting off and on; a few days of daily use with it on and no answer-in-terminal card on a hosted session; then the owner decides whether it becomes the default |
| **M3** | relay executors as hosted sessions (relay asks Cue to start them instead of opening iTerm tabs) | after M2 holds up | an executor packet runs end to end hosted |
| **M4** | the agent host: processes move out of the app so a Cue restart or update no longer stops running work | when restarts interrupting work are felt | Cue restart mid-turn: the turn finishes, the app picks it back up |
| **M5** | every agent hosted and Cue's terminal removed: §9, H1 Pi → H2 Codex → H3 agent host (was M4) → H4 machines → H5 terminal-only gaps → H6 removal | started 2026-11-07 | each step live-tested; H6 only after the rest holds up in daily use |

## 9. Every agent hosted, then no Cue terminal (the owner's call, 2026-11-07)

The goal: Cue's own terminal (tmux panel, `Runs in → Cue`) goes away. Every agent Cue starts is hosted, asks
the same way (a card), and nothing needs a terminal to be answered. Steps, in order; each behind `hosted.on`
until the last, each with recorded fixtures from the real CLI and World tests, each run live before it's handed
over.

**H1. Pi, hosted** (built 2026-11-07: `hosted/protocol.rs` splits what a session's lines mean from its process, `hosted/pi.rs` is Pi's; fixtures recorded from Pi 0.87.1 with DeepSeek; live-tested start, gate card, mid-turn message, Cue restart and resume, Stop). `pi --mode rpc --session-id <id>` in its folder, through the login shell, with `CUE_HOSTED=1`.
Pi has no permission prompts; Cue's Pi extension (`pi/cue.ts`) is already the gate (`pi.gate`: dangerous / all
/ off, `CUE_PI_GATE` for one session) and already reports to Cue over its socket: what it's doing, its asks, its
turn ending. That stays the one channel for Pi's events and asks, so a hosted Pi and a terminal Pi look the same
to the hub. What hosting adds: Cue owns the process (the `Hosted` port, a Pi session beside the Claude one), sends
your messages as RPC `prompt` (idle) or `steer` (mid-turn) and stops with `abort`, and reads from Pi's stdout
only what the extension can't tell: the process is up, a message was taken, it exited. The extension's dialogs
(`ctx.ui.select` for the terminal half of its race) arrive as `extension_ui_request` and are left to Cue's card:
answered `cancelled`, which the extension already treats as "no answer from the terminal".

**H2. Codex, hosted** (built 2026-11-07: `hosted/codex.rs`; a session started hosted is known by Codex's thread id, its rollout file its transcript; + New's Ask is `untrusted`, so Codex asks whatever its config defaults to; fixtures recorded from Codex 0.166; live-tested start, command and file asks, mid-turn message, Cue restart and resume, Stop). `codex app-server` (JSON-RPC 2.0 over stdio, the channel Codex's own apps use; marked
experimental, so the compat suite pins a version). `thread/start` (folder, model, approval policy and sandbox
from + New's permission choice) or `thread/resume`; `turn/start` for a message, `turn/steer` mid-turn,
`turn/interrupt` to stop. Its requests become cards: `item/commandExecution/requestApproval` (a command),
`item/fileChange/requestApproval` (an edit), `item/permissions/requestApproval`, `item/tool/requestUserInput`
(a question), `mcpServer/elicitation/request` (a form, the `input` card). Codex picks the thread id, so the
session's id is known once `thread/start` answers. Codex's hooks skip a hosted Codex (`CUE_HOSTED`, as for
Claude). Cue finds `codex` on the login shell's PATH, else inside the ChatGPT app
(`/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex`).

**H3. The agent host (was M4). Built 2026-11-08** (`docs/dev/cue-agent-host.md`, §12 for how it came out). cue-host
(`cue host`, the same binary, like `cue hook`) owns every hosted agent process and talks to the app over a socket in
Cue's data folder; the app's `Hosts` is its client behind the same `Hosted` port. Quitting or updating Cue no longer
stops a turn: on launch the app reconnects, rebuilds each session's state from the host's log, asks again what still
waits, and carries on with what it said meanwhile. This is what tmux gave terminal sessions; it had to exist before
tmux goes.

**H4. Other machines. Built 2026-11-08** (`docs/dev/cue-agent-host.md` §13). A session on a machine runs under
cue-host there, as on this Mac: Cue carries cue-host for Linux (x86_64, aarch64, static) and macOS
(`scripts/build-host.sh`, in Cue.app), puts it at `~/.cue/bin/cue-host-<build>` on first use (checksum-checked), and
reaches it with `cue-host connect` over the machine's shared SSH connection. Quitting Cue, a dropped connection or the
Mac sleeping no longer stops a turn there: Cue reconnects, catches up on what it said, asks again what still waits, and
sends what you typed meanwhile. + New → a machine → Runs in: Hosted; Move to hosted for a session in tmux there; Move
to terminal takes one back to tmux there. Pi there gets Cue's extension (its gate) unless it has it installed. The
machine's hook script leaves hosted agents alone. Tested against Linux in Docker on both architectures
(`scripts/test-machines.sh`), which found a deadlock in `connect` under sshd (fixed: §13).

**H5. What only a terminal does. Built 2026-11-08** for Claude Code:
- *Full-screen commands and signing in* (`/config`, `/login`, `/theme`, …: what Claude reports as
  `terminal_slash_commands`, plus sign-in and its own settings screens, which headless never offers): sent to a hosted
  session, Cue opens `claude /<command>` in a terminal tab in its folder (on its machine, over SSH) instead; the `/`
  menu marks them "opens a terminal" (`hub/terminal_only.rs`). Never on the session's conversation.
- *Seeing the raw screen when something looks stuck*: judged not needed. Its raw lines are protocol JSON, of use only
  in debugging Cue (cue-host keeps them all in the session's log); for you, the chat shows what it's doing, Stop stops
  it, and an agent that exits says why.
- *The folder-trust dialog* (headless never asks): + New asks "Trust this folder?" before hosting Claude Code in a folder
  it isn't trusted in, once per folder (Claude Code's own trust on this Mac counts; on a machine, Cue's list,
  `hosted.trusted`).
- Not done: Pi's and Codex's own sign-in and settings screens (`pi auth`, `codex login`), not yet checked headless.

**H6. Remove Cue's terminal.** Hosted becomes how every session starts and `hosted.on` goes away. Removed: the
tmux panel and its code, `Runs in`, Move to Cue (a terminal session comes into Cue hosted), the add-on's screen
and keys calls. Kept: Move to terminal (resume in a terminal app) as the way out, and terminal sessions you
start yourself, which Cue still watches through hooks.

## 10. Open questions

1. ~~Teammates headless.~~ Settled in M1: headless has no separate-terminal teammates; a named agent runs
   in-process and its asks arrive with its `agent_id` (compat suite). Nothing is left to answer in a terminal.
2. ~~Always allow.~~ Settled in M1: the rule goes where its suggestion's `destination` says (`localSettings`:
   the folder's `.claude/settings.local.json`) and holds. Open for M2: whether the button names that place.
3. **Terminal-only slash commands.** `init` lists them (`terminal_slash_commands`). The `/` menu should hide
   or grey them for hosted sessions (not done in M2: the menu is the folder's, as for a terminal session).
   Which daily-use ones are affected?
4. **Codex and Pi** (checked 2026-11-07, from their docs and Pi 0.87.1's `--help`; Codex wasn't installed):
   both can be hosted the same way later. Codex: `codex app-server` (JSON-RPC over stdio: `turn/start`,
   `turn/steer` mid-turn, approval requests answered accept / accept for session / decline, `turn/interrupt`,
   `thread/resume`; tokens but no dollar cost). Pi: `pi --mode rpc` (`prompt`, `steer`, `abort`, `--session-id`);
   Pi has no permission prompts of its own, so Cue's Pi extension keeps the gate and its dialogs arrive as
   `extension_ui_request`. Not in v1.
5. **Cost of resuming.** Resuming after a Cue restart re-reads the conversation. Insights already counts
   "cache written again"; hosted resumes should be counted the same way.
6. **Left for after the trial (M2 doesn't do them):** keep the permission mode a session was started with, so a
   resume uses it; send images as images rather than as paths in the text; pass the session's name to Claude
   (`-n`); say which file an "always allow" writes to.
