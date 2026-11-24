# Cue's five chats: what each shows, and a plan for one shared chat

Item #832, 2026-11-24. Nothing in the app changes with this doc. Line numbers are as of commit 3f361844e8.

## The short version

**The rule: add, never take away.** The chats are already mostly one. Four of the five draw their messages the same way, and each one only switches some features off. So making them one chat means switching those features back on. Nothing any chat has today is removed.

Three of today's differences look like they were chosen on purpose. They stay as they are until you say otherwise:
- the lead chat hides the steps between messages;
- the Desk moves Cue's own messages to the Activity tab;
- the lead chat shows bands from other Desks.

**Three rounds, each one small enough to try before the next:**
1. **Turn on what's missing**, about half a day. Every chat gets:
   - Read working;
   - #123 and session names as links;
   - "↗ Send to…";
   - btw with its panel;
   - a status line that names background agents;
   - Escape bringing back a message you stopped, so nothing typed is lost.
2. **One box everywhere**, a day or two. The Desk's item and session boxes become the same box as the Board's and the lead's, with Queue, the "/" menu and Send at a time.
3. **Crew members**, later and only after a mock. A crew member's chat moves onto the same chat, so it gets Listen, Read, links and Show all too.

The long plan further down is the same work in finer steps, for whoever builds it.

## The five chats

- **Lead**: the Desk's lead chat, on the lead tab of a Desk. It is drawn by `deskChat` (ui/desk/page.js:174).
- **Session**: a session opened from the Desk's left column. It is drawn by `deskCSessChat` (ui/desk/c.js:487).
- **Item**: an item's page on the Desk, which shows the chat of the session holding the item, with the item's bar on top. It is drawn by `deskCItemChat` (ui/desk/c.js:395), which calls `deskCSessChat`.
- **Board**: the chat in the Board's open card (the Active pane). It is drawn by `activePane` and `chatBody` (ui/chat/pane.js:103). The mini window uses the same code (ui/mini.js:268). Board cards themselves show no chat, only a summary line and quick answers (ui/board/board.js:421, 491).
- **Crew member**: a teammate's chat. It shows on two pages:
  - on the Desk, `deskCrewPane` (ui/desk/thread.js:115);
  - on the Board's crew page, `crewMateView` (ui/team_mate.js:161).

The lead, session, item and Board chats all draw their messages with the same function, `chatHtml` (ui/chat/render.js:225). They differ in the options each passes, and in the box, status line and tabs around it. The crew member's chat does not use `chatHtml` at all.

## The table

"Yes" means it works as on the Board. "No" means it is missing. "Different" means it is there but works differently.

