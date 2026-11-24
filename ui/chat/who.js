// Cue UI — who said a message in a chat, and where it shows (#837): one rule for every chat. You see what you typed
// (your bubble) and what the AI wrote (its reply); everything Cue sends is Cue's: a quiet line, a card of its own, or
// in the Desk's Activity. Decided from the message's tags (the hub's `role`, `cue` and `kind`), never from its words;
// its words may only pick how Cue's message looks. A message from before the tags (no `cue` on it) is drawn as it
// always was (chat/render.js chatRowHtml).

/** Cue's kinds that keep a design of their own (docs/cue-messages.md "Kinds that keep their own design"): an item's
 *  hand-off card, the CREW card, another session's or Desk's band, your words on Cue's question. */
const CHAT_CUE_CARDS = new Set(["brief", "crew", "peer", "question"]);
/** The chats that move Cue's rows to their Activity tab (desk/runs.js): the Desk's lead, session and item chats. */
const CHAT_DESK_CHATS = new Set(["lead", "session", "item"]);
/** Who wrote message `e`: "you" (a message of yours without Cue's mark), "ai" (the agent's reply), or "cue" (one Cue
 *  sent: `cue` or a `kind` on it, as the hub marks a thread copy, a queued or a kept one; or another session's words
 *  Cue passed on, role "peer"). From its tags only. A queued or kept message may come without a role: it's yours-side. */
function chatFrom(e) {
  if (e?.role === "peer") return "cue";
  if (e && (e.role === "you" || e.role == null)) return e.cue || e.kind ? "cue" : "you";
  return "ai";
}
/** The kind of Cue's message `e` (the hub's `kind`); a marked one saved before kinds is a "notice". "" for one that
 *  isn't Cue's. */
const chatCueKind = (e) => (chatFrom(e) !== "cue" ? "" : e.role === "peer" ? "peer" : e.kind || "notice");
/** Where message `e` shows in `chat` ("lead" | "session" | "item" | "board" | "crew"): "bubble" (yours), "reply" (the
 *  AI's), "card" (Cue's with a design of its own), "activity" (Cue's, out of a Desk chat into its Activity tab, as
 *  desk/runs.js moves them) or "line" (Cue's quiet line). Another session's or Desk's band stays in every chat; in the
 *  Desk's chats the hand-off and CREW cards, and a session's question for the lead (its TO LEAD card, the lead's answer
 *  under it), go to Activity with Cue's other rows. */
function chatPlace(e, chat) {
  const from = chatFrom(e);
  if (from === "you") return "bubble";
  if (from === "ai") return "reply";
  const kind = chatCueKind(e);
  if (kind === "peer") return "card";
  if (CHAT_DESK_CHATS.has(chat)) return "activity";
  return CHAT_CUE_CARDS.has(kind) ? "card" : "line";
}
