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
/** Cue's reply to the lead's own lines (a refusal, SHOW, Queued, a route's result: kind "answer"): it starts no turn. */
const chatCueAnswer = (e) => chatFrom(e) === "cue" && e.kind === "answer";
/** The turn reply `n` of thread `ex` answers (#837): the messages before its run of replies, back to the reply before
 *  them, their indexes oldest first. Cue's replies to the lead's lines (chatCueAnswer) are no part of a turn: a reply
 *  after one goes on with the turn before it. [] for a reply with no turn before it. */
function chatTurnOf(ex, n) {
  let i = n - 1;
  while (i >= 0 && (chatFrom(ex[i]) === "ai" || chatCueAnswer(ex[i]))) i--;
  const turn = [];
  for (; i >= 0 && chatFrom(ex[i]) !== "ai"; i--) if (!chatCueAnswer(ex[i])) turn.unshift(i);
  return turn;
}
/** Whether reply `n` of thread `ex` answers a turn Cue started: its turn (chatTurnOf) is all Cue's, by the tags (a
 *  notice, a session's question, another Desk's band…). Such a turn goes to the lead's Activity whole, every reply in
 *  it with it; a turn with any message of yours stays in the chat. */
function chatCueTurn(ex, n) {
  const turn = chatTurnOf(ex, n);
  return turn.length > 0 && turn.every((i) => chatFrom(ex[i]) === "cue");
}
/** The kind of Cue's message `e` (the hub's `kind`); a marked one saved before kinds is a "notice". "" for one that
 *  isn't Cue's. */
const chatCueKind = (e) => (chatFrom(e) !== "cue" ? "" : e.role === "peer" ? "peer" : e.kind || "notice");
/** Where message `e` shows in `chat` ("lead" | "session" | "item" | "board" | "crew"): "bubble" (yours), "reply" (the
 *  AI's), "card" (Cue's with a design of its own), "activity" (Cue's, out of a Desk chat into its Activity tab, as
 *  desk/runs.js moves them) or "line" (Cue's quiet line). Another session's or Desk's band stays in every chat but the
 *  lead's; in the Desk's chats the hand-off and CREW cards, and a session's question for the lead (its TO LEAD card, the lead's answer
 *  under it), go to Activity with Cue's other rows. */
function chatPlace(e, chat) {
  const from = chatFrom(e);
  if (from === "you") return "bubble";
  if (from === "ai") return "reply";
  const kind = chatCueKind(e);
  // Another Desk's band: Activity's in the lead's chat (that chat holds only your words and the lead's answers to
  // them), its band anywhere else.
  if (kind === "peer") return chat === "lead" ? "activity" : "card";
  if (CHAT_DESK_CHATS.has(chat)) return "activity";
  return CHAT_CUE_CARDS.has(kind) ? "card" : "line";
}
