# #527 step 2: where two copies of a message's style disagreed

Each kind of message now takes one shared rule in `ui/styles/messages.css`; the per-screen copies are gone. Where
the copies differed, the shared one won. Every such difference is below.

| Kind | Screens | What differed | What it is now |
| --- | --- | --- | --- |
| Your own message (the bubble, `.mg-you`) | session chat and item page (chat.css `.cv-you`), Desk lead's timeline (desk-lead.css `.dk-tmsg .cv-you`), Inbox "what you wrote" and pending rows (inbox.css `.ib-yours > .cv-you`, `.ib-pending > .cv-you`), the shared rule | widest it gets: 72% in a chat, 85% in the lead's timeline and the Inbox, 80% in the shared rule (overridden everywhere) | 80% everywhere (messages.css `.mg-you`, which also took the chat copy's `min-width: 0`) |
| Your message picked out (a search hit in a chat, your reply in an opened History row) | sessions.css `.mg-you.cv-hit .mg-text`, sheets.css `.hreply .mg-text` | the search hit had a 2px ring in the selection colour round the bubble; the History reply had none | one rule, `.mg-you:is(.cv-hit, .hreply) .mg-text`, with the ring on both |
| A button in a status line (Insights' Continue/Keep, Out of usage's and Stuck's buttons, the item page's Not started yet) | box.css `.mg-status.limit/.stuck .btn`, desk-page.css `.dk-notyet .btn`, insights.css `.ins-status .btn` | gap before the button: 6px (usage, stuck, not started), 4px (Insights); `flex: none` only on the item page's | `.mg-status .btn { margin-left: 6px; flex: none }` for every status line |
| A status line that needs you (out of usage, stuck, not started yet) in ink | box.css (`.limit`, `.stuck`), desk-page.css (`.dk-notyet`, only inside `.dk-ichat`) | same colour; the item page's only applied inside the item's chat | one rule, `.mg-status:is(.limit, .stuck, .dk-notyet)`, wherever the line shows |
| Markdown inside a message (no margin above the first block, below the last) | desk-page.css `.mg .dk-md`, desk-lead.css `.dk-tmsg .mg-text .dk-md`, chat.css `.peer-msg .msg/.mg-full > :last-child` | the lead's timeline trimmed only inside `.dk-tmsg`; a forwarded message trimmed only in `.peer-msg` | one rule for any card or message text: `:is(.mg, .mg-text) .dk-md`, `.mg :is(.msg, .mg-full) > :last-child` |

No size, weight or radius differed between the remaining copies: the status line's bold (`.ins-status b`) was the
shared `.mg-status b` already, and was dropped as a copy.
