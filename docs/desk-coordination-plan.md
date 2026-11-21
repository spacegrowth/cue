# Desks on the same code: knowing about each other

A plan for #534. Status: option B picked on 20 November 2026, built and landed as #540 (cb8f211dc) on
21 November 2026. That build covers all five steps below.

## The problem

One project can have several Desks. Cue has two today: the main one and a loops Desk, both on Cue's own
code. Each Desk's lead sees only its own items. Every item works in its own copy and lands on main
through the same landing line, so two Desks only find out they touched the same files when the second
one lands.

It already went wrong:

- **20 November.** #509, a refactor on one Desk, was moving `hub/desk_*.rs`. Meanwhile #525, a small
  fix, was working nearby. The only reason #525 didn't land a clash is that the lead noticed by hand
  and told its session to rebase once #509 landed.
- **17 and 18 November.** A session on another Desk changed `ui/core.js` and made `desk/approvals.js`
  while one of this Desk's items was working in the same place. Main broke, and the review spent an
  evening working out whose edit it was (`docs/desk-fable-review-2026-11-17.md`,
  `docs/desk-finish-breaks-main-2026-11-18.md`).

Within one Desk, routing rule 8 (`docs/desk-routing-rules.md`) already makes an item wait behind an
open item that would change the same files. Between Desks there is nothing like it. Cue even refuses
a wait across Desks ("#a belongs to another Desk").

## What it should do

1. **A big item holds the code it changes.** A refactor says which part of the code it holds and till
   when. While the hold stands, new work on other Desks that would touch that part waits.
2. **Leads tell each other before they change shared code,** and agree an order: who lands first, who
   waits, who parks.
3. **You hear only when they can't agree.** Then you get one question that shows both sides and has
   two answers. A heads-up that was settled stays between the leads, and you can read it if you want.

## What's there to build on

