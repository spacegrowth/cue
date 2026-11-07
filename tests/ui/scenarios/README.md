# UI scenarios

Each `<name>.json` here is the window's whole state: one snapshot, exactly what `hub/mod.rs` `snapshot()`
builds and what `get_state` / the `state` event / a socket client's `list` hands the window. `app.html?scenario=<name>`
loads it through the fake bridge (`tests/ui/bridge.js`).

Two sources, same format:

- `world-*.json`: built by the Rust headless harness (`src-tauri/src/testing/`), each by driving a real hub
  through the same path the socket uses, so the shape can't drift from the app's. Regenerate with
  `cd src-tauri && cargo test --no-default-features write_ui_scenarios -- --ignored`
  (`testing/scenarios.rs` lists them and how each is made; `every_ui_scenario_builds` keeps every builder
  working in the normal test run).
- the others: recorded from the preview's sample world by `tests/ui/record.mjs` (`scenarios/preview.js`).

What the `world-*` files promise, so the screenshots taken from them don't change by themselves:

- The clock is fixed at `now_ms` 1793984400000 (the same instant `record.mjs` uses); every time in the
  file is from that clock, card ids (`n<ms>`) too. Agents take 20 s to answer (`World::agents_think`), so a
  scenario starts that much before per answer and ends at that instant.
- Rows in `sessions` are sorted by `session_id`, and every object's keys are sorted (the hub keeps several
  in hash maps; the window sorts what it shows itself).
- Facts read from the Mac that writes them are pinned: `settings.login.open` false,
  `settings.sessions.tmux_installed` and `settings.sessions.tmux` true, `settings.sessions.terminal` "Terminal", `usage_file` null.
  Transcript paths read `/Users/me/.claude/projects/-work/<session>.jsonl`; folders are `/work/<project>`.

| scenario | what's in it |
|---|---|
| world-empty | nothing at all |
| world-one-ask | one session asking to run a Bash command |
| world-many-sessions | finished, working, asking a question, out of usage, driven by a relay lead, Pi, Codex |
| world-relay-review | a working relay lead and two reported executors, one review queued for the lead |
| world-status-list | a finished turn ending in a status list, its prompt carrying the status ask |
| world-parked | one parked session and one under Closed |
| world-restarting | a session restarting for its MCP servers (step "start") |
| world-stalled-terminal | a message typed into a pane that never took it (ring on), and a prompt only its terminal can answer |
| world-long-names | a long folder, a long name, a long prompt and a long answer |
| world-fifty-sessions | 50 sessions over 7 projects, every third finished |
| world-handoff-pending | a session that sent /handoff and hasn't continued |
| world-settings | Finished-turn notifications off, History kept to 50 |
| world-four-choices | one session asking a question with four choices (the 1–4 keys) |
