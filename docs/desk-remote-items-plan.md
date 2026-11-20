# Plan: Desk items worked on other machines

Item #491. A plan only: no code changes. Written to be read aloud.

## In short

Cue can already run a session on another machine, show its chat, answer its questions and commit its files there.
What it can't do yet is treat that session as a full Desk worker: it gets no copy of its own, its work never reaches
the main line on your Mac, nothing checks or installs it, and nothing happens when the machine goes quiet.

The suggestion is option B below: a remote item works in its own copy on the remote machine, and at its DONE Cue
brings that copy's work back to your Mac as one branch and lands it through the same landing line as every other
item. You pick the machine per item, with a per-Desk default.

## What Cue does with remote sessions today

This is read from the code as it is now, not from memory.

**Machines.** You add a machine once (New, then Machine). Cue reaches it with your own secure-shell setup, never asks
for or keeps a password, and keeps one shared connection per machine. When a machine needs a login, Cue opens a
terminal tab and you log in there yourself. Cue installs no agent there; it only puts its own hooks there and,
for hosted sessions, its small host program in a hidden folder in your home on that machine.

**Starting a session there.** A hosted session can start on a machine: the agent runs under Cue's host program on
that machine, which keeps it running when Cue on your Mac quits or the connection drops. Cue asks you once to trust
each folder on each machine. A session can also run in a terminal on the machine, which Cue shows in its built-in
terminal.

**Seeing its state.** Cue copies the session's conversation log from the machine to your Mac as it is written, within
a fraction of a second, so the chat, steps and context look the same as for a local session. Hook events from the
machine arrive as if they came from your Mac, including permission questions, and your answers go back. Cue reads
every machine on a timer: every ten seconds where it has a live session, every minute elsewhere, backing off to
every five minutes for a machine that doesn't answer, without holding the others up.

**Connection drops.** Messages to a hosted session on a machine are held while the connection is made again, then
sent once, in order. When the connection comes back, Cue picks each session up where it left off. If the host there
died and lost the session, Cue lets it go and says it ended. Pictures you paste are copied to the machine first.

**The Desk and a remote session.** A remote session can hold a Desk item: the lead can route to it by name. Its
DONE line moves the item to finishing, and its TO LEAD lines reach the lead the same way as anyone's. After that,
remote work takes a separate, older path:

- It gets **no copy of its own**. Every new Desk item that works on your Mac gets its own copy of the repository and
  its own branch; a remote session keeps working wherever it already is.
- At DONE it goes **straight to "to check"**. Nothing is committed or landed at that point.
- **Looks right commits its files on the remote machine**, in that machine's checkout, with the word check run there.
  Undo takes that commit back, also there. The commit never reaches the main line on your Mac.
- **No checks and no install.** The project's check and Cue's own window check run only for local work. The install
  step is skipped: "nothing to install here".
- **Files another session is busy with** are not looked at on a machine, so two sessions there can tread on each
  other's files without Cue noticing.
- **Starting a fresh session for an item** ("route to new") always starts it on your Mac, in the project's folder
  there. There is no way to say "start it on that machine".
- Crew teammates of a remote lead work there too, and their files are only seen at their finish, not after each
  command.

So today a remote session can do Desk work, but it ends as a commit sitting on another machine that you then have
to move by hand.

## What a Desk item needs to run on another machine

**1. Getting its copy there.** The machine needs the repository, at the main line as it is on your Mac right now,
plus a branch for the item. Two ways: the machine keeps its own clone and fetches the main line from a shared
remote repository, or Cue sends the main line straight from your Mac to the machine over the connection it already
has. The second works even for work you haven't pushed anywhere, which is the usual case for the Desk, since landing
moves your Mac's main line before anything is pushed. Each item then gets a copy on the machine, the same shape as
the local ones: a separate folder, its own branch, at most a few per repository.

**2. Running the session.** This part exists: start a hosted session on that machine, in that copy's folder, with
the item's brief. The new piece is letting the Desk start one there (today "route to new" only starts on your Mac),
and trusting the copies' folder once so it doesn't ask per item.

**3. How Cue sees its state.** Also mostly there: the log copy, hook events and the timer read. What's missing is
the Desk's own view of the copy: which files changed, the diff on the item page, and busy-file checks. These can be
read over the same connection when asked, the way Looks right already reads changed files there today, not on every
tick, so a slow machine doesn't slow the Desk.

**4. How its work comes back and lands on main.** At DONE, Cue takes everything in the remote copy, committed or
not, as one commit on the item's branch there, then brings that branch to your Mac. From there it joins the landing
line like any other item: brought up to date with main, word-checked, checked, then main moves forward. A clash with
main goes back to its holder, who resolves it in the remote copy. The landing line itself doesn't change; only the
step before it does.

