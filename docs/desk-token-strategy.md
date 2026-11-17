# Desk routing strategy, priced (#187)

This builds on the routing rules from #180 (docs/desk-routing-rules.md). It takes each of those rules and keeps it, changes it or adds to it, using what the Desk's own logs say things cost. It covers the Desk from its first decision, 16 Nov 05:36, to 17 Nov 09:26. Nothing in Cue has been changed yet.

Costs are at API list prices, the way Cue's Insights show them. On a subscription nothing is billed, but these are the tokens that fill the 5-hour limit. Savings are per day at that pace.

## The short version

- The Desk used about $464 in 28 hours. About $108 of it, almost a quarter, moved no item forward.
- The whole strategy saves about $40 a day, about 9 percent. No item waits longer because of it.
- Most of the saving comes from two changes. First, no new item for a session over 110k: it hands off between items instead of compacting in the middle of one, about $18 a day. Second, a quieter Desk lead that hands itself off at 80k, about $15 a day.
- Nearly every cold wake came from the 5-hour limit, not from routing. 23 of 25 cold wakes, $15.90 of $16.50, came after the limit stopped everything for about 2 hours.
- Keep-warm pings are cheap but rarely needed. A ping costs about 2 cents for a 100k session, against 78 cents to wake it cold. Only 2 cold wakes, worth 64 cents, happened in normal running.
- Neither of Cue's hand-off notes at cache expiry was written. Both were asked during a limit stop, and both sessions went cold anyway.

## What things cost

These are for a session holding C tokens of context, on Opus 5.5.

- A cold wake writes the whole context into the cache again. It costs about C times $7.80 per million: 78 cents at 100k, $1.17 at 150k.
- A keep-warm ping reads the context once. It costs C times 20 cents per million plus about half a cent: 2 cents at 100k, sent every 55 minutes.
- A hand-off note is one warm turn: C times 20 cents per million plus about 5 cents. The fresh session that reads it costs about 25 cents to start.
- A fresh session costs about 25 cents to start, plus about 50 cents to a dollar to find its way around. An item given to a fresh session cost $1.74 at the median; one given to a session that had worked nearby cost 64 cents.
- A typical item takes about 14 calls. Each one reads the whole context again, so every 10k of old context costs about 3 cents per item while warm.
- Sessions compact by themselves at about 166k. That is before Cue's own hand-off mark at 180k, so that mark never acts today.

## Rule by rule

The numbers are the rule numbers from #180.

### Step 1: who takes it

**Rule 1, keep.** The session that last finished work in this area goes first. One addition: when that session has been handed off, its record moves with the note to the session that took over. Follow-ups ($36) and send-backs ($30) already go back to the session that did the work, and the next item in a session that knows the area is the cheapest kind. This rule is the base the rest builds on. It doesn't slow work.

**Rule 2, change.** When two sessions both have a record, the one with nothing open still wins. Then, if their sizes differ by more than 50k, the smaller one wins; otherwise the one that finished there most recently. 50k more context costs about 14 cents more per item. Saves under a dollar a day. Doesn't slow work.

**Rule 3, change.** Cold or idle is still not a reason to skip a session. Wake it if it holds under 110k, not 150k, the same cap as rule 4. Warm only breaks a tie. A cold wake at 110k costs 86 cents, about what a fresh session costs once finding its way is counted. Above that, a fresh session is cheaper, and the item would likely hit compaction anyway. Saves about 50 cents a day. Doesn't slow work, since a fresh session starts at once.

**Rule 4, change. The biggest saving, about $18 a day.** A session still holds at most two open items. And no new item goes to a session over 110k, not 150k. A session over 110k hands off when its current item is done, and the next item goes to the fresh session that reads its note. Cue's own hand-off mark moves from 180k to about 140k, so it comes before compaction. Today 77 compactions cost $26.50 in rewrites plus about $8 in summaries. Another $36.50 went on reading old items' context again, $25.70 of it in items started above 100k. Doesn't slow work: a note takes about a minute, and a compaction in the middle of an item takes longer.

**Rule 5, change. About $6 a day.** A new session still starts when no session fits, when you ask, or when the work needs fresh eyes. Added: one review and one re-check per crew, then the next round comes to you. "Done?" is sent once per item; some were sent twice, two seconds apart. No second crew for work a crew already holds; the Desk list mocks crew was started twice. Fable only when you ask for it, as today. Crew reviews cost $9.70, Fable reviews $13.40, "Done?" checks $3.40. Doesn't slow work; fewer rounds is faster.

**Rule 6, keep.** When the match is unsure, the lead says so. No cost.

### Step 2: now, or after another item

**Rule 7, keep.** If the session picked has an item at work, the new one waits behind it. A session at work stays warm, so the waiting item starts warm.

