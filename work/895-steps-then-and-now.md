# A reply's steps: 0.1.86 next to today (#895)

Same chat in both: agent-relay on the Board, its second turn. It edits two files, runs the tests, one fails,
it says "One test expected the old column order", reads, edits and runs the tests again, then replies.
The step data is the same fake in both versions. Pictures are in `work/895-steps/`.

| 0.1.86 (3 Nov) | Today (main, 24 Nov) |
|---|---|
| ![0.1.86](895-steps/0.1.86.png) | ![today](895-steps/today.png) |

Today, after clicking the reply's "· 3 steps":

![today, steps opened](895-steps/today-steps-opened.png)

## What you see differently

- **Steps are hidden until you ask.** In 0.1.86 every step was a row in the chat (letter badge, file, result:
  `Edited board.ts +12 −1`, `Ran npm test exit 1` in red, `Tests: 41 passed`). Today the chat is only what was
  said. Each message ends with a small grey "· 3 steps"; clicking it opens the steps that led to that message.
- **A failed command no longer shows at a glance.** 0.1.86 showed `Ran npm test · exit 1` in red. Today it's
  behind "· 3 steps" after "One test expected the old column order", so you only see it if you open that.
- **An opened step is one line.** Clicking a step opens it in full (command, output, diff in green and red).
- **While it works**, the step it's running now shows once as a row with a live dot and its time, and the line
  under the chat says only "Working · 2m" (not in these pictures: both turns here are finished).
- **Background scripts and agents** show after the turn while they run; a failed script stays until the next turn.
- Smaller things: chat text is one size scale (15px messages, then 14px body), and colours come from the design
  tokens. The Desk lead's chat draws no steps at all.

## The changes, in order

| When | Item | What changed |
|---|---|---|
| 8 Nov | (no item), 067104a623 | Folding a turn hides only its steps, never what it said; the turn you were reading stays open while the next one works |
| 17 Nov | #157, 74c2104b08 | The Desk lead's chat drops the steps box |
| 20 Nov | #542, b32e45b18b | **Steps folded in chats (option F)**: folded by default into one "N steps" chip, each message keeps its "· N steps" count |
| 21 Nov | #527, 1c24cc9beb | All chat styles on design tokens, one style per kind of message |
| 21 Nov | #604, 2bf0f71833 (and its re-lands f397da33a1, aa042d0df3, 8bd1511234, 4339fc1cda) | **A step stays one line until you click it**; a message's count opens only the steps that led to it |
| 21 Nov | #685, 423bebab2c | A "Conversation only / Everything" switch on the chat (taken out again by #666, the calmer Desk chat) |
| 22 Nov | #703, 504044846a | Chat text as big as Slack's: 15px messages |
| 23 Nov | #767, 806dd02e1b | The designer's text sizes across the app |
| 24 Nov | #866, 373e15b961 | **The running step shows once**, with its live dot and time; the line under the chat says only "Working · 2m" |
| 24 Nov | #867, ba8ef9523d | Background scripts and agents show after the turn as they run; a failed script until the next turn |

## How the pictures were made

- 0.1.86: the release's files, copied out of the repo into a scratch folder (Cue doesn't allow a worktree from
  an item's session), shown with that release's own preview page. That page had fallen behind its window (it
  missed `status.js` and `about.js`), so the script loads the window's own script list from `ui/index.html`.
- Today: this folder's window on the UI test harness (`tests/ui/app.html`, the `board-waiting` scenario).
- Both: headless Chromium, 1280×900 at 2×, light, the chat scrolled so "Adding the footer total…" is at the top.
  The script is `work/895-steps/shot.mjs`: `node shot.mjs old|new <out.png> [closed|open]`.
