# Audit: crew, background agents and the diff, Board chat versus Desk

Date: 2026-11-28. Read-only audit of the latest main. No code changed.

## What you asked

> "see the crew members, open full, handle it stuff .. and how we show background agents .. explorer, teammate etc.. I am thinking we make crew members show the same in board chat.. and desk shows in the way it shows, but we still have agents tab and in agents we tell what type of agent they are by some category if needed. Also, see the diff tab.. we should make the diff tab same as the desk diff tab.. and see what we can do make these consistent.. maybe audit first to see the differences"

## The three kinds of helper

- **Crew member**: a teammate a hosted lead starts and Cue tracks as its team. Cue reviews it and commits or sends fixes.
- **Background agent**: a Claude Code helper run in the background (Explore, general-purpose, Plan, fork). It has no chat of its own.
- **Named teammate**: a background agent started with a name that stays for more work, but isn't in a Cue crew.

Scripts (shell commands run in the background) are not agents. They have their own Scripts tab on the Board.

## Crew members

| | Board session chat | Desk |
|---|---|---|
| Where | A tray on the message box: "Cue Crew · 2 · focus-fix needs you · 1 working", then Open full ›, Handle it ▾ and a fold arrow. Open, it shows four goal boxes (The job, Done well when, May do alone, Asks you first), then Needs you, Ready, Decisions and steps, Waiting on the lead, Working, and "N done ›". | No tray. Crew members show in three places: as lines under their lead in the left list (dot, name, state, current step); as a "Crew" card on the lead's (Dispatcher) page (name, what it's on, files, how long); and as an item's holder ("web-lead → search-centre"). |
| One member | One row in the tray. Its name opens a menu: Its steps, Review, Diff, Full review, What it was asked. Each row has a Commit or Send fixes button, Allow/Deny when it asks, and Stop while it works. | Its own page: a "Crew · search-centre · Working" header with "in web-lead's crew · on #d3", Stop it and ⋯ (Open its session). Below are three tabs: Chat (what the lead asked it, then its steps), Files (what it changed so far) and Activity. The box writes to its lead. |
| Open full | The crew page takes the whole window. Left: the lead's chat with a Needs you bar over the box. Right: The lead, then one card per member with name, state word, a small progress mark, how long, Stop or Diff, and "2 done ›". Picking a member gives tabs Diff so far, Full review and Full task. | No Open full. The Desk is already the full view. |
| Handle it | Yes. Cue commits reviewed members or sends their fixes back. A mode menu sits on the tray line. | No Handle it button. The Desk lead and Cue settle it, and you see the result in Activity and on the item. |
| Category label | The tray's title says "Cue Crew". Members carry no kind tag. | "Crew" in the page header, with the crew icon. |
| In the Agents tab | Never. Crew members are left out on purpose, so a lead with only crew has no Agents tab. | No Agents tab on the Desk. |

