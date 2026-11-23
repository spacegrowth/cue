# Review of design-fixes' routing suggestion (#774)

The suggestion, as the Desk lead passed it on word for word (its session's own log is on another
machine, so I could not read it there; the questions it put to the review are left out):

> The question: a session worked on the files an item needs, but it's cold (its cache expired).
> Should the Desk wake it, or start a new session? The suggestion: (1) Cold and under ~150k: wake
> it. The one-time re-read is cheap, and it keeps what it learned. This matches today's
> docs/desk-routing-rules.md rule 3. (2) Cold and ~250k or more: start a new session with a short
> pointer to the old item and its commit. The uncached re-read costs about as much as reading
> 250k+ fresh, and big chats get sloppier. (3) In between: the same day, wake it; days later,
> start a new one. (4) Warm vs cold: it's only a tie-breaker, not a reason to skip a session.

## Verdict

Don't adopt it as written. It is right that today's limit is too strict, and its "short pointer"
is its best idea. But it leans on a rule that was replaced, and its numbers are too generous.

**How it works today.** A cold session takes new work only under 60 thousand. Above that, Cue
marks it "cold: no new work" and the lead starts a fresh session. That was built on 18 November,
after cold wakes ate about a fifth of the usage limit in one morning. The 150 thousand rule the
suggestion cites is the older one; the merged rules replaced it.

## Five points

1. **Raise the cold limit, but to about 100 thousand, not 150.** By the merged rules' own
   figures, a session that knows the area saves about a dollar ten per item, and waking a cold
   one costs that much at about 140 thousand. So at 150 the wake wins nothing; at 100 it wins
   about 30 cents. Cost: more of the usage limit goes on wakes, which the 60 limit was built to stop.

2. **Drop the 250 thousand line and the reason given for it.** A new session does not read 250
   thousand; it reads a short note and a few files. The wake cost simply grows with size, so
   there is no special point at 250. One limit is enough, and easier to predict. Cost: none.

3. **Replace "same day or days later" with "has that area changed since?"** A session goes cold
   after about an hour, so that afternoon and next week cost the same to wake. What ages is its
   knowledge: if other work has landed in that area since, start fresh. Cost: Cue must keep each
   finished item's files, which the rules already ask for and is not built.

4. **Keep the short pointer, and have Cue write it.** Today a fresh session gets a note only if
   the old session was handed off; a cold session that was skipped leaves nothing behind. A
   pointer to the old item, its files and its change, made from the record without waking
   anyone, fixes that. Cost: the same record-keeping as point 3.

5. **"Warm or cold is only a tie-breaker" holds only under the limit.** Above it, cold is a
   reason to skip, and should stay one. Cost of the suggestion's wording: it would undo the
   18 November fix.

**Pausing idle sessions after 30 minutes (#773).** I could not see that item's details from
here. If a paused session comes back cold, nearly every session with a record will be cold when
an item arrives, so this becomes the main rule, not the rare case. The pointer then matters
more, and a generous limit costs more.

## What to try first

Build the pointer (point 4) with the files record behind it. Then raise the cold limit from 60
to 100 thousand as a trial, and compare the cost of the next ten items that wake a cold session
with ten that start fresh from a pointer. The dollar ten saving has never been checked; this
trial checks it.
