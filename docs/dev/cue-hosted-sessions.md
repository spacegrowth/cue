# Cue-hosted sessions — design

Status (2026-11-07): **M1 built** on branch `hosted-m1` (the session client and its tests, §4.1, §5); M2, the hub
wiring, is next.

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

**Not in v1:** the agent host process (§8 M4), remote machines, Codex/Pi, starting relay executors as hosted
sessions, removing the built-in terminal panel. Terminal sessions keep working exactly as today throughout.

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
Hosted sessions get a third:

```rust
pub trait Hosted: Send + Sync {
    fn start(&self, spec: HostedSpec) -> Result<Origin, String>;   // new conversation
    fn resume(&self, session_id: &str) -> Result<(), String>;      // after a Cue restart or an exit
    fn send(&self, session_id: &str, text: &str) -> Result<(), String>;
    fn answer(&self, session_id: &str, request_id: &str, a: Answer) -> Result<(), String>;
    fn interrupt(&self, session_id: &str) -> Result<(), String>;
    fn close(&self, session_id: &str);
}
```

`testing::World` gets a fake (records sends and answers, emits scripted `HostedEvent`s), like the other ports.

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

`Origin` gets one explicit field, `hosted: bool`. It decides only three things: replies go through
`Hosted::send` instead of `Keys`, Escape goes through `Hosted::interrupt`, and Go to tab / the terminal panel
aren't offered. None of today's terminal code changes for terminal sessions.

**Cue's own hooks:** a hosted session still runs your hooks (relay's, the iTerm status hook, …), and they see
`CUE_HOSTED=1` (compat suite). Cue's `cue-hook` would report the same events twice, so when it's set the hook
reports nothing to Cue; what it adds to the session's own prompts (the status list, a voice message's context)
still runs. Built in M1 (`hook.rs`).

### 4.4 Lifecycle

- **Start:** `+ New → Cue-hosted session` (folder, model; your normal settings and permission mode apply).
  Cue picks the session id up front (`--session-id`).
- **Cue quits:** stdin closed, process group ended. A turn in progress is lost; the conversation isn't.
- **Cue starts:** hosted sessions that hadn't ended show as *paused* in the Board. The first message you send
  resumes them (`--resume`), not before: resuming re-reads the conversation (cache cost), so nothing is resumed
  that you don't use.
- **Claude exits unexpectedly:** `Exited` → card with Resume. Garbage or unknown lines are logged, never fatal.
- **Unprompted turns:** a background task finishing can start a turn with no message from you; it shows up as
  any finished turn does.
- **Move to Cue** (a terminal session's button) keeps its rule: offered once the session isn't working, it
  ends the session in its tab, then resumes it, as a hosted session from M2 on (in Cue's tmux until then). The
  terminal copy must be gone first: two processes on one conversation would both write its transcript.
  Its history needs no conversion: a terminal session and a hosted one write the same transcript file in the
  same format, so the earlier turns render from the transcript as a terminal session's chat does today, and the
  new ones arrive live (and are appended to that file too). Compat suite: a resume appends to the same file.
- **Move to terminal** (the reverse, the escape hatch): a hosted session that isn't working is closed and
  resumed with `claude --resume <id>` in a new iTerm tab, where it becomes an ordinary terminal session.

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
4. **Hub wiring (M2):** `World` cases for every row of the §4.3 table, plus the stability plan's matrix rows
   for asks and sending, run with a hosted origin.

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
| **M2** | `Hosted` port + World fake, hub wiring (§4.3), `input` card, `+ New → Cue-hosted session`, paused/resume | next (Phase 4 has merged) | World cases green; one real day of daily use with no answer-in-terminal card on a hosted session |
| **M3** | relay executors as hosted sessions (relay asks Cue to start them instead of opening iTerm tabs) | after M2 holds up | an executor packet runs end to end hosted |
| **M4** | the agent host: processes move out of the app so a Cue restart or update no longer stops running work | when restarts interrupting work are felt | Cue restart mid-turn: the turn finishes, the app picks it back up |
| **M5** | retire the built-in terminal panel and Cue's own tmux, once nothing in daily use needs them | last | — |

## 9. Open questions

1. ~~Teammates headless.~~ Settled in M1: headless has no separate-terminal teammates; a named agent runs
   in-process and its asks arrive with its `agent_id` (compat suite). Nothing is left to answer in a terminal.
2. ~~Always allow.~~ Settled in M1: the rule goes where its suggestion's `destination` says (`localSettings`:
   the folder's `.claude/settings.local.json`) and holds. Open for M2: whether the button names that place.
3. **Terminal-only slash commands.** `init` lists them (`terminal_slash_commands`). The `/` menu should hide
   or grey them for hosted sessions. Which daily-use ones are affected?
4. **Codex and Pi** (checked 2026-11-07, from their docs and Pi 0.87.1's `--help`; Codex wasn't installed):
   both can be hosted the same way later. Codex: `codex app-server` (JSON-RPC over stdio: `turn/start`,
   `turn/steer` mid-turn, approval requests answered accept / accept for session / decline, `turn/interrupt`,
   `thread/resume`; tokens but no dollar cost). Pi: `pi --mode rpc` (`prompt`, `steer`, `abort`, `--session-id`);
   Pi has no permission prompts of its own, so Cue's Pi extension keeps the gate and its dialogs arrive as
   `extension_ui_request`. Not in v1.
5. **Cost of resuming.** Resuming after a Cue restart re-reads the conversation. Insights already counts
   "cache written again"; hosted resumes should be counted the same way.