| Feature | Lead | Session (Desk) | Item | Board | Crew member |
|---|---|---|---|---|---|
| ↗ Send to… on replies | No | No | No | Yes | No |
| Questions and permission asks in the thread | Yes | Yes | Yes | Yes | Different: Desk shows the ask under the chat; Board crew page shows it as a red step and a "Needs you" bar |
| Open question or permission card | Yes, replaces the box | Yes | Yes | Yes | Different (see above) |
| Listen on replies | Yes | Yes | Yes | Yes | No |
| Listen's player line over the box | Yes | No | No | Yes | No |
| Read on replies | Yes | Yes | Broken: the button shows but does nothing | Yes | No |
| #123 item links and session-name links | Yes | No | No | No | No |
| File paths in replies open | Yes | Yes | Yes | Yes | No |
| Status line ("Working on…", "Waiting on you…") | Different: no step count, no background agents, no hand-off line | Different: no background agents, no hand-off line | Same as Session, and hidden before the item starts | Yes, the full line | Different: a status line of its own |
| Steps between messages ("Working · 13 steps") | No, hidden on purpose | Yes | Yes | Yes | Different: steps drawn by their own code |
| "+ Desk update" fold under your words | Yes | Yes | Yes | Yes | No |
| Hand-off band, and bands from other Desks | Yes | No | No | No | No |
| Cue's Desk messages moved to Activity | Yes | Yes | Yes | No | No |
| Tabs | Different: lead name, Decisions, Activity, Away | Chat, Files, Activity | Chat, Files, Activity | Different: Chat, Agents, Diff, add-ons | Different: Desk has Chat, Files, Activity (Activity shows the lead's); Board has Diff so far, Full review, Full task |
| ⋯ menu | No | Yes, the Board's | Yes, the Board's plus the item's own | Yes | Different: Desk has one item, "Open its session"; Board crew page has none |
| btw | Different: a panel but no button | Different: a button but no panel | Different: a button but no panel | Yes | No |
| Stop | No | Yes | Yes | Yes | Yes |
| Box | The session's box | Different: a smaller copy | Different: a smaller copy | The session's box | Different: Desk uses the smaller copy, sending to the lead; Board crew page uses the lead's box |
| Quick phrases over the box | Yes | Yes | Yes, but no place tab | Yes | No |
| "/" menu, Send at a time, "!" to run | Yes | No | No | Yes | No |
| Send turns into "Queue" while it works | Yes | No | No | Yes | No |
| Held message (Edit, Send now, Cancel) | Yes | Yes | Different: Sending…, failed and Send again only | Yes | No |
| Attachments (paste, drop, add image) | Yes | Yes | Yes | Yes | Yes on Desk, through the lead's box |
| Mic | Yes | Yes | Yes | Yes | Yes |
| Escape gives back what you just sent | Different: only a message still waiting to go comes back; one already sent is lost in a hosted session | Same as Lead | Same as Lead | Same as Lead; the mini window's Escape only closes the chat | Different: brings back the lead's waiting message, into the crew member's box |
| Show all / Fold on long replies | Yes | Yes | Yes | Yes | No |
| Earlier messages as you scroll up | Different: "Show earlier" after a hand-off | Yes | Yes | Yes | Different: Board shows "N earlier ›"; Desk loses the "earlier steps not shown" line |

### Where each row comes from

The file:line behind each row.

**Send to…**
- The button is drawn by `chatAgentHtml` (ui/chat/render.js:359).
- It is dropped wherever a chat passes `noActs`. Every Desk chat passes it:
  - ui/desk/page.js:125 (lead, session and item);
  - ui/desk/earlier.js:56 (the lead's earlier leads).
- The crew member's bubbles come from `agentChatHtml` (ui/chat/helpers.js:200), which has no actions at all.

**Asks**
- Asks in the thread: `chatAskHtml` (ui/chat/render.js:602), called at render.js:268.
- The open ask under the chat: `askFoot` (ui/chat/pane.js:46).
  - Board: used at pane.js:46.
  - Desk item: ui/desk/view.js:130.
  - Desk session: ui/desk/approvals.js:27.
  - Desk lead: ui/desk/view.js:197.
- Crew member:
  - Desk: `deskSessionRequestFoot` (ui/desk/approvals.js:29-39).
  - Board crew page: the "Needs you" bar (ui/team_bar.js:22-33), and the ask as a red step (ui/team_mate.js:46).

**Listen and Read**
- Listen is shown when a reply has 20 or more words (render.js:347, `LISTEN_WORDS` render.js:778).
- Read is shown only on a "paged" chat (render.js:359). All four `chatHtml` chats are paged.
- The Read click opens the reader (ui/app/clicks_chat.js:120, then `openReader` ui/chat/reader.js:24, which re-renders).
- Where the Desk draws the reader:
  - the Desk session branch checks `deskReader` (ui/desk/view.js:164);
  - the lead branch checks it too (view.js:183);
  - the item branch does not (view.js:158), so Read on an item page re-renders the same chat.
- The player line over the box: `listenLine` (ui/chat/listen.js:430).
  - The Desk draws it only on the lead's box (ui/desk/view.js:76).
  - The Board draws it in `paneFoot` (ui/chat/pane.js:137).

**Links**
- `md()` turns #123 and session names into links only when given `refs` (ui/core/md.js:160-165).
- The Desk passes `refs` only when the session is a Desk lead (ui/desk/page.js:123-125). The Board never passes it (ui/chat/pane.js:104).
- The crew member's `md()` calls get neither `refs` nor `runIn`:
  - ui/chat/helpers.js:211 and 222;
  - ui/team_mate.js:155.

**Status line**
- Board: `statusLine` (ui/chat/render.js:477). It adds `handoffStatus` and the background-agent tray to `whereNow` (render.js:505).
- Desk chats call `whereNow` directly:
  - lead: ui/desk/page.js:196, with `steps=false`;
  - session and item: ui/desk/c.js:511.
- Crew member: its own copies.
  - Desk: `deskCCrewStatus` (ui/desk/thread.js:104-109).
  - Board crew page: `crewMatePill` (ui/team_mate.js:27, 166).

**Steps**
- Lead: `noSteps` (ui/desk/page.js:191).
- Session, item and Board: `withSteps` (ui/chat/render.js:155).
- Crew member:
  - Desk: step folds of its own in `agentChatHtml` (ui/chat/helpers.js:212-219);
  - Board crew page: its own rows (ui/team_mate.js:126-137).

**"+ Desk update"**
- Drawn by `yourBubble` (ui/chat/thread.js:216-232), so every chat that uses `chatHtml` has it.

**Bands**
- `deskPeerBand` and `deskToldBand` (ui/desk/lead.js:25-50) draw only in a lead's chat (render.js:265, 270).
- `deskHandHtml` (render.js:299) is folded to Activity on every Desk chat.

**Cue's messages to Activity**
- `dkFoldRows` runs when a chat passes `lines` or `dkOut` (render.js:253). Every Desk chat passes `dkOut`.
- `dkReplyTake` and `dkNoticeReply` run only with `lines`, which only the lead passes (render.js:350, 354).

**Tabs**
- Lead: ui/desk/view.js:203.
- Session and item: ui/desk/c.js:502.
- Board: `sessTabs` (ui/extensions.js:51).
- Crew member:
  - Desk: ui/desk/c.js:431-437. Activity there is `dkActivityOf(lead)`, at c.js:432.
  - Board crew page: ui/team_mate.js:151.

**⋯ menu and btw**
- The Board's menu: `moreMenu` (ui/chat/render.js:656).
- Desk session and item reuse the Board's head through `activeHead` (ui/desk/c.js:492).
- The item's own ⋯: `deskMore` (ui/desk/page.js:836-842).
- The lead page has no head actions (ui/desk/view.js:202).
- btw:
  - the panel is drawn only on the lead (ui/desk/page.js:200);
  - the button comes with `activeHead` (ui/chat/pane.js:96-98), which the lead doesn't use.
- Crew member:
  - Desk ⋯: ui/desk/thread.js:125-127;
  - Board crew page: only Talk and Stop (ui/team_mate.js:165).

**Box**
- The session's box: `box()` (ui/chat/compose.js:489), which calls `boxHtml` (ui/box/markup.js:26).
  - Board: ui/chat/pane.js:66-81.
  - Desk lead: ui/desk/view.js:73-77.
- Every other Desk box: `deskCBox` (ui/desk/view.js:88). It calls `boxHtml` directly and leaves out:
  - the "/" menu;
  - Send at a time;
  - "Queue";
  - Send greyed out while the box is empty;
  - "!" to run.
- Board crew page: the lead's box with quick phrases off (ui/chat/pane.js:161), and `crewHeldBox`/`crewChipBox` (ui/team_bar.js:132-141).

**Held messages**
- The Board, the session and the lead: `chatWaitRows`, `heldRow`, `queuedRow` and `sendAtRow` (ui/chat/render.js:368-457).
- The item sends through `deskSendItem` (ui/desk/act.js:65) and draws its own pending rows: `deskCItemPending` (ui/desk/c.js:462-486).
- The lead's own pending rows: ui/desk/page.js:177-185.

**Show all**
- render.js:338-339 and 359.

**Earlier messages**
- `olderRow` (ui/chat/render.js:257). The lead skips it after a hand-off: `noOlder`, then `deskEarlierTop` (ui/desk/page.js:190, ui/desk/earlier.js:62).
- Crew member:
  - Board crew page: ui/team_mate.js:121 and 130;
  - "N earlier steps not shown" is drawn only in `helperPane` (ui/chat/helpers.js:240).

## Differences that are bugs today

These are the differences no comment in the code explains or defends.

1. **Read on an item page does nothing.**
   - The Read button is drawn on an item's chat. The click sets the reader and re-renders (ui/chat/reader.js:24).
   - The Desk's item branch never asks for the reader (ui/desk/view.js:158), so the same chat comes back.
   - The session and lead branches do ask (view.js:164, 183).
2. **#123 and session names aren't links outside the lead chat.** This covers the Desk session, the item, the Board and the crew member.
   - Only the lead passes `refs` (ui/desk/page.js:125).
   - The comment at page.js:119-122 explains dropping the lead's Desk lines from other chats (#744), but not dropping links.
3. **"↗ Send to…" is missing on every Desk chat.**
   - The comment at render.js:216 gives the reason for the lead: it hands work on by its own lines.
   - The same `noActs` is also passed for item and session chats (page.js:125), where that reason doesn't apply.
4. **btw is half there on the Desk.**
   - Item and session chats show the btw button (from the Board's head, c.js:492) but never draw the panel.
   - The lead draws the panel (page.js:200) but has no button.
5. **The Desk's item and session boxes are a smaller copy of the session box** (`deskCBox`, view.js:88). They are missing:
   - "Queue" while the session works;
   - the "/" menu and Send at a time;
   - Send greyed out while the box is empty;
   - Listen's player line.
6. **The status line under Desk chats leaves out** the hand-off line and the background-agent tray (`whereNow` instead of `statusLine`). The lead's chat also leaves out the step count (page.js:196).
7. **A crew member's Activity tab on the Desk shows the lead's activity** (`dkActivityOf(lead)`, c.js:432), not the member's.
8. **A crew member's chat has no Listen, Read, Show all, links or Send to**, because it doesn't use `chatHtml` (ui/chat/helpers.js:200-224).
   - The Desk and the Board crew page also draw the same member in two unrelated layouts:
     - Desk: chat first, with Files and Activity tabs (ui/desk/thread.js:115);
     - Board crew page: diff first, with no chat (ui/team_mate.js:161).
   - The comment at thread.js:112 says they match. That is true only of the Board's `helperPane`.
9. **The Board's permission buttons on a card are a hand-built copy** (ui/board/board.js:412) of `permBtns` (board.js:272).
   - On a card, a plan shows plain Allow and Deny, not "Approve plan" and "Keep planning".
   - The card has no "Always…" and no redirect (↪).
   - The mini window has a third copy (ui/mini.js:224).
10. **A restarting session's chat loses its status line and rename notice.** It calls `chatHtml` directly (ui/chat/pane.js:173) instead of `chatBody` (pane.js:103).
11. **A parked session's box is a reduced one** (ui/chat/parked.js:116), with no pictures, "/" menu or quick phrases.
    - It shares its draft with the full box, so pictures pasted earlier are silently not shown there.
    - Its messages are hand-drawn bubbles (parked.js:102-104), with no Listen, Read, Fold or Send to.
12. **Pasting a picture skips the limits.** Paste (ui/app/inputs.js:35) adds pictures with no limit. The Add image picker (ui/chat/patch.js:250-252) caps at 6 pictures and 15 MB.

13. **Escape gives back what you sent only in a terminal; in a hosted session the text is lost.**
    - What Cue's chats do on Escape: `escSession` (ui/app/keys.js:124-142).
      - If Cue is keeping a message for the session (you sent it while it worked), Escape takes it back into the box (`pullHeld`, ui/chat/compose.js:100-108).
      - Otherwise Escape twice stops the turn (`interrupt`, ui/chat/compose.js:112). The message that already went is not put back in the box.
    - Cue keeps a message sent mid-turn for both kinds of session:
      - a session in a terminal: src-tauri/src/hub/send.rs:510-515;
      - a hosted session: send.rs:571-575.

      So a message still waiting comes back in every chat. One that has already gone, or one sent with Send now, does not.
    - **Session in a terminal:** its stop is Escape typed into its terminal. Claude Code then puts the stopped message, or its own queue, back in its own input line there. Cue's box doesn't get it back, but the text isn't lost: it waits in the terminal. The note at send.rs:509-510 says so: "In the terminal's own queue only Claude Code could give it back."
    - **Hosted session:** it has no terminal (send.rs:398-415). Its stop goes over its connection, and the text it was working on comes back nowhere.
    - **Which chats this covers:**
      - Board: through `keyEscape`, ui/app/keys.js:149.
      - Desk lead, session, item and crew: through `deskEscSession`, ui/desk/act.js:584-593. The item takes it back through `deskHeldEdit` (act.js:590).
      - A crew member's page sends Escape to its lead (act.js:592), so the lead's waiting message lands in the crew member's box.
      - Mini window: Escape only closes the chat (`escKey`, ui/mini.js:486-490). It neither takes a message back nor stops the turn.
    - **The fix:** when Escape stops a turn before it answered, Cue puts the stopped message back in that chat's box, for every kind of session. Cue already has the message's text, so this needs no help from the terminal. A message sent with Send now (queued in Claude Code) comes back the same way.

Two old leftovers to clean up along the way. They don't change what anyone sees:
- `deskSessChat` (ui/desk/page.js:500-520) can never run, since `deskCSessChat` covers every case it would;
- `deskCrewPane` has unused variables at ui/desk/thread.js:124, 129, 137 and 138.

## What the chats share now, and what they copy

**Shared by the lead, session, item and Board chats:**
- `chatHtml` and everything under it (ui/chat/render.js):
  - your bubbles, replies and asks;
  - held and queued messages;
  - steps, Show all, Listen and Read;
  - "+ Desk update" (ui/chat/thread.js).
- `md()` (ui/core/md.js).
- `askFoot` for the open ask (ui/chat/pane.js:46).
- `boxHtml`, the box's markup (ui/box/markup.js:26), and its mic, pictures and quick phrases.
- `whereNow`, the status line's text (ui/chat/render.js:505).
- `activeHead`, the Board's top bar, which the Desk's session and item chats reuse (ui/desk/c.js:492).

**Copied, each one a second version of the shared code:**

| What | Shared version | Copy |
|---|---|---|
| The box | `box()` ui/chat/compose.js:489 | `deskCBox` ui/desk/view.js:88 (fewer parts); parked box ui/chat/parked.js:116 |
| Messages not sent yet | outbox in ui/chat/render.js:254, 385 | the lead's ui/desk/page.js:177-185; the item's `deskCItemPending` ui/desk/c.js:462-486 |
| Status line | `statusLine` ui/chat/render.js:477 | `whereNow` alone in ui/desk/page.js:196 and ui/desk/c.js:511; `deskCCrewStatus` ui/desk/thread.js:104; `crewMatePill` ui/team_mate.js:27 |
| A reply's bubble | `chatAgentHtml` ui/chat/render.js:335 | `agentChatHtml` ui/chat/helpers.js:200; `parkedPane` ui/chat/parked.js:102; `quietTurn` ui/chat/render.js:767 |
| Steps | `stepsBlock` ui/chat/render.js:11 | ui/chat/helpers.js:212-219; ui/team_mate.js:37, 126-137 |
| Permission buttons | `permBtns` ui/board/board.js:272 | ui/board/board.js:412; ui/mini.js:224 |
| An ask, drawn two ways | `chatAskHtml` ui/chat/render.js:602 (in the thread) | `askFoot`/`requestBody` (the open one) |
| A crew member's review | ui/team_mate.js:155 | ui/team.js:415 |
| Diff | `diffParse`/`diffOverlay` | `crewDiffHtml` ui/team_mate.js:74-88 |
| "Is this session a lead?" | (none) | ui/desk/page.js:123, 504; ui/desk/c.js:507 |

## The plan: one shared chat

**The aim:** one function draws a chat with its box, status line, tabs and head, given a short description of where it is. Every place in the app calls it. A fix in one chat then reaches all five.

### The shape

There will be one call, `chatView(where, s, it)`, and one table, `CHAT_KINDS`, saying what each kind turns on.
- The kinds are lead, session, item, board and crew.
- The table holds settings like `sendTo`, `links`, `deskLines`, `steps`, `tabs`, `head`, `bands` and `older`.
- Each setting has one reason written next to it.

The `page` option bag grows by hand at each call today. The table replaces it, so the differences live in one place and read as a list.

### Steps

Sizes: S is up to half a day, M is a day or two, L is three days or more. Each step lands alone and leaves the app working.

1. **Fix the plain bugs first, in the code as it is now.** Size S. Low risk.
   - Read on the item page: view.js:158 asks for `deskReader`, as view.js:164 does.
   - Links: pass `refs: deskRefs` on every Desk chat, and on the Board.
   - btw: draw `btwPanel` where its button shows.
   - Crew Activity: show the member's own moves.
   - Paste: cap pictures as the picker does.
   - Restarting pane: call `chatBody`.
   - Escape (bug 13): a stopped message comes back into the box, and a crew member's page takes back only into the lead's box. The hosted session's message is the one that matters most: today it is lost.

   Each is a line or two, with a UI test in tests/ui/specs. This step is worth doing even if the rest waits.
2. **One status line.** Size S. Low risk.
   - Desk chats call `statusLine` instead of `whereNow`. This brings in the hand-off line and the background-agent tray.
   - The lead keeps its own choice on the step count, as a setting.
   - The two crew status copies switch to it.

   Risk: the lead's line could get busier. Check it with a UI scenario of a lead that has teammates.
3. **One box.** Size M. Medium risk.
   - `deskCBox` becomes `box()` with a `pre` tag ("To cue-744") and the Desk's send attribute.
   - The parked box becomes the full box.
   - All of them then get Queue, the "/" menu, Send at a time, Send greyed out while empty, and Listen's player line.

   Risk: the Desk's item box sends as a hub note (act.js:65), not to the session, so "Queue" and "/" must mean the right thing there. "/model" on an item's box should go to its holder. Test sending, holding and sending now on all three Desk boxes.
4. **The kinds table.** Size M. Low risk.
   - Add `CHAT_KINDS` and `chatView`.
   - Move today's `page` options for each call (ui/desk/page.js:125, 191; ui/desk/c.js:511; ui/desk/earlier.js:56; ui/chat/pane.js:104, 112) into it, with no change in what anyone sees.
   - Put the "is this a lead?" test in one place.

   A pure move. Prove it with the UI suite showing no change.
5. **Decide the real differences, as table settings.** Size S. These are product picks:
   - Should Send to show in item and session chats?
   - Should the lead keep its steps hidden?
   - Should item and session chats show the bands?
   - Should the Desk's Activity move apply on the Board too?

   Each answer is one setting.
6. **One pending list.** Size M. Medium risk.
   - The lead's and the item's not-yet-sent rows (ui/desk/page.js:177-185, ui/desk/c.js:462-486) become the shared outbox (ui/chat/render.js:254).
   - The hub notes from the Desk's box get the same states: Sending, Queued, Held, Failed with Send again.

   Risk: double rows once a message lands (#688 and #370 were about exactly this). The existing tests in tests/ui/specs for the Desk's box cover it. Add one for an item.
7. **One ask.** Size M. Medium risk.
   - The Board card's permission buttons and the mini window's become `permBtns` with a compact look.
   - `chatAskHtml` (an ask in the thread) and `askFoot` (the open ask) share the question and option markup.

   Risk: the Board card is tight on space. "Always…" may need to fold.
8. **Crew member on the shared chat.** Size L. Higher risk.
   - A member's feed (its brief, its messages, its steps, its answer) becomes a thread `chatHtml` can draw.
   - `agentChatHtml` and `crewMateView` go. The Desk and the Board then show a member the same way, with Listen, Read, links and Show all.
   - The diff and review keep their own tab.

   Risk: a member's steps come from the lead's feed, not a session of its own. The thread needs to be built from `helpersOf`/`bgFull` and `crewMateSteps`. Needs a design pick for the Board crew page, which today is diff first. Do it last.
9. **Clean up.** Size S.
   - Remove `deskSessChat`, the unused variables, the second review renderer and `crewDiffHtml` (use `diffParse`).
   - Fold `quietTurn` and `parkedPane` into `chatHtml` where they can be. Both draw a session's old turns.

**Order and why:**
- Step 1 first, because it fixes what can be seen today at almost no risk.
- Steps 2 and 3 next: they remove the two largest copies.
- Step 4 makes every later difference a one-line setting.
- Step 5 needs product picks.
- Steps 6 and 7 are independent of each other and can go side by side.
- Step 8 is the largest and needs a design. It goes last.

**Total:** about two weeks of work across 9 steps. After step 4, any new chat feature lands once, in `chatHtml`, `box()` or `statusLine`, and shows wherever the table lets it.
