# The agent host — design

Status: **v2, 2026-11-07, not built.** Steps H3 (this Mac) and H4 (other machines) of `cue-hosted-sessions.md`
§9. v1 was reviewed independently against the code; §11 lists what the review changed.

## 1. Problem

A hosted session's agent (Claude Code, Pi, Codex) is a child of the Cue app today (`hosted::Running` →
`hosted::Session`, its stdin/stdout piped to the app):

- **On this Mac**, quitting, updating or a crash of Cue ends every hosted agent. The conversation is saved and
  the next message resumes it, but a turn in progress is cut off, and an ask it was about to make is lost.
  tmux keeps terminal sessions alive; removing Cue's terminal (H6) needs something that does the same for
  hosted ones.
- **On another machine**, sessions run in tmux there and reach Cue through a chain: Cue types into tmux over
  SSH; a POSIX hook script drops each event as a file in `~/.cue/in` and waits for a permission answer in
  `~/.cue/out`; one SSH stream per machine watches those folders; one `tail -F` per session copies its
  transcript back. When a link is down as the agent asks (the stream reconnecting, the Mac asleep, Cue
  restarting), the script gives up and the ask goes to the terminal on the machine, where nobody sees it.

Piping a remote agent's stdio straight through `ssh` removes the typing and the folders but keeps the worst
failure: the agent is a child of the SSH session, so a dropped connection ends it mid-turn.

## 2. Goal

One small program per machine, the **agent host**, owns hosted agents and outlives Cue and the connection to it:

- A turn runs to its end whether or not Cue is connected.
- Everything an agent says while Cue is away is kept; Cue gets it on reconnecting, in order, and handles it once.
- An ask made while Cue is away waits (never answered for you, never sent to a terminal) and is a card as soon
  as Cue is back.
- Nothing listens on the network; other machines are reached only through your own SSH.
- The host changes rarely: what the agents' protocols mean stays in Cue, which updates with the agents.

Non-goals: agents for anyone but you; machines without SSH; a general process manager.

## 3. Shape: a durable pipe, not a second Cue

```
 Cue app                                                         agent host (one per machine)
 hub ─ Hosted port ─ Hosts (router, by machine) ─ HostClient ──► socket | ssh <machine> cue-agent-host connect
         per session: Protocol (claude / pi / codex)                ├─ child: claude -p …        raw lines, seq'd
                                                                    ├─ child: pi --mode rpc       and kept on disk
                                                                    └─ child: codex app-server
```

The host knows nothing about Claude Code, Pi or Codex. It starts a command it's given, writes lines to the
child's stdin, and keeps every line the child writes (stdout, and stderr as its last words) in a per-session
log with a sequence number, replaying it to Cue after any point. **All protocol knowledge stays in Cue**
(`hosted/protocol.rs`, `translate.rs`, `wire.rs`, `pi.rs`, `codex.rs`), so a new Claude Code, Pi or Codex
version means a Cue update, never a host update, and the host's own protocol almost never changes.

### 3.1 Code

- `crates/agent-host` (new, binary `cue-agent-host`): the host. Process supervision (today's
  `hosted/session.rs` process half: spawn in a session and process group of its own, close → grace → TERM →
  KILL, drain, last words), the per-session line log, the socket server, `connect`. Depends on `serde_json`,
  `libc`. No Tauri, nothing from the app. Built for macOS (inside Cue.app, codesigned with it) and for Linux
  x86_64/aarch64 (static musl).
- The app: `hosted::Session` becomes a session *through a host*: it builds the command line as `launch.rs`
  does today and holds the `Protocol`, feeding it the lines the host relays. `Running` is replaced by `Hosts`
  (§5). The protocols and their fixtures don't move.

### 3.2 One host per machine per user

- Home: `~/.cue/host/` on a machine, Cue's data folder `host/` on this Mac (0700). Holds `host.sock` (0600),
  `host.pid`, `host.lock`, `host.log`, `sessions/<id>/log.jsonl`.
