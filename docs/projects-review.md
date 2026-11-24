# Projects in Cue: what's broken (review for #849, 2026-11-24)

## Short version

Projects mostly hold together. Picking one from the chip narrows the Board, Sessions and History, All projects
brings everything back, and opening a session from another project takes the window there. The problems are
in five groups:

1. **The Desk can't use sessions the window shows under its project.** Put a folder in a project, or move a
   session to one, and the window lists it there. The Desk still says "isn't a session of X" when you add it,
   give it work or make it the lead. This is the worst one.
2. **A picked project can quietly hide everything.** If the project Cue remembers from last time is gone, Cue
   opens on an empty Board and the header says "all clear" while sessions wait on you.
3. **Some panes ignore the pick.** Insights and Read show the projects chip, but picking a project changes
   nothing there. History's session menu and its count ignore the pick too. Empty panes never say a project is
   picked.
4. **+ New doesn't follow the picked project.** It suggests the newest folder from any project, then the window
   jumps to that other project.
5. **Moved or deleted folders split projects.** Deleting a worktree moves its history into a new project
   named after the folder. Renaming a project's folder leaves its name, pin, settings and Desk behind. A Desk
   whose folder is gone still tries to start sessions there.

Most fixes are small (S) and only touch the window. Groups 1 and 5 need medium (M) work in Cue's back end.

Sizes: S = under an hour or so, M = half a day, L = more. Severity: high = you lose sight of work or the
feature fails; medium = wrong or confusing in normal use; low = rare or cosmetic.

