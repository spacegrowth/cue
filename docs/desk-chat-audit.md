# The Desk chat next to the board's chat (#688)

This is a full list of how the Desk lead's chat differs from a session chat on the board, written to be listened to. The board's chat is the model. Mocks for four ways to bring the Desk chat back to it are on the "Cue chat" canvas.

## The short version

Both chats are drawn by the same code. The Desk chat looks different because it switches on six extra options and adds its own styles on top. So nearly every difference can be undone by turning an option off or removing a style, not by building something new.

The biggest difference is not a style. It is how many lines in the Desk chat are not for you. On 21 November one Desk had 173 Desk events and about 171 lead answers to them, next to 146 of your messages and 120 answers to you. More than half the chat was the Desk talking to itself.

## 1. Lines that are not for you

The board's chat has your messages, the session's replies, and the occasional Cue card. The Desk chat has all of that, plus these:

- **The lead's routing lines.** "Gave #43 to cue-611", "Added #690", "Renamed", "Folded into", "is done", "Started", "Closed", "Parked", "Kept context", "Said what you'll see". Each one is a short sentence with a small round sign on its left. Click one and a grey box opens under it. These are for the lead's own record and for steering. They are not for you.
- **The session's lines from #685.** "How to check", "If you pick", "To the lead", "For you to try", "Not done", "Small call". Most are for you, but they read as the same kind of grey line as the routing.
- **Cue's lines to the lead.** "Cue, Desk changes, 3 minutes ago", then what changed. These tell the lead that an item finished or got stuck. They are not for you; the lead usually answers them, and that answer lands in your chat too.
- **Cards with a capital-letter tag**: NOTE, TO LEAD, NOT CLOSED, HAND-OFF and NEW ITEMS are for the lead. STOPPED and COULDN'T START are partly for you, because they have an "Open" button. READY TO CHECK and WAITING FOR INSTALL are for you.
- **Check lines.** "You sent #5 back", "looked at it and said it looks right". Centred, between two thin rules. A record of what you did.
- **The "Decided on its own" block.** The board has this too.
- **Bands from another Desk.** A full-width tinted strip. You asked to keep these.

There is already a place for the lead's actions: the **Dispatcher Actions** tab, in the tab row above the lead's chat, next to Decisions and Away. The same rows also appear inside the chat, so today they show up twice.

### The Conversation only / Everything switch from #685

It sits in the tab row. Everything is the default. Conversation only folds every run of Desk lines into one line, such as "4 Desk updates, gave #15 to…", with a rule above and below. Your messages, the lead's own words, bands from other Desks and asks are never folded.

**Verdict: stays until an option is picked.** You never asked for it and the board's chat has nothing like it, but it is kept for now and goes when the chosen option is built. Every option on the canvas takes the Desk lines out of the chat and puts them somewhere else, so there is nothing left for the switch to fold. Options B and D keep one plain line per gap in the conversation, which is the switch's folded line, but on by default and without the rules.

### The styled lines from #685, one by one

- **Routing lines with round signs**: go. They move to Activity, where they become plain rows.
- **The grey box under a routing line**: goes with them.
- **The bordered box for How to check and If you pick**: keep the words, lose the box. These are for you, so they stay in the chat as plain lines in the lead's reply.
- **For you to try and Not done**: stay in the chat as plain text. They are for you.
- **To the lead and Small call**: move to Activity. They are not for you.
- **The folded "4 Desk updates" line**: goes with the switch, except in option B.

## 2. Backgrounds

You asked that only your messages have a background. Today, in the Desk chat, these have one as well:

- every card listed above, on a light grey fill;
- the round signs on routing lines, some tinted green or lime for done and checked;
- the box that opens under a routing line;
- the bordered box for How to check and If you pick;
- the lead's ask, which is a bordered card tinted lime;
- the composer, which has its own border;
- the right-hand panel (Now, Waits on you, Just did, Working), on a light grey fill.

A note for honesty: the board's chat is not quite "only your bubble" either. Its Cue cards, ask cards, the step chip, the Decided block and code blocks all have a fill. The lead's replies have no background in either chat. The mocks go with your rule: your bubble only, with code blocks as the one exception, since a command without a fill is hard to copy.

## 3. All caps

These are in capitals today:

- every card tag: NOTE, TO LEAD, NOT CLOSED, STOPPED, COULDN'T START, NEW ITEMS, HAND-OFF, READY TO CHECK, WAITING FOR INSTALL, and the ask's "ASKS YOU";
- "DECIDED ON ITS OWN", which the board shares;
- the side panel's headings: NOW, WAITS ON YOU, JUST DID, WORKING;
- the Dispatcher Actions heading;
- the "DESK LEAD" tag next to the lead's name;
- a Desk line the app can't read, which shows raw, such as "ROUTE: …".

