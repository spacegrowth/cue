// Cue UI core: the helpers every view shares (formatting, names, sessions, caches, activity bars, scrolling).
// ---------- helpers ----------
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const now = () => Date.now() + clockSkew;
/** In conversation: what you said in the last few minutes (Waiting's top group in Cue, the mini window's blinking tabs). */
const TALK_MS = 5 * 60000;
/** At work on what you said in the last few minutes: in conversation with you while it works. */
const workingOnYours = (s) => { const you = (s.thread || []).filter((e) => e.role === "you").pop(); return !!you && now() - you.at_ms <= TALK_MS; };
function ago(ms) {
  const s = Math.max(0, Math.round((now() - ms) / 1000));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}
/** How long until a cache runs out, for the header's "going cold" chip ("in 8m 05s"). */
function coldIn(until) {
  const s = Math.max(0, Math.round((until - now()) / 1000));
  return s < 60 ? `in ${s}s` : `in ${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
}
/** How long until `until`, counting down: "in 12m", "in 1h 05m", "in 40s"; "now" once it's passed. */
function untilIn(until) {
  const s = Math.max(0, Math.round((until - now()) / 1000));
  return !s ? "now" : s < 60 ? `in ${s}s` : s < 3600 ? `in ${Math.ceil(s / 60)}m` : `in ${Math.floor(s / 3600)}h ${String(Math.floor((s % 3600) / 60)).padStart(2, "0")}m`;
}
const firstLine = (s) => String(s ?? "").split("\n").find((l) => l.trim()) || "";
const shortPath = (p) => { const parts = String(p).split("/"); return parts.length > 3 ? "…/" + parts.slice(-3).join("/") : p; };
const questions = (it) => it.tool_input?.questions || [];
const tool = (it) => (it.tool_name || "").toLowerCase();
const isBash = (it) => (it.kind === "permission" || it.kind === "terminal") && tool(it) === "bash";
/** A command as wrapping text: it breaks only between its words, so a flag never parts from its dash ("-" / "q -x");
 *  a word longer than the line still breaks inside itself (.cmd-w). */
const cmdWords = (text) => String(text).split(/(\s+)/).map((w, i) => i % 2 ? w : w && `<span class="cmd-w">${esc(w)}</span>`).join("");
/** A prompt waiting in its terminal that Cue can't answer (a sandboxed command's network access): you go there. */
const inTerminal = (it) => it?.kind === "terminal";
/** How each agent is named on screen. */
const agentName = (h) => ({ claude: "Claude Code", codex: "Codex", pi: "Pi" }[h] || h || "agent");
// Each agent gets a Moss letter. (Anthropic's logo needs their written permission, so it isn't used.)
/** The agent's mark: a letter in its own colour (Claude's orange, Pi's black), so you can tell them apart at a glance. */
const agentLetter = (h) => h === "pi" ? "π" : h === "codex" ? "X" : esc((h || "?")[0].toUpperCase());
const badge = (h) => `<span class="badge h-${esc(h || "other")}" title="${esc(agentName(h))}">${agentLetter(h)}</span>`;
const agentBadge = (h) => `<span class="badge h-${esc(h)}" role="img" aria-label="${esc(agentName(h))}">${agentLetter(h)}</span>`;
/** The agent most of your sessions run (live, parked, closed). */
function usualAgent() {
  const n = {};
  for (const x of [...state.sessions, ...(state.parked || []), ...closedAll()]) if (x.harness) n[x.harness] = (n[x.harness] || 0) + 1;
  return Object.keys(n).sort((a, b) => n[b] - n[a])[0] || "claude";
}
/** A card's agent, on the board's cards and the Parked drawer's rows: nothing for your usual one (it
 *  would repeat on every card), its mark for any other, so the odd one out is what you see. */
const oddAgent = (h) => h && h !== usualAgent() ? agentBadge(h) : "";
const draft = (id) => (drafts[id] ||= { text: "", choices: {}, images: [] });
/** A choice's label without its "(Recommended)", and whether it had one: [label, recommended]. */
function choiceLabel(label) {
  const m = /^(.*?)\s*\(recommended\)\s*$/i.exec(label || "");
  return m ? [m[1], true] : [label || "", false];
}
/** The Desk's mark before a session's name while it works for a Desk (desk/lead.js deskNameMark), "" without one. */
const deskSessMark = (sid) => (typeof deskNameMark === "function" ? deskNameMark(sid) : "");
/** A session's name: the Desk's mark and an add-on's before it. */
const nameSpan = (sid, project, cls = "proj") => `${deskSessMark(sid)}${addOn.nameMark(sid)}<span class="${cls}">${esc(nameOf(sid, project))}</span>${hearing(sid)}`;
/** Beside a session's name on any card while one of its replies is read out (Listen): a speaker, its waves
 *  moving while it reads, still while paused. */
const hearing = (sid) => {
  const l = typeof listening === "undefined" ? null : listening;
  if (!l || !sid || l.sid !== sid) return "";
  return `<span class="hearing ${l.playing ? "on" : ""}" role="img" aria-label="${l.playing ? "Reading it out" : "Paused reading it out"}">${SPEAKER_ICON}</span>`;
};
/** A speaker (Listen, and the cards' mark while it reads); its waves move where an "on" class around it says. */
const SPEAKER_ICON = `<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 9v6h4l5 4V5L8 9z" fill="currentColor"/><path class="hw1" d="M16 9.5a3.5 3.5 0 0 1 0 5"/><path class="hw2" d="M18.5 7a7 7 0 0 1 0 10"/></svg>`;
/** A machine's name: what you renamed it to, else its first part (no user@, no domain). (Notifications
 *  do the same: machines::display_name.) */
const machName = (m) => {
  const label = machinesList?.find((x) => x.name === m)?.label;
  if (label) return label;
  const h = String(m || "").replace(/^[^@]*@/, "");
  return /^[\d.]+$/.test(h) ? h : h.split(".")[0] || h;
};
/** Short enough for a line that has little room: past 10 characters, its first 5 and last 5. */
const machShort = (m) => { const n = machName(m); return n.length > 10 ? `${n.slice(0, 5)}…${n.slice(-5)}` : n; };
/** The label on everything about a session that runs on another machine (none for this Mac); on a card,
 *  in its top line, by the name. */
const machTag = (m) => m ? `<span class="mach"><span>${esc(machShort(m))}</span></span>` : "";
/** The same label in full, at the head of a line of text (Parked's rows). */
const machLead = (m) => m ? `<span class="mach lead">${esc(machName(m))}</span>` : "";
/** What Cue calls a session: the name you gave it, else its folder's name, else its project. */
// A session that has stopped (parked or closed) keeps its name: its record as it was.
const recordOf = (sid) => sessionOf(sid) || parkedOf(sid) || closedOf(sid);
/** A folder's last part ("…/cue-mini" → "cue-mini"). */
const baseName = (p) => String(p || "").split("/").filter(Boolean).pop() || "";
const nameOf = (sid, project) => addOn.name(sid, String(recordOf(sid)?.name || "")) || baseName(recordOf(sid)?.cwd) || project;
const findItem = (id) => state.items.find((i) => i.id === id) || fullItem(state.history.find((i) => i.id === id));
/** History's rows come brief (hub/kept_history.rs: no conversation, the request cut to a line): a row opened
 *  asks Cue for its card in full, once, and is drawn again with it. Until then (and if it's gone) the row stands in.
 *  A row that changes (its project moved, say) asks again: the card kept was the row as it was. */
const fullItems = new Map();   // id -> { row: the brief row as it was asked for (JSON), full: its card, once here }
function fullItem(it) {
  if (!it?.brief) return it;
  const row = JSON.stringify(it), kept = fullItems.get(it.id);
  if (kept?.row === row) return kept.full || it;
  const asked = { row, full: null };
  fullItems.set(it.id, asked);
  invoke("history_item", { id: it.id }).then(
    (full) => { if (full && fullItems.get(it.id) === asked) { asked.full = full; renderMain(); } },
    () => { if (fullItems.get(it.id) === asked) fullItems.delete(it.id); });
  return it;
}
const sessionOf = (sid) => state.sessions.find((s) => s.session_id === sid);
/** A session you parked (stopped, kept to resume): not live, so not in state.sessions. */
const parkedOf = (sid) => (state.parked || []).find((p) => p.session_id === sid);
/** Sessions that ended in the last 30 days (at most 30), the Parked drawer's Closed tab: asked for when
 *  that tab shows (again after a minute), not part of every state update. */
let closedList = null, closedAt = 0, closedLoading = false, closedAskedFor = null;
/** Every closed session Cue knows of, newest first: the saved list (full records) and the ones that ended
 *  while Cue ran (in each state update, from the moment the row goes, so a name never lapses to the
 *  folder's on the way: name, folder, project, agent, machine, when), each once, the full record first. */
const closedAll = () => { const seen = new Set(); return [...(closedList || []), ...(state.ended || [])].filter((c) => !seen.has(c.session_id) && seen.add(c.session_id)).sort((a, b) => b.ended_ms - a.ended_ms); };
const closedOf = (sid) => closedAll().find((c) => c.session_id === sid);
async function loadClosed() {
  if (closedLoading) return;
  closedLoading = true;
  try { closedList = await invoke("closed_sessions"); closedAt = now(); }
  catch (e) { closedList ||= []; toast(`Couldn't list closed sessions: ${e}`); }
  finally { closedLoading = false; renderMain(); }
}
/** The open session is a parked or a closed one (Active shows it read-only, with Resume): its record,
 *  with `kind` and when it stopped (`at`). */
function parkedActive() {
  if (!active?.sid || sessionOf(active.sid)) return null;
  const p = parkedOf(active.sid);
  if (p) return { ...p, kind: "parked", at: p.parked_ms };
  const c = closedOf(active.sid);
  // Ended since the saved list was read: only its name and folder are here; its chat needs the record.
  if (c && !c.thread && !closedLoading && closedAt < c.ended_ms) queueMicrotask(loadClosed);
  return c ? { ...c, kind: "closed", at: c.ended_ms } : null;
}

/** A stretch narrower than this share of the bar can't show its stripes (one stripe is about 3.5% of a Sessions card's
 *  bar): its outline closed round a sliver of stripe read as a "0". It's drawn solid in the stripes' color. */
const BAR_THIN = 3.5;
/** A session's activity bar: its last 30 minutes, solid = working, striped = waiting on you, faint = idle. Each
 *  kind of stretch is one layer the bar's width, masked to where that kind held, so a bar is a handful of
 *  elements however many stretches it has: the window is drawn whole on every change, and an element a
 *  stretch made a busy board's bars most of what each draw laid out (700 of them ten minutes into a soak,
 *  every draw slower with each). The stretch still going stays an element of its own: its stripes march. */
function bar(sid) {
  const s = sessionOf(sid);
  const w = state.window_ms || 1800000;
  const from = now() - w;
  const layers = new Map();   // its classes -> [[left %, right %]]
  let live = "";
  for (const g of s?.segments || []) {
    const start = Math.max(g.start_ms, from);
    const left = ((start - from) / w) * 100;
    const width = Math.min(Math.max(0.6, (((g.end_ms ?? now()) - start) / w) * 100), 100 - left);
    const kind = g.kind === "working" ? "w" : g.kind === "idle" ? "i" : "z";
    const thin = width < BAR_THIN ? " thin" : "";
    if (g.end_ms == null) { live = `<i class="${kind} live${thin}" style="left:${left.toFixed(2)}%;width:${width.toFixed(2)}%"></i>`; continue; }
    const cls = kind === "z" ? kind + thin : kind;   // only stripes are drawn otherwise when thin
    if (!layers.has(cls)) layers.set(cls, []);
    layers.get(cls).push([left, left + width]);
  }
  const drawn = [...layers].map(([cls, spans]) => `<i class="${cls} layer" style="${barLayerStyle(cls, spans)}"></i>`);
  return `<div class="bar" title="last 30 min · solid = working, striped = waiting on you">${drawn.join("")}${live}<b class="now"></b></div>`;
}
/** A bar layer's style: masked to its stretches (`spans`, % of the bar). Striped stretches each keep their
 *  outline: its top and bottom are the layer's own, its two ends drawn here (`--ends`). */
function barLayerStyle(cls, spans) {
  const pc = (v) => `${v.toFixed(2)}%`;
  const mask = `linear-gradient(to right,${spans.map(([a, b]) => `transparent ${pc(a)},#000 ${pc(a)},#000 ${pc(b)},transparent ${pc(b)}`).join(",")})`;
  const line = (at, to) => `transparent ${at},var(--accent) ${at},var(--accent) ${to},transparent ${to}`;
  const ends = cls === "z" ? `--ends:linear-gradient(to right,${spans.map(([a, b]) => `${line(pc(a), `calc(${pc(a)} + 1px)`)},${line(`calc(${pc(b)} - 1px)`, pc(b))}`).join(",")});` : "";
  return `${ends}-webkit-mask-image:${mask};mask-image:${mask}`;
}
/** Above the big bar: how long each stretch lasted, over the ones wide enough to hold a label. */
function barLabels(sid) {
  const s = sessionOf(sid), w = state.window_ms || 1800000, from = now() - w;
  const dur = (ms) => ms < 60000 ? `${Math.max(1, Math.round(ms / 1000))}s` : `${Math.round(ms / 60000)}m`;
  const spans = (s?.segments || []).filter((g) => g.kind !== "idle").map((g) => {
    const start = Math.max(g.start_ms, from), end = g.end_ms ?? now();
    return { kind: g.kind, left: ((start - from) / w) * 100, width: ((end - start) / w) * 100, ms: end - start };
  }).filter((g) => g.width >= 6);
  return `<div class="bar-labels">${spans.map((g) => `<span class="${g.kind === "working" ? "w" : "z"}" style="left:${g.left.toFixed(2)}%;width:${g.width.toFixed(2)}%">${dur(g.ms)}</span>`).join("")}</div>`;
}
/** Under the big bar: what the stripes mean and which end is now, so you needn't hover. */
/** Under a session's bar: what its colors mean. The faint one (waiting on its lead or its background
 *  agents, stopped, out of usage: not on you) only when the bar has some. */
const barKey = (sid) => {
  const from = now() - (state.window_ms || 1800000);
  const faint = (sessionOf(sid)?.segments || []).some((g) => g.kind === "idle" && (g.end_ms ?? now()) > from);
  const word = "not on you";
  return `<div class="bar-key"><span>30 min ago</span><span class="grow"></span><span><i class="sw w"></i>working</span><span><i class="sw z"></i>waiting on you</span>${faint ? `<span><i class="sw i"></i>${word}</span>` : ""}<span class="grow"></span><span>now</span></div>`;
};

/** The selected card in each list (Waiting, Sessions, Starred, Recently answered), by list, with whether
 *  it shows in full inside the list's scrolling area. */
function selectedCards() {
  const out = new Map();
  for (const el of document.querySelectorAll(".working.on, .nrow.on, .tl.on")) {
    const sc = scrollerOf(el);
    const a = el.getBoundingClientRect(), b = (sc || document.documentElement).getBoundingClientRect();
    out.set(el.closest(".sec, .col")?.className || "", { el, full: a.top >= b.top - 1 && a.bottom <= b.bottom + 1 });
  }
  return out;
}
/** The list that scrolls a card (its nearest scrolling ancestor), or null. */
function scrollerOf(el) {
  let sc = el?.parentElement;
  while (sc && sc !== document.body && !/(auto|scroll)/.test(getComputedStyle(sc).overflowY)) sc = sc.parentElement;
  return sc && sc !== document.body ? sc : null;
}
/** Glide a list just enough to show a card in full (with a little room past it). Your own scrolling stops it. */
let glide = null;
function glideIntoView(el) {
  const sc = scrollerOf(el);
  if (!sc) return;
  const a = el.getBoundingClientRect(), b = sc.getBoundingClientRect(), M = 12;
  // Cut off below: up, but never past its top. Cut off above: down to its top.
  let delta = a.bottom + M > b.bottom ? Math.min(a.bottom + M - b.bottom, a.top - M - b.top) : a.top - M < b.top ? a.top - M - b.top : 0;
  if (Math.abs(delta) < 1) return;
  // Down: on to where a card starts, so the one at the list's top isn't cut in half (it was, under the card you'd
  // just replied to, until the list was drawn again). Never past this card's own top.
  if (delta > 0) {
    const tops = [...el.parentElement.children].map((c) => c.getBoundingClientRect().top - b.top).filter((t) => t >= delta - 1);
    if (tops.length) delta = Math.min(Math.min(...tops), a.top - b.top);
  }
  // Found again by its list and what it is (a redraw makes new elements).
  const list = el.closest(".sec, .col"), attr = ["data-session", "data-detail", "data-big"].find((n) => el.hasAttribute(n));
  const key = list && attr ? `.${[...list.classList].join(".")} [${attr}="${CSS.escape(el.getAttribute(attr))}"]` : null;
  glideTo(key ? () => scrollerOf(document.querySelector(key)) : () => sc, sc.scrollTop + delta);
}
/** Scroll what `find()` gives (found again each frame) to `to`, easing out over 280 ms; instant with reduced
 *  motion. Driven frame by frame, not the browser's smooth scroll: a redraw (every few seconds while sessions
 *  work) replaces or moves the element, which stops that part-way, and in WebKit one started just after a
 *  redraw moved the element runs from its top. A glide to where one is already going goes on. */
function glideTo(find, to) {
  const sc = find();
  if (!sc) return;
  to = Math.round(Math.max(0, Math.min(sc.scrollHeight - sc.clientHeight, to)));
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) { glide = null; sc.scrollTop = to; return; }
  if (glide?.to === to) return;
  const from = sc.scrollTop, g = (glide = { t0: performance.now(), to });
  const step = (t) => {
    if (glide !== g) return;
    const s = find();
    if (!s) { glide = null; return; }
    const k = Math.max(0, Math.min(1, (t - g.t0) / 280));
    s.scrollTop = from + (to - from) * (1 - Math.pow(1 - k, 3));   // eases out
    if (k < 1) requestAnimationFrame(step); else glide = null;
  };
  requestAnimationFrame(step);
}
