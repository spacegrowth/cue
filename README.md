# Cue

One window for every coding agent that needs you. When a Claude Code, Codex or Pi session finishes
its turn, asks permission, or asks a question, it shows up in Cue — so you stop cycling through
tabs to find who's waiting. A Mac app (macOS 13 or later, Apple silicon and Intel).

- **Finished, your turn** — see the agent's last message and **reply from Cue**: it types your
  message into that session's terminal (iTerm or tmux) and presses Enter. Or jump to the tab.
- **Permission** — Allow, Deny, Always allow, or "tell it what to do instead".
- **Question** — pick an option (1–4) or type your own answer.

The terminal keeps working too: answer there or in Cue, whichever is first wins, and Cue clears
the card either way.

## Install

1. Download `Cue-<version>.dmg` from the [latest release](https://github.com/spacegrowth/cue/releases/latest),
   open it, and drag **Cue** to Applications. It's signed and notarized by Apple.
2. Open Cue and click **Connect Claude Code** (Settings has Connect for Codex and Pi too). That adds
   Cue's hooks to `~/.claude/settings.json`; the file is backed up first and your other settings stay.
   Sessions already open pick it up after a restart (`claude --resume` keeps the conversation).
3. The first time Cue jumps to or types into a tab, macOS asks to let Cue control iTerm or
   Terminal: allow it.

Cue updates itself: it checks for a new version now and then, and the menu bar icon's menu has
**Check for Updates…**.

From source instead: `./install.sh` builds the app and connects Claude Code, Codex and Pi;
`./install.sh --uninstall` removes it all again.

## Privacy

Cue collects nothing and sends nothing anywhere. Everything it keeps (what's waiting, history,
drafts, settings) stays on your Mac, in the folder below. It talks to your agents through a local
socket that only processes on your Mac can reach, and it reads their transcripts on your disk. The
one time it goes online is to check GitHub for a newer version of Cue.

## Using it

| Key | |
|---|---|
| `A` / `D` | allow / deny |
| `1`–`4` | pick an answer |
| `R` | reply / redirect (type, then Enter) |
| `G` | go to the session's tab |
| `←` `→` | flip through what's waiting |
| `H` | history |

The **side panel** (top right, always on top) shows up when something's waiting: one click for
small things, ↗ to open the card in Cue for anything bigger. ✕ hides it until something new.

Everything Cue keeps is in `~/Library/Application Support/dev.spacegrowth.cue/` (the standard place
for a Mac app's data; `CUE_HOME` overrides it), in one SQLite file, `cue.db`, so a restart or crash
loses nothing:

| Table | What |
|---|---|
| `items` | decisions and finished turns, pending and answered (the history) |
| `sessions` | every session, live or ended, with who drives it (`driven_by`) |
| `exchanges` | each session's full back-and-forth, append-only |
| `drafts` | what you typed and haven't sent, per session |

Images you send stay as files in `uploads/` there (rows hold their paths).

Settings stay in `config.json` there (the Pi extension reads it). How much history to keep is
set in Settings or there:

```json
{ "history": { "keep": 300 }, "pi": { "gate": "dangerous" } }
```

Pi has no permission prompts of its own; Cue's extension adds them for risky commands —
`pi.gate` is `"dangerous"` (default), `"all"` (every bash/write/edit), or `"off"`.

### Agents that run other agents

If a tool starts agent sessions on behalf of another agent (a lead handing work to helpers), it can
set `CUE_DRIVEN_BY` in each helper's environment, e.g. `CUE_DRIVEN_BY="its lead (billing)"`. Cue
then keeps that helper out of Waiting: its finished turns are the lead's to handle, so it shows as
idle, "waiting on its lead", with no notification. (Settings can show them anyway.)

## How it works

```
Claude Code ──hook──┐                       ┌── window (spotlight card)
                    ├──► cue.sock ─────────► Cue ──┤
Pi ──extension──────┘    (one JSON line)    └── side panel + notification
```

Each harness has a hook that waits for an answer: Claude's `PermissionRequest`, Pi's `tool_call`.
The hook asks Cue over a local Unix socket and waits. Turn-end events (`Stop`, `agent_end`) put a
"finished" card up; the next prompt (`UserPromptSubmit`, `agent_start`) takes it down.

- **Cue not running?** Hooks exit silently and the agent asks in its terminal as usual.
- **Answered in the terminal?** Claude doesn't tell the hook, so Cue watches the session
  transcript and clears the card. Pi's extension tells Cue directly.
- **Cue restarted?** Waiting hooks reconnect and ask again.

The hook is built into Cue itself (`cue hook <event> <harness>`, see `src-tauri/src/hook.rs`), so
it needs nothing installed: no Python, no Node. Adding another harness means one small hook that
speaks the same socket protocol.

## Develop

```sh
npm install
npm run dev                      # run the app
./tests/run.sh                   # all tests, against a private silent Cue (yours keeps running)
python3 -m http.server 8765      # then open /tests/preview.html — the real UI with sample data
hooks/cue-ctl list               # poke a running Cue from the shell (needs python3)
```

## License

MIT. See [LICENSE](LICENSE).
