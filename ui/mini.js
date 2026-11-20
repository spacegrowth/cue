// Cue mini window — Cue at the edge of your screen while you work in another app (Settings → Mini window).
// One page, two shapes: the rail on the right edge (#rail: picture in picture, as Cue's floating terminal was, a
// small dark tab tucked into a side edge wherever you dragged it, that springs out into a panel) and the dark
// island at the notch (#notch). Both show
// what needs you in the Waiting column's order (in conversation first, then newest first), five at most, and
// open any of them into its whole chat with a box to answer it. The window's size follows what it shows
// (mini_shape); it takes the keyboard only once you click into its box (mini_key), and gives it back after.
// The open chat is Cue's own: mini.html loads the main window's scripts (all but its draw and its boot, app.js), and
// the chat, what's under it and their buttons are drawn and handled by the same code as in Cue's window (chat/pane.js
// paneArgs, activeChat, paneFoot; app/clicks.js CHAT_CLICKS). This file is the frame around it: the tabs, ⤢ In Cue,
// ×, the size and place, the pointer and the keyboard.
// Everything lives inside one strict block: its names stay its own (core.js's state, view, active… are the main
// window's, and a second top-level declaration of one would stop this script from running at all). The one name it
// gives the page is renderMain (and render): what Cue's chat code calls to redraw is this window's paint.
"use strict";
{
  const NOTCH = location.hash === "#notch";
  const MAX = 5;                      // rows at most; the rest are counted, and open in Cue
  // The open chat's height at most, as a share of the screen's: at the notch, enough on a 14" laptop for the chat to
  // show a few lines over the crew's line and Cue's box (with its quick phrases).
  const CAP = NOTCH ? 0.5 : 0.45;
  const WIDTH = NOTCH ? { chat: 560 } : { quiet: 36, chat: 420 };   // the rail: its tab, then its panel
  // The notch: the one the island grows out of (Cue measures it; a display without one gets Cue's own, as tall as
  // its menu bar), how far it widens either side when something needs you, and its shoulders' curve.
  const notch = { w: 185, h: 32, real: false };
  const PEEK = 68, SHOULDER = 14;
  const TAB_H = 76;                   // the rail's tab, tucked away
  const HOVER_MS = 600, LEAVE_MS = 450;
  const root = document.getElementById("mini");
  // shape: quiet | chat. open: the session whose chat is open. keyed: it has the keyboard (you clicked in).
  // The side edge Cue put this display's tab on (where you dragged it; Cue keeps that for each display and
  // says which side with each mini_shape): the arrow and the panel's side follow it.
  const pos = { left: false };
  const m = { shape: "quiet", open: null, keyed: false, over: false, timer: 0, sized: "" };

  // ---------- what needs you, in the Waiting column's order ----------
  /** Back within a few minutes of your last message to it, and not sat there longer than that (board.js talking). */
  function talking(it) {
    if (it.back_ms || now() - it.created_ms > TALK_MS) return false;
    const you = (sessionOf(it.session_id)?.thread || []).filter((e) => e.role === "you" && e.at_ms <= it.created_ms).pop();
    return !!you && it.created_ms - you.at_ms <= TALK_MS;
  }
  const newest = (a, b) => (!!a.back_ms - !!b.back_ms) || (a.back_ms ? a.back_ms - b.back_ms : b.created_ms - a.created_ms);
  /** Conversations first (in the order they came back), then newest first; parked sessions' finished turns stay in Need to decide.
   *  What a Desk already counts (deskCovered) isn't here: it's in the Desk's tab. */
  function needs(most = MAX) {
    const list = once("needs", () => {
      const parked = new Set(state.sessions.filter((s) => s.later_ms).map((s) => s.session_id));
      const covered = deskCovered();
      const all = state.items.filter((i) => (i.kind !== "waiting" || !parked.has(i.session_id)) && !covered(i));
      const talk = all.filter(talking).sort((a, b) => a.created_ms - b.created_ms);
      const rest = all.filter((i) => !talking(i)).sort(newest);
      return [...talk, ...rest];
    });
    return { shown: list.slice(0, most) };
  }
  /** Your last message to it, sent but not yet taken (the chat in Cue rings it): it counts as at work. */
  const onItsWay = (s) => { const you = (s?.thread || []).filter((e) => e.role === "you").pop(); return !!you?.pending && now() - you.at_ms <= TALK_MS; };
  /** Sessions at work (or about to be: a message of yours on its way; or a lead waiting on its own agents, "agent") with nothing waiting on you, newest to start first (the rail's dots: five at most). A message to one is
   *  queued (it reads it when it finishes its current step). */
  function working() {
    const waiting = new Set(state.items.map((i) => i.session_id));
    return state.sessions.filter((s) => (s.state === "working" || s.state === "agent" || onItsWay(s)) && !s.later_ms && !waiting.has(s.session_id)).sort((a, b) => b.since_ms - a.since_ms);
  }
  /** The tabs, five at most: the sessions in conversation with you (one back within a few minutes of your message,
   *  or at work on what you said in the last few minutes), oldest first, then what waits on you, then what's at
   *  work, each newest first. Each { sid, project, cls, talk }. */
  function tabList() {
    const items = needs(Infinity).shown.map((it) => ({ sid: it.session_id, project: it.project, cls: dotCls(it), talk: talking(it), at: it.created_ms, back: it.back_ms || 0 }));
    const works = working().map((s) => ({ sid: s.session_id, project: s.project, cls: "work", talk: workingOnYours(s), at: (s.thread || []).filter((e) => e.role === "you").pop()?.at_ms || s.since_ms, back: 0 }));
    const all = [...items, ...works];
    // In conversation: the ones at work on what you said first, then the ones back to you (your turn).
    const talk = all.filter((t) => t.talk).sort((a, b) => ((a.cls !== "work") - (b.cls !== "work")) || a.at - b.at);
    // Sent to the back (↓) stays below the rest, as in Waiting.
    const rest = (list) => list.filter((t) => !t.talk).sort((a, b) => (!!a.back - !!b.back) || (a.back ? a.back - b.back : b.at - a.at));
    // In conversation first, then what waits on you, then what's only at work.
    return [...talk, ...rest(items), ...rest(works)].slice(0, MAX);
  }
  const asks = (it) => it.kind !== "waiting";
  const itemFor = (sid) => state.items.filter((i) => i.session_id === sid).sort((a, b) => asks(b) - asks(a))[0];
  const dotCls = (it) => asks(it) ? "ask" : "turn";

  // ---------- the Desks (desk.js): what waits on you there, what's in progress ----------
  /** What a paint works out once (the Desks, what waits on you), however many parts of it ask; outside a paint, afresh. */
  let memo = null;
  const once = (key, f) => (!memo ? f() : key in memo ? memo[key] : (memo[key] = f()));
  /** A Desk's tab is open as "desk:<its id>" (m.open); a session's as its id. */
  const DESK = "desk:";
  const deskOpenId = (open = m.open) => (String(open || "").startsWith(DESK) ? open.slice(DESK.length) : null);
  /** The Desks with something waiting on you or at work, the most waiting first: deskGroups' { d, waiting, working, queued… }.
   *  The Desk whose tab is open keeps its tab while you're on it, even once nothing waits or works there. */
  function desks() {
    return once("desks", () => (state.desk?.projects || []).map((p) => deskGroups(p.project))
      .filter((g) => g.waiting.length || g.working.length || `${DESK}${g.d.project}` === m.open)
      // The open Desk first, so the five-tab cap never cuts the tab you're on.
      .sort((a, b) => (`${DESK}${b.d.project}` === m.open) - (`${DESK}${a.d.project}` === m.open) || b.waiting.length - a.waiting.length || b.working.length - a.working.length));
  }
  /** Is the Desk named still there (it can go away while its tab is open)? */
  const deskThere = (id) => (state.desk?.projects || []).some((p) => p.project === id);
  const deskWaitN = () => desks().reduce((n, g) => n + g.waiting.length, 0);
  /** The sessions' cards a Desk already counts: its waiting items' asks, and the turns of the sessions that hold them
   *  (answered from the Desk's tab, so not counted twice). */
  function deskCovered() {
    return once("covered", () => {
      const ids = new Set(), sids = new Set();
      for (const g of desks()) for (const i of g.waiting) {
        if (i.ask?.item_id) ids.add(i.ask.item_id);
        if (i.ask?.sid) sids.add(i.ask.sid);
        if (i.holder) sids.add(i.holder.kind === "crew" ? i.holder.lead_sid : i.holder.sid);
      }
      return (it) => ids.has(it.id) || sids.has(it.session_id);
    });
  }
  /** What a session is on a Desk, as its tab's mark: the Desk's lead or a lead of a crew ("lead"), else a session; "" off any Desk. */
  function deskMark(sid) {
    for (const p of state.desk?.projects || []) {
      const d = deskOf(p.project);
      if (!deskHasSession(d, sid) && !d.items.some((i) => deskGroupKey(i.holder) === sid)) continue;
      return deskSessKind(sid, d) === "session" ? "session" : "lead";
    }
    return "";
  }
  /** Item `i`'s pending ask in Cue's cards (a question or a permission), if any. */
  const deskReq = (i) => i.state === "needs_you" ? deskItemRequest(i) : null;
  /** The box that answers item `i` in its row: a question card's own (its answer goes as the card's), else the box to
   *  its holder, as its page's foot (desk.js deskItemFoot). */
  function deskAnswerKey(i, req = deskReq(i)) {
    if (req) return `mdk:${req.id}`;
    const sid = i.ask?.sid || (i.holder?.kind === "crew" ? i.holder.lead_sid : i.holder?.sid);
    return deskBoxKey(i.holder?.kind === "crew" && !i.ask ? { project: i.project, sid, mate: i.holder.name, item: i.id } : { project: i.project, sid, item: i.id });
  }
  const deskFind = (id) => { for (const p of state.desk?.projects || []) { const i = deskItemsWithRequests(deskOf(p.project)).find((x) => x.id === id); if (i) return i; } return null; };

  // ---------- drawing ----------
  const dot = (it, extra = "") => `<i class="m-dot ${dotCls(it)} ${extra}"></i>`;

  /** The rail tucked away: a small tab at the edge, ‹ over a live dot, the dot red while a session asks, amber
   *  while one waits for your turn, breathing while sessions only work. */
  function tuckHtml() {
    const { shown } = needs(Infinity);
    const cls = shown.find(asks) || deskWaitN() ? "ask" : shown.length ? "turn" : working().length || desks().length ? "work" : "idle";
    return `<button class="m-panel m-tuck ${cls} ${m.over ? "over" : ""}" data-out aria-label="Bring Cue out"><span aria-hidden="true">${pos.left ? "›" : "‹"}</span><i class="m-live"></i></button>`;
  }
  /** The tabs across the chat's top: first a tab for each Desk with something waiting or at work (its tray, its name,
   *  how many wait on you), then the sessions' in tabList's order, five in all. A session's dot blinks while it works,
   *  is full while it's your turn in conversation, striped while it's your turn otherwise (mini.css); a session on a
   *  Desk carries its mark (a lead, a session). A click switches; each keeps what you typed in it. */
  function tabsHtml() {
    const dk = desks().slice(0, MAX).map((g) => {
      const id = `${DESK}${g.d.project}`, n = g.waiting.length;
      return `<button class="m-tab m-dtab ${id === m.open ? "on" : ""}" data-mopen="${esc(id)}">${deskKindIcon("tray")}${esc(deskName(g.d))}${n ? `<span class="m-tn">${n}</span>` : ""}</button>`;
    });
    const ss = tabList().slice(0, MAX - dk.length).map((t) => {
      const mark = deskMark(t.sid);
      return `<button class="m-tab ${t.sid === m.open ? "on" : ""} ${t.talk ? "talk" : ""}" data-mopen="${esc(t.sid)}"><i class="m-dot ${t.cls}"></i>${mark ? deskKindIcon(mark) : ""}${esc(nameOf(t.sid, t.project))}</button>`;
    });
    return [...dk, ...ss].join("");
  }
  function islandHtml() {
    // Left: the Desks first (their tray, and how many of their items wait on you), then what else waits on you,
    // counted, steady. Right: how many are at work, its dot blinking (a blink is always work, as in the tabs; your
    // turn never blinks).
    const { shown } = needs();
    const ask = shown.filter(asks).length, turn = shown.length - ask, busy = working().length, dn = deskWaitN();
    const desk = dn ? `<span class="m-n m-dk-n">${deskKindIcon("tray")}${dot({ kind: "ask" })}${dn}</span>` : "";
    const left = `${desk}${ask ? `<span class="m-n">${dot({ kind: "ask" })}${ask}</span>` : ""}${turn ? `<span class="m-n">${dot({ kind: "waiting" })}${turn}</span>` : ""}`;
    const right = busy ? `<span class="m-n m-quiet"><i class="m-dot work"></i>${busy}</span>` : "";
    // Nothing to show: just the notch (black on its black; on a display without one, Cue's own).
    if (!left && !right) return `<div class="m-panel m-island idle"></div>`;
    return `<div class="m-panel m-island peek"><span class="m-wing">${left}</span><span class="g"></span><span class="m-wing">${right}</span></div>`;
  }

  // ---------- the chat: the frame's head, then Cue's own chat (chat/pane.js) ----------
  /** The head: at the notch the tabs, then the open session's state, ⤢ In Cue and ×; on the rail one bar (dragged
   *  by it) with the tabs, ⤢ and ⇥. */
  function headHtml(sid) {
    if (deskOpenId(sid) !== null) return deskHeadHtml(deskOpenId(sid));
    const it = itemFor(sid), s = sessionOf(sid);
    if (!NOTCH) return `<div class="m-bar" data-bar><div class="m-tabs">${tabsHtml()}</div><button class="m-pipbtn" data-cue="${esc(sid)}" aria-label="Open in Cue">⤢</button><button class="m-pipbtn" data-close aria-label="Tuck it away">⇥</button></div>`;
    const tag = it ? `<span class="m-tag ${dotCls(it)}">${asks(it) ? "ASKING" : "YOUR TURN"}</span>` : s?.state === "working" ? `<span class="m-tag work">WORKING</span>` : "";
    return `<div class="m-tabs">${tabsHtml()}</div><div class="m-chat-head">${tag}<span class="g"></span><button class="m-link" data-cue="${esc(sid)}">⤢ In Cue</button><button class="m-link" data-close aria-label="Close">×</button></div>`;
  }
  /** The open chat's panel: its head, and the pane drawPane fills (a Desk's: its list). */
  const panelHtml = () => `<div class="m-panel m-chat${deskOpenId() !== null ? " m-dkp" : ""}"><div class="m-head" data-head>${headHtml(m.open)}</div><div class="${deskOpenId() !== null ? "m-dk" : "active-pane"}" data-pane></div></div>`;

  // ---------- a Desk's tab: what waits on you there, answered in its row; what's in progress ----------
  /** The Desk's head: how many wait on you, are at work, are queued; ⤢ Open Desk and × (on the rail, ⤢ and ⇥ in its bar). */
  function deskHeadHtml(id) {
    const g = deskGroups(id), q = g.queued.length + g.resume.length + g.paused.length;
    if (!NOTCH) return `<div class="m-bar" data-bar><div class="m-tabs">${tabsHtml()}</div><button class="m-pipbtn" data-mdesk="${esc(id)}" aria-label="Open the Desk in Cue">⤢</button><button class="m-pipbtn" data-close aria-label="Tuck it away">⇥</button></div>${deskCountsHtml(g, q)}`;
    return `<div class="m-tabs">${tabsHtml()}</div><div class="m-chat-head">${deskCountsHtml(g, q, true)}<span class="g"></span><button class="m-link" data-mdesk="${esc(id)}">⤢ Open Desk</button><button class="m-link" data-close aria-label="Close">×</button></div>`;
  }
  /** Red dot N waiting, the accent dot N at work, the clock N queued (each only when there are any). */
  function deskCountsHtml(g, q, inHead = false) {
    const c = [g.waiting.length ? `<span class="m-cnt">${dot({ kind: "ask" })}${g.waiting.length}</span>` : "",
      g.working.length ? `<span class="m-cnt"><i class="m-dot acc"></i>${g.working.length}</span>` : "",
      q ? `<span class="m-cnt">${deskStateIcon({ state: "queued" })}${q}</span>` : ""].join("");
    return inHead ? c : `<div class="m-dk-cnts">${c}</div>`;
  }
  /** Who holds it: the lead's mark and name, → its crew member's; or the session's. */
  function deskWhoHtml(i) {
    const h = i.holder;
    if (!h) return "";
    if (h.kind === "crew") return `${deskKindIcon("lead")}<span>${esc(deskHolderName(h.lead_sid))}</span><span class="m-dk-to">→</span>${deskKindIcon("crew")}<span>${esc(h.name)}</span>`;
    return `${deskKindIcon(h.kind === "lead" ? "lead" : "session")}<span>${esc(h.name || deskHolderName(h.sid))}</span>`;
  }
  /** What item `i` waits on you for, under its row, with its buttons in the row: a question's choices (and Answer, to
   *  type one), Allow / No, Looks right / Send back, or Answer for a question its holder ended its turn on. Answer
   *  opens a box in the row; once sent, the row says what you answered. */
  function deskAskHtml(i, showActions) {
    const said = deskAnswerOf(deskItem(i.project, i.id) || i);
    if (said) return `<div class="m-dk-said"><span class="sent-check">✓</span> You answered: ${esc(said.a)}</div>`;
    if (!showActions) return "";
    if (i.state === "to_check") {
      if (deskBackFor === i.id) return `<div class="m-dk-ask">${deskBackHtml(i.id)}</div>`;
      const line = deskVerb(i)?.[1] || "";
      const acts = deskLooksSoon.has(i.id) ? deskLooksSoonHtml(i.id)
        : `<button class="btn small primary" data-desk-act="looks_right" data-id="${esc(i.id)}">Looks right</button><button class="btn small" data-desk-act="send_back" data-id="${esc(i.id)}">Send back</button>`;
      return `<div class="m-dk-ask">${line ? `<div class="m-dk-q">${esc(line)}</div>` : ""}<div class="m-dk-acts">${acts}</div></div>`;
    }
    const req = deskReq(i);
    const line = deskOneLine(req?.kind === "question" ? questions(req)[0]?.question || deskQuestion(i) : deskQuestion(i));
    const off = req && answering.has(req.id) ? " disabled" : "";
    let acts = "";
    if (req?.kind === "permission") acts = `<button class="btn small primary" data-desk-decision="allow" data-id="${esc(req.id)}"${off}>Allow</button><button class="btn small" data-desk-decision="deny" data-id="${esc(req.id)}"${off}>No</button>`;
    else {
      const q = req ? questions(req) : [];
      // A question (#383): Send its suggestions (board.js), Choose… opens its item on the Desk in Cue, where its whole
      // card is; Answer types your own.
      const choose = `<button class="btn small" data-mdesk-choose="${esc(i.id)}" data-project="${esc(i.project || "")}">Choose…</button>`;
      acts = m.answer === i.id ? "" : `${q.length ? qFoldBtns(req, choose, "btn small") : ""}<button class="btn small" data-mdk-answer="${esc(i.id)}">Answer</button>`;
    }
    const key = m.answer === i.id ? deskAnswerKey(i, req) : "";
    const box = key ? `<div class="m-dk-box"><textarea rows="1" data-text="${esc(key)}" data-mdk-box="${esc(i.id)}" aria-label="Your answer" placeholder="Your answer" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off">${esc(draft(key).text)}</textarea><button class="btn primary small" data-mdk-send="${esc(i.id)}">Send</button></div>` : "";
    return `<div class="m-dk-ask">${line ? `<div class="m-dk-q">${esc(line)}</div>` : ""}${req?.kind === "question" ? qFoldLine(req) : ""}${acts ? `<div class="m-dk-acts">${acts}</div>` : ""}${box}</div>`;
  }
  /** One item: its state, title and age; under them its kind and number, who holds it; then (waiting on you) its ask. */
  function deskRowHtml(i, wait) {
    const at = deskMs(i.state === "working" ? i.updated || i.created : i.created);
    return `<div class="m-dk-it" role="button" tabindex="0" data-mdesk-item="${esc(i.id)}" data-project="${esc(i.project || "")}">
      <div class="m-dk-1">${deskStateIcon(i)}<span class="m-dk-t">${esc(i.title)}</span><span class="m-dk-age" data-ago="${at}">${ago(at)}</span></div>
      <div class="m-dk-2">${deskKindIcon(i)}<span class="m-dk-id">#${esc(i.id)}</span><span class="m-dk-who">${deskWhoHtml(i)}</span></div>${deskAskHtml(i, wait)}</div>`;
  }
  const IN_PROGRESS = 3;   // rows of what's in progress; the rest counted under them
  /** The Desk's list: Waiting on you, then In progress, then a line counting the rest. */
  function deskBodyHtml(id) {
    const g = deskGroups(id);
    const rows = (list, wait) => list.map((i) => deskRowHtml({ ...i, project: i.project || id }, wait)).join("");
    const more = g.working.length - IN_PROGRESS, q = g.queued.length + g.resume.length + g.paused.length;
    const rest = more > 0 || q ? `<div class="m-dk-rest">${more > 0 ? `<span class="m-cnt"><i class="m-dot acc"></i>+${more} working</span>` : ""}${q ? `<span class="m-cnt">${deskStateIcon({ state: "queued" })}${q} queued</span>` : ""}</div>` : "";
    const head = (words, n) => `<div class="m-dk-g">${words}<span class="m-dk-gn">${n}</span></div>`;
    return `${g.waiting.length ? `${head("Waiting on you", g.waiting.length)}${rows(g.waiting, true)}` : `<div class="m-dk-none">Nothing waits on you here.</div>`}${g.review.length ? `${head("Ready to review", g.review.length)}${rows(g.review, true)}` : ""}${g.working.length ? `${head("In progress", g.working.length)}${rows(g.working.slice(0, IN_PROGRESS), false)}` : ""}${rest}`;
  }
  /** The Desk's list drawn again: where it was scrolled and the box with the cursor kept. */
  function drawDesk(id) {
    const pane = root.querySelector("[data-pane]");
    const caret = caretNow(), top = pane.scrollTop;
    put(pane, deskBodyHtml(id));
    pane.querySelectorAll("textarea[data-text]").forEach(grow);
    pane.scrollTop = top;
    caretBack(caret);
  }
  /** What Cue's window shows in its pane for the open session (chat/pane.js openPane), but its head, tabs and
   *  terminal: the light along the top while it works, the chat (its steps, Cue's bubbles, a crew's reviews folded
   *  into them, what it's doing now, your held and queued messages), the reply being read out, the crew's tray and
   *  the box (its quick phrases, its "/" menu) or the ask's card; a picture opened full size over it. */
  function paneHtml(cur) {
    const a = paneArgs(cur);
    const { full } = fullView(a.sid, a.it, a.s, a.harness);
    return `${sweep(a.sid, a.s)}${activeChat(a, full)}${paneFoot(a, full)}${lightboxHtml()}`;
  }
  /** The box with the cursor, where it was in it, and how it was scrolled: put back after a draw. */
  function caretNow() {
    const el = document.activeElement;
    return el?.dataset?.text && root.contains(el) ? { key: el.dataset.text, at: [el.selectionStart, el.selectionEnd], top: el.scrollTop } : null;
  }
  function caretBack(c) {
    const el = c && root.querySelector(`[data-text="${CSS.escape(c.key)}"]`);
    if (!el) return;
    if (el !== document.activeElement) { el.focus(); if (el.setSelectionRange) el.setSelectionRange(...c.at); }
    el.scrollTop = c.top;
  }
  /** The pane drawn afresh, as Cue's window draws its own (app/render.js renderMain): the chat's element lifted out
   *  and put back (chat/patch.js), so it keeps its scroll, its selection and what's fading in; the box keeps the
   *  cursor; the chat stays at its end if it was; the crew's tray and a reply read out placed after. */
  function drawPane() {
    const pane = root.querySelector("[data-pane]");
    if (!pane || !m.open) return;
    if (deskOpenId() !== null) return drawDesk(deskOpenId());
    if (active?.sid !== m.open) active = { id: itemFor(m.open)?.id || null, sid: m.open };
    const cur = current(), old = document.querySelector("[data-chat]"), caret = caretNow();
    const was = { oldChat: old, chatPinned: !!old && old.scrollHeight - old.scrollTop - old.clientHeight < 24 };
    const lifted = liftChat();
    pane.className = `active-pane ${cur?.s?.state === "working" || cur?.s?.compacting_ms ? "busy" : ""}`;
    pane.innerHTML = cur ? paneHtml(cur) : "";
    const kept = settleChat(lifted);
    pane.querySelectorAll("textarea[data-text]").forEach(grow);
    scrollChat(document.querySelector("[data-chat]"), was, lifted, kept);
    if (listening) listenFollow(listening);
    placeTeamRows();
    listenKeep(m.open);
    caretBack(caret);
  }

  /** `el` shows `html`, touched only when that changed: a redraw of the same head (state comes several times a
   *  second) doesn't rebuild it under the pointer, which reset the cursor each time. */
  const drawn = new WeakMap();   // element -> the html it was last given
  const put = (el, html) => { if (el && drawn.get(el) !== html) { el.innerHTML = html; drawn.set(el, html); } };
  // ---------- size and place ----------
  /** The window as big as what it shows: the panel's own size, the chat no taller than its share of the screen. */
  function fit() {
    const p = root.querySelector(".m-panel");
    const cap = Math.round(screen.height * CAP);
    document.documentElement.style.setProperty("--cap", `${cap}px`);
    document.documentElement.style.setProperty("--chat-w", `${WIDTH.chat}px`);
    // The notch's island: the notch's own size with nothing to show (black on its black), wider with its shoulders
    // when something needs you; what drops from it is as wide as WIDTH says, its shoulders either side.
    // Each wing as wide as its counts need (the Desk's tray and count make the left one longer), PEEK at least.
    const wings = [...root.querySelectorAll(".m-island.peek .m-wing")].map((x) => x.getBoundingClientRect().width);
    const peek = Math.max(PEEK, Math.ceil(Math.max(0, ...wings)) + 12 + 10);
    document.documentElement.style.setProperty("--peek", `${peek}px`);
    const w = !NOTCH ? WIDTH[m.shape] || WIDTH.quiet
      : m.shape === "chat" ? WIDTH[m.shape] + 2 * SHOULDER
      : root.querySelector(".m-island.peek") ? notch.w + 2 * (peek + SHOULDER) : notch.w;
    // The chat is always its full height (it scrolls inside), so new text while a session works never resizes it.
    const h = m.shape === "chat" ? cap : !NOTCH ? TAB_H : p ? Math.ceil(p.getBoundingClientRect().height) : 30;
    const args = { w, h, show: true };
    const key = JSON.stringify(args);
    if (key === m.sized) return opened();   // the same size and place: nothing to move
    m.sized = key;
    invoke("mini_shape", args).then((r) => { side(r); measured(r?.notch); repaint(); opened(); }, () => { m.sized = ""; opened(); });
  }
  /** The notch's chat, drawn shut, opens now that its window is its full size: it grows out of the notch
   *  (mini.css .m-shut), on the next frames so the shut shape is painted first. */
  function opened() {
    if (!m.shut || m.leaving) return;
    requestAnimationFrame(() => requestAnimationFrame(() => { m.shut = false; document.body.classList.remove("m-shut"); }));
  }
  /** The notch Cue measured on this display: the island takes its size (drawn again when it changed). */
  function measured(n) {
    if (!NOTCH || !n || (n.w === notch.w && n.h === notch.h && n.real === notch.real)) return;
    Object.assign(notch, { w: Math.round(n.w), h: Math.round(n.h), real: !!n.real });
    const s = document.documentElement.style;
    s.setProperty("--nw", `${notch.w}px`);
    s.setProperty("--nh", `${notch.h}px`);
    s.setProperty("--sh", `${SHOULDER}px`);
    m.sized = "";
    drawn.delete(root);
    paint();
  }
  /** Drawn again once the window has its new size. The page is see-through and WebKit doesn't always paint what a
   *  grown window newly shows: the chat stayed empty until you scrolled it. A scroll of a pixel and back (and a
   *  touch of the panel's opacity) is what makes it paint, as your scroll did. */
  function repaint() {
    const p = root.querySelector(".m-panel");
    if (!p) return;
    p.style.opacity = "0.999";
    requestAnimationFrame(() => {
      p.style.opacity = "";
      const t = root.querySelector("[data-chat]");
      if (!t) return;
      const y = t.scrollTop, atEnd = t.scrollHeight - y - t.clientHeight < 24;
      t.scrollTop = y > 0 ? y - 1 : y + 1;
      requestAnimationFrame(() => { t.scrollTop = atEnd ? t.scrollHeight : y; });
    });
  }
  /** The side Cue says the tab is on: drawn again when that changed (its arrow, the panel's side). */
  function side(r) {
    const left = !NOTCH && !!r?.left;
    if (left === pos.left) return;
    pos.left = left;
    drawn.delete(root);
    paint();
  }
  function shapeHtml() {
    if (m.shape === "chat" && m.open) return panelHtml();
    return NOTCH ? islandHtml() : tuckHtml();
  }
  /** Draw what the shape shows. The same session's chat, drawn again: its head (only when that changed) and its pane
   *  (drawPane: the chat's element and the box's cursor live on). */
  function paint() {
    memo = {};
    try { paintNow(); } finally { memo = null; }
  }
  function paintNow() {
    // The open Desk went away: the tab the window would open on instead (or it folds, with nothing left to show).
    if (m.shape === "chat" && deskOpenId() !== null && !deskThere(deskOpenId())) {
      m.answer = null;
      m.open = first() || null;
      if (!m.open) m.shape = "quiet";
      memo = {};   // worked out for the Desk that went
    }
    // A new shape slides in (mini.css .m-enter); a redraw of the same one doesn't move.
    // The notch's opens out of the notch instead: drawn shut (the notch's size), opened once the window has its
    // new size (fit: opened), so the shape grows in a window already big enough rather than jumping with it.
    if (m.shape !== m.drawnShape && m.shape !== "quiet") {
      if (NOTCH) m.shut = m.shut || !m.drawnShape || m.drawnShape === "quiet";
      else {
        m.entering = true;
        clearTimeout(m.enterT);
        m.enterT = setTimeout(() => { m.entering = false; document.body.classList.remove("m-enter"); }, 300);
      }
    }
    m.drawnShape = m.shape;
    const drawnOpen = m.drawnOpen;
    m.drawnOpen = m.open;
    document.body.className = `mini ${NOTCH ? "notch" : "rail"}${pos.left ? " side-left" : ""} s-${m.shape}${m.entering ? " m-enter" : ""}${m.shut ? " m-shut" : ""}${m.leaving ? " m-leave" : ""}${m.dragging ? " m-drag" : ""}`;
    if (m.shape === "chat" && root.querySelector(".m-chat") && m.open === drawnOpen) {
      put(root.querySelector("[data-head]"), headHtml(m.open));
      drawPane();
    } else {
      const html = shapeHtml();
      if (drawn.get(root) !== html) {
        root.innerHTML = html;
        drawn.set(root, html);
        if (m.shape === "chat") { drawPane(); toLatest(); }
      }
    }
    fit();
  }
  /** Cue's chat code redraws by calling renderMain (and render): here that's this window's paint, held while you
   *  drag out a selection, press a button or type through an input method, as Cue's window holds its own
   *  (app/render.js renderMain; window.js lets it go). */
  globalThis.renderMain = globalThis.render = () => {
    if (dragSelecting() || onPress || composing) { heldDraw = true; return; }
    paint();
  };
  /** The end of the chat: what it's doing now, under its last reply. Again on the next frame, once the window
   *  has its full height and the reply its final layout (scrolled before that, it stopped short of the end). */
  function toLatest() {
    const end = () => { const t = root.querySelector("[data-chat]"); if (t) t.scrollTop = t.scrollHeight; };
    end();
    requestAnimationFrame(() => { end(); requestAnimationFrame(end); });
    setTimeout(end, 250);   // opened from the tab: the window grows to the panel's height a moment later
  }
  const go = (shape) => { if (m.shape !== shape) { m.shape = shape; paint(); } };

  // ---------- what you do ----------
  /** The session the rail opens on: the one you wrote to last, if that was in the last few minutes (from here or
   *  from Cue), else one that asks, else the first in Waiting's order, else the newest at work. */
  function first() {
    const tabs = tabList();
    const lastAt = (sid) => (sessionOf(sid)?.thread || []).filter((e) => e.role === "you").pop()?.at_ms || 0;
    const recent = tabs.map((t) => ({ sid: t.sid, at: lastAt(t.sid) })).filter((t) => now() - t.at <= TALK_MS).sort((a, b) => b.at - a.at)[0];
    // A Desk with something waiting on you: its tab (the first such Desk), before a session's ask.
    const desk = desks().find((g) => g.waiting.length);
    return recent?.sid || (desk && `${DESK}${desk.d.project}`) || needs(Infinity).shown.find(asks)?.session_id || tabs[0]?.sid || (desks()[0] && `${DESK}${desks()[0].d.project}`);
  }
  /** Open a session's chat with the cursor in its box, ready to type (this window comes forward alone, not Cue's
   *  main one; the keyboard goes back to the app you were in when it folds). */
  function openChat(sid) {
    // A Desk's tab: its list, no session's chat (the keyboard comes once you click into a box: bind's focusin).
    if (deskOpenId(sid) !== null) {
      m.open = sid;
      m.answer = null;
      active = null;
      invoke("mini_reading", { sessionId: null }).catch(() => {});
      m.shape = "chat";
      return paint();
    }
    m.open = sid;
    m.answer = null;
    active = { id: itemFor(sid)?.id || null, sid };
    invoke("mini_reading", { sessionId: sid }).catch(() => {});
    m.shape = "chat";
    paint();
    loadSteps(sid);   // now, not on the next tick
    invoke("mini_key", { on: true }).then(() => { m.keyed = true; focusComposer(); }, () => {});
  }
  /** Fold the chat back to the notch or the rail; `giveBack`: hand the keyboard back too (not needed when it
   *  already went, by a click outside). What you typed is kept (the session's draft, as in Cue's chat). */
  function closeChat(giveBack = true) {
    if (m.shape !== "chat" || m.leaving) return;
    if (giveBack && m.keyed) invoke("mini_key", { on: false }).catch(() => {});
    m.keyed = false;
    const fold = () => { m.leaving = false; m.shut = false; document.body.classList.remove("m-leave", "m-shut"); m.open = null; active = null; invoke("mini_reading", { sessionId: null }).catch(() => {}); go("quiet"); };
    // It goes back first (the rail's panel into the edge, the notch's chat into the notch); then the window is the
    // tab or the island again.
    m.leaving = true;
    if (NOTCH) m.shut = true;
    document.body.classList.add("m-leave", ...(NOTCH ? ["m-shut"] : []));
    setTimeout(fold, NOTCH ? 220 : 160);
  }
  /** Esc: a background agent's view or a reply in the reader goes back to the chat; else the chat folds. */
  function escKey() {
    if (helperView && helperViewed(m.open)) return closeHelperView();
    if (readerShown()) return closeReader();
    closeChat();
  }
  /** The rail dragged anywhere, the tab or the panel by its bar (the window itself moves, mini_drag): let go, it
   *  takes the nearer side edge of the display it's over, at that height, and stays there from then on. A drag is
   *  never a click: the tab doesn't open under it. */
  function drag(e, tab) {
    if (e.button !== 0 || (!tab && e.target.closest("button"))) return;
    clearTimeout(m.timer);   // pressed on the tab: not a hover that opens it
    const x0 = e.screenX, y0 = e.screenY;
    const move = (ev) => {
      if (Math.hypot(ev.screenX - x0, ev.screenY - y0) < 4) return;
      // A drag, not a click: Cue moves the window with the pointer from here until you let go (mini_drag).
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      m.dragging = true;
      document.body.classList.add("m-drag");
      const eat = (c) => { c.stopPropagation(); c.preventDefault(); };
      window.addEventListener("click", eat, { capture: true, once: true });
      invoke("mini_drag").then((p) => {
        side(p);
      }, () => {}).finally(() => {
        m.dragging = false;
        m.sized = "";
        drawn.delete(root);   // its arrow and side follow the edge it took
        paint();
        setTimeout(() => window.removeEventListener("click", eat, { capture: true }), 300);
      });
    };
    const up = () => window.removeEventListener("pointermove", move);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up, { once: true });
  }
  const inCue = (sid, doc) => invoke("mini_open_in_cue", { sessionId: sid || null, ...(doc ? { doc } : {}) }).catch(() => {});
  /** The Desk in Cue's window, on item `item` when one is named. */
  const deskInCue = (desk, item) => invoke("mini_open_in_cue", { sessionId: null, desk, ...(item ? { item } : {}) }).catch(() => {});
  /** Answer in an item's row: the box opens there, the cursor in it. */
  function deskAnswerOpen(id) {
    m.answer = id;
    paint();
    root.querySelector(`textarea[data-mdk-box="${CSS.escape(id)}"]`)?.focus();
  }
  /** Send what's typed in item `id`'s row: a question card's answer goes as its answer (board.js respond); a question
   *  its holder ended its turn on goes to the holder about the item, as from its page (desk.js deskSendBox). Either
   *  way the row then says what you answered (desk.js deskAnswerOf). */
  async function deskAnswerSend(id) {
    const i = deskFind(id);
    if (!i) return;
    const req = deskReq(i), key = deskAnswerKey(i, req), text = draft(key).text.trim();
    if (!text) return;
    m.answer = null;
    if (req) {
      const q = questions(req)[0];
      delete drafts[key];
      saveDrafts();
      return respond(req, q ? { behavior: "allow", answers: { [q.question]: text } } : { behavior: "deny", message: text });
    }
    // deskSendBox finds the item on the Desk picked in Cue's window: this item's, while it starts (it reads that
    // before it first waits).
    const was = deskSelected;
    deskSelected = i.project;
    let sent;
    try { sent = deskSendBox(key); } finally { deskSelected = was; }
    await sent;
    renderMain();
  }
  /** Keys in a Desk row's box: Enter sends (⇧Enter a new line), Esc closes it. Send back's box the same (desk.js). */
  function keyDesk(e) {
    const t = e.target, back = /^back:/.test(t.dataset?.text || "") ? t.dataset.text.slice(5) : null, id = t.dataset?.mdkBox || back;
    if (!id || e.isComposing) return false;
    if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); back ? deskAct(id, "back_go") : deskAnswerSend(id); return true; }
    if (e.key !== "Escape") return false;
    e.preventDefault();
    if (back) deskAct(id, "back_cancel");
    else { m.answer = null; paint(); }
    return true;
  }
  /** A link in the chat: a markdown file in Cue's reader, for the session that named it (as a click in Cue's own chat
   *  does); anything else where it opens. */
  function openLink(link, e) {
    e.preventDefault();
    const href = link.dataset.href || "", path = link.dataset.path || (/^file:\/\//i.test(href) ? decodeURI(href.replace(/^file:\/\//i, "")) : "");
    const sid = link.dataset.sid || m.open, local = !sessionOf(sid)?.machine;
    // A file on this Mac: in Cue's window (markdown in Read, any other in its pop-up viewer, file_view.js).
    if (path && local && (sid || /^[/~]/.test(path))) return inCue(sid, path);
    invoke("open_link", { url: href || path }).catch(() => {});
  }
  /** The frame's own buttons (the rail's tab, a tab, ⤢ In Cue, ×), a link, and a terminal's button (the terminal is in
   *  Cue's window); then the chat's, as in Cue's window. */
  const CLICKS_HERE = [
    rule("[data-out]", () => { clearTimeout(m.timer); const sid = first(); if (sid) openChat(sid); }),
    rule("[data-mopen]", (t) => openChat(t.dataset.mopen)),
    // A Desk's tab: ⤢ Open Desk, the buttons in its rows (Desk's own: desk.js deskAct and DESK_CLICKS; a question's
    // choices are Cue's pick, in CHAT_CLICKS), a row itself opening its item on the Desk in Cue.
    rule("[data-mdesk]", (t) => deskInCue(t.dataset.mdesk)),
    rule("[data-desk-act]", (t) => deskAct(t.dataset.id, t.dataset.deskAct, t)),
    rule("[data-desk-decision]", (t) => DESK_CLICKS.deskDecision(t)),
    rule("[data-mdk-answer]", (t) => deskAnswerOpen(t.dataset.mdkAnswer)),
    rule("[data-mdesk-choose]", (t) => deskInCue(t.dataset.project, t.dataset.mdeskChoose)),
    rule("[data-mdk-send]", (t) => deskAnswerSend(t.dataset.mdkSend)),
    rule("[data-mdesk-item]", (t) => deskInCue(t.dataset.project, t.dataset.mdeskItem), (_, x) => !x.closest("button, textarea, .m-dk-ask")),
    rule("[data-cue]", (t) => inCue(t.dataset.cue)),
    rule("[data-close]", () => closeChat()),
    rule("a[data-href], a[data-path]", openLink),
    rule("[data-act=term], [data-act=term-open], [data-act=term-card]", () => inCue(m.open)),
    ...CHAT_CLICKS,
  ];
  /** Keys in the chat: its box's own ("/" menu, ↑ for what you sent, Enter sends, ⌘↵ sends what waits now), a picture
   *  shown full size, and Esc (escKey). The crew's tray takes its Esc first (team.js crewEsc). */
  function keyHere(e) {
    if (keyDesk(e) || keyInBox(e) || keyLightbox(e)) return;
    if (e.key === "Escape") { e.preventDefault(); return escKey(); }
    if (keyTyping(e)) return;
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter" && m.open && sendWaitingNow(m.open)) e.preventDefault();
  }
  function bind() {
    bindChat(root);
    delegate(root, "click", CLICKS_HERE);
    root.addEventListener("focusin", (e) => { if (!m.keyed && e.target.matches("textarea, input")) { m.keyed = true; invoke("mini_key", { on: true }).catch(() => {}); } });
    // A click anywhere outside takes the keyboard elsewhere: the open chat folds away, as a menu would. Not for Finder's
    // picker (📎 in the box, chat/patch.js pickImages): the pictures land in the box that opened it.
    window.addEventListener("blur", () => { if (!picking) closeChat(false); });
    root.addEventListener("keydown", keyHere);
    // Hover opens the first session's chat after a short pause (a pointer passing by doesn't); the dots switch.
    // Leaving folds it back, unless you've clicked into it (then a click outside or Esc does).
    // Where there are no dots to point at (the notch's island), hovering opens the first session's chat.
    // The pointer comes from two places: the page's own events (Cue in front) and Cue itself, which watches it over
    // this window (mini-pointer: WebKit tells a window of an app that isn't in front nothing about the pointer).
    document.body.addEventListener("mouseenter", () => pointerIn());
    document.body.addEventListener("mouseleave", () => pointerOut());
    root.addEventListener("mousemove", () => pointerIn());
    root.addEventListener("pointerdown", (e) => { if (e.target.closest("[data-out]")) drag(e, true); else if (e.target.closest("[data-bar]")) drag(e, false); });
  }
  /** The pointer came over the mini window (the rail's dot, the notch's island): after a short pause the chat
   *  opens, on the session a new card nodded for, else the first in Waiting's order, the cursor in its box. */
  function pointerIn() {
    if (m.over) return;
    m.over = true;
    root.querySelector(".m-tuck")?.classList.add("over");
    clearTimeout(m.timer);
    if (m.shape === "quiet") m.timer = setTimeout(() => { const sid = first(); if (sid) openChat(sid); }, HOVER_MS);
  }
  /** Something typed in the open chat's box (kept: the chat doesn't fold under it). */
  const typing = () => [...root.querySelectorAll("textarea[data-text]")].some((t) => t.value.trim());
  /** It left: an open chat with nothing typed in it folds, the keyboard back to the app you were in. With
   *  something typed it stays (Send, Esc or a click outside). */
  function pointerOut() {
    if (!m.over || m.dragging) return;
    m.over = false;
    root.querySelector(".m-tuck")?.classList.remove("over");
    clearTimeout(m.timer);
    if (m.shape === "chat" && !typing()) m.timer = setTimeout(() => { if (!typing()) closeChat(true); }, LEAVE_MS);
  }
  function theme() {
    // The notch is always Cue's dark theme (in your Color), light mode or not: it hangs from the notch.
    const mode = NOTCH ? "dark" : state.settings?.appearance?.mode ?? "system";
    if (mode === "system") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.dataset.theme = mode;
    // Cue's window's Color, as All projects wears it (core.js projTheme), not a project picked in Cue's window:
    // the rail sits over every app, for every project.
    projFilter = "";
    projTheme();
  }
  /** Cue's state, as the main window takes it (app.js setState): its clock, what's landed out of the outbox. */
  function take(s) {
    state = s;
    if (s.now_ms) clockSkew = s.now_ms - Date.now();
    dropLanded();
    theme();
    renderMain();
  }

  async function boot() {
    bind();
    await loadDrafts();   // the drafts Cue's chat keeps (window.js): what you typed to a session, here or there
    take(await invoke("get_state"));
    await T.event.listen("state", (e) => take(e.payload));
    // The mic in the box (chat/compose.js), and a reply read out in a natural voice (chat/listen.js), as in Cue.
    await T.event.listen("dictation", (e) => onDictation(e.payload));
    await T.event.listen("speak", (e) => { if (listening || kokoroAt) kokoroLine(e.payload); });
    // Ages move on with no new state: once a minute is enough for "4m". In a chat, only its head (its tabs: one
    // stops blinking when its few minutes in conversation are up), its live counts every second, its steps as Cue's.
    setInterval(() => { if (m.shape !== "chat") paint(); else put(root.querySelector("[data-head]"), headHtml(m.open)); }, 60000);
    setInterval(() => { if (m.shape === "chat") followSteps(); }, 500);
    setInterval(() => { if (m.shape === "chat") tickAges(); }, 1000);
    // Cue's window came forward or went back, maybe on another display: placed again, on that one.
    // These two are this window's own (a tab on each display): T.event.listen would hear every window's.
    const own = T.webviewWindow?.getCurrentWebviewWindow?.() || T.event;
    await own.listen("mini-place", () => { m.sized = ""; fit(); });
    // Another display's mini window opened a chat: this one folds its own (only one open at a time).
    await own.listen("mini-fold", () => closeChat(false));
    await own.listen("mini-pointer", (e) => { if (e.payload.out) pointerOut(); else pointerIn(); });
  }
  boot();
}
