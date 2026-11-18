// A send-back uses the same held, queued and pending message state as the recipient's session.
function deskBackLabel(step, item) {
  const sid = step.delivery?.session_id || item.holder?.lead_sid || item.holder?.sid;
  const session = sid && (sessionOf(sid) || parkedOf(sid));
  if (!session) return "Sent back";
  const normalize = (text) => String(text || "").replace(/\s+/g, " ").trim();
  const expected = step.delivery && normalize(step.delivery.text);
  const start = step.delivery?.at_ms || deskMs(step.at);
  const following = expected ? deskItemLog(item, deskOf(item.project)).filter((c) => c.delivery?.session_id === sid
    && normalize(c.delivery.text) === expected && c.delivery.at_ms > start).map((c) => c.delivery.at_ms) : [];
  const before = following.length ? Math.min(...following) : Infinity;
  const matches = (message) => {
    if (!message) return false;
    const text = normalize(message.text);
    if (expected) return text.includes(expected) || (text.length > 1000 && text.endsWith("…") && expected.startsWith(text.slice(0, -1)));
    return text.includes(`#${item.id}`) && text.includes(normalize(step.text));
  };
  const history = (session.thread || []).filter((e) => e.role === "you" && e.at_ms >= start && e.at_ms < before && matches(e));
  if (history.some((e) => !e.pending && !e.unsent)) return "Sent back";
  const held = [session.held, session.queued].some((e) => matches(e) && (e.at_ms || start) < before);
  const pending = history.some((e) => e.pending || e.unsent);
  return held || pending ? "Sent back (queued)" : "Sent back";
}
