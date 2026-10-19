# Actions

Actions are a project's buttons for its developer commands: install, test, clean a cache, start a dev
server. They belong to the project (its folder, and the machine it's on for a session there), not to a
session: they run any time, whether an agent is working or not, and whether a session is open or not.

Cue runs each one in a tmux session of its own, so a server it started keeps running when Cue quits or
restarts, and the next Cue finds it again. Without tmux on this Mac, an action runs as a process of
Cue's own and ends when Cue quits. A session's actions on another machine run there, in tmux.

## Where you find them

- **⌘E** opens the actions of the open project: type to find one, **Return** runs it (or restarts one
  that's running), **⌘Return** runs it and opens its output. **⌘⇧E** runs the last action again.
- The **Actions** tab of an open session lists them all: running, the project's, yours, and the ones
  Cue found, each with its last run. A failed run shows its last lines.
- A project's chip in the top bar gets a dot for each action running in it; click the dots for Log,
  Restart and Stop, and to run the rest.
- Under an agent's reply, Cue offers the actions whose files that turn changed ("Install · package.json
  changed").

## Which project a session is in

A session's project comes from its folder, by the first of three rules that says one:

1. **You put the folder in a project.** + New's Project line, or a session's ⋯ → Move to project, puts
   its folder there: every session in it, now and later, shows under that project. A repository is put
   in as a whole, worktrees too. A folder above a repository doesn't reach into it.
2. **Its repository says so.** `"project": "web-app"` in its `.cue/actions.json`, for a docs or plugin
   repository that goes with another one.
3. **Its git repository**, by its main checkout's name, so every worktree and subfolder of `web-app`
   is `web-app`. Outside a repository, the folder's own name.

A session's actions are its checkout's: a session in `web-app/ui` has web-app's, run in web-app. A
worktree's are its own (its branch's `.cue/actions.json`), run in the worktree, so they run that
branch's code. Yours (`.cue/actions.local.json`) and what you allowed are kept once for the whole
repository, in its main checkout, so a new worktree has them too.

A running action is on its project's chip by the folder it runs in, never by a session: it stays there
with every session closed. When two checkouts of a project run actions, each row says its branch.

On another machine Cue never reads the repository from this Mac: a session there is in its folder's
project unless you put the folder in another, and its actions are its folder's.

## The two files

A project's actions are written in its checkout:

- `.cue/actions.json`: shared with everyone who works on the repo. Commit it.
- `.cue/actions.local.json`: only for you. Add it to your `.gitignore` (Cue says when git doesn't ignore
  it; it never edits `.gitignore` itself).

Both have the same shape:

```json
{
  "about": [
    "Actions are buttons in Cue for this project: install, test, start a server, clean a cache.",
    "..."
  ],
  "actions": [
    { "name": "Install", "run": "npm ci", "suggest": ["package.json", "package-lock.json"], "ask": true },
    { "name": "API", "run": "npm run dev:api", "keep": true, "suggest": ["server/**"] }
  ]
}
```

- `name` and `run` (one line, run by your login shell in the project's folder) are all an action needs.
- `"keep": true` for a server or a watcher that keeps running: Cue shows its port (read from what it
  prints, such as `http://localhost:5173`), and Restart and Stop.
- `suggest` lists files whose change in an agent's turn offers the action under the reply. `*` matches
  within one folder, `**` any number of folders; a plain path matches that file only.
- `"ask": true` for a command that could trip an agent working in the project (an install, a clean):
  run while that project's session is working, Cue asks whether to run it now or after the turn.

The `about` notes at the top are for whoever edits the file, people and agents alike. Cue writes them
into a new file, and keeps a file's own notes and any keys it doesn't know when it saves an action.

You can also add, change and delete actions in Cue: **+ New action** or the edit button on one, in the
Actions tab. It shows the line it writes, and you choose which file it goes in.

## Found actions

Cue also lists what it finds in the project's own files:

- `package.json` scripts, run with the package manager its lockfile names (`pnpm-lock.yaml`: pnpm,
  `yarn.lock`: yarn, `bun.lockb` or `bun.lock`: bun, else npm): `npm run test`.
- `justfile` recipes: `just test`.
- `Makefile` targets: `make clean` (plain targets only, not `.PHONY`, patterns or variables).

One with the same name or command as an action in the files isn't listed twice. Pin the ones you use,
hide the ones you don't, or edit one to copy it into your own file.

## Trust

Cue never runs a command from a file before you allow it. The first time a project's actions are
listed, and whenever one is added or its command changes (a found one's: its script), Cue shows them,
the old command struck through beside a changed one, and runs nothing until you click **Allow**. What
you allowed is kept in Cue's own folder, never in the project, so a repository can't allow its own.
Actions you save in Cue's edit sheet are allowed by being saved.

Every run is logged, with its command, folder and exit code, in `actions.log` in Cue's folder.

## Ask the agent to set up actions

**Ask the agent to set up actions** (in ⌘E and the Actions tab) puts Cue's actions prompt into the box
of the project's open session, for you to read and send yourself: it asks the agent to look at the
project and write its actions into the two files, following the notes at the top. Cue never sends it
for you. Change the prompt in **Settings → Actions** (**Reset to Cue's** puts it back).
