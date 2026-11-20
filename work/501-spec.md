# #501 Messages in one family (option B), Mac only

Styles: `ui/styles/messages.css` (loaded right after base.css in ui/index.html and ui/mini.html, so every window has it).
Kinds: docs/cue-messages.md. Rules: no coloured left edge, no `title="…"`, light and dark from base.css's tokens.

## Option B in words

Every message Cue or the Desk shows is the **same compact card** on `--panel-2`, radius `--radius-card`, padding
`--card-pad-y` × `--card-pad-x`, rows `--card-gap` apart:

1. **Head row** (`.mg-h`, meta size): Cue's ring mark (`.mg-mk`, accent) or none, **who** (`.mg-who`, ink 650),
   `→ to` (`.mg-to`), a mono **TAG** (`.mg-tag`: NOTE, NOT DONE, CREW, DONE?, LANDING, FROM ANOTHER DESK, ASKS YOU…),
   the **item chip** (`.mg-id`, a button when it opens the item), the **age** at the right (`.mg-age`, mono).
2. **What it says** (`.mg-t`, body size, ink): two lines; More opens the rest in place, the full text Cue sent in
   `.mg-full` (panel fill, body size, a log's lines in mono).
3. **Foot** (`.mg-f`, meta size): how it went at the left (`.mg-st`: "✓ Delivered to cue-383 · 14:09", "◌ Queued
   for…", `.failed` red mark), actions at the right (`.mg-acts`: `.mg-lk` quiet links More/Less/Answer in words, `.mg-btn`
   small buttons Open #461 / Try again / Its log / Send now; `.pri` for the one filled choice).

An **ask** is that card outlined all round in `--deny` on `--panel` (`.mg.ask`), its tag filled deny. Answered, it
goes back to the plain card (`.mg.ask.answered`) and the answer is the foot's status, "✓ Answered: Yes, delete it".
A message still going is dashed (`.mg.sending/.queued/.held`), one that couldn't go is outlined deny (`.mg.failed`).
The agent's reply stays plain text (`.mg-agent`: `.mg-h` head "Claude Code · 6m ago", `.mg-text`, `.mg-f` with
Listen/Read as `.mg-btn`); your message stays the bubble at the right (`.mg-you`). Another Desk's message is the same
card with the Desk's chip (`.mg-chip`) in the head, no band. The lead's own Desk lines (ROUTE, ITEM, DONE…) are
`.mg-line` rows; the status line under a chat is `.mg-status`; Inbox/board/mini text is `.mg-plain`.

**Two sizes only.** Anything said: `--mg-body` (13.5px/1.5, weight 400). Anything around it: `--mg-meta` (12px).
Plus the mono tag (10.5px) and chip/age (11px). The mock's 12.5px card text is raised to the body size on purpose:
that mismatch is the bug below. A plain line (`.mg-plain`) inherits its row's size (a board card's 12.5px).

## Tokens used

Colour (base.css): `--panel`, `--panel-2`, `--line`, `--ink`, `--muted`, `--accent`, `--on-accent`, `--sel`, `--on-sel`,
`--deny`, `--slab`, `--on-slab`, `--sans`, `--mono`, `--pill-radius`. Added in messages.css `:root` (tokens.json names,
base.css lacks them): `--card-pad-y/x`, `--card-gap`, `--col-gap`, `--radius-tag/btn/card/pane/pill`, `--type-body`,
`--type-body-lh`, `--type-button`, `--type-meta`, `--type-pill`, `--type-tag`, `--on-deny` (dark value in both dark
blocks), and the family's `--mg-body`, `--mg-meta`. No hex outside `:root`.

## Each kind → class