**5. Checks and installs.** Once the work is on your Mac, the project's check and Cue's window check run here, as
for any item, and the install runs here as today. Running checks on the remote machine is possible later, for
projects that need its hardware, but it isn't needed to start.

**6. TO LEAD messages.** Already work: they come from the session's reply, which Cue reads from the log copy. No
change needed.

**7. Hand-offs.** Moving an item from a remote session to another session already writes a hand-off note. The new
part is where the work is: if it moves to a session on your Mac, Cue first brings the remote copy's work over as a
branch, so the new holder starts from it. Moving it to another machine works the same way through your Mac.

**8. When the machine goes away.** Today the session just looks quiet and the item stays "working". Needed:
- After a few minutes with no answer from the machine, the item shows "machine not answering" in plain words,
  with how long.
- Messages to it are held and sent when it's back, as they are now.
- After a longer time (say thirty minutes, the same wait the Desk uses before asking you), the lead asks you: wait,
  or move the item. Moving it takes whatever Cue last brought back, which is why step 4 also brings the branch over
  at each DONE and can bring it over on request.
- A machine that comes back with the copy intact picks up where it was. A machine that comes back empty (wiped,
  or a fresh container) means the item restarts from its last brought-back branch, or from main.

## How you pick where an item runs

- **Per Desk default.** In the Desk's settings: "New sessions start on: this Mac / <machine>". Most Desks stay on
  this Mac.
- **Per item.** On the item page and in the new-item box, a small "Runs on" choice with the same list. Setting it
  on an item that's already working asks first, since it means a hand-off.
- **The lead can suggest it**, for example for a heavy build or a long test run, but routing an item to another
  machine for the first time asks you, the same way the lead asks before pulling in a session outside the Desk.
- The item card shows the machine's name next to the holder, so you always see where the work is.

## Cost and risks

**Cost.** Option B is roughly four pieces of work: start a Desk session on a chosen machine; make and remove copies
there; bring a copy's work back to your Mac at DONE and on hand-off; show a machine that stopped answering. Each is
about the size of one of the recent Desk landing items. Running cost: one extra transfer per item at DONE, small
for code changes.

**Risks.**
- **Two copies of the truth.** Work lives on the machine until it's brought back. A machine that dies before that
  loses what wasn't brought back. Bringing work back at each DONE and on request limits this.
- **Slow or flaky connections.** Anything the Desk reads from the machine must never block the Desk's own updates.
  Cue already has a rule that it never looks up a remote folder from your Mac on the spot; the new reads must follow it.
- **Different tools on the machine.** A project's check may not run the same there. Running checks on your Mac
  avoids this for now.
- **Private code on another machine.** The repository goes to a machine you chose; nothing goes anywhere new beyond
  that. Worth saying once when you first pick a machine for a Desk.
- **Containers that reset.** A container that loses its disk on restart loses unreturned work. The "comes back
  empty" path above covers it, but the work since the last DONE is gone.

## Options

**A. Keep today's path, finish it.** The remote session works where it is, as now, and Looks right commits there.
Add only: start a Desk session on a machine, show "machine not answering", and a button to bring the commit to your
Mac by hand. Smallest change. But remote items never go through the landing line, so they skip checks, the word check
on your Mac, and the "one item, one commit on main" rule. You'd still move work by hand.

**B. Remote copy, local landing (suggested).** Each remote item gets its own copy on the machine; at DONE its work is
brought back to your Mac as a branch and lands through the same landing line as every other item. Checks and install
run on your Mac. Every item lands the same way no matter where it ran, the landing line doesn't change, and a machine
that goes away loses at most the work since its last DONE or bring-back.

**C. Fully remote Desk.** The remote machine has its own main line, its own landing and checks, and pushes to a
shared remote repository; your Mac only fetches. Best for a machine that is the real home of a project (for example
a server project you never build on your Mac). But it's two landing lines to keep in step, and installs of Cue
itself can't happen there. Too much for now; B can grow into it later for one project at a time.

**Suggested: B.** It reuses everything that already works for remote sessions (starting, log copies, held messages,
reconnecting) and everything that already works for landing, and only adds the step between them.

## Suggested order, if you pick B

1. Desk starts a new session on a chosen machine (per-Desk default, per-item choice, machine name on the card).
2. Remote copies: made at start, removed after landing, kept on a drop, as local copies are.
3. Bring back at DONE and on hand-off, then into the landing line.
4. "Machine not answering" on the item, and the lead asking you after thirty minutes.
5. Later, only if a project needs it: checks run on the machine.