**Rule 8, keep, with keep-warm pings added.** An item that touches the same files as another session's open item waits behind it. Added: if the session it goes to is over 60k and idle, Cue pings it just before its cache runs out, up to 4 times, while the item ahead is still open. The same goes for a session whose item is waiting for your check. No ping while the 5-hour limit is hit. A ping pays for itself if the work comes back within about 30 hours. In these logs no such session went cold, so it saves almost nothing yet; it is cheap insurance. Pinging every idle session instead would cost about $2.30 a day to save 64 cents, because 21 idle sessions were never woken again. Doesn't slow work.

**Rule 9, keep.** If the item ahead has been open over 30 minutes, the lead asks you first. This is about time, not tokens.

**Rule 10, keep.** The why line names the record. No cost.

### Added rules

**Rule 11, new. A smaller, quieter Desk lead. About $15 a day.** Cue wakes the lead only for what it acts on: a new item, your message, an ask, a DONE, a send-back, a crew report. Other Desk changes ride along with its next wake. The lead hands itself off at about 80k. It woke 407 times for $28.90, at a median context of 107k, and 130 of those turns decided nothing. Doesn't slow work, since everything it would act on still wakes it.

**Rule 12, new. Near the 5-hour limit.** At about 90 percent used, the lead starts no new item and no crew, and lets running items finish. After a stop, it wakes only sessions holding an open item, nearest to done first. Four stops of about 2 hours each sent every session cold: 23 wakes cost $15.90. Neither a ping nor a hand-off note can help there. Nothing can be sent during a stop, the stop lasts longer than the hour's cache, and notes asked just before a stop would save at most 44 cents. 12 sessions came back in the first 10 minutes after the 01:20 reset, and the limit was hit again 3 hours later. The token saving is small; the point is not losing 2 hours of work to a stop.

**Rule 13, change. Cue's hand-off at cache expiry.** Keep it: from 100k for a session holding an item, or for the lead. But don't ask during a limit stop, because then it writes nothing. At 100k a note and a fresh start cost about 32 cents, against 78 cents for a cold wake, and the note keeps the area's record for rule 1.

## When to wake cold, ping, hand off or start fresh

- Under 110k: wake it, cold or warm.
- Over 110k: it takes no new item. It hands off after its current item, and the fresh session carries its record.
- Work promised to it soon, either queued behind another session's item or waiting for your check: ping it to keep it warm, up to 4 times.
- Nothing promised to it: let it go cold. Most idle sessions are never woken again.

## What Cue would need to change, on top of #180's list

1. The lead's instructions take these rules, with 110k in place of 150k.
2. The Desk's hand-off mark moves from 180k to about 140k. A session over 110k hands off after its item.
3. Cue wakes the lead only for the events in rule 11, and hands the lead off at 80k.
4. "Done?" is sent once per item, and a second crew for work a crew already holds is refused.
5. No hand-off ask, ping or new start while the 5-hour limit is hit. Near 90 percent, the lead is told to start nothing new.
6. Keep-warm pings, for the two cases in rule 8 only.

Build them in that order. The first three carry almost all of the saving.

## Where the tokens went

### By kind of work

What moved items forward:

- Item work in sessions: $160.90, 35 percent.
- Crews building: $75.60, 16 percent, across 18 crews.
- Your follow-ups the lead passed on to a session: $36.
- Your send-backs: $30.30, most of them on mocks.
- Your messages straight to a session: $18.60.
- Fable reviews: $13.40, $8.90 of it for the routing review in #180.
- Crew reviews: $9.70, nearly all on Sonnet.
- Sessions reading crew reports: $5.60.

What moved nothing forward, about $108:

- Old items' context read again: $36.50.
- The lead waking on Desk changes: $28.90.
- Compaction rewrites: $26.50, plus about $8 for the summaries.
- Waking cold: $18.20 in all. That is $8.70 for new work, $6.70 for resuming after the usage limit, and $2.80 for the lead.
- "Done?" checks: $3.40.

### The five biggest costs that moved nothing forward

1. Old items' context read again, $36.50. 96 of 171 items went to a session already over 100k.
2. Compactions, about $35. Each one writes about 60k again. 55 of the 77 came in items started above 100k.
3. The lead woken for every Desk change, $28.90, at about 107k each time.
4. Cold wakes, $18.20, nearly all after a limit stop.
5. Checking more than once, $9.20: extra review rounds, "Done?" sent twice, and one crew started twice.

### By session

The biggest sessions: cue-new-idea $92.60, of which $41.40 was crews and 13 compactions. mac-crrash $59.20, with 13 compactions. cue-124 $42.80. cue-y142 $32.20. The first Desk lead $28.50, and the current one $13.10. Fourteen smaller sessions together: $90.30.

### By item

171 items had work, and half of them cost under $1.32. The ten biggest make up $136, mostly crews and rounds of mocks. The most expensive were #181 (done means it's in your app) at $18.70, #135 (mocks for the lead chats) at $18, and #153 at $15. The lead itself cost $41.60.

## Not checked

The compaction summary calls aren't in the transcripts, so they are estimated at about 10 cents each. A crew is counted under the item its session was on when it started. Your messages are told apart from the lead's by their wording, so a few may be in the wrong group. The fresh-versus-used comparison mixes item sizes. The savings overlap a little.
