# Plan: Desk items worked on other machines

Item #585. It replaces the plan from #491. A plan only: no code changes. Written to be read aloud.

## In short

Another machine, a Linux machine or a second Mac, does a whole item by itself. It works in its own clone of the
repository there, runs the checks it can run, and pushes its work as a branch to the shared remote. It never
pushes main.

The Desk on this Mac then fetches that branch and lands it the normal way, through the same landing line as every
other item: caught up with main, word-checked, checked, and installed here. The full check for things that must
not go public runs before both pushes: the branch from the other machine, and main from this Mac.

For this to work, main has to be pushed often, so the other machines start from current work. And the Desk has to
say so when a machine goes quiet.

Most of the parts exist. Five pieces are new work, listed near the end.

## What Cue does today

This was read from the code as it is now.

**Machines and sessions.** You add a machine once. Cue reaches it over your own secure-shell setup and keeps one
shared connection per machine. A hosted session can run there under Cue's small host program, which keeps the agent
running when Cue quits or the connection drops. Cue carries that host program for Linux and for macOS.

**Seeing a session there.** Cue copies the session's log to this Mac as it is written, so its chat, its DONE line and
its TO LEAD lines read the same as a local session's. Cue reads every machine on a timer: every ten seconds where it
has a live session, every minute elsewhere, and less and less often, down to every five minutes, for a machine that
doesn't answer. A machine that can't be reached is marked so on its sessions.

**Drops.** Messages to a hosted session are held while the connection is made again, then sent once, in order. If
the host there died and lost the session, Cue says the session ended.

**A remote session holding a Desk item.** This works, but on an older, shorter path:

- It gets no copy of its own. Only a session on this Mac gets a copy and a branch from the Desk.
- Its DONE goes straight to "to check". Nothing is committed, checked or installed at that point.
- Looks right commits its changed files on that machine, never pushed. Cue first reads the changes from there and
  checks them and the title against the never-commit words kept on this Mac. That commit stays on the machine.
- The lead is told today that work on another machine "changes nothing in the repo", so it expects no commit.
- A new session for an item always starts on this Mac. The lead can't say "start it on that machine".
- "Ask me before", the rule that a push to main waits for your yes, is checked on every session on this Mac,
  hosted ones too. The hooks Cue sets up on another machine leave that check out. Only a Pi session there carries
  it, through Cue's own extension.

**The landing line.** For an item in its own copy on this Mac, the line does this, one item at a time: bring the
work up to main, check the title and the added lines against the project's never-commit words, run the project's
check in a clean copy, then move main forward by one commit. A clash goes back to the holder. For Cue itself the
check is the build, the Rust tests, and the window loaded from the clean copy. After landing, Cue installs, or
serves a window-only change in place.

**Pushing and fetching.** The Desk can already push and fetch, with every network call cut off after a minute. It
pushes an item's branch for a pull request, and it fetches the remote's main once a pull request is merged. The
push setting has three choices: never, ask each time, after each landing. Cue's own project is on "ask each time".

**Not in Cue at all.** The wider check before a push, reading commit messages and added lines for things that must
never be public, is done by hand today. The landing line's word check covers part of it, not all.

## How a remote item runs

**1. It starts from the shared remote.** The machine has its own clone. The session fetches main from the shared
remote and makes the item's branch from it, named the same way the Desk names branches here. It works there to the
end, as a session does on this Mac.

**2. It runs what it can.** On a second Mac that is the whole check. On Linux it is less, see the next section. The
session says in its DONE which checks it ran and which it could not.

**3. Its work is pushed as a branch.** Never main. The DONE line names the branch and its last commit. The plan is
that Cue makes this push, not the session. Cue already runs git on the machine for Looks right, so it can read the
branch's changes and commit messages from there before anything leaves. For a public project a pushed branch is
public at once, even if it never lands. So the word check alone is not enough here. The full check for things that
must not go public runs first, the same one that runs before main is pushed from this Mac. If it finds anything,
nothing is pushed, and the item says what was found and where. Sessions still never push.

**4. The Desk fetches it.** On DONE, Cue fetches that branch into the repository on this Mac and checks that its
last commit is the one the DONE named. If it isn't there, or it's a different commit, the item does not move on. It
says "waiting for its branch", and Cue tries the push again.

**5. It joins the landing line.** From here nothing is special. The branch is brought up to main, word-checked,
checked in a clean copy, and lands as one commit. Then the install runs here. The item's page shows the diff from
the fetched branch, so Looks right and Send back work as for any item.

**6. A clash or a failed check goes back to the machine.** The holder there fetches main, fixes its branch, and says
DONE again. So the remote session has to stay reachable until its item has landed, not only until its DONE.

## What can't run on Linux

Two things, said plainly.

**Cue's window checks.** The window load and the window tests drive the Mac's own web view. There is no such thing
on Linux. The plain script tests can run there. Whether the app's build and the Rust tests run on Linux is not
known: the build skips its Mac-only helper programs off a Mac, but nobody has tried the rest. It needs one try on a
real machine; this plan doesn't promise it.

**The install.** Cue installs on this Mac. Nothing installs from another machine.

