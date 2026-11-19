# Why chats on items don't feel live (#386)

What's here comes from reading the code: a fresh look by Fable. The session that built #369 then checked causes 1–4 itself; 5–7 are Fable's reading. Nothing has been built yet.

## The short answer

New words reach an item's page just as fast as they reach a session's chat. Both check for new words every half second, and Cue tells both at once when something changes. The difference is in how the item page draws them. A session chat keeps its messages in place, adds only what's new and stays at the bottom while you're there. An item's chat throws its whole list away and draws it again from scratch every half second. On top of that come a few ordering slips at the moment a turn ends.

## What you see, worst first

1. **The reply being written runs off the bottom.** The "Writing" message grows, but the page doesn't move with it, so you scroll to keep reading. It only moves down when a whole new message is added. When one is added, it moves down even if you had scrolled up to read something (the same kind of jump as #145).
2. **At the end of each turn, the full reply shrinks to one line, then comes back.** When the session stops, Cue first marks it stopped. Then it asks git what isn't committed yet. Only after that does it save the reply onto the item. In that gap the page shows the item as stopped and the reply's last line as if that were the whole reply. Sometimes that line is a "Not committed: …" note. A moment later the full reply appears.
3. **Some turns never show on the item's page at all.** An item's chat only counts a turn as the item's when the message that started it names the item ("#123"). If you type in the session's own chat, or anything else reaches it without the number, the session works and answers, but the item's page shows no "Working…" and never gets the reply. That's the fix for #176 (no mixing of items) going too far when the session holds only this one item. It also brings #8 back in a new way.
4. **Your message doesn't show until the send finishes.** On an item's page, the box empties at once. Your message only appears in the chat after Cue hears back from the session, and a message that's only a picture never appears. Messages to the lead and Send back already show at once.
5. **Nothing seems to arrive; it just swaps.** The whole list is redrawn every half second while the session works, so new words replace old ones with no sense of a message coming in. This probably also loses any text you've selected there.
6. **A rare ordering slip.** Your messages are timed to the second, replies to the thousandth of a second. A message you send in the same second a reply lands can sort above that reply (like #160).
7. **The words change slightly at hand-over.** While it writes you see everything, including DONE:/ITEM: lines and the stop-hook part. The saved reply strips those, so they vanish when the turn ends.

Fixes for #160, #165, #241, #310, #353 and #369 still hold. #276 ("each reply in full and live") is only half met, because of 1, 2 and 5.

## Ways to fix it

- **A. The small fixes (recommended first).** Show your message at once (4). Follow the bottom only while you're at the bottom, the way the session chat does (1). Keep showing the live reply until the saved one is in (2). Count an unnumbered turn as the item's when its session holds only that one item (3). Time your messages to the thousandth of a second (6). Strip the live words the same way as the saved ones (7). These are all small changes in the item page and one rule in Cue's core. Cost: about half a day with tests.
- **B. A plus the turn-end order in Cue's core.** Save the reply in the same step that marks the session stopped, before asking git. That way the shrink-then-return in 2 can't happen at all, not just be hidden on the page. Cost: a few more hours, with a test that the reply and the stopped state arrive together.
- **C. B plus redraw the item chat the way the session chat does.** Messages stay in place, only new or changed ones are touched, and new words appear as they're written. This is what makes it *feel* live (5), and it removes the scrolling problems for good. It's the biggest change: the item chat moves onto the session chat's drawing code. Cost: one to two days.

My pick: **B now, then C.** B fixes everything you can name: the jumps, the missing turns, the late messages and the one-line flash. C is what makes it feel like a real chat rather than a page that refreshes.

Not checked: sessions on another machine, crew members holding an item, and real timings in the running app.
