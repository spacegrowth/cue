# Teams in hosted sessions — design

**Named Crew in the UI (2026-11-09):** the switch, the group ("Crew · N"), `/crew`, its messages and the
`crew/<name>` branch say Crew, so it isn't confused with Claude Code's own "agent teams". The code, its files and
this doc keep "team"; the words to Claude keep "teammate", Claude Code's own name for a named agent.

Status (2026-11-09): **built** (T0–T6; T6 removed Cue's relay integration). §11 says what
shipped beyond the first plan and the answers that were settled on the way. Builds on `cue-hosted-sessions.md` (hosted sessions, §9 H1–H6) and
`cue-agent-host.md` (the agent host). It replaces that plan's M3 ("relay executors as hosted sessions"): with this,
Cue needs no relay integration at all.

## 1. What and why

A lead session hands work to other agents and reviews what they did. Today Cue supports that through relay: relay
opens each executor as its own terminal session, passes work as packet files, wakes the lead with hooks, and Cue
reads relay's ledger to show who is doing what. That is a second system beside Claude Code's own agents, and it
broke in ways this week showed: an executor closed for idleness before anyone reviewed it, two sessions committing
over each other's uncommitted work, a Review button that froze Cue.

Claude Code now does most of this itself. A lead starts **teammates** with its Agent tool: each a named agent with
its own context and model, run in the background, messaged by name with `SendMessage` (which resumes it with its
context), and the lead is told when one finishes. In a hosted session Cue owns the lead's process, so it sees all
of this as it happens: every line a teammate writes and every ask it makes arrives on the session's stream, labelled
with the agent it came from.

**Teams are first-class in hosted sessions, and only there.** A hosted lead's chat shows its team; each teammate's
state, current step, asks and changes are Cue's own, with no hooks, packet files or ledger. Terminal sessions
(iTerm, Terminal) stay watched sessions without a team view. Cue's own terminal goes away as planned (H6), and with
this, Cue's relay integration goes too. The relay tool itself is not touched; it can keep running outside Cue.

## 2. What the CLI does

Known from the hosted client (`hosted/translate.rs`):
- Each stream line carries `parent_tool_use_id`, the Agent tool call that started the agent it comes from (none
  for the lead's own lines); `translate.rs` already turns it into `agent: Some(tool_use_id)` on Said, ToolCall and
  ToolResult. Cue can file every line under its teammate (fixture `hosted/fixtures/named_agent.jsonl`).
- A teammate's permission ask carries its `agent_id` (`compat_tests.rs`: "asked from inside the agent, and named
  so"). Its card can say which teammate asks.
- Headless has no separate-terminal teammates: a named agent runs inside the lead's process
  (`cue-hosted-sessions.md` §2, M1).

Checked against the real CLI in T0, each with a recorded fixture and a test in the compatibility suite:
1. **Live activity: yes.** A background teammate's lines stream while it works, not only when it ends; each
   carries `parent_tool_use_id` (the Agent call that started it), so Cue files it under its teammate as it arrives.
2. **Its name: at `task_started`.** A `task_started` line with `task_type: local_agent` carries `tool_use_id` (the
   Agent call, which has the `name`) and `task_id` (the teammate's `agent_id`). From there a row is titled by name,
   and its asks and a later `SendMessage` (both keyed by `agent_id`) find it.
3. **Resume: reaches it, doesn't replay it.** After `--resume`, `SendMessage` to an earlier teammate still reaches
   it, but its earlier lines are not replayed on the stream. So Cue keeps the team itself: each teammate's name,
   state, files and summary are on the lead's record (`sessions/team.rs`) and survive a restart.
4. **Worktrees: under the lead's folder.** A teammate started with `isolation: "worktree"` works in
   `<lead cwd>/.claude/worktrees/agent-<agent_id>`. Cue reads its changes there with git.

## 3. Folders: Cue never forces a worktree

The lead decides; the team skill (§6) asks it for `isolation: "worktree"` by default. Cue works either way:

- **Same folder as the lead** (the default). Teammates edit the lead's checkout. Cue knows which files each one
  changed from its own Edit, Write and NotebookEdit calls on the stream (a teammate's Bash can change files too:
  see §7).
- **A worktree** (the lead passed `isolation: "worktree"`). The teammate works in
  `<lead cwd>/.claude/worktrees/agent-<agent_id>` (§2); its changes are that worktree's diff, shell writes included.

Either way a teammate has **its files**: the list Cue shows, reviews and commits.

**Two teammates on one file.** In the same folder, Cue marks both rows ("also changed by reader") and holds Commit
on both until you choose. The team skill (§6) tells the lead to give teammates separate files; the mark catches it
when that slips.

## 4. What you see

A hosted lead's chat holds its **team**, a compact group under the title (the same group the review rows use,
capped in height, scrolling past a few rows). One row per teammate:

| State | Row | Buttons |
|---|---|---|
| working | name · what it is doing now (its current step) · files so far | Stop, ⋯ |
| asks | name · its question, as a card in the lead's chat titled with its name | the card's own answers |
| finished, not reviewed | name · finished · its summary line · N files +A −D | Review, Diff, ⋯ |
| reviewing | name · reviewing… | Diff, ⋯ |
| reviewed | name · reviewed: commit · N notes › | Commit, Send fixes, ⋯ |

- ⋯ holds Diff (when not on the row), Message (types to the teammate through the lead), What it was asked, Set aside.
- Clicking the name opens the teammate's own steps, the same way a background agent's view opens today.
- A committed or set-aside teammate leaves the group. A finished one stays until you act: nothing closes it for
  being idle.

Mock: Cue Teams design canvas, https://claude.ai/artifact/AVfeQHx1NVTVqmDrrsKfyV. The six UX directions for a day of teams: Cue UX Directions canvas, https://claude.ai/artifact/1Awsm1whUpdgFbDw6JvL2N.

## 5. Review and commit

- **Review** asks the lead to review that teammate's work through a fork (as `/relay:review` does today, without
  relay): read the teammate's files' diff against what it was asked, run its tests, return numbered findings and a
  recommendation. Cue sends it only when the lead is free, one at a time (`Hub::review`'s queue, kept).
- The findings come back as the lead's reply; Cue files them under the teammate (the reply to a Review request is
  its review) and the row turns **reviewed**.
- **Commit** commits that teammate's files only, `git commit -- <its files>`, with a message from what it was asked
  and its summary (in its worktree when it has one). Held while another teammate has uncommitted changes in one of
  those files. Never on the main line: when the folder is on main, master or a detached HEAD, Commit offers **Make a
  branch and commit** instead (§11).
- **Send fixes** sends the findings back to the teammate (`SendMessage` through the lead); the row goes back to
  working.
- Pushing stays yours, as today.

## 6. The team skill

Relay's lasting value is a habit, not machinery, and Cue knows who is who from the stream, so the skill carries
none of relay's ceremony: no packet or report files, no status formats, no review gates, no name prefixes or role
tags (lead and teammate are told apart by Cue's layout, never by text in a name). Cue appends it to every hosted
Claude Code lead's system prompt at launch (`--append-system-prompt`; when your own launch options already append a
prompt, the skill goes after your text in the same option; with `--append-system-prompt-file` it is left out). Pi
and Codex leads get nothing: they have no Agent tool. The text, as shipped (`hub/team_out.rs` `TEAM_SKILL`):

```
# Working as a team lead (Cue)
You run inside Cue, which shows each teammate you start under you; the user looks over and commits their work there.
- When a task has two or more independent parts (different files, no part needing another's result) and each is real work, not a one-line change, don't do them yourself: start one teammate per part with the Agent tool, each with a short plain `name` for its part (like "parser tests"), `run_in_background: true` and `isolation: "worktree"`.
- One teammate per part, never two on the same files. Tell each what to do, which files are its own, and not to commit.
- Don't set a `model`: teammates run on yours.
- Then wait: you're told as each one finishes. When all have, say in plain words what each did. Don't commit their work.
- One part, small quick changes, or parts that share files: do it yourself.
- A message that ends with "(Team: on …)" or "(Team: off …)" is the user's choice for that message.
```

Two choices changed from the first plan: there is no model rubric (teammates inherit the lead's model and are never
put on a cheaper one), and there is no fixed summary format (a plain-words summary only). The skill asks for
`isolation: "worktree"` by default, so a teammate's changes are its worktree's diff and never land in your checkout
until you commit them.

## 7. Where it falls short, and what we do

- **Bash edits.** A teammate that changes files with a shell command leaves no Edit call. Cue credits them anyway,
  preferring ground truth to parsing: a teammate in a worktree of its own is credited with whatever `git status`
  shows changed there (read-only git, off the watcher's tick thread, after each of its Bash calls and when it
  finishes). A teammate in the lead's folder gets a best-effort read of its commands (redirects, `tee`, `sed -i`,
  `mv`/`cp` targets, `git checkout`), marked *via shell*; files read that way which turn out unchanged when it
  finishes are let go. Shell-written files count everywhere Edit-written ones do: its row, the same-file mark and the
  hand-out overlap preview.
