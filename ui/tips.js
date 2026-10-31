// Cue UI — tips: now and then, one short note at the end of the line under the chat (chat.js whereNow) about a
// key or a command that fits what the session is doing, each with its own icon. At most TIPS_PER_DAY a day, the
// same one at most once a day, and it stays until that line changes. One you use, or close with ×, never comes
// back. Settings → Insights → Show tips turns them off. What was shown and closed is kept in this window
// (localStorage): losing it only means a tip may come round again.
//
// Tell me (⌘D, or ⋯) has the same place: while it's on, the line says so with Undo, whatever Show tips says.
// Its tip, "⌘D tells you when it's done", comes on every turn past TELL_AFTER_MS until you've used ⌘D.

const TIPS_PER_DAY = 3;
const TELL_AFTER_MS = 2 * 60 * 1000;

const tipSvg = (body) => `<svg class="tip-ico" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
const TIP_ICONS = {
  chat: tipSvg('<path d="M21 12a8 8 0 0 1-11.6 7.1L4 20.5l1.4-4.6A8 8 0 1 1 21 12z"/>'),
  search: tipSvg('<circle cx="11" cy="11" r="6.5"/><path d="M20 20l-4.2-4.2"/>'),
  terminal: tipSvg('<rect x="2.5" y="4" width="19" height="16" rx="2.5"/><path d="M7 9l3 3-3 3"/><path d="M12.5 15H17"/>'),
  plus: tipSvg('<path d="M12 5v14M5 12h14"/>'),
};

/** Every tip: the line it goes on ("working": a turn running; "waiting": your turn), whether it fits now
 *  (`when`), and its words. Only ⌘ keys and buttons: a single key (N, H, A) works only outside a text box, and
 *  the reply box nearly always has the cursor. In order of use: the first that fits is the one shown. */
const TIPS = [
  { id: "btw", line: "working", icon: "chat", when: (it, s) => s.harness === "claude", text: "<b>btw</b> (top right) asks a side question without stopping it" },
  { id: "find", line: "waiting", icon: "search", text: "<kbd>⌘K</kbd> finds any session, or what was said in it" },
  { id: "shell", line: "waiting", icon: "terminal", text: "<kbd>⌘J</kbd> opens a terminal of your own" },
  { id: "new", line: "waiting", icon: "plus", text: "<kbd>⌘N</kbd> starts a new session" },
];

// ---------- what's been shown and closed ----------
const TIPS_KEY = "cue.tips";
/** { gone: [ids you used or closed], day: "2026-10-31", shown: [ids shown that day] } */
let tipsMem = (() => { try { return JSON.parse(localStorage.getItem(TIPS_KEY) || "{}"); } catch { return {}; } })();
const tipsSave = () => { try { localStorage.setItem(TIPS_KEY, JSON.stringify(tipsMem)); } catch {} };
const tipGoneAlready = (id) => (tipsMem.gone || []).includes(id);
/** Today's count, started afresh on a new day (by this Mac's date). */
function tipsToday() {
  const day = new Date().toLocaleDateString("en-CA");
  if (tipsMem.day !== day) { tipsMem.day = day; tipsMem.shown = []; }
  return tipsMem.shown;
}
/** You used what a tip is about (its key, its button): that tip never shows again. */
function tipUsed(id) {
  if (tipGoneAlready(id)) return;
  (tipsMem.gone ||= []).push(id);
  tipsSave();
}

// ---------- the tip on the line now ----------
/** The tip on the open session's line: { at: "sid|line", id, seen }. It stays while that line does; a closed
 *  or used one leaves the line empty rather than making room for the next. */
let tipNow = null;

/** The tip (or Tell me) to end the open session's status line with, `line` being "working" or "waiting". */
function tipFor(it, s, line) {
  if (!s?.session_id) return "";
  const tell = tellTip(s, line);
  if (tell) return tell;
  if (state.settings?.tips?.on === false) return "";
  const at = `${s.session_id}|${line}`;
  if (tipNow?.at !== at) tipNow = { at, id: pickTip(it, s, line), seen: false };
  const tip = tipNow.id && !tipGoneAlready(tipNow.id) ? TIPS.find((t) => t.id === tipNow.id) : null;
  if (!tip) return "";
  return `<span class="cv-tip" data-tip-id="${tip.id}"><span class="tip-sep">·</span>${TIP_ICONS[tip.icon]}<span>${tip.text}</span><button class="tip-x" data-act="tip-x" data-tip-id="${tip.id}" aria-label="Don't show this tip again">×</button></span>`;
}
/** The first tip for this line that fits now, isn't gone, and wasn't shown today, while today has room. */
function pickTip(it, s, line) {
  const shown = tipsToday();
  if (shown.length >= TIPS_PER_DAY) return null;
  return TIPS.find((t) => t.line === line && !tipGoneAlready(t.id) && !shown.includes(t.id) && (!t.when || t.when(it, s)))?.id || null;
}
/** Tell me on a running turn: while it's on (⌘D or ⋯), the line says so, with Undo. Off, on a turn past
 *  TELL_AFTER_MS: its tip, until ⌘D has been used once (or it's closed), outside today's count. */
function tellTip(s, line) {
  if (line !== "working" || !atWork(s)) return "";
  if (s.watch_ms) return `<span class="cv-tip tip-tell on"><span class="tip-sep">·</span>${tadaIcon(12, true)}<span>Telling you when it's done</span><button class="tip-link" data-act="watch" data-sid="${esc(s.session_id)}" data-on="0">Undo</button></span>`;
  if (Date.now() - s.since_ms < TELL_AFTER_MS || tipGoneAlready("tell") || state.settings?.tips?.on === false) return "";
  return `<span class="cv-tip tip-tell" data-tip-id="tell"><span class="tip-sep">·</span>${tadaIcon(12)}<span><kbd>⌘D</kbd> tells you when it's done</span><button class="tip-x" data-act="tip-x" data-tip-id="tell" aria-label="Don't show this tip again">×</button></span>`;
}

/** After a render (and a resize): a tip that would put its line on a second row goes, so a long line gets no
 *  tip; one that shows counts toward today. Tell me always stays (the text before it shortens instead). */
function fitTips() {
  const el = document.querySelector(".cv-status .cv-tip:not(.tip-tell)");
  if (!el) return;
  const line = el.closest(".cv-status");
  el.hidden = true;
  const without = line.getBoundingClientRect().height;
  el.hidden = false;
  if (line.getBoundingClientRect().height > without + 2) { el.hidden = true; return; }
  if (tipNow?.id !== el.dataset.tipId || tipNow.seen) return;
  tipNow.seen = true;
  const shown = tipsToday();
  if (!shown.includes(tipNow.id)) { shown.push(tipNow.id); tipsSave(); }
}

/** Settings → Insights: the one switch. */
const tipsSettings = (st) => `<div class="set-group">Tips</div>${setRow("Show tips", `Now and then, one short note at the end of the line under the chat about a key or a command that fits what the session is doing. At most ${TIPS_PER_DAY} a day; one you use, or close with ×, never comes back.`, toggle("tips.on", st.tips?.on !== false))}`;
