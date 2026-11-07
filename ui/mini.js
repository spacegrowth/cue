// Cue mini window — Cue at the edge of your screen while you work in another app (Settings → Mini window).
// One page, two shapes: the rail on the right edge (#rail) and the dark island at the notch (#notch). Both show
// what needs you in the Waiting column's order (in conversation first, then newest first), five at most, and
// open any of them into its whole chat with a box to answer it. The window's size follows what it shows
// (mini_shape); it takes the keyboard only once you click into its box (mini_key), and gives it back after.
// Everything lives inside one strict block: its names stay its own (core.js's state, view, active… are the main
// window's, and a second top-level declaration of one would stop this script from running at all).
"use strict";
{
  const NOTCH = location.hash === "#notch";
  const MAX = 5;                      // rows at most; the rest are counted, and open in Cue
  const TALK = 5 * 60000;             // in conversation: back within 5 minutes of your message (board.js TALK_MS)
  const CAP = NOTCH ? 0.33 : 0.45;    // the open chat's height at most, as a share of the screen's
  const WIDTH = NOTCH ? { quiet: 260, nod: 500, chat: 560 } : { quiet: 28, chat: 448 };   // the rail's chat: 420 + its dots
  const HOVER_MS = 280, LEAVE_MS = 450, NOD_MS = 6000;
  const root = document.getElementById("mini");
  // shape: quiet | nod | chat. open: the session whose chat is open. keyed: it has the keyboard (you clicked in).
  const m = { shape: "quiet", open: null, keyed: false, timer: 0, nodT: 0, sized: "", sending: false, typed: {}, seen: null, nod: null, done: new Set(), note: "" };

  // ---------- what needs you, in the Waiting column's order ----------
  /** Back within a few minutes of your last message to it, and not sat there longer than that (board.js talking). */
  function talking(it) {
    if (it.back_ms || now() - it.created_ms > TALK) return false;
    const you = (sessionOf(it.session_id)?.thread || []).filter((e) => e.role === "you" && e.at_ms <= it.created_ms).pop();
    return !!you && it.created_ms - you.at_ms <= TALK;
  }
  const newest = (a, b) => (!!a.back_ms - !!b.back_ms) || (a.back_ms ? a.back_ms - b.back_ms : b.created_ms - a.created_ms);
  /** Conversations first (in the order they came back), then newest first; parked sessions' finished turns stay in Need to decide. */
  function needs(most = MAX) {
    const parked = new Set(state.sessions.filter((s) => s.later_ms).map((s) => s.session_id));
    const all = state.items.filter((i) => i.kind !== "waiting" || !parked.has(i.session_id));
    const talk = all.filter(talking).sort((a, b) => a.created_ms - b.created_ms);
    const rest = all.filter((i) => !talking(i)).sort(newest);
    const shown = [...talk, ...rest].slice(0, most);
    return { shown, talk: Math.min(talk.length, most) };
  }
  /** Sessions at work with nothing waiting on you, newest to start first (the rail's dots: five at most). A message to one is
   *  queued (it reads it when it finishes its current step). */
  function working() {
    const waiting = new Set(state.items.map((i) => i.session_id));
    return state.sessions.filter((s) => s.state === "working" && !s.later_ms && !waiting.has(s.session_id)).sort((a, b) => b.since_ms - a.since_ms);
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
  const sep = (n, i) => n && i === n ? `<div class="m-sep" role="separator"></div>` : "";

  /** The rail: a dot per session that needs you (in conversation, a separator, newest first), then, under
   *  another separator, a solid dot per session at work. */
  function dotsHtml() {
    const { shown, talk } = needs();
    const busy = working().slice(0, MAX);
    const on = (sid) => m.shape === "chat" && sid === m.open ? "on" : "";
    const btn = (sid, name, inner) => `<button class="m-dotb ${on(sid)}" data-open="${esc(sid)}" aria-label="${esc(name)}">${inner}</button>`;
    const waits = shown.map((it, i) => sep(talk, i) + btn(it.session_id, name(it), dot(it))).join("");
    const works = busy.map((s) => btn(s.session_id, nameOf(s.session_id, s.project), `<i class="m-dot work"></i>`)).join("");
    return `<div class="m-panel m-dots">${waits}${waits && works ? `<div class="m-sep" role="separator"></div>` : ""}${works}</div>`;
  }
  const anything = () => needs().shown.length > 0 || working().length > 0;
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
  const msg = (e) => e.role === "you" ? `<div class="m-you">${esc(e.text)}</div>` : e.role === "agent" && e.text ? `<div class="m-agent msg">${md(e.text)}</div>` : "";
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
  function waitingHtml(s) {
    const one = (m2, note) => m2?.text ? `<div class="m-you m-queued">${esc(m2.text)}<div class="m-quiet">${note}</div></div>` : "";
    return one(s?.held, "Kept in Cue · it goes when this turn ends") + one(s?.queued, "Queued · it reads this when it finishes the current step");
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
    return `<div class="m-panel m-chat"><div class="m-chat-head">${it ? dot(it) : ""}<b>${esc(nameOf(sid, s?.project || it?.project))}</b>${tag}<span class="g"></span>${NOTCH ? dotsHtml() : ""}<button class="m-link" data-cue="${esc(sid)}">⤢ In Cue</button><button class="m-link" data-close aria-label="Close">×</button></div>
      <div data-ask>${askHtml(sid)}</div><div class="m-thread" data-thread>${threadHtml(sid)}</div>
      <div class="m-foot"><textarea class="m-box" data-box rows="1" placeholder="${s?.state === "working" && !it ? "Queue a message for" : "Reply to"} ${esc(nameOf(sid, s?.project || it?.project))}…">${esc(m.typed[sid] || "")}</textarea>
      <div class="m-foot-row"><span class="m-quiet">${esc(m.note || "⇧↵ new line")}</span><span class="g"></span><button class="m-btn" data-send ${m.sending ? "disabled" : ""}>Send ↵</button></div></div></div>`;
  }

  /** `el` shows `html`, touched only when that changed: a redraw of the same chat (state comes several times a
   *  second) doesn't rebuild it under the pointer, which reset the cursor each time. */
  const drawn = new WeakMap();   // element -> the html it was last given
  const put = (el, html) => { if (el && drawn.get(el) !== html) { el.innerHTML = html; drawn.set(el, html); } };
  // ---------- size and place ----------
  /** The window as big as what it shows: the panel's own size, the chat no taller than its share of the screen. */
  function fit() {
    const p = root.querySelector(".m-panel");
    const cap = Math.round(screen.height * CAP);
    document.documentElement.style.setProperty("--cap", `${cap}px`);
    const empty = !NOTCH && m.shape === "quiet" && !anything();
    const w = empty ? 6 : WIDTH[m.shape] || WIDTH.quiet;   // nothing running at all: the rail is a thin strip
    // The chat is always its full height (it scrolls inside), so new text while a session works never resizes it.
    const h = empty ? 200 : m.shape === "chat" ? cap : p ? Math.ceil(p.getBoundingClientRect().height) : 30;
    const args = { w, h, show: true };
    const key = JSON.stringify(args);
    if (key === m.sized) return;   // the same size and place: nothing to move
    m.sized = key;
    invoke("mini_shape", args).catch(() => { m.sized = ""; });
  }
  function shapeHtml() {
    if (m.shape === "chat" && m.open) return NOTCH ? chatHtml() : `<div class="m-panel m-pair">${chatHtml()}${dotsHtml()}</div>`;
    if (m.shape === "nod") return nodHtml();
    if (NOTCH) return islandHtml();
    return anything() ? dotsHtml() : `<div class="m-panel m-hair"></div>`;
  }
  /** Draw what the shape shows. A chat you're typing in keeps its box: only the ask and the thread are redrawn. */
  function paint() {
    // A new shape slides in (mini.css .m-enter); a redraw of the same one doesn't move.
    if (m.shape !== m.drawnShape && m.shape !== "quiet") {
      m.entering = true;
      clearTimeout(m.enterT);
      m.enterT = setTimeout(() => { m.entering = false; document.body.classList.remove("m-enter"); }, 200);
    }
    m.drawnShape = m.shape;
    const drawnOpen = m.drawnOpen;
    m.drawnOpen = m.open;
    document.body.className = `mini ${NOTCH ? "notch" : "rail"} s-${m.shape}${m.entering ? " m-enter" : ""}`;
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
      const dots = root.querySelector(".m-pair > .m-dots, .m-chat-head .m-dots");
      if (dots && dots.outerHTML !== dotsHtml()) dots.outerHTML = dotsHtml();
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
  /** The newest reply in sight from its start (a long one reads from the top), else the end of the chat. */
  function toLatest() {
    const t = root.querySelector("[data-thread]");
    const last = [...t.querySelectorAll(".m-agent")].pop();
    t.scrollTop = last && last.offsetHeight > t.clientHeight ? last.offsetTop - t.offsetTop : t.scrollHeight;
  }
  const go = (shape) => { if (m.shape !== shape) { m.shape = shape; paint(); } };

  // ---------- what you do ----------
  /** The session the rail opens on when you hover it: the first in Waiting's order, else the newest at work. */
  const first = () => needs(1).shown[0]?.session_id || working()[0]?.session_id;
  /** Open a session's chat. `take`: a click, so the box gets the cursor (this window comes forward alone, not
   *  Cue's main one); a hover leaves the keyboard where it is. */
  function openChat(sid, take = true) {
    m.open = sid;
    m.note = "";
    m.shape = "chat";
    paint();
    if (take) invoke("mini_key", { on: true }).then(() => { m.keyed = true; root.querySelector("[data-box]")?.focus(); }, () => {});
  }
  /** Fold the chat back to the notch or the rail; `giveBack`: hand the keyboard back too (not needed when it
   *  already went, by a click outside). What you typed is kept for when you open it again. */
  function closeChat(giveBack = true) {
    if (m.shape !== "chat") return;
    m.open = null;
    if (giveBack && m.keyed) invoke("mini_key", { on: false }).catch(() => {});
    m.keyed = false;
    go("quiet");
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
      box.value = "";
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
      if (t.dataset.open !== undefined) openChat(t.dataset.open, true);
      else if (t.dataset.cue !== undefined) invoke("mini_open_in_cue", { sessionId: t.dataset.cue || null }).catch(() => {});
      else if (t.dataset.close !== undefined) closeChat();
      else if (t.dataset.allow) answer(t.dataset.allow, "allow");
      else if (t.dataset.deny) answer(t.dataset.deny, "deny");
      else if (t.dataset.send !== undefined) send();
      else if (t.dataset.copyCode !== undefined) invoke("copy_text", { text: codeText(t) }).then(() => { m.note = "Copied"; paint(); }, () => {});
    });
    root.addEventListener("focusin", (e) => { if (e.target.matches("[data-box]")) { m.keyed = true; invoke("mini_key", { on: true }).catch(() => {}); } });
    // A click anywhere outside takes the keyboard elsewhere: the open chat folds away, as a menu would.
    window.addEventListener("blur", () => closeChat(false));
    root.addEventListener("input", (e) => { if (e.target.matches("[data-box]")) { m.typed[m.open] = e.target.value; grow(e.target); } });
    root.addEventListener("keydown", (e) => {
      if (e.key === "Escape") { e.preventDefault(); closeChat(); }
      else if (e.key === "Enter" && !e.shiftKey && e.target.matches("[data-box]")) { e.preventDefault(); send(); }
    });
    // Hover opens the first session's chat after a short pause (a pointer passing by doesn't); the dots switch.
    // Leaving folds it back, unless you've clicked into it (then a click outside or Esc does).
    // Where there are no dots to point at (the notch's island), hovering opens the first session's chat.
    document.body.addEventListener("mouseenter", () => {
      clearTimeout(m.timer);
      if ((m.shape === "quiet" || m.shape === "nod") && !root.querySelector(".m-dotb")) m.timer = setTimeout(() => { const sid = m.nod ? state.items.find((i) => i.id === m.nod)?.session_id || first() : first(); if (sid) openChat(sid, false); }, HOVER_MS);
    });
    document.body.addEventListener("mouseleave", () => { clearTimeout(m.timer); m.hoverSid = null; magnify(null); if (m.shape === "chat" && !m.keyed) m.timer = setTimeout(() => closeChat(false), LEAVE_MS); });
    // Along the dots, as the Dock does: the one under the pointer grows, its neighbours a little. Resting on one
    // opens its chat (the keyboard stays where it is); moving on to another switches to it, unless you're typing.
    root.addEventListener("mousemove", (e) => {
      magnify(e);
      const b = e.target.closest(".m-dotb");
      if (!b || b.dataset.open === m.hoverSid) return;
      m.hoverSid = b.dataset.open;
      clearTimeout(m.timer);
      m.timer = setTimeout(() => { if (!m.keyed && m.open !== m.hoverSid) openChat(m.hoverSid, false); }, HOVER_MS);
    });
  }
  /** Each dot's size from the pointer's distance to it, along the dots (down the rail, across the notch's head):
   *  up to 2.2 times at the pointer, back to its own size 56 pt away. `e` null: all back to their own size. */
  function magnify(e) {
    for (const b of root.querySelectorAll(".m-dotb")) {
      const r = b.getBoundingClientRect();
      const d = !e ? Infinity : NOTCH ? Math.abs(e.clientX - (r.left + r.width / 2)) : Math.abs(e.clientY - (r.top + r.height / 2));
      const grow = 1 + 1.2 * Math.max(0, 1 - d / 56);
      const dot = b.querySelector(".m-dot");
      if (dot) dot.style.transform = grow > 1.001 ? `scale(${grow.toFixed(3)})` : "";
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
  }
  function take(s) {
    state = s;
    theme();
    arrivals();
    paint();
  }

  async function boot() {
    bind();
    take(await invoke("get_state"));
    await T.event.listen("state", (e) => take(e.payload));
    // Ages move on with no new state: once a minute is enough for "4m".
    setInterval(() => { if (m.shape !== "chat") paint(); }, 60000);
    // Cue's window came forward or went back, maybe on another display: placed again, on that one.
    await T.event.listen("mini-place", () => { m.sized = ""; fit(); });
  }
  boot();
}