- **Started on demand.** `cue-agent-host connect` connects to `host.sock`; if nothing answers (no socket, or a
  stale one whose `host.pid` isn't alive) it takes `host.lock`, checks again, starts `serve` (double fork,
  `setsid`, **every inherited fd closed** so sshd never waits on it, stdio to `host.log`), waits for the socket,
  then copies bytes between its own stdio and the socket. The server checks the peer's uid
  (`LOCAL_PEERCRED`/`SO_PEERCRED`) besides the file modes.
- **Idle exit**: with no running child and no client for 10 minutes, it unlinks `host.sock` *first*, then exits;
  a `connect` racing that finds no socket and starts a fresh host under the lock.
- **Its children** run in sessions of their own, as today. If the host dies, a child's stdin closes and it
  ends; its conversation stays on disk and resumes on the next message. Rare, logged. Cue never kills a host
  by heuristic (no `ps` matching on machines); a host whose Cue is gone is the point.
- Commands run through the login shell as today, with `-lc` (no `-i`: with no terminal, bash's job control
  warnings would fill the last words).

### 3.3 Versions

The host's protocol is small and stable (§4). `hello` carries both versions. A newer Cue meeting an older host
uses the old protocol if it can; if it can't and the host has running children, those sessions keep running on
it and new starts wait until it's idle, when Cue asks it to exit and starts the new one. A turn is never cut off
for an update.

### 3.4 Installing on a machine

On first hosted use of a machine: `uname -sm`, pick the build, send it with `machines::run_with_bytes` over the
shared connection to `~/.cue/bin/cue-agent-host-<version>` (temporary name, then `mv`, `chmod 700`), verify its
SHA-256 (`sha256sum`, or `shasum -a 256` on a Mac), point `~/.cue/bin/cue-agent-host` at it; remove old
versions no host of theirs runs. Settings → Machines says Cue puts this one program in `~/.cue/bin` (the owner
agreed). A machine where it can't go (no build for its architecture, read-only home) keeps tmux sessions and
isn't offered Hosted.

## 4. The protocol between Cue and the host

JSON lines over the socket, or over `connect`'s stdio through SSH. Requests carry Cue-made ids and get one
response each.

