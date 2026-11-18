# What burned 31% of the limit in 20 minutes (2026-11-18, ~11:00–11:32 PT)

Read from the Claude Code transcripts of the last 40 minutes. Usage counted once per API reply.

## Totals

32.0M input tokens in all: 30.3M cache reads, 1.55M cache writes, 0.17M output.
Weighted roughly by price (read 0.1, write 1.25, output 5): **reads ~52%, writes ~33%, output ~15%**.
So most of it was big contexts read over and over again, not new text.

## Who used it

| Share | What | Turns | Context | Why |
|---|---|---|---|---|
| **44%** | "loops-fixes" teammate (Build step 1, five Loops fixes), Opus | 153 | grew to 168k | One shell command per turn (145 Bash calls: sed/grep/cargo), so a context of ~110k was read again 153 times in 20 min |
| 10% | "finish-248" teammate, Opus | 43 | up to 118k | Same pattern: 44 single Bash calls, cargo test reruns |
| 8.5% | "loops-mocks" design teammate, Fable | 13 | up to 171k | Canvas artifact read in whole (~200k written to cache in 13 turns) |
| 5% | "item-page" design teammate, woken after 3 h | 6 | 157k | Cold wake: a 138k context written again from nothing |
| ~25% | Six lead sessions (incl. the Desk lead) | 3–20 each | 70–150k | Four of them were woken after ~3 h idle: each paid 60–120k cache writes on its first turn |

Cold wakes (sessions or agents idle ~3 h, cache gone) came to ~820k cache writes, about **18% of the burn**.
Test polling and loops were not the cause: no session waited in a loop. The cost is turn count × context size.

## Does time of day matter?

Not for the token counts: the same work costs the same tokens at any hour. It can matter for the
*limit*: Anthropic has said session limits drain faster at weekday peak hours (US mornings). This
burst was at 11:00–11:30 PT, at the end of that window, so it may have counted a bit heavier. The
logs can't show that; the turn × context pattern above explains the 31% on its own.

## Top changes to cut it

1. **Fewer, bigger turns for build teammates.** Brief them to batch reads (one command that prints
   all the pieces they need), and to run a step's tests once at the end, not after each edit. Split a
   long build into steps with a fresh teammate per step (the crew rule already says so) so context
   stays under ~80k. Halving turns on the loops-fixes agent alone would have saved ~20% of this burst.
2. **Don't wake cold big sessions for new work; start fresh with a short brief.** A 3 h idle
   session pays its whole context in cache writes on the first turn (60–140k here). The merged
   rules (`docs/desk-routing-merged.md`, rule 17) still allow a cold wake up to 110k for new work
   and 140k for follow-ons; for new work, ~60k would have avoided most of today's cold wakes.
3. **Design teammates read one canvas page, not the whole artifact**, and run on Sonnet when the work
   is moving mocks and editing a doc.

## Does Cue's own reading or polling make sessions burn tokens?

No, not by much. Cue reads session logs and the hub's state from files, and that costs no tokens.
Tokens are spent only when Cue *writes into* a session and makes it take a turn. I matched every
turn (since 10:55 PT) to the message that started it:

| What started the turn | Messages | Share of the burn |
|---|---|---|
| Work handed out: Desk item routes, teammate briefs, owner's own messages | 26 | ~62% |
| Turns of the loops-fixes teammate after its context was compacted (still the same work) | 2 | ~28% |
| "Carry on, your usage limit reset" (resumes a session, cold cache) | 2 | ~5% |
| Desk lead wake-ups (`<desk>` updates) | 4 | 1.4% |
| Teammate finished / agent-to-agent messages | 6 | 1.5% |
| "Done? Write DONE: #n" nudges | 2 | 0.7% |

So Cue's nudges (lead wakes, "Done?", notices) come to about 4%, or about 9% if the limit-reset
resumes count. Those resumes are real work restarting after a limit, not overhead. An event layer
(sessions and hooks posting to a bus that Cue reacts to) would trim part of that 4% at most. It is
not worth building for cost; I didn't sketch one.

**The real cause** is described above: big teammates doing one small step per turn at 100k+
context, a teammate that ran long enough to compact twice, and large sessions woken after their
cache expired. The changes listed above are where the savings are.
