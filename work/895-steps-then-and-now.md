# A reply's steps: 0.1.86 next to today (#895)

The same chat in both versions: agent-relay on the Board, asked to "add a spend column to the board and keep the
footer total". It edits two files, runs the tests (they fail), says "One test expected the old column order",
reads, edits and runs the tests again. The step data is identical in both. Pictures are in `work/895-steps/`.

## 1. A reply still working

| 0.1.86 (3 Nov) | Today (24 Nov) |
|---|---|
| ![0.1.86 working](895-steps/0.1.86-working.png) | ![today working](895-steps/today-working.png) |

**0.1.86:** every step showed as it ran. Under "Working · 6 steps" (with a Show all button) came the whole
turn: each message, then its steps as rows with a letter badge and result (`Edited board.ts +12 −1`,
`Ran npm test exit 1` in red, `Read board.test.ts 140 lines`). The newest row was "Running npm test", with no
timer. The line under the chat said "Working on: Add a spend column to the board and kee… · 6m", repeating
your message.

**Today:** the turn opens folded. One chip says "Working · 6 steps · 2 commands, 1 read, 3 edits, 1 failed ▸".
Under it are only the messages, each ending in a grey count ("· 3 steps") that opens its steps. Of the steps,
only the one running now shows: "Running npm test" with a live dot and how long it has run (1m). The line under
the chat says only "Working · 6m". The failed test run is not in view: the "1 failed" in the chip is the only
sign until you open it.

## 2. A finished reply, steps folded shut

| 0.1.86, folded by hand | Today, as it opens |
|---|---|
| ![0.1.86 folded](895-steps/0.1.86-folded.png) | ![today folded](895-steps/today-folded.png) |

**0.1.86:** a finished turn was folded or open per turn. Older turns came folded; the newest came open, with
all its steps showing (`0.1.86-finished-as-opened.png`). Folded, the turn shrank to one chip,
"▸ 6 steps · read 1 file · 3 edits · 2 commands · 1 failed", and **the turn's in-between messages went with it**:
"Adding the footer total…" and "One test expected the old column order" disappeared. Only the final reply stayed.

**Today:** every turn comes folded, the newest too. Folding hides only the steps, never what was said: both
in-between messages stay, each with its "· 3 steps" count. Clicking a count opens just the steps that led to that
message (`today-steps-opened.png`). Each opened step is one line until you click it.

## What changed it

| When | Item | What changed |
|---|---|---|
| 8 Nov | (no item), 067104a623 | A fold hides the steps, never what it said; the turn you're reading stays open while the next one works |
| 17 Nov | #157, 74c2104b08 | The Desk lead's chat drops the steps box |
| 20 Nov | #542, b32e45b18b | **Steps folded by default (option F)**: one chip per turn, while working too, and a "· N steps" count after each message |
| 21 Nov | #604, 2bf0f71833 (re-landed as f397da33a1, aa042d0df3, 8bd1511234, 4339fc1cda) | **A step stays one line until you click it**; a message's count opens only the steps that led to it |
| 21 Nov | #527, 1c24cc9beb | Chat styles on design tokens, one style per kind of message |
| 21 Nov | #685, 423bebab2c | A "Conversation only / Everything" switch (taken out again by #666) |
| 22–23 Nov | #703, #767 | Chat text sizes: 15px messages, then the designer's scale |
| 24 Nov | #866, 373e15b961 | **The running step shows once**, with a live dot and its time; the line under the chat says only "Working · 6m" |
| 24 Nov | #867, ba8ef9523d | Background scripts and agents show after the turn while they run; a failed script stays until the next turn |

## How the pictures were made

- 0.1.86: the release's files copied out of the repo into a scratch folder (Cue doesn't allow an item's session
  its own worktree), shown with that release's own preview page. That page had fallen behind its window (it
  missed `status.js` and `about.js`), so the script loads the window's own script list from `ui/index.html`.
- Today: this folder's window on the UI test harness (`tests/ui/app.html`, the `board-waiting` scenario).
- The working state is the same fake turn put into both, with agent-relay set to working.
- Headless Chromium, 1280×900 at 2×, light theme. Script: `work/895-steps/shot.mjs`,
  run as `node shot.mjs old|new <out.png> [closed|open|working|folded]`.
