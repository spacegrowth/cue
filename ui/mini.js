// Cue mini window — Cue at the edge of your screen while you work in another app (Settings → Mini window).
// One page, two shapes: the rail on the right edge (#rail: picture in picture, as Cue's floating terminal was, a
// small dark tab tucked into a side edge wherever you dragged it, that springs out into a panel) and the dark
// island at the notch (#notch). Both show
// what needs you in the Waiting column's order (in conversation first, then newest first), five at most, and
// open any of them into its whole chat with a box to answer it. The window's size follows what it shows
// (mini_shape); it takes the keyboard only once you click into its box (mini_key), and gives it back after.
// Everything lives inside one strict block: its names stay its own (core.js's state, view, active… are the main
// window's, and a second top-level declaration of one would stop this script from running at all).
"use strict";
{
  const NOTCH = location.hash === "#notch";
  const MAX = 5;                      // rows at most; the rest are counted, and open in Cue
  const CAP = NOTCH ? 0.33 : 0.45;    // the open chat's height at most, as a share of the screen's
  const WIDTH = NOTCH ? { quiet: 260, nod: 500, chat: 560 } : { quiet: 36, chat: 420 };   // the rail: its tab, then its panel
  const TAB_H = 76;                   // the rail's tab, tucked away
  const HOVER_MS = 600, LEAVE_MS = 450, NOD_MS = 6000;
  const root = document.getElementById("mini");
  // shape: quiet | nod | chat. open: the session whose chat is open. keyed: it has the keyboard (you clicked in).
  // The side edge Cue put this display's tab on (where you dragged it; Cue keeps that for each display and
  // says which side with each mini_shape): the arrow and the panel's side follow it.
  const pos = { left: false };
  const m = { shape: "quiet", open: null, keyed: false, over: false, timer: 0, nodT: 0, sized: "", sending: false, typed: {}, images: {}, draftT: {}, seen: null, nod: null, done: new Set(), note: "" };

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
  /** Sessions at work (or about to be: a message of yours on its way) with nothing waiting on you, newest to start first (the rail's dots: five at most). A message to one is
   *  queued (it reads it when it finishes its current step). */
  function working() {
    const waiting = new Set(state.items.map((i) => i.session_id));
    return state.sessions.filter((s) => (s.state === "working" || onItsWay(s)) && !s.later_ms && !waiting.has(s.session_id)).sort((a, b) => b.since_ms - a.since_ms);
  }
  /** The tabs, five at most: the sessions in conversation with you (one back within a few minutes of your message,
   *  or at work on what you said in the last few minutes), oldest first, then everything else newest first: what
   *  waits on you and what's at work, together. Each { sid, project, cls, talk }. */
  function tabList() {
    const items = needs(Infinity).shown.map((it) => ({ sid: it.session_id, project: it.project, cls: dotCls(it), talk: talking(it), at: it.created_ms, back: it.back_ms || 0 }));
    const works = working().map((s) => ({ sid: s.session_id, project: s.project, cls: "work", talk: workingOnYours(s), at: (s.thread || []).filter((e) => e.role === "you").pop()?.at_ms || s.since_ms, back: 0 }));
    const all = [...items, ...works];
    const talk = all.filter((t) => t.talk).sort((a, b) => a.at - b.at);
    // Sent to the back (↓) stays below the rest, as in Waiting.
    const rest = all.filter((t) => !t.talk).sort((a, b) => (!!a.back - !!b.back) || (a.back ? a.back - b.back : b.at - a.at));
    return [...talk, ...rest].slice(0, MAX);
  }
  const doing = (s) => s.doing || (s.prompt ? `› ${s.prompt}` : "working");
  const asks = (it) => it.kind !== "waiting";
  const itemFor = (sid) => state.items.filter((i) => i.session_id === sid).sort((a, b) => asks(b) - asks(a))[0];
  const name = (it) => nameOf(it.session_id, it.project);
  /** What a finished turn came back with, without cutting any of it short: its size and what's in it. */
  function shapeOf(text) {
    const t = String(text || "");
    const words = plain(t).split(" ").filter(Boolean).length;
    const bits = [words > 60 ? `${words} words` : ""];
    if (/```/.test(t)) bits.push("code");
    if (/^\s*\|.*\|\s*$/m.test(t)) bits.push("a table");
    return bits.filter(Boolean).join(" · ");
  }
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
  /** The tabs across the chat's top (tabList's order); a session in conversation blinks. A click switches; each
   *  keeps what you typed in it. */
  function tabsHtml() {
    return tabList().map((t) => `<button class="m-tab ${t.sid === m.open ? "on" : ""} ${t.talk ? "talk" : ""}" data-open="${esc(t.sid)}"><i class="m-dot ${t.cls}"></i>${esc(nameOf(t.sid, t.project))}</button>`).join("");
  }
  function islandHtml() {
    const { shown } = needs();
    const ask = shown.filter(asks).length, turn = shown.length - ask, busy = working().length;
    const left = `${ask ? `<span class="m-n">${dot({ kind: "ask" })}${ask}</span>` : ""}${turn ? `<span class="m-n">${dot({ kind: "waiting" })}${turn}</span>` : ""}`;
    return `<div class="m-panel m-island"><span class="m-wing">${left}</span><span class="g"></span><span class="m-wing m-quiet">${busy ? `<i class="m-dot work"></i>${busy}` : ""}</span></div>`;
  }
  function nodHtml() {
    const it = state.items.find((i) => i.id === m.nod);
    if (!it) return islandHtml();
    return `<div class="m-panel m-nod" data-open="${esc(it.session_id)}"><div class="m-nod-top">${dot(it)}<b>${esc(name(it))}</b><span class="m-tag ${dotCls(it)}">${asks(it) ? "ASKING" : "YOUR TURN"}</span><span class="g"></span><span class="m-quiet">click to open</span></div>
      <div class="m-quiet">${esc(asks(it) ? summary(it) : shapeOf(finishedText(it)) || "Finished. Your turn.")}</div></div>`;
  }

  // ---------- the chat: the whole conversation, the ask on top, a box at the foot ----------
  const msg = (e) => e.role === "you" ? (e.pending ? youHtml(e.text, "pending", now() - e.at_ms > 30000 ? "Cue hasn't seen it take this yet" : "Waiting for it to take this") : `<div class="m-you">${esc(e.text)}</div>`) : e.role === "agent" && e.text ? `<div class="m-agent msg">${md(e.text)}</div>` : "";
  function threadHtml(sid) {
    const it = itemFor(sid);
    const all = sessionOf(sid)?.thread || it?.thread || [];
    const s = sessionOf(sid);
    // What you queued shows once, as queued, not also as the thread's copy of it (chat/thread.js does the same).
    const q = s?.queued?.text?.trim() || "";
    const isQueued = (e) => !!q && e.role === "you" && Math.abs(e.at_ms - s.queued.at_ms) < 60000 && (e.text.trim() === q || (q.length >= 20 && e.text.includes(q)));
    const msgs = (all.length ? all : it ? [{ role: "agent", text: finishedText(it) }] : []).filter((e) => !isQueued(e));
    return (msgs.map(msg).join("") || `<div class="m-quiet">Nothing said yet.</div>`) + waitingHtml(s) + nowHtml(sid);
  }
  /** Your messages still on their way, as the chat in Cue shows them: kept in Cue until the turn ends, or queued
   *  in the agent until it finishes its current step. */
  /** Your message not taken yet, as the chat in Cue shows it: an outline with a ring going round it, and under
   *  it what's happening to it (and its buttons). `cls`: pending (sent, not taken yet), queued or held. */
  function youHtml(text, cls, note) {
    return `<div class="m-you-wait ${cls}"><div class="m-you">${esc(text)}</div><div class="m-you-meta">${note}</div></div>`;
  }
  function waitingHtml(s) {
    if (!s) return "";
    const sid = esc(s.session_id);
    const why = s.held?.unsent ? (s.held.delivery || []).map((d) => d.what).filter((w) => w.startsWith("couldn't send: ")).pop() : "";
    const held = s.held?.text ? youHtml(s.held.text, "held", `${why ? `Couldn't send it: ${esc(why.slice(15))}` : "Kept in Cue · it goes when this turn ends"} · <button class="m-qnow" data-held-edit="${sid}">Edit</button> · <button class="m-qnow" data-held-now="${sid}">Send now</button>`) : "";
    const queued = s.queued?.text ? youHtml(s.queued.text, "queued", `Queued · it reads this when it finishes the current step · <button class="m-qnow" data-queued-now="${sid}">Send now</button>`) : "";
    return held + queued;
  }
  /** At work: what it's doing now, last in the chat, so its last reply isn't read as the answer to you. */
  function nowHtml(sid) {
    const s = sessionOf(sid);
    if (s?.state !== "working") return "";
    return `<div class="m-now"><i class="m-dot work"></i><span class="m-now-what">${esc(doing(s))}</span><span class="m-age">${ago(s.since_ms)}</span></div>`;
  }
  function askHtml(sid) {
    const it = itemFor(sid);
    if (!it || !asks(it)) return "";
    const answered = m.done.has(it.id);
    const act = it.kind === "permission" && !inTerminal(it)
      ? `<button class="m-btn" data-allow="${esc(it.id)}" ${answered ? "disabled" : ""}>Allow</button><button class="m-btn deny" data-deny="${esc(it.id)}" ${answered ? "disabled" : ""}>Deny</button>`
      : `<button class="m-btn" data-cue="${esc(sid)}">Answer in Cue</button>`;
    return `<div class="m-ask"><b>${esc(verb(it))}</b><div class="m-cmd">${esc(summary(it))}</div><div class="m-acts">${act}</div></div>`;
  }
  function chatHtml() {
    const sid = m.open;
    const it = itemFor(sid);
    const s = sessionOf(sid);
    const tag = it ? `<span class="m-tag ${dotCls(it)}">${asks(it) ? "ASKING" : "YOUR TURN"}</span>` : s?.state === "working" ? `<span class="m-tag work">WORKING</span>` : "";
    const head = NOTCH ? `<div class="m-tabs" data-tabs>${tabsHtml()}</div><div class="m-chat-head">${tag}<span class="g"></span><button class="m-link" data-cue="${esc(sid)}">⤢ In Cue</button><button class="m-link" data-close aria-label="Close">×</button></div>`
      : `<div class="m-bar" data-bar><div class="m-tabs" data-tabs>${tabsHtml()}</div><button class="m-pipbtn" data-cue="${esc(sid)}" aria-label="Open in Cue">⤢</button><button class="m-pipbtn" data-close aria-label="Tuck it away">⇥</button></div>`;
    return `<div class="m-panel m-chat">${head}
      <div data-ask>${askHtml(sid)}</div><div class="m-thread" data-thread>${threadHtml(sid)}</div>
      <div class="m-foot">${quickHtml()}<textarea class="m-box" data-box rows="1" placeholder="${s?.state === "working" && !it ? "Queue a message for" : "Reply to"} ${esc(nameOf(sid, s?.project || it?.project))}…">${esc(m.typed[sid] || "")}</textarea>
      <div class="m-foot-row"><span class="m-quiet">${esc(m.note || "⇧↵ new line")}</span><span class="g"></span><button class="m-btn" data-send ${m.sending ? "disabled" : ""}>Send ↵</button></div></div></div>`;
  }

  /** `el` shows `html`, touched only when that changed: a redraw of the same chat (state comes several times a
   *  second) doesn't rebuild it under the pointer, which reset the cursor each time. */
  const drawn = new WeakMap();   // element -> the html it was last given
  const put = (el, html) => { if (el && drawn.get(el) !== html) { el.innerHTML = html; drawn.set(el, html); } };
  /** Your quick phrases (Settings, the same list as Cue's chat): a click adds one after what you've typed and
   *  sends it at once, as in Cue (chat/compose.js sendPhrase). */
  const quickHtml = () => {
    const list = state.settings?.quick?.phrases || [];
    return list.length ? `<div class="m-quick">${list.map((p) => `<button class="m-qp" data-phrase="${esc(p)}">${esc(p)}</button>`).join("")}</div>` : "";
  };
  function sendPhrase(p) {
    const box = root.querySelector("[data-box]");
    if (!box || m.sending) return;
    box.value = box.value.trim() ? `${box.value.trimEnd()} ${p}` : p;
    send();
  }
  // ---------- size and place ----------
  /** The window as big as what it shows: the panel's own size, the chat no taller than its share of the screen. */
  function fit() {
    const p = root.querySelector(".m-panel");
    const cap = Math.round(screen.height * CAP);
    document.documentElement.style.setProperty("--cap", `${cap}px`);
    document.documentElement.style.setProperty("--chat-w", `${WIDTH.chat}px`);
    const w = WIDTH[m.shape] || WIDTH.quiet;
    // The chat is always its full height (it scrolls inside), so new text while a session works never resizes it.
    const h = m.shape === "chat" ? cap : !NOTCH ? TAB_H : p ? Math.ceil(p.getBoundingClientRect().height) : 30;
    const args = { w, h, show: true };
    const key = JSON.stringify(args);
    if (key === m.sized) return;   // the same size and place: nothing to move
    m.sized = key;
    invoke("mini_shape", args).then((r) => side(r), () => { m.sized = ""; });
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
    if (m.shape === "chat" && m.open) return chatHtml();
    if (m.shape === "nod") return nodHtml();
    return NOTCH ? islandHtml() : tuckHtml();
  }
  /** Draw what the shape shows. A chat you're typing in keeps its box: only the ask and the thread are redrawn. */
  function paint() {
    // A new shape slides in (mini.css .m-enter); a redraw of the same one doesn't move.
    if (m.shape !== m.drawnShape && m.shape !== "quiet") {
      m.entering = true;
      clearTimeout(m.enterT);
      m.enterT = setTimeout(() => { m.entering = false; document.body.classList.remove("m-enter"); }, NOTCH ? 200 : 300);
    }
    m.drawnShape = m.shape;
    const drawnOpen = m.drawnOpen;
    m.drawnOpen = m.open;
    document.body.className = `mini ${NOTCH ? "notch" : "rail"}${pos.left ? " side-left" : ""} s-${m.shape}${m.entering ? " m-enter" : ""}${m.dragging ? " m-drag" : ""}`;
    const box = root.querySelector("[data-box]");
    // The same session's chat, redrawn: only its ask, its messages and the dots (the box you're typing in stays).
    // Another session (a dot clicked): drawn whole.
    if (m.shape === "chat" && box && root.querySelector(".m-chat") && m.open === drawnOpen) {
      const t = root.querySelector("[data-thread]");
      const atEnd = t.scrollHeight - t.scrollTop - t.clientHeight < 24;
      put(root.querySelector("[data-ask]"), askHtml(m.open));
      put(t, threadHtml(m.open));
      if (atEnd) t.scrollTop = t.scrollHeight;
      root.querySelector("[data-send]").disabled = m.sending;
      root.querySelector(".m-foot-row .m-quiet").textContent = m.note || "⇧↵ new line";
      put(root.querySelector("[data-tabs]"), tabsHtml());
    } else {
      const html = shapeHtml();
      if (drawn.get(root) !== html) {
        root.innerHTML = html;
        drawn.set(root, html);
        if (m.shape === "chat") toLatest();
      }
    }
    fit();
  }
  /** The end of the chat: what it's doing now, under its last reply. Again on the next frame, once the window
   *  has its full height and the reply its final layout (scrolled before that, it stopped short of the end). */
  function toLatest() {
    const end = () => { const t = root.querySelector("[data-thread]"); if (t) t.scrollTop = t.scrollHeight; };
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
    invoke("mini_reading", { sessionId: sid }).catch(() => {});
    m.note = "";
    m.shape = "chat";
    paint();
    invoke("mini_key", { on: true }).then(() => { m.keyed = true; root.querySelector("[data-box]")?.focus(); }, () => {});
  }
  /** Fold the chat back to the notch or the rail; `giveBack`: hand the keyboard back too (not needed when it
   *  already went, by a click outside). What you typed is kept for when you open it again. */
  function closeChat(giveBack = true) {
    if (m.shape !== "chat" || m.leaving) return;
    if (giveBack && m.keyed) invoke("mini_key", { on: false }).catch(() => {});
    m.keyed = false;
    const fold = () => { m.leaving = false; document.body.classList.remove("m-leave"); m.open = null; invoke("mini_reading", { sessionId: null }).catch(() => {}); go("quiet"); };
    if (NOTCH) return fold();
    // The rail's panel slides back into the edge first; then the window is the tab again.
    m.leaving = true;
    document.body.classList.add("m-leave");
    setTimeout(fold, 160);
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
  /** The message Cue kept, back in the box to change (it doesn't go when the turn ends any more). */
  async function heldEdit(sid) {
    const h = await invoke("unhold_message", { sessionId: sid }).catch(() => null);
    if (!h) return;
    const box = root.querySelector("[data-box]");
    const typed = (box?.value || m.typed[sid] || "").trim();
    m.typed[sid] = typed ? `${h.text}\n\n${typed}` : h.text;
    if (box) { box.value = m.typed[sid]; box.focus(); }
    keepDraft(sid);
    paint();
  }
  async function answer(id, behavior) {
    if (m.done.has(id)) return;   // one click: it can't go twice
    m.done.add(id);
    paint();
    await invoke("respond", { id, decision: { behavior } }).catch((e) => { m.done.delete(id); m.note = String(e); paint(); });
  }
  async function send() {
    const sid = m.open, box = root.querySelector("[data-box]");
    const text = box?.value.trim();
    if (!sid || !text || m.sending) return;
    const it = state.items.find((i) => i.session_id === sid && i.kind === "waiting");
    m.sending = true;
    paint();
    try {
      const said = it ? await invoke("reply", { id: it.id, text }) : await invoke("send_to_session", { sessionId: sid, text });
      m.typed[sid] = "";
      m.images[sid] = [];
      box.value = "";
      keepDraft(sid);
      m.note = typeof said === "string" && said ? said : `Sent to ${nameOf(sid, sessionOf(sid)?.project)}`;
    } catch (e) {
      m.note = String(e);
    }
    m.sending = false;
    paint();
  }
  /** A new card since the last state: the island nods with it for a few seconds; the rail's dot bounces. */
  function arrivals() {
    const ids = new Set(state.items.map((i) => i.id));
    const all = m.seen ? state.items.filter((i) => !m.seen.has(i.id)).sort((a, b) => b.created_ms - a.created_ms) : [];
    const fresh = all[0];
    if (NOTCH && fresh && m.shape === "quiet") {
      m.nod = fresh.id;
      m.shape = "nod";
      clearTimeout(m.nodT);
      m.nodT = setTimeout(() => { if (m.shape === "nod") go("quiet"); }, NOD_MS);
    }
    m.seen = ids;
  }

  function bind() {
    root.addEventListener("click", (e) => {
      const link = e.target.closest("a[data-href], a[data-path]");
      if (link) { e.preventDefault(); invoke("open_link", { url: link.dataset.href || link.dataset.path }).catch(() => {}); return; }
      const t = e.target.closest("button, [data-open]");
      if (!t) return;
      if (t.dataset.out !== undefined) { clearTimeout(m.timer); const sid = first(); if (sid) openChat(sid); }
      else if (t.dataset.open !== undefined) openChat(t.dataset.open);
      else if (t.dataset.cue !== undefined) invoke("mini_open_in_cue", { sessionId: t.dataset.cue || null }).catch(() => {});
      else if (t.dataset.close !== undefined) closeChat();
      else if (t.dataset.allow) answer(t.dataset.allow, "allow");
      else if (t.dataset.deny) answer(t.dataset.deny, "deny");
      else if (t.dataset.send !== undefined) send();
      else if (t.dataset.phrase) sendPhrase(t.dataset.phrase);
      else if (t.dataset.queuedNow) invoke("send_queued_now", { sessionId: t.dataset.queuedNow }).then(() => { m.note = "Sent now: it stopped its step and reads it next"; paint(); }, (err) => { m.note = String(err); paint(); });
      else if (t.dataset.heldNow) invoke("send_held_now", { sessionId: t.dataset.heldNow }).then(() => { m.note = "Sent now: it stopped its turn and reads it next"; paint(); }, (err) => { m.note = String(err); paint(); });
      else if (t.dataset.heldEdit) heldEdit(t.dataset.heldEdit);
      else if (t.dataset.copyCode !== undefined) invoke("copy_text", { text: codeText(t) }).then(() => { m.note = "Copied"; paint(); }, () => {});
    });
    root.addEventListener("focusin", (e) => { if (e.target.matches("[data-box]")) { m.keyed = true; invoke("mini_key", { on: true }).catch(() => {}); } });
    // A click anywhere outside takes the keyboard elsewhere: the open chat folds away, as a menu would.
    window.addEventListener("blur", () => closeChat(false));
    root.addEventListener("input", (e) => { if (e.target.matches("[data-box]")) { m.typed[m.open] = e.target.value; grow(e.target); keepDraft(m.open); } });
    root.addEventListener("keydown", (e) => {
      if (e.key === "Escape") { e.preventDefault(); closeChat(); }
      else if (e.key === "Enter" && !e.shiftKey && e.target.matches("[data-box]")) { e.preventDefault(); send(); }
    });
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
    if (m.shape === "quiet" || m.shape === "nod") m.timer = setTimeout(() => { const sid = m.nod ? state.items.find((i) => i.id === m.nod)?.session_id || first() : first(); if (sid) openChat(sid); }, HOVER_MS);
  }
  /** Something typed in the open chat's box (kept: the chat doesn't fold under it). */
  const typing = () => !!root.querySelector("[data-box]")?.value.trim();
  /** It left: an open chat with nothing typed in it folds, the keyboard back to the app you were in. With
   *  something typed it stays (Send, Esc or a click outside). */
  function pointerOut() {
    if (!m.over || m.dragging) return;
    m.over = false;
    root.querySelector(".m-tuck")?.classList.remove("over");
    clearTimeout(m.timer);
    if (m.shape === "chat" && !typing()) m.timer = setTimeout(() => { if (!typing()) closeChat(true); }, LEAVE_MS);
  }
  /** What you type is that session's draft, kept in Cue's database as Cue's own chat keeps it (the same key,
   *  "s:<session>"), so it's still there after a restart; saved half a second after you stop. Its images, if
   *  Cue's chat has any for it, are kept as they are. */
  function keepDraft(sid) {
    clearTimeout(m.draftT[sid]);   // one wait per session: switching sessions doesn't drop the last one's save
    m.draftT[sid] = setTimeout(() => invoke("set_draft", { key: `s:${sid}`, text: m.typed[sid] || "", images: m.images[sid] || [] }).catch(() => {}), 500);
  }
  async function loadDrafts() {
    const saved = await invoke("get_drafts").catch(() => ({}));
    for (const [k, d] of Object.entries(saved || {})) {
      if (!k.startsWith("s:")) continue;
      m.typed[k.slice(2)] = d.text || "";
      m.images[k.slice(2)] = d.images || [];
    }
  }
  /** The box grows with what you type: 5 lines on the rail, 3 at the notch, then it scrolls. */
  function grow(box) {
    const line = 19.5, most = NOTCH ? 3 : 5;
    box.style.height = "auto";
    box.style.height = `${Math.min(box.scrollHeight, line * most + 14)}px`;
  }
  function theme() {
    const mode = state.settings?.appearance?.mode ?? "system";
    if (mode === "system") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.dataset.theme = mode;
    // Cue's window's Color, as All projects wears it (core.js projTheme), not a project picked in Cue's window:
    // the rail sits over every app, for every project.
    projFilter = "";
    projTheme();
  }
  function take(s) {
    state = s;
    theme();
    arrivals();
    paint();
  }

  async function boot() {
    bind();
    await loadDrafts();
    take(await invoke("get_state"));
    await T.event.listen("state", (e) => take(e.payload));
    // Ages move on with no new state: once a minute is enough for "4m". In a chat, only its tabs: one stops
    // blinking when its few minutes in conversation are up (the box you're typing in isn't touched).
    setInterval(() => { if (m.shape !== "chat") paint(); else put(root.querySelector("[data-tabs]"), tabsHtml()); }, 60000);
    // Cue's window came forward or went back, maybe on another display: placed again, on that one.
    // These two are this window's own (a tab on each display): T.event.listen would hear every window's.
    const own = T.webviewWindow?.getCurrentWebviewWindow?.() || T.event;
    await own.listen("mini-place", () => { m.sized = ""; fit(); });
    await own.listen("mini-pointer", (e) => { if (e.payload.out) pointerOut(); else pointerIn(); });
  }
  boot();
}
