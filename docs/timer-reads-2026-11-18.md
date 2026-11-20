# What Cue reads on a timer (measured 2026-11-18)

Installed Cue (d4e54b92), today's sessions open (98 sessions with transcripts, 941 MB of them), measured for about 4.5 minutes.
Nothing was changed.

## How it was measured

- `fs_usage` and `dtrace` need root, and `sudo` asks for a password here, so neither could run. Instead:
- `/usr/bin/sample <pid> 180`: a stack every 1 ms on every thread for 3 minutes. Each sample on a thread counts as 1 ms of that thread's time.
- `proc_pid_rusage` (bytes read and written to disk, CPU time) at the start and end of 274 s.
- The timer loops in `src-tauri/src` read alongside, to name what each stack is doing.

## The whole app

| | per minute |
|---|---|
| CPU (all threads, including the web view's work in the app's own process) | about 11 s, so 18% of one core |
| Read from disk (not from the page cache) | 1.6 MB |
| Written to disk | 31 MB (mostly `cue.db` and its WAL: live session writes) |

Almost all reads come out of the page cache, so the disk number is small. The cost is CPU: reading whole files again and parsing them.

## Each timer

Time is thread time from the sample (1 ms per sample), turned into a per-minute rate. "Waiting on git" is time a Cue
thread sat waiting for a `git` child process. Git's own CPU counts against git's process, not Cue's.

| What | How often | What it reads | Per minute |
|---|---|---|---|
| **Desk finish, while an item waits to commit** (`desk_finish_tick` → `finish_commit` → `own_of` → `theirs`/`read_edits`, `desk/hub/landing/finish.rs`, `desk/hub/theirs/theirs.rs`) | every 10 s (`usage_tick`, every 10th 1 s tick) | **every** session's whole transcript on this Mac, plus its agent logs, read into memory and searched line by line (up to about 940 MB each pass today). Then `git status`, `git log -1` and `git show HEAD:` for each changed file | **2.6 s CPU** reading and searching, **3 s waiting on git**. This is the biggest timed cost in Cue. It runs only while an item sits in `commit`/`commit_wait`. |
| Session activity (`refresh_activity`, `hub/activity.rs`) | every 1 s | the new tail of each working session's transcript (`activity_of`), and the agent logs of open feeds (`agent_count` → `Plan::fetch` opens each agent's file) | 0.8 s CPU |
| Open terminal's screen (`session_screen`, asked by the window) | every 2 s while a terminal is open | `tmux capture-pane`, not a file | 0.9 s, nearly all waiting on tmux |
| What a session is about (`read_about`, `logs/about.rs`) | each listed Claude transcript again once 5 s (`AGAIN_MS`) have passed | the transcript | small: under 0.1 s in the sample |
| Steps of the open chat (`followSteps` → `loadSteps`, `ui/chat/thread.js`) | every 500 ms while working, at least every 2 s otherwise | the open session's transcript from its last offset, plus agent logs (`edited_between`, `agent_logs`) | about 0.3 s CPU |
| Window redraw (`hub/redraw.rs`) | up to every 200 ms, only after a change | no files: it builds and sends the state | 0.3 s CPU |
| Settings file (`config.rs`) | every 2 s | `config.json` mtime | too small to show |
| macOS facts (`config.rs`) | every 60 s | system settings | too small to show |
| Turns log (`logs/turns.rs`) | every 600 s, every 20 s while behind | transcripts since the last turn read | not seen in this window |
| Remote cursors (`hosted/remote.rs` `Cursor::save`) | on each remote line, not a timer | writes a small JSON file each time | 0.04 s |
| Waiting asks (`settle_the_gone`, `server/mod.rs`) | every 1 s | `kill -0` and the transcript of each session with an open ask | too small to show |

The other threads that show up (`cue-snapshot-timer`, `cue-notify`, the lock watchdog) spend the whole window
asleep. The mini panel's 700 ms / 120 ms loop looks at the mouse, not at files.

## What `notify` file events and an in-app bus would remove

Cue would subscribe to the files it already knows (each session's transcript and agent logs, `config.json`, the
item folders) and pass "this changed" messages over a bus inside the app.

**Would go away**
- **The Desk finish re-reads, the big one.** Every 10 s it reads all transcripts in full. With events it would read a
  transcript only when that transcript grows, and only the new bytes (keeping each session's edits found so
  far). It would re-check `git status` only when a file in the item's folder changes. This removes nearly all of
  the 2.6 s/min of CPU and most of the 3 s/min waiting on git.
- The 1 s activity read of working transcripts and the agent-log opens in `agent_count`: one read per append instead of
  one per second per session.
- The window asking for steps every 500 ms / 2 s: the hub would push new steps over the bus when the transcript
  grows, so the window stops polling.
- `read_about`'s 5 s re-read, the `config.json` check every 2 s, and the turns log's 20 s catch-up.

**Would stay** (no file to watch, or the clock is the trigger)
- The tmux screen of the open terminal: it isn't a file. tmux control mode could push it, but that is a separate change.
- `kill -0` checks that a session's process is still alive.
- Clock-driven timers: cache-expiry warnings, send-at, usage-limit resets, the 60 s macOS facts.
- The mini panel's mouse tracking.

## Measurement limits

- Without `fs_usage` there is no per-file byte count. The bytes per pass above come from the code and the
  transcripts' sizes, not from tracing.
- No item was waiting to commit when the list was read afterwards, so how long the Desk finish loop ran during the
  sample isn't known exactly. 2.6 s/min is an average over the window, and a pass that runs the whole minute costs more.