How this was checked: the code was read end to end, and the window's flows were run in the UI test harness
(pick a project, switch views, back to All projects, + New project…, remove a made project, a picked project
with nothing live, a remembered project that's gone). Points marked *(code)* come from the code only. Nothing
ran on live data. Project colors are left out (removed by #848).

## How projects work now (one paragraph)

A session's project is decided by the first of these that applies:

- a folder you put in a project (+ New's Project line);
- a repository's `.cue/actions.json` naming one;
- the git repository's name, shared by all its worktrees;
- otherwise the folder's name. On another machine it's always the folder's name, never looked up on disk
  (`src-tauri/src/model.rs:396-460`).

Move to project overrides this for one session (`hub/mod.rs:763`). Cue's store keeps only the folder's *own*
project. The put/moved project is painted on only when the window gets its state (`hub/mod.rs:722-799`). The
window remembers the picked project per window (`ui/core/colors.js:197`) and narrows each pane with `inProj`.
Names, pins, made projects, Ask me before and words never committed are keyed by the project's name. A Desk's
model, machine and how work lands are keyed by the Desk.

## Problems

### 1. The Desk refuses sessions the window shows under its project — high, M
- **You see:** a session listed under project X on the Board and in Sessions. On X's Desk, Add to Desk, Give
  to, picking it as lead, or the lead routing to it fails with "that session isn't of X". The lead's summary
  doesn't list it as someone it can route to. This happens to any session in a folder you put in X, any
  session moved with Move to project, and any repo whose `.cue/actions.json` names X.
- **Cause:** the back end compares the session's stored *own* project with the Desk's project, which includes
  put and moved projects. The comparisons are at `desk/hub/core/desk.rs:114` (`routable`),
  `desk/hub/core/members.rs:68`, `desk/hub/lead/run.rs:128`, `desk/hub/turn/act.rs:687`,
  `desk/hub/turn/route.rs:721-730` and `desk/model.rs:833-836`. The Desk page in the window uses the painted
  project (`ui/desk/home.js:20`), so the window and the back end disagree. Only routing to a new session is
  tested (`testing/tests/desk_routing.rs:176`).
- **Fix (M):** compare with the project the window shows. `hub/crew_log.rs:95 project_shown_for` already
  covers moves; extend it to cover put folders and the repo's own name. Then add tests for add, give and lead
  with a put folder and with a moved session.

### 2. A remembered project that's gone hides everything at launch — high, S
- **You see (harness):** Cue opens with the chip on a project that has nothing left. The Board is empty, the
  header says **all clear** while 6 sessions wait, and Sessions says "No live sessions on this Mac". The only
  hint is a small "6 waiting in other projects · show all" line in Waiting. The chip's list shows that
  project as "history" even when it has none.
- **Cause:** the pick is read back without a check (`ui/core/colors.js:197`). The chip keeps listing it on
  purpose (`ui/board/board.js:13`, label at `:48`). The header's counts only cover the picked project
  (`ui/board_motion.js:143` via `board.js:197`).
- **Fix (S):** when the first state arrives, drop a pick that has no live, parked, made, pinned or history
  sessions. While a project is picked, say "+N elsewhere" beside the header counts. This also settles the
  notch counting every project while the window counts one (`ui/mini.js:653`).

### 3. + New suggests another project's folder, then the window jumps there — medium, S
- **You see (harness):** with web-app picked, + New offers `~/code/api-server`, the newest folder from any
  project. Starting it switches the window to api-server. With a project you made picked, the short form
  shows no Project line, but the session still lands in that project.
- **Cause:** the default folder ignores the pick (`ui/sessions.js:345`); only a made project is carried over
  (`:344`). Starting a session follows it to its project (`ui/window.js:193` via `sessions.js:371`). A made
  project counts as "this session only" only when it's in the made list (`sessions.js:430`), so one that came
  from a move moves the whole folder instead *(code)*.
- **Fix (S):** with a project picked, suggest its newest folder. Show "Project: X" in the short form too, and
  use `madeProject()` for the check at `:430`.

### 4. Insights and Read show the projects chip but ignore it — medium, S
- **You see (harness):** picking a project on Insights or Read changes nothing on the page, but the pick
  carries over to the Board and Sessions.
- **Cause:** every view except the Desk and add-ons shows the chip (`ui/app/head_view.js:21`). Insights and
  Read never filter. History's Today strip is always all projects too (`ui/history.js:20`).
- **Fix:** hide the chip on Insights and Read (S). Filtering them by project instead would be M.

### 5. A deleted worktree's history becomes a new project — medium, S
- **You see:** after you delete a worktree that sits beside the repo (e.g. `../cue-insights`), its sessions,
  answers and parked rows leave "cue" for a new project named after that folder.
- **Cause:** `reproject` works every project out again from disk whenever folders change
  (`hub/projects.rs:22-46`). A missing folder has no repository, so it falls back to the folder's name. The
  rule Cue's own migration follows is the opposite: folders no longer on disk keep their name
  (`db/schema.rs:482`). The same thing makes worktree rows flicker under the folder's name just after launch,
  before the disk lookup answers (`logs/repos.rs:6-8, 43`).
- **Fix (S):** have the disk lookup say "not there" or "not known yet" apart from "no repository", and keep
  the stored project in both cases.

### 6. Renaming or moving a project's folder splits the project — medium, M
- **You see:** after renaming `~/dev/cue` to `~/dev/cue2`, new sessions show under cue2 and old history stays
  under cue. The name you gave it, its pin, Ask me before, words never committed and its Desk all stay with
  cue, so the Desk tab on cue2 shows an empty Desk. A folder you'd put in a project quietly drops out of it.
- **Cause:** a project is known by its folder or repository name (`model.rs:396-399`). Its settings are keyed
  by that name (`config.rs:127-175, 452-466`, `desk/one_way.rs:41-59`), and put folders by their full path
  (`config.rs:289-318`).
- **Fix (M):** a "Merge into…" action on a project in the chip's list that moves its names, pin, settings,
  Desk and put folders. Later: offer it on its own when a new project's repository looks like one that
  vanished (L).

### 7. A Desk whose folder is gone still starts sessions there — medium, S–M
- **You see:** on that Desk, a new lead or routing to "new" fails with an unclear error, and nothing points
  at the missing folder. *(code)*
- **Cause:** the Desk's folder is checked only when the Desk is made (`desk/hub/lead/mod.rs:46-49`; used
  unchecked at `desk/hub/lead/run.rs:232`). Starting a session there has no folder check
  (`hub/starting.rs:111-134`), unlike + New (`:56-59`). Fork has the same gap (`ipc/sessions.rs:255-280`).
- **Fix (S–M):** check the folder. The Desk's header then says "Folder X is gone · Change folder" (a small
  new setting), and Fork says "X isn't there any more".

### 8. The two kinds of Desk behave differently — medium, M
- **You see:** ✎ on a Desk made from a project's Desk tab renames the whole project. On one made with + New
  Desk, it renames only the Desk. Two Desks on one project share Ask me before and words never committed, but
  not model, machine or how work lands. Renaming a project leaves its Desk's name as it was.
- **Cause:** a Desk made from the Desk tab is keyed by the project's name, while + New Desk makes a separate
  one (`desk/model.rs:790-822`). Rename picks its path at `ui/core/colors.js:101-117`. Settings keys are mixed
  (`desk/hub/core/view.rs:448` vs `desk/hub/core/desk.rs:123`, `landing/commit.rs:53`).
- **Fix (M):** make every new Desk the + New Desk kind, and turn the old ones into it once, keeping their id.
  ✎ on a Desk renames only the Desk.

### 9. A picked project with two Desks shows every Desk of every project — low–medium, S
- **You see:** with X picked and two Desks on X, the Desk tab opens the list of all Desks, not X's two. A
  Desk's project is also fixed when it's made, so putting its folder in another project later doesn't move
  it. *(code)*
- **Cause:** `ui/desk/desk.js:4-9` gives up on 2+ matches; Desk home isn't narrowed (`ui/desk/home.js:79-87`).
  The project is stored once (`desk/hub/lead/mod.rs:50`).
- **Fix (S):** narrow Desk home to the picked project, and work a Desk's project out from its folder when its
  state is sent.

### 10. Desk sessions on another machine may show as their own project ("desk-17") — medium, S, needs checking
- **You see (likely):** a Desk item started on another machine shows in the projects list under its work
  folder's name, not the Desk's project, and giving it work by hand fails as in #1.
- **Cause:** on another machine a project is the folder's last name (`model.rs:396-399`), and the work folder
  is `<clone>/.claude/worktrees/desk-<id>` (`desk/hub/landing/remote_setup.rs:101-104`). Not seen on a real
  remote item yet.
- **Fix (S):** when Cue sets up a clone for a Desk, put that clone's folder in the Desk's project.

### 11. Empty panes never say a project is picked — low–medium, S
- **You see (harness):** Sessions says "No live sessions on this Mac", History says "Nothing here with these
  filters". Neither names the project or offers All projects. The Board does ("N waiting in other projects ·
  show all", `ui/board/board.js:535`). A project picked in History that has only past answers leaves the
  Board and Sessions empty in the same way (`board.js:11`).
- **Fix (S):** reuse the Board's line: "Nothing live in X · N in other projects · show all".

### 12. History's session menu and count ignore the pick — low, S
- **You see (harness):** with one project picked, the Session menu still lists other projects' sessions, and
  picking one shows nothing. The footer says "3 of the last 300 kept" next to 1 row.
- **Cause:** `ui/history.js:56` and `:87` use all of History.
- **Fix (S):** narrow both, and drop a picked session from another project when the pick changes.

### 13. A picked pinned project makes the chip read "All projects" — low, S
- **You see:** pick a pinned project and its chip lights up, but the projects chip beside it reads "All
  projects" while only one project is shown.
- **Cause:** `ui/board/board.js:54-56` shows the picked name only for projects that aren't pinned.
- **Fix (S):** have the main chip read "Projects" in that case.

### 14. + New project… in the chip doesn't go to the new project — low, S
- **You see (harness):** after Enter the list stays open on All projects. + New project… under Move to
  project does go there (`ui/chat/render.js:680`).
- **Fix (S):** pick the new project, the way Move to project's does (`ui/board/board.js:20-32`).

### 15. Smaller things — low, S each
- The crew page stays up after you switch to a project with nothing waiting (`ui/team_page.js:71-76`; close
  it in `setProjFilter`). *(code)*
- A project can be renamed to a name another project already shows, giving two identical rows (`saveProjName`,
  `ui/core/colors.js:101`, doesn't use `takenProject`).
- Renaming a Desk cuts its name to 40 characters, though 80 are allowed when it's made
  (`ui/desk/sessions.js:265`, `ui/core/colors.js:103`).
- The moved-sessions list is never cleaned out. At 500, Move to project stops working, and sessions after
  /clear or a hand-off silently stop following their move (`config.rs:272-283, 327-340`; error ignored at
  `hub/mod.rs:726`). Prune ended sessions when saving.
- A deleted Desk's model, machine and how-work-lands settings stay behind, so a new Desk with the same name
  inherits them (`desk/hub/core/spaces.rs:52-95`).
- Old comments still describe project dots (`ui/app/clicks_board.js:1-2, 39`, `ui/board/board.js:55, 58`,
  `src-tauri/src/config.rs:120`).

## Panes at a glance

| Pane | Projects chip | Narrowed by the pick |
|---|---|---|
| Board | yes | yes, with "N waiting in other projects · show all" |
| Sessions | yes | yes (tiles, counts, Parked/Closed) |
| History | yes, plus projects with only past answers | rows yes; session menu, footer and Today strip no |
| Insights | yes | **no** |
| Read | yes | **no** |
| Desk | replaced by the Desks menu | picks the project's Desk if it has one; list of Desks not narrowed |
| Terminal | yes | picking a project goes to the Board |
| Header counts | — | yes; the notch always counts every project |
| ⌘K / live list | — | no, on purpose; opening one switches project |

## What works well

- All worktrees of one repository group under one project, including the Desk's own work folders.
- Folders on other machines are never looked up on the Mac, and disk lookups never stall Cue.
- Put and moved projects are an overlay, so undoing one rewrites nothing stored.
- Moves follow a session through /clear, forks and hand-offs.
- Remove project cleans its moves, folders, pin and name together, and goes back to All projects if it was
  picked.
- Opening a session from another project takes the window there; with All projects it stays on All projects.

## Suggested order

1. Quick window fixes, all S: #2, #3, #4 (hide the chip), #11, #12, #13, #14, and #15's crew page, rename
   and comments.
2. #1, the Desk sees the sessions the window shows (M), with tests.
3. Gone and moved folders: #5 (S), #7 (S–M), #10 (S, after checking on a real remote item), then #6 (M).
4. #8 and #9, one kind of Desk (M).