- **Handoff.** A teammate lives inside its lead's process. The agent host keeps it alive through Cue restarts, but
  a handoff to a fresh lead starts a new process. Cue carries the team over as plain facts (name, what it was asked,
  files, worktree, state), with no relay-style memo: the rows move to the successor and leave the old lead, so a
  teammate shows once and is committed from one lead only. The successor can Review and Commit the earlier
  teammates' work. Work in progress on a teammate that was mid-turn is lost with the old process; its files stay.
- **Other machines.** A teammate runs where its lead runs. A hosted lead on another machine (H4) brings its team
  along with nothing extra: the same stream, through cue-host there.
- **Context.** Each teammate's final summary lands in the lead's context. The skill keeps it to a few lines; the
  detail stays in its steps.

## 8. What goes away

- Cue's relay code (gone in T6): the relay and pi-lead scan and index (`leads/`), the relay review rows
  (`ui/reviewcard.js`) and their inputs, the LEAD / RELAY tags and auto switch, the packet and lead cards, the lead
  trees on the board and in Sessions, relay's handoff and Start/Stop relay, and relay's counts in Insights. A
  session relay starts is a plain session to Cue.
- Cue's terminal (H6, already planned).
- Kept and reused: the review queue (`hub/reviews.rs`, teammates' only now), set aside, the background agent view.
  Left in the database (dropping them would need a migration): the `set_aside` table and `turns.relay`, no longer
  read or written.

## 9. Steps

| | What | Done when |
|---|---|---|
| **T0** | Check §2's four questions against the real CLI; record fixtures | a fixture and a test per question, answers written into §2 |
| **T1** | The hub keeps a team per hosted lead: teammates from the stream (name, state, current step, asks, files) | World tests: start, live step, ask, finish, same-file mark, resume |
| **T2** | The team group in the lead's chat; asks titled with the teammate | headless render matches the mock; live run with two teammates |
| **T3** | Review, Commit (its files only), Send fixes, Set aside on teammates | an end-to-end run: hand out, review, commit, with a second teammate's work left untouched |
| **T4** | The team skill, loaded by hosted leads | one real feature built with a team, no relay |
| **T5** | Handoff carries the team | a handoff mid-team; the successor reviews and commits the earlier teammates' work |
| **T6** | Remove Cue's relay integration | **done 2026-11-09**: Cue reads nothing of relay's; relay keeps working on its own |

T0–T5 are built: §2 holds T0's answers; T1–T3 the team on the lead's record, its group and the row actions; T4 the
skill (§6) with the Team switch; T5 the handoff carry (§7). T6 removed Cue's relay integration (§8).

## 10. Open questions

1. A teammate asking while you are not in its lead's chat: show its ask in the waiting list like any session's,
   titled "<lead> › <teammate>"?
2. Teammates of a non-hosted lead (iTerm): show only a count, or nothing?
3. Should Commit offer to commit several reviewed teammates at once?

## 11. Built: settled answers and what shipped since the plan

**First cut, then full scope.** T0–T3 went in first: T0's answers (§2), the team kept on the lead's record from
its stream, the group in the lead's chat and Review / Diff / Commit / Send fixes / Stop / Set aside on each row.
The full scope followed in one pass: the skill, the Team switch, the handoff carry, drag hand-out and
branch-and-commit, then shell-write credit.

**Settled answers** (from the canvas's open questions; the Teams desktop canvas,
https://claude.ai/artifact/NHGzaVDABY7aveRuqjM1Js):
- Commit on main: offer a new branch, never refuse outright and never commit on the main line.
- The Cue Teams canvas (§4's mock) is the look to follow: the cleaner of the two.

**What shipped beyond the first plan:**
- **The team skill** (§6), appended to every hosted Claude Code lead at launch. The user never types any of it: a
  plain request with independent parts becomes a team on its own.
- **The Team switch** on a hosted Claude lead's message box: *Auto* (default; the lead decides from the skill), *On*
  (split what you send among teammates), *Off* (no teammates). On and Off add one short line after your message
  ("(Team: on. …)", "(Team: off. Do this yourself: start no teammates.)"); Auto adds nothing. `/team` is Cue's own
  command: the message after it goes with the switch On, this once.
- **Handoff carry** (§7): the team moves to the successor as plain facts, and leaves the old lead.
- **A quiet teammate** (sessions/quiet.rs, 2026-11-09): no line from a teammate (or a background agent) for 15 min is
  judged once by its last line. Its own words: it finished and lingered, so it shows done. A tool call still open: a
  long run, left to it ("Running: cargo test for 20m"). A result and nothing after: stuck; its row says "quiet for Nm —
  looks stuck" with Resume / Stop, and a hosted lead gets one short nudge for that quiet stretch (sent at once, since
  its turn waits on that teammate). A new line ends the stretch.
- **Drag hand-out:** text dropped on a hosted lead's card under Sessions goes to it as a new teammate's task
  (one named background teammate in a worktree of its own, not held for the lead's current work). Before it goes, a
  preview lists the files the text names and who already changes each (a current teammate), with
  Send anyway / Cancel. SHORTCUT: a task's files are only the paths its text names; a task naming none shows nothing
  taken. Upgrade: ask a small model which files it would touch.
- **Make a branch and commit, without moving the folder.** When a teammate's folder is on main, master or a detached
  HEAD (often the shared main checkout, where other sessions keep uncommitted work), Commit offers *Make a branch and
  commit*. Cue never switches that checkout: it makes `team/<name>` (or `team/<name>-2`, …) at HEAD and commits only
  the teammate's files onto it through a temporary index (`GIT_INDEX_FILE`, `write-tree`, `commit-tree`,
  `update-ref`). The folder's HEAD, index and files stay as they were; on any failure no half-made branch is left.
  It says so plainly: "Committed N files to team/<name> (<sha>); your folder stays on main."
- **Shell-write credit** (§7): files a teammate writes through Bash are its files, from its worktree's `git status`
  when it has one, else read off its commands and marked *via shell*.

**Crew reviews in the chat, and a crew that tidies itself (2026-11-10).**
- What Cue sends a lead for its crew (a review request, Send fixes, Stop, the quiet nudges, a hand-out) is drawn as one
  quiet line ("Cue asked for docs's review · what it asked"), never as your message. The lead's reply that is a
  review is one line: "docs reviewed: send fixes · 1 should-fix · 2 notes", with the button it calls for (only on that
  member's current review) and Open ›, which opens the crew with its findings under its row. ⋯ → Full review shows the
  lead's whole reply (kept on the member as `review`). A queued review is a quiet line with Skip.
- Which reply is a review: after Cue's request for X, up to your own next message or Cue's next review request, the
  reply that is X's saved review word for word, else the last one with a Recommendation line. Verdict: the words after
  "Recommendation:". Counts: should-fix (with must-fix, blocker) and notes (with nit, question, or no kind). Every
  window that draws a crew is held to the same cases: `tests/fixtures/crew-review.json`.
- The crew tidies itself (hub/team.rs `team_tidy`, at most once a minute, off the tick's thread): a member that stopped
  having written nothing is set aside at once; a finished one whose files are all committed since it started, by
  Commit or by you outside Cue, leaves as committed ("Committed outside Cue", its commit); one whose files are clean
  with nothing committed since it started leaves as set aside ("Its changes are gone"). Files git can't speak for
  (outside a repo, ignored) keep it in the crew. Never a loop's crew, never on another machine.
