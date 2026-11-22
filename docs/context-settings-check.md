# Why sessions squash their history at 200k on a 1M model

Checked 2026-11-21 against Claude Code 2.1.330. Nothing was changed; this is only the list.

## Short answer

One line in `~/.claude/settings.json` does it: `"autoCompactWindow": 200000`. Claude Code's own
words for that setting: it "summarizes the conversation as the context nears this size, or your
model's maximum context window if that is smaller". The model is 1M, the setting is 200k, so the
smaller one wins. In practice the squash comes at about 166k (the 200k less the room kept for the
reply), which is the figure the Desk logs show.

Raising that line alone is not enough. Two more limits sit right behind it, at 180k and 200k, and
would become the new wall.

## What is set, and what it does

| Where | Setting | Now | What it does |
|---|---|---|---|
| `~/.claude/settings.json` line 271 | `autoCompactWindow` | `200000` | **The cause.** Sessions summarize themselves near 200k whatever the model holds. |
| `~/.claude/settings.json` line 56 | `model` | `opus[1m]` | Right. This is the 1M model. Sessions started as Fable also get the 1M version. |
| `~/.claude/settings.json` | `idleCompaction` | not set, so on | A session over 200k left alone is summarized just before its one-hour cache runs out. Second wall, at 200k, for anything you step away from. |
| `~/.claude/settings.json` | `PreCompact` hook | Cue's | Harmless. It only tells Cue "this session is compacting" so the card shows it. It causes nothing. |
| Cue's `config.json` | `desk.handoff_k` | not set, so 180 | On a Desk, a session that reaches 180k is handed off to a fresh one. Never acts today, because the squash at 166k comes first. Third wall once the first is lifted. |
| Environment, project and local settings | | nothing | No override anywhere: no `CLAUDE_CODE_AUTO_COMPACT_WINDOW`, nothing in the project's `.claude/settings.json` or the two `settings.local.json` files. |

## Cue's own rules

These are in Cue's code and are about money, not the window. A session whose cache has run out
re-writes its whole context on its next turn, so Cue keeps cold sessions small.

- **Hand-off note before the cache runs out** (`src-tauri/src/hub/cache_tick.rs`): from 50k for a
  session with an item at work, from 100k for one holding any open item or the lead, and none from
  200k up, because Claude Code's idle compaction covers those. This follows `idleCompaction`
  by itself: turn that off and Cue asks for notes above 200k again.
- **"Cold: no new work"** (`src-tauri/src/desk/hub/lead/run.rs`): a cold session of 60k or more is
  given no new items. Not tied to the window. Keep.
- **Hand off at 180k** (`desk.handoff_k`, `src-tauri/src/config.rs`): see the table.
- **Insights' "fresh session due"**: 60% of the session's real window, so on 1M it already waits
  until 600k. Fine as it is.
- **The routing rules** (`docs/desk-routing-merged.md`, rule 17): the 110k and 140k limits were
  written around the 166k squash. They are words for the lead, not code. They need new numbers if
  the window goes up.

## What each should be for a 1M window

| Setting | Suggested | Why |
|---|---|---|
| `autoCompactWindow` | `500000` | Two and a half times today's room, with a known number the other limits can be fitted to. The alternative is to delete the line ("auto"), which Claude Code recommends as tuned per model, but I could not see what number auto gives for this model. `1000000` is allowed too. |
| `idleCompaction` | leave on to start | It only acts on a session left idle for an hour, and it saves the price of re-writing 200k and more on return. Set `false` if you want a session to keep its full history across a break; then every return to a big cold session costs a full re-write. |
| `desk.handoff_k` | `400` | Must sit under the window or it does nothing, and over 180 or Desk sessions gain nothing. `0` turns it off. |
| `model`, the hook, the cold limits | no change | They are right, or are about cost and not size. |
| Routing rule 17 | rewrite the 110k and 140k lines | Only after the numbers above are picked. |

## The cost to know before saying yes

Every turn reads the whole context from the cache, and a cold return writes all of it again. A
500k session costs about three times a 166k one per turn, and its cold return about three times
as much. Fewer squashes mid-item is what that buys.

## To apply

Two lines to edit (`autoCompactWindow` in Claude Code's settings, `desk.handoff_k` in Cue's
config), then the wording of rule 17. New sessions pick the window up when they start; I did
not check whether running ones do without a restart. Nothing is done until you say which numbers.
