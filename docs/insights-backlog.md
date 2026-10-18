# Insights: what's next

What Cue's Insights could add, from studying the tools that analyse agent sessions (2026-10). The insights
on cards and the Insights tab already use some of it: the turn as the unit, corrections read from the next
prompt, interruptions, tool errors, context growth (the hand-off insight), cache expiry, and keeping turns in
Cue's own database because Claude Code deletes transcripts after about 30 days (`cleanupPeriodDays`; Cue never
changes that setting). The rest is below.

## Habits, ranked by what they cost

Each habit is a filter over the turns table, the cost at stake for a matching turn, and one fix to suggest.
Rank by cost at stake, not by a severity label, and compare this week with the week before.

| Habit | A turn matches when | At stake | The fix to suggest |
|---|---|---|---|
| Corrected | the next prompt reads as a correction | the turn's cost | Front-load the file, the expected behaviour and how to check it, so the first attempt lands. |
| Interrupted | you stopped it mid-turn | the turn's cost | State the scope and a stopping point up front ("only touch X", "plan first, then wait"). |
| Quick check-ins on a premium model | a top-tier model, prompt ≤ 100 chars, no tool calls | the cost minus what a cheaper model would charge | Fold "are we done?" into the next real instruction, or ask on the side (btw). |
| Long sessions | turns past the 40th in a session | the cache reads | Hand off or start fresh when the task changes. |
| Struggling turns | 3 or more failed steps in one turn | the turn's cost | Give the exact command, path or setting it keeps missing, or put it in CLAUDE.md. |

Costs need a price table: list prices per million tokens by model-id prefix (longest prefix wins), with cache
reads at the model's published rate (else 0.1× input) and cache writes at 1.25× input (5-minute) and 2×
(1-hour). Price at API rates even on a subscription, so weeks and models compare. Unknown models price at 0
and are flagged. Checked against Claude Code's own `cost-state` totals, this lands within about 3%.

## More to detect

All of these are computed from the logs; none needs a model to judge.

- **Cost and waste:** cache-hit rate under 10% on turns over 5k tokens; high or max effort on most turns (the
  log has an `effort` field); over 5k output tokens for a prompt under 200 chars; runaway turns with 15+ tool
  calls; what background agents cost as a share of the session.
- **Prompting:** a session's first prompt with no plan, list or file reference when the session ran 3+ turns;
  prompts repeated across sessions (fuzzy match) as candidates for a skill; instructions repeated across 2+
  sessions as candidates for CLAUDE.md. Don't flag a short follow-up ("go ahead", ≤ 8 words) as lazy.
- **Sessions:** 4+ kinds of work in one session (drift); single-prompt sessions as a share of all; several
  sessions active within the same 30 minutes (show, don't judge).
- **Friction:** the same step failing 3+ times in a row (error loops); a session whose last 3 turns failed, or
  whose second half fails 1.5× as often as its first (abandoned); a reply of 20+ changed lines accepted within
  15 seconds.
- **Outcome:** whether a turn's work was committed (a `git commit` step, or the branch's log afterwards), so
  prompts are judged by what happened, not by how they read; cost per finished task, by model, effort and
  project.
- **Signals other tools drop:** slash commands, permission mode, effort, skills, hooks, background agents,
  where the session started (terminal, voice, web), multi-repo workspaces.
- **Trends you can trust:** weekly rolling medians per measure, split by model and kind of task, with
  before-and-after markers ("started planning first on the 20th: corrections went from X to Y").

## Optional: a model's judgment

Claude Code's built-in `/insights` asks a model, per session, for: goal categories, outcome
(fully / mostly / partially / not achieved), satisfaction taken only from your explicit words ("great",
"try again"), session type, and friction (misunderstood request, wrong approach, buggy code, rejected action,
excessive changes). Fixed categories keep its charts reproducible. Its limits: no trend, tokens but no cost,
lossy (it cuts each message to a few hundred characters), and at most 50 new sessions per run.

If Cue adds this: only when you ask, cached per session by its log's hash, and with keys, emails and tokens
redacted first.

## Prior art

- Microsoft AI-Engineering-Coach (github.com/microsoft/AI-Engineering-Coach): 46 deterministic rules as data
  (thresholds plus inline test cases), weekly practice scores. Weak on cost; parses Claude Code partly.
- ctxray (github.com/ctxray/ctxray): a 0–100 prompt score from regexes and word counts, 7-day trends.
- Prompt-sensei (github.com/chengzhongwei/Prompt-sensei): a model scores prompts on 7 dimensions, by stage.
- ccusage (github.com/ccusage/ccusage): cost reports only.
- Claude Code's OpenTelemetry export records only from the day it's enabled; the logs already hold more.
