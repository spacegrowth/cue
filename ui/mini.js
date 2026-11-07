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
  const WIDTH = NOTCH ? { quiet: 260, nod: 500, list: 560, chat: 560 } : { quiet: 28, list: 320, chat: 448 };   // the rail's chat: 420 + its dots
  const HOVER_MS = 280, LEAVE_MS = 450, NOD_MS = 6000;
  const root = document.getElementById("mini");
  // shape: quiet | nod | list | chat. open: the session whose chat is open. at: where on the screen it opened.
  const m = { shape: "quiet", open: null, at: null, timer: 0, nodT: 0, sized: "", fresh: new Set(), sending: false, typed: {}, seen: null, nod: null, done: new Set(), note: "" };

  // ---------- what needs you, in the Waiting column's order ----------
  /** Back within a few minutes of your last message to it, and not sat there longer than that (board.js talking). */
  function talking(it) {
    if (it.back_ms || now() - it.created_ms > TALK) return false;
    const you = (sessionOf(it.session_id)?.thread || []).filter((e) => e.role === "you" && e.at_ms <= it.created_ms).pop();
    return !!you && it.created_ms - you.at_ms <= TALK;
  }
  const newest = (a, b) => (!!a.back_ms - !!b.back_ms) || (a.back_ms ? a.back_ms - b.back_ms : b.created_ms - a.created_ms);
  /** Conversations first (in the order they came back), then newest first; parked sessions' finished turns stay in Need to decide. */
  function needs() {
    const parked = new Set(state.sessions.filter((s) => s.later_ms).map((s) => s.session_id));
    const all = state.items.filter((i) => i.kind !== "waiting" || !parked.has(i.session_id));
    const talk = all.filter(talking).sort((a, b) => a.created_ms - b.created_ms);
    const rest = all.filter((i) => !talking(i)).sort(newest);
    const shown = [...talk, ...rest].slice(0, MAX);
    return { shown, talk: Math.min(talk.length, MAX), more: all.length - shown.length };
  }
  const working = () => state.sessions.filter((s) => s.state === "working").length;
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
  const what = (it) => asks(it) ? `${verb(it)} · ${summary(it)}` : ["your turn", shapeOf(finishedText(it))].filter(Boolean).join(" · ");
  const dotCls = (it) => asks(it) ? "ask" : "turn";

  // ---------- drawing ----------
  const dot = (it, extra = "") => `<i class="m-dot ${dotCls(it)} ${extra}"></i>`;
  const row = (it) => `<button class="m-row" data-open="${esc(it.session_id)}">${dot(it)}<span class="m-name">${esc(name(it))}</span><span class="m-what">${esc(what(it))}</span><span class="m-age">${ago(it.created_ms)}</span></button>`;
  const sep = (n, i) => n && i === n ? `<div class="m-sep" role="separator"></div>` : "";

  function dotsHtml() {
    const { shown, talk } = needs();
    const on = (it) => m.shape === "chat" && it.session_id === m.open ? "on" : "";
    return `<div class="m-panel m-dots">${shown.map((it, i) => sep(talk, i) + `<button class="m-dotb ${on(it)}" data-open="${esc(it.session_id)}" aria-label="${esc(name(it))}">${dot(it, m.fresh.has(it.id) ? "new" : "")}</button>`).join("")}</div>`;
  }
  function listHtml() {
    const { shown, talk, more } = needs();
    const busy = working();
    const head = shown.length ? `${state.items.length} need${state.items.length === 1 ? "s" : ""} you` : "Nothing needs you";
    const foot = [busy ? `${busy} working` : "", more ? `${more} more in Cue` : ""].filter(Boolean).join(" · ");
    return `<div class="m-panel m-list"><div class="m-head"><b>${head}</b><span class="g"></span><button class="m-link" data-cue="">Open Cue</button></div>
      ${shown.map((it, i) => sep(talk, i) + row(it)).join("")}${foot ? `<div class="m-foot-line">${foot}</div>` : ""}</div>`;
  }
  function islandHtml() {
    const { shown } = needs();
    const ask = shown.filter(asks).length, turn = shown.length - ask, busy = working();
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
    const msgs = all.length ? all : it ? [{ role: "agent", text: finishedText(it) }] : [];
    return msgs.map(msg).join("") || `<div class="m-quiet">Nothing said yet.</div>`;
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
    return `<div class="m-panel m-chat"><div class="m-chat-head">${it ? dot(it) : ""}<b>${esc(nameOf(sid, s?.project || it?.project))}</b>${tag}<span class="g"></span><button class="m-link" data-cue="${esc(sid)}">⤢ In Cue</button><button class="m-link" data-close aria-label="Close">×</button></div>
      <div data-ask>${askHtml(sid)}</div><div class="m-thread" data-thread>${threadHtml(sid)}</div>
      <div class="m-foot"><textarea class="m-box" data-box rows="1" placeholder="Reply to ${esc(nameOf(sid, s?.project || it?.project))}…">${esc(m.typed[sid] || "")}</textarea>
      <div class="m-foot-row"><span class="m-quiet">${esc(m.note || "⇧↵ new line")}</span><span class="g"></span><button class="m-btn" data-send ${m.sending ? "disabled" : ""}>Send ↵</button></div></div></div>`;
  }

  // ---------- size and place ----------
  /** The window as big as what it shows: the panel's own size, the chat no taller than its share of the screen. */
  function fit(force = false) {
    const p = root.querySelector(".m-panel");
    const cap = Math.round(screen.height * CAP);
    document.documentElement.style.setProperty("--cap", `${cap}px`);
    const empty = !NOTCH && m.shape === "quiet" && !needs().shown.length;
    const w = empty ? 4 : WIDTH[m.shape] || WIDTH.quiet;   // nothing waiting: the rail is a hairline
    const h = empty ? 200 : Math.min(p ? Math.ceil(p.getBoundingClientRect().height) : 30, m.shape === "chat" ? cap : 4000);
    const args = { w, h, at: m.shape === "chat" ? m.at : null, show: true };
    const key = JSON.stringify(args);
    if (key === m.sized && !force) return;   // the same size and place: nothing to move
    m.sized = key;
    invoke("mini_shape", args).catch(() => { m.sized = ""; });
  }
  function shapeHtml() {
    if (m.shape === "chat" && m.open) return NOTCH ? chatHtml() : `<div class="m-panel m-pair">${chatHtml()}${dotsHtml()}</div>`;
    if (m.shape === "list") return listHtml();
    if (m.shape === "nod") return nodHtml();
    if (NOTCH) return islandHtml();
    return needs().shown.length ? dotsHtml() : `<div class="m-panel m-hair"></div>`;
  }
  /** Draw what the shape shows. A chat you're typing in keeps its box: only the ask and the thread are redrawn. */
  function paint() {
    document.body.className = `mini ${NOTCH ? "notch" : "rail"} s-${m.shape}`;
    const box = root.querySelector("[data-box]");
    if (m.shape === "chat" && box && root.querySelector(".m-chat")) {
      const t = root.querySelector("[data-thread]");
      const atEnd = t.scrollHeight - t.scrollTop - t.clientHeight < 24;
      root.querySelector("[data-ask]").innerHTML = askHtml(m.open);
      t.innerHTML = threadHtml(m.open);
      if (atEnd) t.scrollTop = t.scrollHeight;
      root.querySelector("[data-send]").disabled = m.sending;
      root.querySelector(".m-foot-row .m-quiet").textContent = m.note || "⇧↵ new line";
      const dots = root.querySelector(".m-pair > .m-dots");
      if (dots) dots.outerHTML = dotsHtml();
    } else {
      root.innerHTML = shapeHtml();
      if (m.shape === "chat") toLatest();
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
  function openChat(sid, el) {
    const r = el?.getBoundingClientRect();
    m.at = r ? window.screenY + r.top + r.height / 2 : null;
    m.open = sid;
    m.note = "";
    m.shape = "chat";
    paint();
  }
  function closeChat() {
    m.open = null;
    invoke("mini_key", { on: false }).catch(() => {});
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
    if (all.length) {
      m.fresh = new Set(all.map((i) => i.id));
      setTimeout(() => { m.fresh = new Set(); }, 1500);
    }
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
      if (t.dataset.open !== undefined) openChat(t.dataset.open, t);
      else if (t.dataset.cue !== undefined) invoke("mini_open_in_cue", { sessionId: t.dataset.cue || null }).catch(() => {});
      else if (t.dataset.close !== undefined) closeChat();
      else if (t.dataset.allow) answer(t.dataset.allow, "allow");
      else if (t.dataset.deny) answer(t.dataset.deny, "deny");
      else if (t.dataset.send !== undefined) send();
      else if (t.dataset.copyCode !== undefined) invoke("copy_text", { text: codeText(t) }).then(() => { m.note = "Copied"; paint(); }, () => {});
    });
    root.addEventListener("focusin", (e) => { if (e.target.matches("[data-box]")) invoke("mini_key", { on: true }).catch(() => {}); });
    root.addEventListener("input", (e) => { if (e.target.matches("[data-box]")) { m.typed[m.open] = e.target.value; grow(e.target); } });
    root.addEventListener("keydown", (e) => {
      if (e.key === "Escape") { e.preventDefault(); closeChat(); }
      else if (e.key === "Enter" && !e.shiftKey && e.target.matches("[data-box]")) { e.preventDefault(); send(); }
    });
    // Hover opens the list after a short pause (a pointer passing by doesn't); leaving folds it back. An open chat stays.
    document.body.addEventListener("mouseenter", () => { clearTimeout(m.timer); if (m.shape === "quiet" || m.shape === "nod") m.timer = setTimeout(() => go("list"), HOVER_MS); });
    document.body.addEventListener("mouseleave", () => { clearTimeout(m.timer); if (m.shape === "list") m.timer = setTimeout(() => go("quiet"), LEAVE_MS); });
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
    // Cue's window went to the back: the mini window comes back as it was.
    await T.event.listen("mini-again", () => fit(true));
    // Ages move on with no new state: once a minute is enough for "4m".
    setInterval(() => { if (m.shape !== "chat") paint(); }, 60000);
  }
  boot();
}