| request | what |
|---|---|
| `hello {version}` | its version, and its sessions: id, running or exited (and exit code), last `seq`, log start |
| `start {session, argv, env, cwd}` | start a child under that id. **An id already running is refused**, never restarted: a second Cue, or a start racing a reconnect, must not kill a turn |
| `write {session, line, msg}` | a line to its stdin; `msg` (Cue's message id) lets the host refuse a repeat: it keeps the last 100 per session |
| `attach {session, after}` | its log after `seq`, then live lines; `{seq, out \| err \| exited}` |
| `close {session}` | close the child (as today: input closed, grace, TERM, KILL) and keep the log until Cue forgets it |
| `forget {session}` | drop the log (Cue closed or archived the session) |
| `tail {path, from}` | stream a file's appended lines (a remote transcript, §6) |
| `rekey {from, to}` | a session started under Cue's id is known by the agent's own (Codex's thread id) |

There is **no acknowledgement**: Cue's saved `seq` per session is the only cursor. The log is bounded by size
(§4.1), so nothing needs the host to know what Cue has handled, and two clients can't drop each other's events.

### 4.1 Nothing lost, handled once

- The host appends each line to `log.jsonl` before relaying it. A torn last line (crash mid-append) is skipped
  on reading. Bound: 50 MB per session; tool output inside a line is never cut (the host doesn't parse lines),
  so the bound is generous; past it the oldest lines go and `attach` says from where it starts, so Cue knows
  it missed some and reads the transcript for them.
- Cue saves per session the last `seq` it handled, **with a snapshot of its `Protocol` state** (Claude's
  pending tool inputs and in-turn flag, Pi's dialogs, Codex's thread and turn ids, counters) **and the cost
  base** (today `run_hosted` takes it at start; an attach has no start). Saved in the same write as the
  session, so the cursor and the state never disagree. On reconnect: restore the snapshot, attach after `seq`.
  A line at or below the saved `seq` is ignored, so a replay is harmless.
- **Asks** are lines like any other, so a waiting ask replays with the rest. The hub keys asks by the agent's
  request id per session (`Item.request_id`, built): `add_keyed_ask` with an id whose card is still open makes
  no new card; the open card's waiter answers it. (In memory only for now: H3b saves it with the session, and
  a reattached card gets a new waiter.)
- An answer to an ask the agent no longer waits for is refused by the agent's own protocol (Claude ignores a
  stale `control_response`; Codex/Pi answer an error), and the card says so.

### 4.2 While Cue is away

The child runs as if nothing happened. Nothing can be sent (Cue isn't there). A message Cue sent whose `write`
it never saw answered is resent on reconnect with the same `msg`; the host refuses it if it got it.

## 5. Cue's side

- **`Hosts`** implements `Hosted`: one `HostClient` per machine (this Mac is one), chosen by `Spec.machine`
  (new; from `Origin.machine`). It reconnects each client with backoff (1 s → 30 s).
- **`running()`** answers *yes / no / unknown*; while a client is disconnected it's unknown, and the hub
  doesn't wake (start) a session in that state: its sessions show *"reconnecting to <machine>"*, and a send is
  refused with that reason (Send again), never queued silently.
- **Disconnecting is not exiting.** `HostClient` never reports `Exited` because a connection dropped (that would
  finish asks as gone and unsend messages); only the host's `exited` line does.
- On connect: `hello`, then `attach` each session Cue knows after its saved `seq`. Sessions the host has and
  Cue doesn't are logged and closed after a day.
- Quitting Cue just disconnects: `close_all` on exit, `end_left` and `end_leftovers` go (the host keeping
  agents alive is the point). Close and Park close the child in the host, as today.
- `FakeHosted` changes with the trait (attach/reconnect semantics; `running` is three-valued) and gains a
  "disconnected" switch for World tests.

## 6. Other machines

- The machine's shared SSH connection (ControlMaster) carries `ssh <machine> ~/.cue/bin/cue-agent-host connect`.
  One SSH session per machine carries everything: sessions, and transcript copies (`tail`), instead of one
  `tail -F` session per transcript (sshd allows 10 sessions per connection by default, already shared with the
  watcher).
- **Remote hook script**: hosted agents must not report through it. The machine's `~/.cue/cue-hook` gets the
  same check the Mac's hook has: `[ -n "$CUE_HOSTED" ] && exit 0`, shipped by the existing refresh in H4a.
- **Pi's gate** (built): hosted, Cue's Pi extension asks through one of Pi's own dialogs, never Cue's local
  socket (unreachable from another machine): an `input` dialog titled `cue:ask` with the ask (kind, tool, input)
  as JSON in its placeholder, answered with Cue's decision as JSON; cancelled is a denial. `pi.rs` makes it a
  permission or question card; any other extension's dialog stays a question or form card. Its `ask_user` asks
  the same way. Still on the socket when hosted: its "doing" notes and its `/` command list (H4b).
- Agent versions: hosting needs Claude Code's `--permission-prompts host`, Pi's RPC mode, Codex's app server.
  Before the first remote start Cue runs the compatibility suite's version checks there and refuses with a
  reason ("Claude Code 2.1.302 on build-box is too old for hosting: update it there").

## 7. Failure cases

| what happens | what you see |
|---|---|
| Cue quits or updates mid-turn | the agent carries on; on launch Cue attaches and shows what happened meanwhile |
| the Mac sleeps / network drops, remote session mid-turn | the turn finishes there; on reconnect its reply and any asks appear |
| a start or wake races a reconnect | refused by the host (already running); Cue attaches instead |
| two Macs on one machine | both get every line; the first answer to an ask wins, the other card closes as answered elsewhere |
| the machine reboots | host and children gone; sessions paused; the next message starts a host and resumes |
| the host crashes | children end; sessions paused; next message resumes; `host.log` says why |
| disk full on the machine | lines relayed live but not kept; Cue is told; sessions keep running |

## 8. Testing

- `agent-host` in-process: log append/replay after `seq`, torn last line, size bound and the "starts at"
  notice, `start` refused for a running id, `write` dedupe by `msg`, idle exit with unlink-first and a racing
  `connect`, peer uid check, `rekey`.
- Cue: `Protocol` snapshot round trip for all three agents (state after the fixtures = state restored), a
  reconnect that replays an open ask makes no second card, cost after attach, `running()` unknown while
  disconnected (no wake, sends refused), no `Exited` on disconnect.
- End to end on this Mac with the fake Claude: disconnect mid-turn, lines while away, reconnect, nothing lost or
  doubled; Cue relaunch mid-turn; host killed; seq carries on across a resume.
- Remote: the same suite over `ssh localhost`, and the existing SSH chaos tests (`testing/chaos`) with a drop
  mid-turn; then live on a real Linux machine with Claude Code, Pi and Codex: kill the SSH connection mid-turn,
  sleep the Mac, restart Cue, reboot the machine. Run the compatibility suite there first.

## 9. Decisions on the open questions

1. **Remote transcripts**: a `tail` request on the host connection, written to the same local copy Cue's readers
   use today; not in the seq log (different lifetime, large), and no longer one SSH session each.
2. **Linux builds**: prebuilt static musl binaries, cross-built in the release script (`cargo zigbuild` or
   Docker), SHA-256 verified on install. Never built on the machine.
3. **This Mac**: on demand only, no launchd agent; idle exit covers it, and a Mac reboot ends agents either way.
4. **Two Macs on one machine**: several clients allowed, no acks, first answer wins.
5. **Log bound**: by size, 50 MB per session.
6. **Updates**: the host rarely changes (protocols live in Cue); when it does, running sessions finish on the
   old host, and it's replaced once idle.

## 10. Milestones

| | what | done when |
|---|---|---|
| H3a | Pi's gate through RPC when hosted; `Item.request_id` and `add_ask` dedupe; `Protocol` snapshot/restore | tests green; hosted Pi asks are cards with no socket |
| H3b | `cue-agent-host` + `Hosts`/`HostClient` on this Mac, behind `hosted.on` | Cue restart mid-turn: the turn finishes and shows after relaunch; live with all three agents |
| H4a | Linux builds, install over SSH, `connect` and `tail` over the shared connection, remote hook skips hosted | a hosted Claude session on a Linux machine survives killing the SSH connection mid-turn |
| H4b | Pi and Codex remote, version checks, Settings → Machines wording | live on a real machine with all three |

## 11. What the review changed (from v1)

- The host no longer runs the agents' protocols (v1 put them in a shared library inside it): it keeps raw lines,
  so agent updates never need a host update, and v1's version dance mostly goes away.
- `ack` dropped: Cue's saved `seq` is the only cursor; the log is bounded by size.
- `start` refuses a running id; `running()` is three-valued; a disconnect never reports `Exited`.
- v1 said `add_ask` already ignores a known request id: it doesn't; `Item.request_id` and dedupe are new.
- Pi's gate moves into Pi's RPC (it used Cue's local socket, unreachable remotely); the remote hook script skips
  hosted agents.
- Per-machine routing (`Hosts`, `Spec.machine`) made explicit; `FakeHosted` changes.
- Daemon details: close inherited fds, unlink before idle exit, peer uid, `-lc`.
- Cost base and `Protocol` state saved with the cursor; transcripts over one connection; size-bounded log.
