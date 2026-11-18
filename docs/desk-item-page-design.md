# The item page: sending, where to talk, what to test, and crew members

Suggestions only. Nothing here is built. It fits `docs/desk-landing-design.md`: the steps after done stay
"Done, being checked", "Checked, landing", "On main", "On main and live", and Looks right shows only from "On main" on.

The item page is the page for one task, feature or bug on the Desk.

## 1. After you send a message

**Today.** Your message shows in the thread at once. If the session is busy, it gets a small "Queued" tag. When the
session picks it up, the tag goes away and nothing takes its place. A reply shows only when the session's whole turn
ends. Until then one grey line says what it is doing. While the item is being worked on, the box sends to the Desk's
lead, not to the session doing the work, so the answer lands in the lead's chat and never on the item.

**The problem.** You cannot tell that it was taken, or by whom. Often the reply is somewhere else. So you open the
session to find out, and from then on you talk there.

**Suggestion.**

- Under your message, one line: "cue-268 took it, 10:42". Before that: "Waiting: it reads this after its current step".
- The live line names who is working and what it is doing now.
- The reply lands on the page, right under your message, as soon as it is said. When the lead answers about an item,
  that answer goes on the item's thread too.

Why: the answer should be where you asked. Cost: medium. The page change is small. Underneath, Cue has to keep replies
as they come instead of at the end of the turn, and note the moment a message was taken.

## 2. Where to talk

**Today.** The box changes who it sends to without saying much. While the item is being worked on: the lead. When the
session asks you something: the session. When it is ready to check: the session, and whatever you type sends the item
back, even a plain question. Next to that sits "Open cue-268", a second place to talk, where nothing you say is tied to
the item.

**The problem.** Two places, and the first one changes its meaning.

**Suggestion: one rule.** Talk on the item page. It always goes to whoever has the item. The lead gets it only when
nobody has the item yet. The box says who, in words: "To cue-268, who has this item".

- The session is for watching. Its button says "Watch cue-268", not "Open".
- If you do write in the session, and it is working on this item, those words show on the item's thread as well. So
  the item stays the whole story either way.
- Send back is a button only. Typing while the item is ready asks a question and keeps it ready.

Why: one place to talk is the only rule you never have to think about. Cost: small to medium. Sending to the holder
already works for questions. Copying session words onto the item is the new part. The lead no longer sees every message
on a held item; Cue would tell it in its next Desk summary.

## 3. What to test when it is ready

**Today.** The page says "Ready to check", with Looks right and Send back. "Done when" shows ticks that are all on or
all off; they are not real checks. What to try is whatever the session happened to write in its last reply. One quiet
line says if the change is in the installed app.

**The problem.** You have to work out what to test and whether you can test it yet.

**Suggestion.**

- With "done", the session gives a short list: what to try, in plain steps you can see. The page shows it as a
  "Try it" card, numbered.
- The card says where the change is, in the landing design's words: "Done, being checked" and "Checked, landing" with
  no buttons; then "On main" or "On main and live" with Looks right and Send back. A project with a live step says
  when: "On main, in the next install at about 21:30". No check set: "landed, not checked".
- Send back asks "Undo it" or "Fix it", as the landing design says.
- "Done when" stays in the panel as the goal. Its ticks go away until something real can tick them.

Why: the one who made the change knows best how to see it. Cost: medium. A new line for sessions to write, kept on
the item; the card is small. It needs the landing steps to exist first, or it shows today's two states.

## 4. Opening a crew member

A crew member is a teammate that a lead session starts.

**Today.** On the Desk, a crew member's page shows its file names, its last line, and a note: to steer it, tell its
lead. "Open its session" leaves the Desk and opens the lead's crew page with that member picked. There you can read
its changes, its review and its task, and its steps unfold under its card. Its replies over time are kept nowhere:
only its last words. In the list of sessions it is not there at all; you reach it through its lead.

**The problem.** The useful view is on another page, under the lead. Opening the member itself shows almost nothing,
and its replies cannot be read back anywhere.

**Suggestion: one page for a crew member, the same from everywhere** (the Desk's crew row, an item's "Held by", the
lead's crew list).

- **Work**: one list by time. What it was asked, its steps, its replies, and your messages with who passed them on.
- **Changes**: its files, and what changed in the picked file, readable while it works.
- **Asked to do**: its task in full. Its review shows in the side panel once there is one.
- The box says the truth: "To button-builder, through cue-268". A crew member can only be reached through its lead.
- Ties: the item it is on is one click away, and its replies show on that item's thread like any holder's. The item
  page names both: "button-builder, in cue-268's crew". The lead's crew page stays the place to see the whole crew.

Why: you open a thing to see that thing. Cost: medium. The changes view exists on the lead's crew page and can be
shown here. Keeping each member's replies over time is new.

## The options in the mocks

The mocks are on the ideas canvas, page "Item page: sending and testing". Each option is drawn next to today's page,
in light and dark, at two moments: just after Send, and ready to try.

- **A. The thread answers.** The page stays as it is. Under your message: who took it. The reply lands below it. When
  ready, a Try it card ends the thread, with Looks right on it. Cost: medium.
- **B. The panel tracks it.** The same rule for the box, but the news lives in the side panel: "Working on your
  message", then sent, taken, replied. When ready, a Try it list you tick, and "Where it is". Cost: medium, a bit more
  than A. The panel is narrow and can be hidden.
- **C. The page is the chat.** The item page shows the session's own conversation. Writing here is writing there.
  The thread becomes a Record tab. When ready, the Try it card takes the box's place. Cost: large.
- **Crew member page.** A row of its own. It goes with any of the three. With A or B it is as drawn; with C its
  Work tab would be its chat.

**My pick: A**, with the crew member page. It puts the answer right under what you sent, and what to try right above
the two buttons that close the item. It keeps the item's record on the page, and it is the smallest change.

## For the owner to decide

1. Should a message on an item that is being worked on go straight to the session, past the lead?
2. Must a session give try-it steps before its "done" counts, or is the card just left out when it gives none?
3. Should the steps be tickable, and must they be ticked before Looks right? (A says no; B draws ticks.)
4. Should a message to a crew member always go through its lead, shown as such? (Today there is no other way.)