So for a Linux item, the real check is the one in the landing line here. The remote run is a first pass that
catches the cheap mistakes early. An item that passes there can still fail here, and then it goes back, as in
step 6. A second Mac can run everything except the install, but the landing line runs the check again anyway,
because only that run sees main plus the change.

## Keeping main current on the remote

Landing moves main on this Mac. The other machines only see main once it is pushed. So:

- **Push after each install.** Installs already batch landings about every half hour. One push per install keeps the
  remote at most half an hour behind, and the full check runs once per push. Until piece 2 below builds that
  check in, it is done by hand at that moment.
- **Push before a remote item starts.** If main here is ahead of the remote when the lead routes an item to a
  machine, the Desk says how many landings it is ahead and asks to push first.
- **Cue's own project stays on "ask each time".** It is public, so each push still waits for your yes, with the
  full check first. A private project can use "after each landing" and never be asked.

**If an item started from an old main.** Nothing breaks. The landing line brings every item up to main before it
checks it, and a remote item is no different. If the two fit, it lands. If they clash, it goes back to the machine
with the clashing files named, and by then main should be pushed so the session can fetch it. The cost of a stale
start is a wasted round, not wrong work on main. The item page shows how far behind it started, so you can tell.

## When a machine goes quiet

What you see, and what the Desk does.

**The connection drops.** Today the machine is marked as not reachable on its sessions, and the item just stays
"working". New: the item says "machine not answering" with how long. Your messages are held and sent when it is
back, as today. The session keeps working there under the host program, so a short drop costs nothing.

**It stays quiet.** After thirty minutes the lead asks you: wait, or move the item. Thirty minutes is the same wait
the routing rules use before asking you about a stuck item.

**The host died.** Cue already says the session ended. The item then goes to the lead like any item whose holder
is gone. The clone on the machine still has the work if the machine itself is alive.

**The branch was never pushed.** A DONE with no branch on the remote does not reach the landing line. The item says
"waiting for its branch". If the machine answers, Cue tries the push again, and says why if it fails. If the
machine is gone, the work is only on that machine, and the item says exactly that: "its work is on the machine,
not pushed".

**Half-pushed work.** Git moves a branch whole or not at all, so "half" means the branch is there but older than
the work: later commits, or edits never committed. The commit named in the DONE line catches this, as in step 4.
To keep the loss small when a machine dies mid-item, the branch is also pushed at each natural stop, not only at
DONE. The item page shows when it was last pushed.

**Moving an item away.** The new holder starts from the last pushed branch, fetched here, with the usual hand-off
note. Anything not pushed stays on the quiet machine. If it comes back, its session is told the item moved.

## What is new work

1. **A remote DONE that lands.** Fetch the named branch, compare its last commit, and put it in the landing line in
   place of the straight jump to "to check". The lead's instructions stop saying remote work changes nothing.
2. **Cue's push from the machine.** The word check and the full check for things that must not go public, then
   the branch pushed, at DONE and at each stop. That full check is done by hand today, so it has to be built into
   Cue as part of this piece, and it then runs before the push of main from this Mac as well. Plus the
   "Ask me before" check added to the hooks on other machines, so a Claude or Codex session there can't push main
   unasked.
3. **Starting a Desk session on a machine.** Per item, with a default per Desk, and the machine shown on the card.
   The first time the lead wants to use a machine for a Desk, it asks you.
4. **Quiet machines on the item.** "Machine not answering", "waiting for its branch", last pushed, and the lead's
   question after thirty minutes.
5. **Main pushed on time.** The ask before a remote start, and the push after each install.

Piece 2 is built first. The full check covers both pushes: every branch pushed from another machine, and every
push of main from this Mac. Until it is built into Cue, no branch is pushed from another machine, so remote items
don't run at all, and the push of main keeps its by-hand check. Everything above about pushes at DONE and at each
stop describes how it works once piece 2 is in place.

To start small: one item at a time per clone on a machine. Several at once needs a copy per item there, not
planned here.

## Risks

- **A public branch.** For a public project, anything pushed is public at once. That is why Cue pushes, after the
  full check, and not the session. A private remote just for work branches is the safer choice if you want one.
- **Work only on the machine.** Between pushes the work lives in one place. Pushing at each stop limits the loss.
  But if the full check finds something at a stop, that push is blocked, and the work stays only on the machine
  until the finding is fixed.
- **Two checks that disagree.** Passed there, failed here. The item goes back, and its page says which check failed
  where, so it doesn't read as a mystery.

## Considered

**Keep today's path and add a button to bring the commit over.** The smallest change. Not chosen: remote items
would still skip the landing line, so no check, no word check here, and work moved by hand.

**Cue brings the remote copy's work back itself (the #491 suggestion).** Cue would send main to the machine over
its own connection and pull the work back at DONE, with no shared remote in between. Not chosen: it makes Cue a
second way to move git history, and the machine can't work at all without this Mac. A pushed branch is plain git,
is still there when this Mac is off, and is how a second person would work too.

**A fully remote Desk.** The machine keeps its own main, lands and checks there, and this Mac only fetches. Not
chosen: two landing lines to keep in step, and for Cue the window checks and the install can't run there anyway.