All of these become normal sentence case. It's a small change: one style rule and the tag words.

## 4. Sending

On the board:

- The box is two lines tall.
- The button says **Queue** while the session is busy and **Send** when it is free, and the placeholder says when your message will go.
- It has the slash command menu, the Send at clock, and "!" to run a command.
- Enter sends. Command-Enter sends now, and with nothing typed it sends the message that is waiting.

On the Desk:

- The box is one line tall and has its own border.
- The button always says Send, even when the lead is busy and your message will wait.
- There is no command menu, clock or Run.
- Command-Enter works the same as on the board.
- About an item that is ready to check, sending becomes Send back.

The bubbles mostly match. Both chats draw a sending, kept or queued message with a dashed outline, and the line under it says "Sending…", "Not sent yet", "Queued for…" or "Delivered". The Send now, Edit and Cancel buttons are the same. The Desk differs in three ways:

- its own Sending rows come from a separate list, so a message can briefly show twice;
- a failed send offers Retry and Edit instead of Send again;
- the line that says where the lead is now is hidden while a message is sending.

Older chat note from #642: you asked for a dotted outline on waiting messages and got a dashed one. That is still true in both chats.

To match: the Desk uses the board's composer, with Queue when the lead is busy, two lines, the command menu and the clock, and one list for sending rows. A medium change.

## 5. Listen and Read

Same buttons, same place, under the lead's reply. The differences:

- **No Listening bar.** On the board, while a reply is read aloud, a bar above the box says which sentence it's on, with Pause and Stop. The Desk never draws it, so you can't see where it is or stop it from the chat.
- **Short routing replies get no Listen or Read**, because routing lines don't count as words. Once routing leaves the chat this no longer matters.
- **Read opens inside the chat card** instead of the board's reader.
- **No "Send to…" button.**
- **The buttons stick 10 pixels off**, because their place was set for the board's padding, not the Desk's.

To match: draw the Listening bar, open Read the same way as the board, and fix the offset. A small change.

## 6. Everything else

- **No step chips.** The lead's chat hides the session's steps and its status line has none. Desk session chats still show them. That's probably right for a lead, which mostly talks, so the mocks keep steps hidden.
- **Width.** The Desk chat has a 272-pixel side panel (Now, Waits on you, Just did, Working) with a Hide panel switch. The board has none. Waits on you repeats the item list's first group. Options A, B and C drop the panel and give its "Now" line to the chat's status line, as on the board. Option D keeps it and puts Activity at its bottom.
- **Names.** Your messages on the Desk carry an item header saying which item they are about. The board has none. Keep it: it's useful, and it's plain text.
- **Scrolling.** The Desk keeps you at the bottom. Opening a folded run keeps that row still. This goes away with the folds.
- **The lead's ask** is cut out of its reply and shown as a lime card at the foot instead of the composer. The board shows an ask card under the chat. The mocks keep the ask above the box but without the lime fill and border.

## Compared with the older chat (#642)

The older build used more room for the lead's acts: 1597 pixels of Desk activity against 1133 today. Since then:

- steps fold into a chip;
- Show all was removed;
- "Waiting on you" left the item page;
- the status line went away;
- cards from another Desk became bands.

Those were steps in the right direction, but the Desk chat then gained the routing lines and #685's styled lines, so it is busy again in a new way. The older chat also had no Dispatcher Actions tab, which the three options now rely on.

## What #660 and #665 found

- **#660** measured the Desk chat card at 878 pixels wide with an 842-pixel text line, close to the board's. Width is no longer the issue.
- **#665** counted the lines on one Desk on 21 November and found the Desk talking to itself more than to you. Its option C became #685's switch. This audit's view is that the switch was the smallest fix and not enough: the lines need to leave, not fold.

## What all four options share

- Only your messages have a background.
- The lead's replies are plain, as on the board.
- No capitals anywhere in the chat.
- Sending, Command-Enter, Queue, and the waiting bubbles look and work as on the board.
- Listen, Read and the Listening bar are as on the board.
- Kept: bands between Desks, the chat itself, and the item list on the right.
- Lines meant for you stay in the chat: the lead's asks, Ready to check, Stopped, How to check, If you pick, For you to try, and Not done.
- The #685 switch goes when the option is built; until then it stays in the app.
- Something shows when activity arrives while you look elsewhere (see below).

