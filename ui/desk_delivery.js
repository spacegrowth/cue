// A message from an item's page (yours to its holder or the Desk's lead, a send-back, the lead's note) is in the same
// states as any message in the session it went to (#422): its thread, held or queued in it, still on its way.
/** Message `step` (its words, when, and the hub's delivery record when there is one) from item `item`'s page, where it
 *  is in the session it went to (`to`: that session's id; else its delivery's, else the item's holder's): { st, at,
 *  why, sid } as msgLine takes it. `fallback`: its state when that session shows no sign of it (an older message whose
 *  thread is gone: delivered; one just written: sending). */
function deskMsgState(step, item, to, fallback = "delivered") {
  const sid = step.delivery?.session_id || to || item.holder?.lead_sid || item.holder?.sid;
  const session = sid && (sessionOf(sid) || parkedOf(sid));
  const start = step.delivery?.at_ms || deskMs(step.at);
  const none = { st: fallback, at: fallback === "delivered" ? start : 0, sid };
  // The hub's own word that it didn't go (#437): red, why, and Retry from its step.
  const went = step.delivery?.went || "";
  if (went.startsWith("failed")) return { st: "failed", why: esc(went.replace(/^failed:?\s*/, "") || "it didn't go"), sid, step: step.id };
  if (!session) return none;
  const normalize = (text) => String(text || "").replace(/\s+/g, " ").trim();
  const expected = step.delivery && normalize(step.delivery.text);
  // The same words sent again later: what's in the session after that is the later one's.
  const following = expected ? deskItemLog(item, deskOf(item.project)).filter((c) => c.delivery?.session_id === sid
    && normalize(c.delivery.text) === expected && c.delivery.at_ms > start).map((c) => c.delivery.at_ms) : [];
  const before = following.length ? Math.min(...following) : Infinity;
  const matches = (message) => {
    if (!message) return false;
    const text = normalize(message.text);
    if (expected) return text.includes(expected) || (text.length > 1000 && text.endsWith("…") && expected.startsWith(text.slice(0, -1)));
    return text.includes(`#${item.id}`) && text.includes(normalize(step.text));
  };
  // Without the hub's record, its step is kept to the second and may be saved a moment after the session had it.
  const from = expected ? start : start - 60000;
  const history = (session.thread || []).filter((e) => e.role === "you" && e.at_ms >= from && e.at_ms < before && matches(e));
  const took = history.find((e) => !e.pending && !e.unsent);
  if (took) return { st: "delivered", at: msgTaken(took), sid };
  const kept = (e) => matches(e) && (e.at_ms || start) < before;
  if (kept(session.held)) {
    const why = session.held.unsent ? (session.held.delivery || []).map((x) => x.what).filter((w) => w.startsWith("couldn't send: ")).pop() : "";
    return why ? { st: "failed", why: esc(why.slice(15)), sid, held: true } : { st: "held", sid };
  }
  if (kept(session.queued)) return { st: "queued", sid };
  if (history.some((e) => e.unsent)) return { st: "failed", why: "it may have run as a command, or still be in its box", sid };
  if (history.some((e) => e.pending)) return { st: "sending", sid };
  return none;
}
/** The session item `item`'s page means by a name in a step's "to <name>" / "sent back to <name>" / "note to <name>":
 *  its holder (or the lead it works for) or the Desk's lead; "" for neither. */
function deskToSid(item, name) {
  const h = item.holder, lead = deskLead(deskOf(item.project));
  return h && name === h.name ? h.lead_sid || h.sid : lead && name === lead.name ? lead.sid : "";
}
/** The line's controls for a message on an item's page (#422): Send now while it's queued in that session; Edit (back
 *  into this page's box, desk_act.js deskHeldEdit) and Send now while Cue keeps it there, as the session's own chat
 *  has them, and Cancel (deskHeldCancel); Retry and Edit on one from here that couldn't go (`retry`: its time, deskMsgRetry, deskMsgEdit). */
function deskMsgActs(m) {
  const btn = (act, label, more = "") => `<button class="q-now" ${act}${more}>${label}</button>`;
  const sid = ` data-sid="${esc(m.sid || "")}"`, at = ` data-id="${esc(m.item || "")}" data-at="${esc(String(m.retry || ""))}"`;
  if (m.st === "queued") return btn('data-act="send-now"', "Send now", sid);
  if (m.st === "held" || (m.st === "failed" && m.held)) return `${m.item ? `${btn('data-desk-act="held_edit"', "Edit", `${sid} data-id="${esc(m.item)}"`)} · ` : ""}${btn('data-act="held-now"', "Send now", sid)} · ${btn('data-desk-act="held_cancel"', "Cancel", `${sid} data-id="${esc(m.item || "")}"`)}`;
  if (m.st === "failed" && m.step) return btn(`data-desk-retry="${esc(m.step)}"`, "Retry", ` data-item="${esc(m.item || "")}"`);
  if (m.st === "failed" && m.retry) return `${btn('data-desk-act="msg_retry"', "Retry", at)} · ${btn('data-desk-act="msg_edit"', "Edit", at)}`;
  return "";
}
/** Retry on step `step` of item `id`: its message sent again, as it was. */
async function deskRetry(id, step) {
  try { await invoke("desk_item_act", { id, act: "retry", text: step }); toast("Sent again"); }
  catch (e) { toast(`Not sent: ${e}`, 5000); }
  renderMain();
}