Screenshots: `board-lead-crew-tray.png`, `baseline-team-open.png`, `baseline-team-page.png` (Open full), `desk-crew-chat.png`, `desk-crew-files.png`, `desk-lead-session-chat.png` (the Crew card on the lead's page).

## Background agents and named teammates

| | Board session chat | Desk | Agents tab (Board only) |
|---|---|---|---|
| What shows | An "AGENTS" list above the box with up to 4 rows that need you or are at work, then "N more · Agents ›". Each row has an icon, the name, a grey "· Explore · 4m 12s", a live dot (amber when stuck) and its latest step. | The same rows above the box in a session's chat, because the Desk draws the same status line. The left list only says "working · 2 helpers". The Dispatcher and session summary pages don't list them. | All of them, grouped Needs you, Working and Done, with clock times and tokens. |
| Actions | Click opens it in full in place of the chat. Allow/Deny when it asks. Resume/Stop when it's stuck. The title line has "N agents ›" and a chip for one that needs you. | Same rows, same actions, but only in a session's chat. There is no Agents tab and no Scripts tab, so finished agents can't be found on the Desk. | Same row actions, plus "Show earlier". |
| Category label | Partly. It's the agent's own type in grey, as the agent reports it: "Explore", "general-purpose", "fork". A named teammate is tagged **"crew"**, which is wrong, because it isn't in a Cue crew. | Same as the Board, in the chat only. | Same grey type, as the screenshot shows ("general-purpose", "Explore"). By the code, an agent opened in full shows it as an uppercase tag ("EXPLORE"); not captured. |

Screenshots: `baseline-agents-quiet.png` (the agents list above the box, with one stuck), `board-agents-tab.png` (Working and Done groups, each row with its raw type, start time, run time and tokens, Resume and Stop on a stuck one; note its tab label reads "Agents · 0 · 5 working", a count that looks wrong), `desk-session-agents.png` (a Desk session's summary page, which doesn't list agents).

## Diff

| | Board session Diff tab | Desk Files tab (session, crew member, item) |
|---|---|---|
| Where | A tab beside Chat/Activity/Agents, labelled "Diff 2 +6 −1". | A tab beside Chat and Activity, labelled "Files 2" (a count only). Also an item's "Files changed". |
| Scope | Four choices: This session's edits, This turn, Whole session, Uncommitted. Refresh is on the right. | Fixed: a session shows "Changed this turn", then each item it worked on with that item's files. A crew member shows "Changed so far". |
| List | Every file stacked down the page with its diff right there. The first 3 are open and the rest open on a click. Files are grouped by repo, and a folder of 20+ new files becomes one row. | A plain list: path, "new", +N −N. It shows the first 3 by size, then "and N more files ›". |
| One file | Opens in place, inside the stack. | Opens a large overlay: all files on the left (↑ ↓ to step through them), the chosen file's diff on the right, and Esc to close. You stay where you were. |
| Crew member | Diff on a member opens the session's Diff tab filtered to it, with a chip "skill's 3 files ✕". A file on the member's own page opens the same large overlay the Desk uses. | The member's Files tab, then the overlay. |
| Shared code | Diff reading and the green/red colours are shared. Its own drawing. | The file list is the Desk's own. The large overlay is shared: the Board's crew member page already opens it; only the Board Diff tab's stacked list is separate. |

Screenshots: `baseline-session-diff.png` (Board Diff tab, clean), `board-diff-tab-lead.png` (Board Diff tab squeezed under an open crew tray), `desk-crew-files.png` (Desk Files tab), `desk-file-overlay.png` (Desk diff overlay).

## Differences that matter, ranked

1. **Crew looks completely different in the two places.** On the Board it's a tray with grouped rows and buttons that gets bigger when opened. On the Desk it's quiet lines and cards, with a page per member and Chat/Files/Activity tabs. The same member has different buttons in each: the Board has Commit, Send fixes, Review and Handle it; the Desk has only Stop it and Open its session.
2. **The diff works two ways.** The Board stacks every diff inline and has four scopes. The Desk lists files and opens one file at a time in an overlay with no scope choice. The tab names differ too ("Diff 2 +6 −1" against "Files 2").
3. **A named teammate is labelled "crew"** in agent rows and the Agents tab, though it's not in a Cue crew. Background agents show their raw type ("general-purpose") with no friendly category.
4. **The Desk has no Agents tab.** Background agents only show in a session's chat while they work. Finished ones and scripts can't be found from the Desk.
5. **Crew members are missing from the Agents tab**, so the tab can't be the one list of everything a session started.
6. **The open crew tray squeezes the Board chat.** With the tray open, the Diff tab shows only one file's header above it (see `board-diff-tab-lead.png`).

## Options

### A. Board chat borrows the Desk's crew look; the Agents tab lists every kind (M)

- **Changes:** The Board's crew tray becomes the Desk's way of showing crew: quiet one-line rows (dot, name, state, step) and a click that opens the member's page with Chat, Files and Activity tabs in the chat's place. Needs you stays as one bar over the box. Handle it moves to that bar's line. The Agents tab lists crew members too, each with a category word: Crew, Teammate, Explorer, Planner, Background. "Teammate" replaces the wrong "crew" label.
- **Stays:** The Desk as it is. The Open full crew page, kept for now as the Board's wide view.
- **Size:** M.

### B. Option A, plus the Board Diff tab becomes the Desk's list and overlay (L)

- **Changes:** Everything in A. The Board Diff tab also becomes the Desk's Files view: a list by size with +/−, a click opening the overlay with ↑ ↓ (the same overlay the Board's crew page already uses). The tab is named the same in both places (one name, file count plus +/−). The Board's four scopes stay as a small choice at the top of its list. Diff on a crew member opens the overlay on its files. Adding the same scope choice to the Desk would change how the Desk looks, so it's left out here; it needs your say.
- **Stays:** The Desk's look. Diff colours and reading.
- **Size:** L. This is the full "make them consistent" pass.

### C. Labels and diff first, crew later (S, then M)

- **Changes:** First pass (S): add category words to agent rows and the Agents tab (Teammate for named ones, Explorer, Planner, Background for the rest), and add an Agents tab to the Desk's session page using the same list. Second pass (M): the Board Diff tab becomes the Desk's list and overlay, as in B. The crew tray stays until you've used the Desk's crew view more.
- **Stays:** The Board crew tray, Open full and Handle it, for now.
- **Note:** This skips your main ask: crew in Board chat still looks different from the Desk. Treat it as the first steps toward A or B, not a replacement for them.
- **Size:** S for the first pass, M for the second.

## Also drawn two ways (brief)

- **Headers.** The Board chat has its own top bar (name, chips, timeline). The Desk uses one header for every page ("Crew · name · state", quiet line under).
- **Activity tab.** Both have one, but they show different things. The Board's is the session's background activity; the Desk's is the Desk's moves on that session or item.
- **Asks from a crew member.** On the Board they're answered on the member's tray row or the crew page's Needs you bar. On the Desk they're on the item and in the lead's chat.

## Screenshot paths

Taken with the UI test harness and kept locally, not in the repo:

- Board: `board-lead-crew-tray.png`, `baseline-team-open.png`, `baseline-team-page.png`, `baseline-team-diff.png`, `baseline-agents-quiet.png`, `board-agents-tab.png`, `baseline-session-diff.png`, `board-diff-tab-lead.png`
- Desk: `desk-home.png`, `desk-lead-session-chat.png`, `desk-crew-chat.png`, `desk-crew-files.png`, `desk-file-overlay.png`, `desk-session-agents.png`, `baseline-desk-layout-laptop.png`
