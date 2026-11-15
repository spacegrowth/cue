// ---------- Inbox: every session on one line, what needs you first (the header's Inbox, after Board) ----------
// The list: one line per session under three small headings, Needs you (in needsYou()'s order, board.js), Working, and
// Done (sessions that finished a turn lately and wait on nothing). Each line: its dot, its name, a subject and a dim
// snippet (the ask, or what it's on now), its time at the right. A click opens that session in the list's place: a
// head line (‹ Inbox, its name, its state in a word, "N of M ›" to the next that needs you) over the Board's own chat
// pane (chat/pane.js activePane), as the crew's page draws its lead's (team_page.js). Answered and back at work, it
// goes back to the list. Esc or ‹ Inbox goes back too. With a project picked in the header, only its sessions.

/** The session open in the Inbox's place; null while the list shows. */
let inboxSid = null;
/** The open session had something waiting on you while it was open: once that's answered and it works again, the
 *  Inbox goes back to the list. */
let inboxAsked = false;
/** The session last opened from the list: its line is lit when the list comes back, so you see where you were. */
let inboxLast = null;
/** How long a finished session stays under Done, and how many show there at most. */
const INBOX_DONE_MS = 2 * 3600000, INBOX_DONE_MAX = 6;

/** What needs you, a session once each (its first in needsYou()'s order). */
function inboxNeeds() {
  const seen = new Set();
  return needsYou().filter((i) => !seen.has(i.session_id) && seen.add(i.session_id));
}
/** How many sessions need you, for the header's "Inbox 3". */
const inboxCount = () => inboxNeeds().length;
/** The sessions at work (a turn, compacting, or on their agents) that ask nothing, in Cue's own order. */
function inboxWorking() {
  const asking = new Set(state.items.map((i) => i.session_id));
  return state.sessions.filter((s) => inProj(s.project) && !asking.has(s.session_id) && !cutOff(s) && (s.state === "working" || s.state === "agent" || s.compacting_ms));
}
/** The sessions that finished lately and wait on nothing, newest first, a few. */
function inboxDone() {
  const asking = new Set(state.items.map((i) => i.session_id)), busy = new Set(inboxWorking().map((s) => s.session_id));
  return state.sessions.filter((s) => inProj(s.project) && !asking.has(s.session_id) && !busy.has(s.session_id) && !cutOff(s) && !hiddenIdle.has(idleKey(s)) && now() - (s.since_ms || 0) < INBOX_DONE_MS)
    .sort((a, b) => (b.since_ms || 0) - (a.since_ms || 0)).slice(0, INBOX_DONE_MAX);
}

/** One line: its dot, its name, the subject and its snippet, the time; lit when it was the last one opened. */
function inboxLine(sid, project, dot, subject, snip, at) {
  return `<button class="ib-row ${inboxLast === sid ? "on" : ""}" data-inbox-open="${esc(sid)}" style="${projStyle(project)}">`
    + `<span class="ib-dot">${dot}</span><span class="ib-who">${nameSpan(sid, project, "ib-name")}</span>`
    + `<span class="ib-what"><span class="ib-subj">${esc(subject)}</span>${snip ? `<span class="ib-snip"> — ${esc(snip)}</span>` : ""}</span>`
    + `<span class="ib-time">${ago(at)}</span></button>`;
}
/** A line under Needs you: an ask's dot in the ask's red, a finished turn's in its project's colour; what it asks. */
function inboxNeedLine(it) {
  const ask = it.kind !== "waiting";
  const subject = ask ? verb(it).replace(/^./, (c) => c.toUpperCase()) : it.interrupted ? "Interrupted" : "Your turn";
  return inboxLine(it.session_id, it.project, ask ? `<span class="hdot ask"></span>` : projDot(it.project), subject, plain(summary(it)), it.created_ms);
}
/** A line under Working: its dot pulsing, what it's on now, and what you asked it. */
function inboxWorkLine(s) {
  const subject = s.compacting_ms ? "Compacting…" : s.state === "agent" ? idleNote(s) : s.doing || "Thinking";
  return inboxLine(s.session_id, s.project, projDot(s.project, true), subject, s.prompt ? `› ${s.prompt}` : "", s.since_ms);
}
/** A line under Done: its dot still, how it ended, and the first line of what it said last. */
function inboxDoneLine(s) {
  const subject = s.state === "stopped" ? "Stopped" : s.state === "limited" ? "Out of usage" : "Finished";
  return inboxLine(s.session_id, s.project, `<span class="ib-still">${projDot(s.project)}</span>`, subject, plain(firstLine(lastSaid(s))), s.since_ms);
}
/** A heading and its lines; nothing when it has none. */
const inboxSection = (title, lines) => lines.length ? `<div class="ib-sect">${title}</div>${lines.join("")}` : "";

/** The list: its head (how many need you, work, are done), then the three groups. */
function inboxList() {
  const needs = inboxNeeds(), work = inboxWorking(), done = inboxDone();
  const counts = [[needs.length, "need you"], [work.length, "working"], [done.length, "done"]].filter(([n]) => n).map(([n, w]) => `${n} ${w}`).join(" · ");
  const body = inboxSection("Needs you", needs.map(inboxNeedLine)) + inboxSection("Working", work.map(inboxWorkLine)) + inboxSection("Done", done.map(inboxDoneLine));
  return `<div class="inbox"><div class="ib-top"><b>Inbox</b><span class="ib-counts">${esc(counts)}</span></div>
    <div class="ib-scroll" data-scroll-key="inbox">${body || `<div class="ib-empty">Nothing needs you, and nothing is working.</div>`}</div></div>`;
}

