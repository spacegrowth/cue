// Cue UI Desk — earlier leads in the lead's chat (#792, option B of #788). After a hand-off the chat starts at a moss
// band ("desk-lead-13 → desk-lead-14"); over it, "Show earlier · desk-lead-13 · 58 messages · 09:10 to 21:30". A click
// puts that lead's messages back above the band, with the same marker for the lead before it at their top. Nothing
// loads until you click. Who led before whom: Cue's (ipc desk_lead_before); their messages: Cue's log (session_log).

/** Per Desk: { lead (the lead it's for), before: sid → the lead before it ({ sid, name, at_ms, handoff, count, first_ms,
 *  last_ms, note, cwd }, null for none, "asking" while Cue is asked), shown: [{ sid, msgs }] oldest first, busy,
 *  tail: sid → that lead's last exchanges (desk/earlier.js deskEarlierTail; null while asked) }. */
const deskEarlier = new Map();

function deskEarlierOf(d) {
  const lead = deskLead(d)?.sid || "";
  let e = deskEarlier.get(d.project);
  if (!e || e.lead !== lead) { e = { lead, before: new Map(), shown: [], busy: false, tail: new Map() }; deskEarlier.set(d.project, e); }
  return e;
}

/** The lead before `sid` on Desk `d`, as known now: asked of Cue the first time (undefined till it answers). */
function deskEarlierBefore(d, e, sid) {
  if (!e.before.has(sid)) {
    e.before.set(sid, "asking");
    invoke("desk_lead_before", { project: d.project, sid }).then((m) => { e.before.set(sid, m?.sid ? m : null); renderMain(); }, () => e.before.set(sid, null));
  }
  const m = e.before.get(sid);
  return m === "asking" ? undefined : m;
}

/** Whether the lead chat starts at a hand-off: its lead had one before it, and its own messages are loaded back to
 *  its start (till then, Earlier messages pages them in as before). */
function deskEarlierStarts(d, s) {
  if (!s || !deskLead(d)) return false;
  const e = deskEarlierOf(d), o = older.get(s.session_id);
  return !!deskEarlierBefore(d, e, s.session_id) && !!o?.done && !o.busy;
}

const deskEarlierHm = (ms) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
const deskEarlierName = (m) => m.name || "The earlier lead";

/** The moss band where the Desk passed from lead `m` to `to`; its hand-off note a quiet link at its right. */
function deskEarlierBand(m, to) {
  const say = m.handoff ? `Hand-off · ${deskEarlierHm(m.at_ms)} · the Desk passed to a fresh lead` : `${deskEarlierHm(m.at_ms)} · the Desk passed to a new lead`;
  const note = m.note ? `<button class="dk-hoff-note" data-desk-earlier-note="${esc(m.sid)}">Hand-off note</button>` : "";
  return `<div class="dk-hoff" data-key="hoff:${esc(m.sid)}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 12h13"/><path d="m13 7 5 5-5 5"/><path d="M20 5v14"/></svg><div class="dk-hoff-w"><div class="dk-hoff-a">${esc(deskEarlierName(m))}<span aria-hidden="true">→</span>${esc(to)}</div><div class="dk-hoff-b">${esc(say)}</div></div>${note}</div>`;
}

/** "Show earlier" for lead `m`: its name, how many messages and from when to when; "Loading…" once clicked. */
function deskEarlierMarker(m, busy) {
  const span = m.count ? ` · ${m.count} ${m.count === 1 ? "message" : "messages"}${m.first_ms ? ` · ${deskEarlierHm(m.first_ms)} to ${deskEarlierHm(m.last_ms || m.at_ms)}` : ""}` : " · no messages kept";
  const inner = busy ? `<span>Loading ${esc(deskEarlierName(m))}'s messages…</span>`
    : `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 14 6-6 6 6"/></svg><span>Show earlier ·</span><b>${esc(deskEarlierName(m))}</b><span>${esc(span)}</span>`;
  return `<button class="dk-earl" data-key="earl:${esc(m.sid)}" data-desk-earlier="${esc(m.sid)}" ${busy || !m.count ? "disabled" : ""}>${inner}</button>`;
}

/** An earlier lead's messages, drawn as the lead's chat draws its own (its Desk lines as lines, Cue's notes short). */
function deskEarlierRows(sid, msgs) {
  const s = { session_id: sid, harness: "claude", thread: msgs, team: [] };
  const rows = chatHtml(null, s, "claude", false, { lines: deskLineHtml, refs: deskRefs, dkOut: true, you: deskLeadNote, youHead: deskAboutHead, noActs: true, noSteps: true });
  return `<div class="dk-earlier" data-key="lead:${esc(sid)}">${rows}</div>`;
}