- **Messages between Desks (#472).** A message from another Desk's session shows in a lead's chat as a
  band marked FROM ANOTHER DESK, with that Desk's chip, the sender and the item it holds. The lead
  answers it, and you don't need to. Today the message gets there through a session messaging another
  session directly. The lead has no line of its own for it.
- **Lead lines.** ITEM, ROUTE (with `after:`), ASK, PARK, LAND FIRST and the others. Cue reads each
  line and answers every route ("Taken", "Refused: …").
- **The one list of waits** (`docs/desk-landing-design.md`, part 5). Every wait goes in one list that
  says who waits on what, with a check for circles.
- **Each item's own copy.** Cue knows which files each open item has changed: the diff of its copy
  against main. That is a fact, not a guess.

## Three ways to do it

### A. Heads-up only

A lead about to start a big change writes a line, and Cue sends it as a band to every other lead whose
Desk is on the same repo:

```
TELL DESKS: #509 is moving hub/desk_*.rs and ui/desk_*.js for about 2 hours. Hold new work there.
```

The other leads reply with the same kind of line, and each sets its own order with the `after:` and
PARK it already has. Cue enforces nothing.

- **Cost:** small. One new line, plus the band that already exists.
- **Weak spot:** it works only if every lead reads the band and remembers it. A lead that starts later,
  or comes back after a compact, never saw it. Nothing stops an item on another Desk from starting in
  the held files. This is the 17 November failure again, just with a polite note first.

### B. Holds with heads-up (suggested)

A big item takes a **hold**. Cue keeps the hold and shows it to every Desk on that repo. Leads talk
about it in bands, and Cue makes the waits they agree on real.

**Taking a hold.** The lead of the big item writes:

```
HOLD: #509 | area: src-tauri/src/hub/desk_*.rs, ui/desk_*.js | till: it lands | why: the refactor moves these files
```

Cue then does four things:

1. It saves the hold against the repo, not the Desk, so every Desk on that repo sees it.
2. It sends each other lead on the repo a band: "cue (main) holds `hub/desk_*.rs`, `ui/desk_*.js` for
   #509 till it lands".
3. It lists any open item on another Desk whose copy has already changed a file in the area, by name
   and file. The holder's lead sees these at once, not when they land.
4. It puts a line in each lead's summary of its Desk, so a lead that starts later or comes back after
   a compact sees the hold.

**New work waits.** A lead on another Desk routing new work into the area writes the wait it already
knows, now allowed across Desks: `after: cue#509`. If it routes into a held area without that, Cue
answers "Refused: `hub/desk_*.rs` is held by cue #509 till it lands. Wait after it, ask its lead, or
ask the owner." This stays the lead's judgment of the item's area, as rule 8 is today. Cue does not guess
the files of work that hasn't started.

Two checks are Cue's own, because they are facts:

- **At landing.** An item from another Desk whose diff touches a held file doesn't land while the
  hold stands. It waits in the landing line with "waits for cue #509's hold" and offers "Ask its lead".
- **When the hold is taken.** Cue lists the clashes with items already working, as in step 3 above.

**Agreeing an order.** For an item already mid-way in the area, the two leads settle it in bands:

```
TELL DESK: cue | #533 is half done in ui/desk/thread.js, about 20 min from landing. Land it before #509 starts moving files?
```

The holder's lead answers with one of three:

- **Yes.** It writes `LAND FIRST: loops#533 | why: 20 min, before the move`. That is the line it
  already has, now allowed across Desks.
- **No.** It says why, and the other lead parks its item or rebases it after the hold.
- **It can't agree.** One question goes to you.

Every move goes in the decisions list of both Desks, so you can see who moved what and why.

**When you hear.** Only in three cases:

- The leads write that they disagree.
- A band about a clash went unanswered for 15 minutes.
- A hold has stood past its own "till" by more than an hour.

Then you get one question in Waiting on you, on both Desks, quoting both sides. The answers are the
two orders, for example "Land #533 first" or "#509 goes first, #533 waits". Your answer goes to both
leads.

**Lifting the hold.** The hold ends by itself when its item lands, is dropped, or parks. The holder's
lead can lift it early with `UNHOLD: #509`. When it lifts, the items that waited on it start, and each
of them rebases on the new main first. That is the step #525 had to be told to take by hand.

- **Cost:** medium. It needs:
  - two lines, HOLD and UNHOLD, plus TELL DESK and TELL DESKS;
  - the hold saved per repo;
  - `after:` and LAND FIRST allowed to name another Desk's item;
  - one check at landing against held files;
  - holds shown in the lead's summary, on the Desk header and on waiting rows;
  - the escalation question.
  All of it builds on the waits list and the landing line.
- **Weak spot:** the lead still judges the area of new work. A lead that ignores the hold is caught
  at landing, not at the start. That is late, but it is before main breaks.

### C. Cue spots overlaps itself

There are no holds. Cue watches the files each open item's copy has changed, on every Desk on the
repo. When two items from different Desks touch the same file, it tells both leads at once in a band.
A refactor is just an item with a big diff.

- **Cost:** medium to high. It needs a watcher on every copy, and a rule for what counts as a clash:
  the same file, or the same lines?
- **Weak spot:** it sees a clash only once both items have edits. A refactor that is about to move a
  folder has no diff yet, so other work starts there freely. Nothing says "wait, this is coming". It
  is also noisy: small shared files such as `ui/core.js` would set it off all day.

## Picked: B

B is the only one that stops new work before it starts in a place that is about to move, and it keeps
the leads, not you, as the ones who settle the order. It takes from A the band, which is the talk
between leads. It takes from C one fact: the files a working copy has really changed. Cue uses that
fact twice, when a hold is taken and at landing, and never as a guess about work that hasn't started.

The lead's rules gain one paragraph:

> Before an item will move or rewrite a part of the code other Desks might touch (a refactor, a
> rename, a folder move), take a HOLD on that part. Before routing into a part another Desk holds,
> wait after its item, or ask its lead with TELL DESK. Agree the order between you. Ask the owner only if
> you can't.

## What each person sees (the canvas page)

The canvas page "Desks on the same code" shows option B, in light and dark:

- **The holder's lead (main Desk).** Its HOLD line, then Cue's answer: "Held. Told loops. 1 item there
  already changed a held file: #533 `ui/desk/thread.js`". Then the band from the loops lead, and its
  `LAND FIRST` reply.
- **The other lead (loops Desk).** The FROM ANOTHER DESK band with the hold. Its own new item routed
  `after: cue#509`. Its TELL DESK asking to land #533 first, and the answer.
- **You, on either Desk.** A quiet line under the Desk header, "cue #509 holds `hub/desk_*`,
  `ui/desk_*` till it lands". A waiting row: "Waits for cue #509's hold". Nothing in Waiting on you.
- **You, when they can't agree.** One question in Waiting on you, with both sides and two answers.

## Steps (B, all built in #540)

1. HOLD and UNHOLD lines, the hold saved per repo, holds in the lead's summary and on the Desk header.
2. TELL DESK and TELL DESKS lines, sent as the #472 band, with no lookups by the lead.
3. `after:` and LAND FIRST allowed across Desks on the same repo. Waits across Desks go into the one
   waits list, and its circle check covers them.
4. The check at landing against held files, and the clash list when a hold is taken.
5. The question to you when the leads can't agree, a band goes unanswered or a hold overstays.

Steps 1 and 2 are useful on their own. With them, leads can already hold and talk, as A does.

What #540 (cb8f211dc) built, step by step:

1. Holds: `src-tauri/src/desk/holds.rs`, saved in `desk_holds`, shown in the lead's Desk block and under
   the Desk header. A hold ends by itself when its item lands, is done, dropped or parked.
2. TELL DESK / TELL DESKS: `src-tauri/src/desk/tell.rs`, sent as the FROM ANOTHER DESK band
   (`ui/desk/lead.js`), each band kept in `desk_bands` with whether it was answered.
3. `after:` and LAND FIRST across Desks: `src-tauri/src/desk/across.rs`, as `#509` or `cue #509`, in the
   one waits list and its circle check.
4. The check against held files: `src-tauri/src/desk/hold_check.rs`, both when a hold is taken (the clash
   list) and at landing ("Waits for cue #509's hold on …").
5. The question to you: `src-tauri/src/desk/escalate.rs`, for a DISAGREE line, a band unanswered for
   15 minutes, or a hold more than an hour past its till.