They differ only in where the Desk lines go:

- **A**: into an Activity tab, the renamed Dispatcher Actions.
- **B**: the same tab, plus one plain line counting them between your turns.
- **C**: into the item list, as a third view next to List and Progress.
- **D**: B's count lines in the chat. The side panel stays as today (Now, Waits on you), and Activity sits at its bottom: the last 3 rows, with "14 more in Activity ›" opening the Activity tab. It takes Just did's place. The item list on the right stays. No "2 new" badge anywhere.

Activity holds Cue's own lines as well as the lead's: copies made, items finished or stuck, send-backs, and notes to sessions. A Cue line meant for you, such as Ready to check or Stopped, stays in the chat.

## The other chats

The same problems are in the other chats on the Desk and the crew page.

- **Item page chat.** Folds "Worked on #x" in a grey fill, centred rule lines ("cue-611 took this item"), a capital "Not started yet" card, a "To cue-611 · Task #611 · sent as #611" tag over the box, a box that always says Send, no Listening bar, and a side panel with capital Now and Who labels. Tabs: Chat, Record, Decisions, Files.
- **A Desk session's chat.** The same Desk lines as the lead's chat, a box that always says Send, no Listening bar, and the same Now and Who panel.
- **Crew member chat (crew page).** Already uses the board's box, so the Listening bar is there. Capitals in the kind tags (Committed, Decided), "Needs you", "The crew · N" and the side labels. The Undo and Make a revert buttons on decisions stay.
- **Not chats:** the Desk's crew member page is a list, not a chat. There is no loop chat.

Each option treats them the way it treats the lead's chat:

- **A**: each gets an Activity tab next to Chat. On the item page Activity takes Record's place.
- **B**: plain count lines between your turns in each.
- **C**: an Activity list on the right of each ("Crew activity" on the crew page), in place of the Now and Who panel.
- **D**: count lines in each; their panel (Now and Who, or the crew) stays, with the last 3 Activity rows at its bottom and a link to the Activity tab.

## Seeing activity as it happens

Today nothing tells you that Desk or crew activity happened in a chat you are not looking at. The only marks are "said something since you left" on the crew page, the blinking dot, and the Desk's "landed" mark.

- **A**: the Activity tab shows a dot and "2 new" while lines arrive; new rows are marked until you open it.
- **B**: the newest count line has a dot and "just now" and grows as lines come in.
- **C**: the Activity view's tab has a dot; new rows are bold until seen, and the item they touched shows "just now".
- **D**: the newest count line has a dot and grows; new rows appear at the top of the panel's Activity. No badge.

## Your message, every state

The canvas grid is screenshots of the real app, not drawings: the same message set up in each state and shot in the board's session chat, the Desk lead's chat and the Desk item chat, light and dark. All three draw your bubble with the board's code (the same small bottom-right corner, the dashed outline while it waits, the red outline when it fails), so the differences are in the words and in a few messages that aren't drawn as yours at all:

- **Delivered, via iTerm, joined to the one above, sending, sending for 30 seconds, typed while it works, couldn't send, kept then couldn't send.** The same on all three. The item chat adds the item's head ("Feature #d2 · its title") inside the bubble.
- **Kept by Cue till its turn ends.** Board and lead chat: "Not sent yet · Cue sends it when its turn ends" with Edit, Send now, Cancel, and the "Ask it now, by the way" offer (with "It's been at this 1h 10m" after 45 minutes). The item chat says "Waiting: web-lead is finishing something, your note goes in next", drops the item head and never offers the by-the-way question.
- **Queued in its box.** Board and lead chat: "Queued for web-lead · it reads this after its current step · Send now". The item chat uses the same "Waiting: …" words.
- **Sends at a time, or when the usage limit resets.** Board and lead chat alike. The item chat shows the raw "About #d2 (…)" text, without the item head.
- **Sent from the Desk box.** Only the lead chat. "Sending…" never turns into "Cue hasn't seen it take this yet", and a failed one offers Retry and Edit where the board offers Send again.
- **"Not done" from an item.** Never drawn as your message. The lead chat shows a Cue line ("+ Cue · Not done") with the board's status line under it while it waits, and no Delivered line once it went. The item chat shows "Waiting: …" while it waits (its "Not done" and the item head dropped) and a Cue card "Cue → web-lead" once delivered.

After #688 both Desk chats draw every one of these with the board's code: the board's words and buttons, the by-the-way offer in the item chat too, the item head on every message about an item (timed ones included), Send again when a Desk-box send fails, and "Not done" as your own bubble with the board's line under it.