/** The open session's state in a word, and its look: needs you, your turn, working, else how it sits. */
function inboxState(sid) {
  const s = sessionOf(sid), it = state.items.find((i) => i.session_id === sid);
  if (it) return it.kind === "waiting" ? ["your turn", "need"] : ["needs you", "need"];
  if (s?.state === "working" || s?.compacting_ms) return ["working", "run"];
  return [{ agent: "on its agents", stopped: "stopped", limited: "out of usage" }[s?.state] || "done", ""];
}
/** "N of M ›": where the open session is among those that need you, and the next one; "M need you ›" when it isn't
 *  one of them; nothing when nothing does. */
function inboxStep(sid) {
  const needs = inboxNeeds(), at = needs.findIndex((i) => i.session_id === sid);
  if (!needs.length || (at >= 0 && needs.length === 1)) return at >= 0 ? `<span class="ib-of">1 of 1</span>` : "";
  return `<button class="ib-next" data-inbox-next>${at >= 0 ? `${at + 1} of ${needs.length}` : `${needs.length} need${needs.length === 1 ? "s" : ""} you`} ›</button>`;
}
/** The open session: its head line over the Board's own chat pane. */
function inboxOpened(sid) {
  const s = sessionOf(sid), [word, look] = inboxState(sid);
  return `<div class="inbox-page" data-inbox-page="${esc(sid)}"><div class="ib-head"><button class="ib-back" data-inbox-back>‹ Inbox</button>`
    + `<span class="ib-hname">${esc(nameOf(sid, s.project))}</span><span class="ib-pill ${look}">${esc(word)}</span><span class="grow"></span>${inboxStep(sid)}</div>`
    + `<div class="ib-chat">${activePane()}</div></div>`;
}

/** Whether the open session is done with you: what it waited on is answered and it's at work again. Not while you type
 *  in its box (that would take the box from under you). */
function inboxAnswered(sid) {
  if (state.items.some((i) => i.session_id === sid)) { inboxAsked = true; return false; }
  const f = document.activeElement;
  return inboxAsked && sessionOf(sid)?.state === "working" && !(f?.matches?.("textarea, input") && f.value);
}
/** The Inbox view: the open session, or the list. The open one closes when it goes away, another opens on the Board's
 *  chat, or it was answered and works again. */
function inboxView() {
  const sid = inboxSid;
  if (sid && (!sessionOf(sid) || active?.sid !== sid || inboxAnswered(sid))) { inboxSid = null; inboxAsked = false; }
  return `<div class="board ib-board">${inboxSid ? inboxOpened(inboxSid) : inboxList()}</div>`;
}

/** Open session `sid` in the Inbox's place, on what it waits on (the Board's own pick, setActive), its chat at its end. */
function inboxOpen(sid) {
  const it = state.items.find((i) => i.session_id === sid);
  setActive(it?.id || null, sid);
  view = "inbox";
  inboxSid = inboxLast = sid;
  inboxAsked = !!it;
  drawAtChatEnd();
  focusComposer();
}
/** Back to the list. */
function inboxClose() {
  inboxSid = null;
  inboxAsked = false;
  renderMain();
}
/** The next (`dir` 1) or previous (-1) session that needs you, after the open one; wraps. */
function inboxStepTo(dir = 1) {
  const needs = inboxNeeds().filter((i) => i.session_id !== inboxSid);
  if (!needs.length) return;
  const all = inboxNeeds(), at = all.findIndex((i) => i.session_id === inboxSid);
  const next = at < 0 ? needs[dir > 0 ? 0 : needs.length - 1] : all[(at + dir + all.length) % all.length];
  inboxOpen(next.session_id);
}

/** Keys on the open session: Esc back to the list, N or → the next that needs you, ← the one before. Not while you
 *  type in a box with text in it, or something is open over it or in its chat (those take Esc first). True when it
 *  took the key. */
function inboxKey(e) {
  if (view !== "inbox" || !inboxSid || e.metaKey || e.ctrlKey || e.altKey || e.isComposing) return false;
  if (sheet || lightbox || liveOpen || deleteAsk || renaming || btwFor || menuFor || redirectFor || (typeof moreFor !== "undefined" && moreFor)) return false;
  if (readerShown() || (helperView && helperViewed(helperView.sid))) return false;
  const f = document.activeElement, typing = f?.matches?.("textarea, input, select");
  if (e.key === "Escape" && !e.shiftKey) { if (typing && f.value) return false; f?.blur?.(); inboxClose(); return true; }
  if (typing) return false;
  if (e.key === "n" || e.key === "ArrowRight") { inboxStepTo(1); return true; }
  if (e.key === "ArrowLeft") { inboxStepTo(-1); return true; }
  return false;
}

if (typeof document !== "undefined") {
  document.addEventListener("keydown", (e) => { if (inboxKey(e)) { e.preventDefault(); e.stopImmediatePropagation(); } }, true);
  document.addEventListener("click", (e) => {
    // Inbox in the header while a session is open in it: back to the list (the click goes on to switch the view).
    if (e.target.closest?.('[data-view="inbox"]')) { if (view === "inbox") inboxSid = null; return; }
    const el = e.target.closest?.("[data-inbox-open], [data-inbox-back], [data-inbox-next]");
    if (!el) return;
    e.stopPropagation();
    if (el.dataset.inboxOpen) return inboxOpen(el.dataset.inboxOpen);
    if (el.matches("[data-inbox-back]")) return inboxClose();
    inboxStepTo(1);
  }, true);
}
