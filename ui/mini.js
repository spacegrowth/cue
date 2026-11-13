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
  /** Conversations first (in the order they came back), then newest first; parked sessions' finished turns stay in Need to decide. */
  function needs(most = MAX) {
    const parked = new Set(state.sessions.filter((s) => s.later_ms).map((s) => s.session_id));
    const all = state.items.filter((i) => i.kind !== "waiting" || !parked.has(i.session_id));
    const talk = all.filter(talking).sort((a, b) => a.created_ms - b.created_ms);
    const rest = all.filter((i) => !talking(i)).sort(newest);
    const shown = [...talk, ...rest].slice(0, most);
    return { shown };
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

  // ---------- drawing ----------
  const dot = (it, extra = "") => `<i class="m-dot ${dotCls(it)} ${extra}"></i>`;

  /** The rail tucked away: a small tab at the edge, ‹ over a live dot, the dot red while a session asks, amber
   *  while one waits for your turn, breathing while sessions only work. */
  function tuckHtml() {
    const { shown } = needs(Infinity);
    const cls = shown.find(asks) ? "ask" : shown.length ? "turn" : working().length ? "work" : "idle";
    return `<button class="m-panel m-tuck ${cls} ${m.over ? "over" : ""}" data-out aria-label="Bring Cue out"><span aria-hidden="true">${pos.left ? "›" : "‹"}</span><i class="m-live"></i></button>`;
  }
  /** The tabs across the chat's top (tabList's order): a dot that blinks while it works, full while it's your turn in
   *  conversation, striped while it's your turn otherwise (mini.css). A click switches; each
   *  keeps what you typed in it. */
  function tabsHtml() {
    return tabList().map((t) => `<button class="m-tab ${t.sid === m.open ? "on" : ""} ${t.talk ? "talk" : ""}" data-mopen="${esc(t.sid)}"><i class="m-dot ${t.cls}"></i>${esc(nameOf(t.sid, t.project))}</button>`).join("");
  }
  function islandHtml() {
    // Left: what waits on you, counted, steady. Right: how many are at work, its dot blinking (a blink is always
    // work, as in the tabs; your turn never blinks).
    const { shown } = needs();
    const ask = shown.filter(asks).length, turn = shown.length - ask, busy = working().length;
    const left = `${ask ? `<span class="m-n">${dot({ kind: "ask" })}${ask}</span>` : ""}${turn ? `<span class="m-n">${dot({ kind: "waiting" })}${turn}</span>` : ""}`;
    const right = busy ? `<span class="m-n m-quiet"><i class="m-dot work"></i>${busy}</span>` : "";
    // Nothing to show: just the notch (black on its black; on a display without one, Cue's own).
    if (!left && !right) return `<div class="m-panel m-island idle"></div>`;
    return `<div class="m-panel m-island peek"><span class="m-wing">${left}</span><span class="g"></span><span class="m-wing">${right}</span></div>`;
  }

  // ---------- the chat: the frame's head, then Cue's own chat (chat/pane.js) ----------
  /** The head: at the notch the tabs, then the open session's state, ⤢ In Cue and ×; on the rail one bar (dragged
   *  by it) with the tabs, ⤢ and ⇥. */
  function headHtml(sid) {
    const it = itemFor(sid), s = sessionOf(sid);
    if (!NOTCH) return `<div class="m-bar" data-bar><div class="m-tabs">${tabsHtml()}</div><button class="m-pipbtn" data-cue="${esc(sid)}" aria-label="Open in Cue">⤢</button><button class="m-pipbtn" data-close aria-label="Tuck it away">⇥</button></div>`;
    const tag = it ? `<span class="m-tag ${dotCls(it)}">${asks(it) ? "ASKING" : "YOUR TURN"}</span>` : s?.state === "working" ? `<span class="m-tag work">WORKING</span>` : "";
    return `<div class="m-tabs">${tabsHtml()}</div><div class="m-chat-head">${tag}<span class="g"></span><button class="m-link" data-cue="${esc(sid)}">⤢ In Cue</button><button class="m-link" data-close aria-label="Close">×</button></div>`;
  }
  /** The open chat's panel: its head, and the pane drawPane fills. */
  const panelHtml = () => `<div class="m-panel m-chat"><div class="m-head" data-head>${headHtml(m.open)}</div><div class="active-pane" data-pane></div></div>`;
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
    const w = !NOTCH ? WIDTH[m.shape] || WIDTH.quiet
      : m.shape === "chat" ? WIDTH[m.shape] + 2 * SHOULDER
      : root.querySelector(".m-island.peek") ? notch.w + 2 * (PEEK + SHOULDER) : notch.w;
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
    s.setProperty("--peek", `${PEEK}px`);
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
    if (dragSelecting() || (mouseDown && onPress) || composing) { heldDraw = true; return; }
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
    return recent?.sid || needs(Infinity).shown.find(asks)?.session_id || tabs[0]?.sid;
  }
  /** Open a session's chat with the cursor in its box, ready to type (this window comes forward alone, not Cue's
   *  main one; the keyboard goes back to the app you were in when it folds). */
  function openChat(sid) {
    m.open = sid;
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
  /** A link in the chat: a markdown file in Cue's reader, for the session that named it (as a click in Cue's own chat
   *  does); anything else where it opens. */
  function openLink(link, e) {
    e.preventDefault();
    const path = link.dataset.path, sid = link.dataset.sid || m.open;
    if (path && sid && isDocPath(path)) return inCue(sid, path);
    invoke("open_link", { url: link.dataset.href || path }).catch(() => {});
  }
  /** The frame's own buttons (the rail's tab, a tab, ⤢ In Cue, ×), a link, and a terminal's button (the terminal is in
   *  Cue's window); then the chat's, as in Cue's window. */
  const CLICKS_HERE = [
    rule("[data-out]", () => { clearTimeout(m.timer); const sid = first(); if (sid) openChat(sid); }),
    rule("[data-mopen]", (t) => openChat(t.dataset.mopen)),
    rule("[data-cue]", (t) => inCue(t.dataset.cue)),
    rule("[data-close]", () => closeChat()),
    rule("a[data-href], a[data-path]", openLink),
    rule("[data-act=term], [data-act=term-open], [data-act=term-card]", () => inCue(m.open)),
    ...CHAT_CLICKS,
  ];
  /** Keys in the chat: its box's own ("/" menu, ↑ for what you sent, Enter sends, ⌘↵ sends what waits now), a picture
   *  shown full size, and Esc (escKey). The crew's tray takes its Esc first (team.js crewEsc). */
  function keyHere(e) {
    if (keyInBox(e) || keyLightbox(e)) return;
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
