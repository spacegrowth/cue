# Does the Desk work for any project? An audit

Checked 24 Nov 2026. I read the Desk settings, the landing steps, the item brief, the Tests setting, the now line
and New Desk, looking for anything that assumes Cue's own code, Rust, or this Mac. Nothing in the code was changed.

## The short answer

Mostly yes. Cue's own steps (its check, its install, its full run of every test, its test script) are turned on only
for Cue's own project. For any other project they're hidden: they don't appear in Desk settings at all. A new
project starts with sensible defaults: each item in its own copy, one commit per item, no check, nothing after
landing, ask before sending to GitHub, and "All tests".

There's one real problem and a few small ones. The real problem: in another project, an item's copy starts with no
installed packages, so its tests and your own check steps are likely to fail.

## What's already fine

- **Desk settings.** "Cue's own check" and "Install or serve Cue" are offered only in Cue's own project. Elsewhere
  they're left off the screen, and Cue refuses them if the lead asks for them. A folder that isn't a git repository
  gets a plain explanation and only the choices that work there.
  Where: `src-tauri/src/desk/lands.rs` (`allowed`, `defaults`) and `ui/lands.js`.
- **Landing steps.** The full run, the install and the window check only run where the folder builds Cue itself.
  Where: `install.rs`, `full_run.rs`, `finish.rs` in `src-tauri/src/desk/hub/landing/`.
- **Now line.** "Building Cue", "Installing Cue", "Rust tests" and "Cue's own check" appear only while Cue's own
  steps run. Another project's line shows its own step as typed ("its check: npm test").
  Where: `landing/now_line.rs` and `ui/desk/now_line.js`.
- **Tests setting in the brief.** The lines about `tests/run.sh` and `CUE_FULL_RUN` go only to sessions in Cue's
  own code. Elsewhere the session gets the plain line for the setting.
  Where: `src-tauri/src/desk/copies.rs` (`card_tests_line`) and `desk/mod.rs` (`tests_rule`).
- **Ask me before.** Release and publish are spotted for npm, cargo, pip (twine), gems, poetry, git tags and GitHub
  releases, not only for Cue's release script. Where: `src-tauri/src/desk/one_way.rs`.
- **New Desk.** It takes any folder and any installed agent. Nothing in it is specific to Cue.

## What a new user would hit

### 1. An item's copy has no installed packages (size: M, the one that matters)
**What they see:** in a JavaScript, Python or Rust project, the item's session runs the tests and they fail with
"module not found" or similar. Or, with "Your own steps" as the check, a step like `npm test` fails and nothing
reaches main, even though the work is fine.
**Why:** a new copy is a fresh checkout. Only Cue's own project links its `node_modules` and build folder into each
copy. The check runs in another fresh copy, with the same gap.
**Where:** `src-tauri/src/desk/hub/landing/tree_files.rs`: `link_shared`, called only when `builds_this_cue`, both
for an item's copy and in `check_steps_on`.
**Fix:** add an optional "Set up a new copy" step to Desk settings (for example `npm ci`, or `uv sync`) that runs
when a copy is made and before a check. A smaller start would be to also link `node_modules` from the main folder
when it exists, for any project.

### 2. The project's own git status lists `.claude/` (size: S)
**What they see:** in their own terminal or editor, a new untracked `.claude/` folder in the project, which holds
the items' copies. Someone running `git add .` could add it. I couldn't confirm this here because Cue blocks making
copies from inside an item's session, but nested copies normally show up this way.
**Where:** `landing/trees.rs` (`TREES = ".claude/worktrees"`).
**Fix:** when Cue makes a project's first copy, add `.claude/worktrees/` to that repository's private exclude list
(`.git/info/exclude`). That touches none of the project's own files.

### 3. "Nothing changed here" checks know only Cue's folder names (size: S)
**What they see:** nothing yet, but a dropped item's copy in another project can be counted as holding work just
because packages or build output exist in it, so it's kept and listed instead of being cleaned up.
**Where:** `landing/tree_files.rs` (`left_has_work`, with `node_modules` and `src-tauri/target` written in).
**Fix:** ignore whatever the project's own `.gitignore` ignores (git does that by default), and drop the hard-coded
names.

### 4. The item brief assumes design canvases (size: S)
**What they see:** every session is told "If you put its designs on a canvas… say DESIGNS:", which means nothing in
a project without claude.ai canvases. The tests line also asks for "the full suite" even when a project has no
tests.
**Where:** `src-tauri/src/desk/mod.rs` (`handed_words`, `tests_rule`).
**Fix:** send the DESIGNS line only to items that have designs, or that have the word "design" or "mock" in their
title. Make "All tests" say "if the project has tests".

### 5. New Desk doesn't mention the check (size: S)
**What they see:** a new Desk lands every item with "landed, not checked" until they find Desk settings. Nothing
says a check can be set.
**Where:** `ui/desk/home.js` (New Desk), `ui/lands.js`.
**Fix:** one hint line in New Desk ("Items land unchecked until you add a check in Desk settings"), or a one-time
note on the first item that lands without a check.

### 6. Spotting "running tests" misses some test commands (size: S)
**What they see:** an item's row shows a plain "Running…" instead of "testing" for commands like `make check` or
`rspec`.
**Where:** `src-tauri/src/desk/hub/core/steps.rs` (`runs_tests`: the words test, spec, playwright, pytest, jest,
vitest).
**Fix:** add check, rspec and ctest, or treat any of the Desk's own check steps as tests.

### 7. Someone else's build of Cue counts as "Cue's own" (size: S, note only)
**What they see:** if another developer downloads Cue's code and opens a Desk on it, they get Cue's install and
test steps, which expect this setup (`install.sh`, the add-on folder).
**Where:** `lead/checks.rs` (`builds_this_cue` reads Cue's app ID) and `logs/repos` (`facts().cue`).
**Fix:** probably fine as it is, since that's who those steps are for. To be safe, show them only when
`install.sh` is also present.

## Suggested order
1 first: it's the only one likely to make a new user's Desk fail. Then 2 and 4, which are small and visible.
3, 5 and 6 can be one small cleanup together. 7 can stay as it is.