| Kind (docs/cue-messages.md) | Class | Tag |
| --- | --- | --- |
| Every "To a session" row (not done, open again, pick up, moved, parked, paused, resumed, copy taken back, closed, dropped, option picked, folded into, desk paused/resumed, TO LEAD failed/not sent, not a new item, several Desks, held steps answered, Ask me before changed, go back, undo a crew call, teammate quiet, hand off, carry on, no longer the lead, team carried over) | `.mg` (Cue mark, `Cue → X`, chip, `.mg-t` = the short line, `.mg-full` = Cue's text) | its verb word: NOT DONE, OPEN AGAIN, NOTE, MOVED, PARKED, PAUSED, CREW, HAND-OFF… |
| Lead's note to a session | `.mg` | LEAD'S NOTE |
| Done? | `.mg` | DONE? |
| Every "To a Desk lead" row (landing question, wait circle, routes, lands answered, decide open ASKs, away, replies Refused/Noted/…) | `.mg` | LANDING, WAITS, ROUTED, AWAY, REFUSED… |
| Hand-off of an item (#174) | `.mg` with the title bold in `.mg-t`, SEE under it | HAND-OFF |
| New items from you | `.mg`, `You → lead`, one `.mg-line` per item | NEW ITEMS |
| Stopped without DONE / couldn't start | `.mg` + Open #/Try again `.mg-btn` | STOPPED / COULDN'T START |
| TO LEAD from a session | `.mg`, who = the session | TO LEAD |
| Items done | `.mg`, a `.mg-line` per item | DONE |
| New lead hello / hand-over | `.mg` | NEW LEAD |
| From another Desk (#472 band) | `.mg` + `.mg-chip`; foot "The lead answers it below" | FROM ANOTHER DESK |
| Crew lead's `CUE_SAID` (review, fixes, stop, commit, wake, hand out) | `.mg`, "What Cue sent" = `.mg-full` | CREW |
| Lead's ASK / an agent's question or tool ask | `.mg.ask` (`.mg-code` for a command) → `.answered` | ASKS YOU |
| Lead's NOTE: line | `.mg`, `cue-lead → cue-481` | NOTE |
| Lead's ROUTE / ITEM / DONE / TITLE / MERGE / CONTEXT / SEE lines | `.mg-line` (verb `.mg-v`, item `.mg-id`), opened text `.mg-full` | none |
| Ready to check / Waiting for install | `.mg`, item in head, Check it › as `.mg-lk` | READY TO CHECK |
| "Not a short line yet" kinds (question from the item page, carry on after limit, /handoff resume, background agent, cut refused, CLOSE not on main) | `.mg` once cue_said.js knows them; until then `.mg-you` | — |
| /compact, Compacting, Compacted, Working on, Waiting on you, Stopped | `.mg-status` | — |
| Agent reply | `.mg-agent` | — |
| Your message (and its msgLine) | `.mg-you` + `.mg-f` | — |
| Queued/held line on a board card, Inbox row, "Cue asked" in the brief, mini rows | `.mg-plain` (cueMsgPlain's words: `.mg-who`, `.mg-id`, `.mg-w`) | — |

## Per screen: what to switch (one teammate per group, no shared files)

Each group adds the `mg-*` classes to its markup and deletes the old rules **only in the files it owns**; a rule an
owned file still needs from another group's file stays until the last step. Classes in tests/ui/specs move with the
markup (same group).

| Group | Owns (JS) | Owns (CSS) | Today → B |
| --- | --- | --- | --- |
| **A. Session and item chat** (also the mini's chat, which is the same chatHtml) | ui/chat/render.js, ui/chat/thread.js, ui/desk/cue_said.js, ui/team.js | chat.css, box.css, insights.css, team.css, sessions.css | `.cv-cue*` → `.mg`; `msgLine` `.msg-line/.ml-*` → `.mg-f .mg-st`; `.cv-agent .cv-meta` → `.mg-h`, `.cv-text` → `.mg-text` (14px → 13.5px); `.cv-pill`, `.ls-btn` Listen/Read → `.mg-btn`; `.cv-ask*` → `.mg.ask`, `.cv-ask-ans` → `.mg-st` in `.mg-f`; `.cv-status` → `.mg-status`; `.cv-you` → `.mg-you`; `.cue-you` (CUE_SAID) → `.mg`; `.cv-peer` → `.mg` + `.mg-chip` |
| **B. Desk: lead's chat and item page** | ui/desk/lead.js, ui/desk/commit.js, ui/desk/thread.js, ui/desk/page.js | desk-lead.css, desk-page.css | `.dk-say/.dk-sl*` (deskLineKit `ln`) → `.mg-line`; `.dk-say-id` → `.mg-id`; `.dk-note*` → `.mg` NOTE; `.dk-xband/.dk-xb-*` → `.mg` + `.mg-chip`; `.dk-news/.dk-new*` → `.mg` NEW ITEMS; `.dk-sn*` (stop note) and `.dk-nc*` (done note) → `.mg`; `.dk-rchk*` → `.mg` READY TO CHECK; deskHandHtml's hand-off card → `.mg` HAND-OFF |
| **C. Inbox** | ui/inbox/list.js, ui/inbox/side.js, ui/inbox/box.js, ui/desk/inbox.js | inbox.css, inbox_desk.css | `.ib-msg-line`, run lines' Cue/queued words, `.ib-you` "Cue asked" → `.mg-plain`; inboxPendingLine "Queued for X: …" → cueMsgPlain words in `.mg-plain` |
| **D. Notch / mini** | ui/mini.js | mini.css | `.m-dk-q` → `.mg-plain` at the row size; `.m-dk-said` "✓ You answered" → `.mg-st`; its chat follows group A (only spacing tweaks in mini.css) |
| **E. Board card** | ui/board/board.js | cards.css, board.css | `.queued-note` "Queued: “raw”" → `.mg-plain` with cueMsgPlain's words; `.working .prompt` Cue words → `.mg-plain` |

Last step (after A–E): remove leftovers (`.cv-status` tip rules in board.css lines 149–151 move to `.mg-status`),
check `tests/ui/specs/pills.mjs` passes (`.mg-tag`, `.mg-id`, `.mg-chip` are capsules; `.mg-btn` is a button), and
add the UI test below.

## Test case from the Desk lead (lead's chat today)

Today in one lead chat: the lead's reply (`.cv-agent`, 14px/1.6) is larger than the grey NOTE card ("Note to cue-481
· Bug #525", `.dk-note` 12.5px); "Claude Code · 6m ago" is a grey bold 11px `.cv-meta`; "Asked:" is bold with a
hanging indent (`.cv-ask-head` in a flex row, 13px); "Answered: Yes, delete it" is a pill (`.cv-ask-ans`); "Task #527
waits on #510", "Added Task #529: …", "Gave Task #529 to cue-496" are bold `.dk-say-t/.dk-say-id` lines; Listen/Read
are 28px outlined capsules (`.cv-pill`); "Ready to check: … Check it ›" is a 13px band between rules (`.dk-rchk`);
"Compacted: …" is a 12px centred `.cv-status`. Six sizes, four weights.

After #501: reply text, NOTE text, ask text, the item lines and Ready to check's words are all `--mg-body`
(13.5px, 400; only names and the item chip are 650/mono). The header, every foot ("✓ Answered: Yes, delete it"),
and the Compacted line are `--mg-meta` (12px). NOTE, ASK and Ready to check are the same `.mg` card (an ask outlined,
answered goes plain); item lines are `.mg-line` rows with the chip, not bold; Listen/Read are `.mg-btn` (24px, the
same as Open #/Try again). No hanging indent: the ask's words are `.mg-t` under its head row.

**Proposed UI test** `tests/ui/specs/messages-one-family.spec.mjs` (light and dark): a lead chat scenario with that
exact mix (a Cue message, a NOTE line, an answered ASK, ROUTE/ITEM lines, an agent reply with Listen and Read, a Ready
to check, a Compacted status) plus a session chat with a send-back. In the page, for every element under `.ap-chat`
that has its own text: collect computed `font-size`/`font-weight`. Expect the set of sizes ⊆ {13.5, 12, 11, 10.5};
every `.mg-t, .mg-text, .mg-line, .mg-full` is 13.5px/400; every `.mg-h, .mg-f, .mg-status` 12px; `.mg-btn` all one
height; no element with a left border wider than its right one; no `[title]` in the chat; the ask card's border colour
equals `--deny` and, answered, is transparent. A node check (tests/*.test.js) that messages.css has no hex colour
outside `:root` and dark blocks.
