// "Open N" on the Inbox side, under Running now: what a crew left open, as facts read by Cue (hub/open.rs):
// teammates done but not committed, a merge that stopped, a review whose fixes weren't sent, an ask waiting on you.
// One quiet button each. Rows keep the seat they were first seen in, so nothing moves while you look.

const openSeats = new Map();   // row key -> the order it was first seen in
let openSeatNext = 0;
const openSent = new Map();    // row key -> { title, who, at }: Send fixes went, shown in its seat for a while
const OPEN_SENT_MS = 10 * 60 * 1000;
const openSending = new Set(); // row keys whose Send fixes is on its way: their seats are kept for the Sent row

/** The rows of every lead in the shown project, each in its seat. */
function openRows() {
  const rows = [];
  for (const l of state.open || []) if (inProj(l.project)) for (const r of l.rows || []) rows.push(r);
  const t = now(), live = new Set(rows.map((r) => r.key));
  for (const [k, g] of openSent) if (t - g.at > OPEN_SENT_MS) openSent.delete(k);
  for (const k of [...openSeats.keys()]) if (!live.has(k) && !openSent.has(k) && !openSending.has(k)) openSeats.delete(k);
  for (const r of rows) if (!openSeats.has(r.key)) openSeats.set(r.key, openSeatNext++);
  const all = [...rows.filter((r) => !openSent.has(r.key)), ...[...openSent].map(([key, g]) => ({ key, sent: g }))];
  return all.filter((r) => openSeats.has(r.key)).sort((a, b) => openSeats.get(a.key) - openSeats.get(b.key));
}

/** A row's few words (#220): what kind of loose end it is, never the review's note, files or lines (Open or Send
 *  fixes shows those); an ask says what it asks, as its card does, when the ask is still there. */
function openTitle(r) {
  if (r.kind === "review") return "Review not sent";
  if (r.kind !== "ask" || !r.item) return firstLine(r.title);
  const it = (state.items || []).find((x) => x.id === r.item);
  return firstLine(it ? inboxAskSubject(it) : r.title);
}
/** Whose it is, a name: the teammate it's about, else the name the hub put first, else its lead. */
function openName(r) {
  const mate = r.agent && (sessionOf(r.sid)?.team || []).find((t) => (t.agent_id || t.call) === r.agent)?.name;
  if (mate) return mate;
  if (r.kind === "review") return r.who.replace(/^review · for /, "");
  if (r.kind === "uncommitted") return (state.open || []).find((l) => l.lead_sid === r.sid)?.lead || "";
  return (r.who || "").split(" · ")[0];
}

const OPEN_ACT = { open: "Open", reply: "Reply", fixes: "Send fixes" };

/** One line: its few words · whose it is, each cut with an ellipsis when long; one button. */
function openRowHtml(r) {
  const name = (n) => (n ? `<span class="ib-op-o">${esc(n)}</span>` : "");
  if (r.sent) {
    const when = r.sent.at > now() - 60000 ? "just now" : `${ago(r.sent.at)} ago`;
    return `<div class="ib-op sent"><span class="ib-op-l"><span class="ib-op-w">${esc(r.sent.title)}</span>${name(`${r.sent.who} · ${when}`)}</span><span class="ib-op-act done">Sent</span></div>`;
  }
  return `<div class="ib-op${r.needs_you ? " you" : ""}" data-open-key="${esc(r.key)}"><span class="ib-op-l"><span class="ib-op-w">${esc(openTitle(r))}</span>${name(openName(r))}</span>`
    + `<button class="ib-op-act" data-open-act="${esc(r.key)}">${OPEN_ACT[r.act] || "Open"}</button></div>`;
}

/** The group, or "" when nothing is open. */
function inboxOpenBlock() {
  const rows = openRows();
  if (!rows.length) return "";
  const n = rows.filter((r) => !r.sent).length;
  return `<div class="ib-blk ib-open"><div class="ib-k">Open${n ? ` · ${n}` : ""}</div>${rows.map(openRowHtml).join("")}</div>`;
}

function openRowFind(key) {
  for (const l of state.open || []) for (const r of l.rows || []) if (r.key === key) return r;
  return null;
}

async function openAct(r) {
  if (r.act === "reply") return inboxOpen(r.sid);
  if (r.act === "fixes") {
    const name = (sessionOf(r.sid)?.team || []).find((t) => (t.agent_id || t.call) === r.agent)?.name || r.agent;
    // Shown as sent only once it went: refused, the row stays as it was (mateAct keeps the reason under its crew row).
    if (openSending.has(r.key)) return;
    openSending.add(r.key);
    try { await mateAct(r.sid, r.agent, "fixes"); } finally { openSending.delete(r.key); }
    if (mateNotes.has(`${r.sid}|${r.agent}`)) return;
    openSent.set(r.key, { title: "Fixes sent", who: name, at: now() });
    return renderMain();
  }
  // Open: the teammate's row in its lead's chat, else the lead's chat.
  if (r.agent && typeof mateOpenLeads !== "undefined") { mateOpenLeads.add(r.sid); mateShow = `${r.sid}|${r.agent}`; }
  inboxFullChat(r.sid);
}

document.addEventListener("click", (e) => {
  const el = e.target.closest?.("[data-open-act]");
  if (!el) return;
  e.stopPropagation();
  const r = openRowFind(el.dataset.openAct);
  if (r) openAct(r);
}, true);