/** What goes over the lead's own messages in its chat: the marker for the lead before the oldest one shown, then each
 *  earlier lead shown (its messages, the band to the next) down to the band into the lead now. "" with no lead before. */
function deskEarlierTop(d, s) {
  if (!deskEarlierStarts(d, s)) return "";
  const e = deskEarlierOf(d);
  const chain = [...e.shown, { sid: s.session_id }];
  let out = "";
  chain.forEach((seg, n) => {
    const m = deskEarlierBefore(d, e, seg.sid);
    // The band names who it passed to: this lead (the one now, or an earlier one by the name the next band knew it by).
    const name = n === chain.length - 1 ? deskLeadName(d) : deskEarlierName(e.before.get(chain[n + 1].sid) || {});
    if (n === 0 && m) out += deskEarlierMarker(m, e.busy);
    // Into the lead now, its lead before not shown in full: that lead's last exchanges over the band (#841).
    if (m && chain.length === 1) out += deskEarlierTail(e, m);
    if (m) out += deskEarlierBand(m, name);
    if (seg.msgs) out += deskEarlierRows(seg.sid, seg.msgs);
  });
  return out;
}

/** How many of the earlier lead's exchanges show over the band (#841). */
const DESK_TAIL_EXCHANGES = 5;

/** The last `n` exchanges of messages `msgs` (an earlier lead's log): your messages and its replies only, as the chat
 *  draws them; no Desk blocks alone and none of Cue's messages to it. */
function deskEarlierLast(msgs, n = DESK_TAIL_EXCHANGES) {
  const yours = (e) => e.role === "you" && !!deskSaid(e.text).trim() && !cueMsgOf(e.text) && !deskCueNote(e);
  const kept = msgs.filter((e) => yours(e) || (e.role === "agent" && String(e.text ?? "").trim()));
  const at = kept.flatMap((e, i) => (e.role === "you" ? [i] : []));
  return at.length > n ? kept.slice(at[at.length - n]) : at.length ? kept.slice(at[0]) : kept.slice(-n);
}

/** Lead `m`'s last exchanges, muted and read-only, over the band into the lead after it: asked of Cue's log once. */
function deskEarlierTail(e, m) {
  if (!e.tail.has(m.sid)) {
    e.tail.set(m.sid, null);
    invoke("session_log", { sessionId: m.sid }).then((msgs) => { e.tail.set(m.sid, deskEarlierLast(Array.isArray(msgs) ? msgs : [])); renderMain(); }, () => e.tail.set(m.sid, []));
  }
  const msgs = e.tail.get(m.sid);
  if (!msgs?.length) return "";
  const s = { session_id: m.sid, harness: "claude", thread: msgs, team: [] };
  const rows = chatHtml(null, s, "claude", false, { lines: deskLineHtml, refs: deskRefs, dkOut: true, you: deskLeadNote, youHead: deskAboutHead, noActs: true, noSteps: true });
  return `<div class="dk-earlier dk-tail" data-key="tail:${esc(m.sid)}" inert>${rows}</div>`;
}

/** Show earlier: lead `sid`'s messages go back above the band (the chat keeps your place), and Cue is asked who led
 *  before it. */
async function deskEarlierShow(project, sid) {
  const d = deskOf(project), e = d && deskEarlierOf(d);
  if (!e || e.busy || e.shown.some((x) => x.sid === sid)) return;
  e.busy = true;
  renderMain();
  let msgs = await invoke("session_log", { sessionId: sid }).catch(() => []);
  if (!Array.isArray(msgs)) msgs = [];
  e.busy = false;
  if (deskEarlier.get(project) !== e) return;
  e.shown.unshift({ sid, msgs });
  deskEarlierBefore(d, e, sid);
  // What you were reading stays where it was: the messages go in above it.
  const where = `.dk-chat[data-desk-chat="${CSS.escape(project)}"]`, before = document.querySelector(where);
  const hold = before ? before.scrollHeight - before.scrollTop : null;
  deskChatEnd = false;
  renderMain();
  const chat = document.querySelector(where);
  if (chat && hold != null) chat.scrollTop = chat.scrollHeight - hold;
}

/** Its hand-off note, in Read. */
function deskEarlierNote(project, sid) {
  const e = deskEarlier.get(project), m = e?.before && [...e.before.values()].find((x) => x?.sid === sid);
  if (m?.note) readOpen(m.note, sid, m.cwd || null);
}
